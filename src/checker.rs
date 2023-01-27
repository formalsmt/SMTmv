use crate::lemma::{Lemma, Theory};
use fs_extra::dir::CopyOptions;
use isabelle::client::{AsyncResult, IsabelleClient};
use isabelle::commands::{PurgeTheoryArgs, UseTheoryArgs};
use isabelle::process;
use std::env::temp_dir;
use std::os::unix::prelude::FileExt;
use std::path::PathBuf;
use std::str::FromStr;
use std::{fs, io};

pub enum CheckResult {
    OK,
    FailedUnknown,
    FailedInvalid,
}

pub trait ModelVerifier {
    fn check_model(&mut self, lemma: &Lemma) -> CheckResult;
}

pub struct BatchVerifier {
    theory_root: String,
}

impl BatchVerifier {
    #[allow(unused)]
    pub fn new(theory_root: &str) -> Self {
        Self {
            theory_root: theory_root.to_string(),
        }
    }

    fn run_isabelle(&self, dir: &PathBuf, theory_root: &str) -> Result<CheckResult, String> {
        let mut options = process::OptionsBuilder::new();
        options
            .build_pide_reports(false)
            .pide_reports(false)
            .process_output_limit(1)
            .process_output_tail(1)
            .quick_and_dirty(true);

        let args = process::ProcessArgs {
            theories: vec!["Validation".to_owned()],
            session_dirs: vec![theory_root.to_owned()],
            logic: None,
            options: options.into(),
        };
        let output = match process::batch_process(&args, Some(dir)) {
            Ok(o) => o,
            Err(e) => return Err(e.to_string()),
        };

        let stderr = String::from_utf8(output.stderr).expect("Failed to decode stderr");
        let stdout = String::from_utf8(output.stdout).expect("Failed to decode stdout");

        if output.status.success() {
            Ok(CheckResult::OK)
        } else if stdout.contains("Failed to finish proof") {
            log::debug!("{}", stdout);
            if stdout.contains("1. False") {
                // Heuristic
                Ok(CheckResult::FailedInvalid)
            } else {
                Ok(CheckResult::FailedUnknown)
            }
        } else {
            log::warn!("{}", stdout);
            log::warn!("{}", stderr);
            Err(stderr)
        }
    }
}

impl ModelVerifier for BatchVerifier {
    fn check_model(&mut self, lemma: &Lemma) -> CheckResult {
        // Create temporary folder
        let dir = make_dir();

        // Copy Isabelle theory files
        let mut options = CopyOptions::new();
        options.content_only = true;
        options.depth = 1;
        options.overwrite = true;

        if let Err(e) = fs_extra::dir::copy(&self.theory_root, &dir, &options) {
            panic!("{}", e);
        }

        // Create new theory file with lemma
        let mut theory = Theory::new("Validation", false);
        theory.add_theory_import("QF_S");
        theory.add_lemma(lemma.clone());

        let th = theory.to_isabelle();

        log::debug!("{}", th);

        match fs::File::create(dir.join("Validation.thy")) {
            Ok(th_file) => {
                if let Err(e) = th_file.write_all_at(th.as_bytes(), 0) {
                    panic!("{}", e)
                }
            }
            Err(e) => panic!("{}", e),
        }

        // Call isabelle
        match self.run_isabelle(&dir, &self.theory_root) {
            Ok(r) => r,
            Err(e) => {
                panic!("Failed to check lemma {:?}", lemma);
            }
        }
    }
}

pub struct ClientVerifier {
    client: IsabelleClient,
    theory_root: String,
    session_id: String,
    runtime: tokio::runtime::Runtime,
    temp_dir: String,
}

impl ClientVerifier {
    pub fn start_server(theory_root: &str) -> io::Result<Self> {
        let (port, pass) = isabelle::server::run_server(Some("vmv_server"))?;
        log::debug!("Isabelle server is running on port {}", port);
        let client = IsabelleClient::connect(None, port, &pass);
        let runtime = tokio::runtime::Runtime::new()?;

        let mut v = Self {
            client,
            theory_root: theory_root.to_string(),
            runtime,
            session_id: "".to_owned(),
            temp_dir: "".to_owned(),
        };

        v.start_session()?;
        v.copy_files();
        v.load_theory("QF_S")?;

        Ok(v)
    }

    fn start_session(&mut self) -> io::Result<()> {
        log::debug!("Staring HOL session");
        let mut args = isabelle::commands::SessionBuildStartArgs::session("HOL");
        args.options = Some(vec![
            "system_log=false".to_owned(),
            "process_output_limit=1".to_owned(),
            "process_output_tail=1".to_owned(),
            "pide_reports=false".to_owned(),
            "build_pide_reports=false".to_owned(),
            "headless_check_limit=1".to_owned(),
        ]);
        let res = async { self.client.session_start(&args).await };
        let resp = self.runtime.block_on(res)?;
        match resp {
            AsyncResult::Finished(r) => {
                self.session_id = r.session_id;
                self.temp_dir = r.tmp_dir.unwrap();
                Ok(())
            }
            AsyncResult::Error(m) => panic!("{:?}", m),
            AsyncResult::Failed(f) => panic!("{:?}", f),
        }
    }

    fn copy_files(&self) {
        // Copy Isabelle theory files
        let mut options = CopyOptions::new();
        options.content_only = true;
        options.depth = 1;
        options.overwrite = true;

        if let Err(e) = fs_extra::dir::copy(&self.theory_root, self.temp_dir.clone(), &options) {
            panic!("{}", e);
        }
    }

    fn load_theory(&mut self, name: &str) -> io::Result<()> {
        log::debug!("Loading theory {}", name);
        let session_id = self.session_id.clone();
        let mut args: UseTheoryArgs = UseTheoryArgs::for_session(&session_id, &[name]);
        args.master_dir = Some(self.theory_root.clone());

        let res = async { self.client.use_theories(&args).await };

        let resp = self.runtime.block_on(res)?;
        match resp {
            AsyncResult::Finished(_) => {
                log::debug!("Loaded theory {}", name);
                Ok(())
            }
            AsyncResult::Error(m) => panic!("{:?}", m),
            AsyncResult::Failed(f) => panic!("{:?}", f),
        }
    }
}

impl ModelVerifier for ClientVerifier {
    fn check_model(&mut self, lemma: &Lemma) -> CheckResult {
        // Create temporary folder

        let session_id = self.session_id.clone();

        let mut theory = Theory::new("Validation", false);
        theory.add_lemma(lemma.clone());
        theory.add_theory_import("QF_S");

        let dir = PathBuf::from_str(&self.temp_dir).unwrap();
        match fs::File::create(dir.join("Validation.thy")) {
            Ok(th_file) => {
                if let Err(e) = th_file.write_all_at(&theory.to_isabelle().as_bytes(), 0) {
                    panic!("{}", e)
                }
            }
            Err(e) => panic!("{}", e),
        }

        let path = dir.join("Validation");
        let path = path.to_str().unwrap();

        let mut args: UseTheoryArgs = UseTheoryArgs::for_session(&session_id, &[path]);
        args.master_dir = Some(self.theory_root.clone());
        //args.nodes_status_delay = Some(-1.0);
        args.check_limit = Some(1);
        args.unicode_symbols = Some(true);

        log::debug!("Checking\n{}", theory.to_isabelle());

        let result = match self
            .runtime
            .block_on(self.client.use_theories(&args))
            .unwrap()
        {
            AsyncResult::Error(e) => {
                log::warn!("Error proving theory: {:?}", e);
                CheckResult::FailedUnknown
            }
            AsyncResult::Failed(f) => {
                // TODO: Check reason!
                log::warn!("Proving theory failed: {:?}", f.message);
                CheckResult::FailedUnknown
            }
            AsyncResult::Finished(f) => {
                if f.ok {
                    CheckResult::OK
                } else {
                    log::warn!("Proving theory unsuccessful: {:?}", f.errors);
                    log::warn!("{}", theory.to_isabelle());
                    CheckResult::FailedUnknown
                }
            }
        };

        // Purge theory to release resources
        let mut args: PurgeTheoryArgs = PurgeTheoryArgs::for_session(&session_id, &[path]);
        args.master_dir = Some(self.theory_root.clone());

        match self.runtime.block_on(self.client.purge_theories(args)) {
            Ok(ok) => (),
            Err(e) => panic!("Failed to purge theory, aborting: {:?}", e),
        }

        result
    }
}

fn make_dir() -> PathBuf {
    let temp_dir = temp_dir().join("isabelle_checker");
    if let std::io::Result::Err(e) = fs::create_dir_all(&temp_dir) {
        panic!("{}", e)
    }
    temp_dir
}
