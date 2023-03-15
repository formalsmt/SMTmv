use crate::lemma::{Lemma, Theory};
use fs_extra::dir::CopyOptions;
use isabelle_client::client::args::{PurgeTheoryArgs, UseTheoriesArgs};
use isabelle_client::client::{AsyncResult, IsabelleClient};
use isabelle_client::process;
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
            .record_proofs(0)
            .parallel_proofs(2)
            .quick_and_dirty(true);

        let args = process::ProcessArgs {
            theories: vec!["Validation".to_owned()],
            session_dirs: vec![theory_root.to_owned()],
            logic: Some("smt".to_string()),
            options: options.into(),
        };

        log::debug!("Temp dir: {:?}", dir);
        let output = match tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(process::batch_process(&args, Some(dir)))
        {
            Ok(o) => o,
            Err(e) => return Err(e.to_string()),
        };

        let stderr = String::from_utf8(output.stderr).expect("Failed to decode stderr");
        let stdout = String::from_utf8(output.stdout).expect("Failed to decode stdout");
        log::debug!("Stdout: {}", stdout);
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
            Err(format!("Failed to check proof for {};{}", stdout, stderr))
        }
    }
}

impl ModelVerifier for BatchVerifier {
    fn check_model(&mut self, lemma: &Lemma) -> CheckResult {
        // Create temporary folder
        let dir = make_dir();

        // Remove old files
        //fs::remove_dir_all(&dir).unwrap();

        // Copy Isabelle theory files
        let mut options = CopyOptions::new();
        options.depth = 0;
        options.content_only = true;
        options.skip_exist = true;

        if let Err(e) = fs_extra::dir::copy(&self.theory_root, &dir, &options) {
            panic!("{}", e);
        }

        // Create new theory file with lemma
        let mut theory = Theory::new("Validation", false);
        theory.add_theory_import("smt.Strings");
        theory.add_theory_import("smt.Core");
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
        log::debug!("Dir: {:?}, THROOT: {}", dir.to_str(), &self.theory_root);
        match self.run_isabelle(&dir, &self.theory_root) {
            Ok(CheckResult::OK) => CheckResult::OK,
            Ok(CheckResult::FailedInvalid) => {
                log::warn!("{}", th.as_str());
                CheckResult::FailedInvalid
            }
            Ok(CheckResult::FailedUnknown) => {
                log::warn!("{}", th.as_str());
                CheckResult::FailedUnknown
            }
            Err(e) => {
                log::error!("{}", e);
                panic!("Failed to check lemma:\n{}", th.as_str());
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
    #[allow(unused)]
    pub fn start_server(theory_root: &str) -> io::Result<Self> {
        let server = isabelle_client::server::run_server(Some("vmv_server"))?;
        log::debug!("Isabelle server is running on port {}", server.port());
        let client = IsabelleClient::connect(None, server.port(), server.password());
        let runtime = tokio::runtime::Runtime::new()?;

        let mut v = Self {
            client,
            theory_root: theory_root.to_string(),
            runtime,
            session_id: "".to_owned(),
            temp_dir: "".to_owned(),
        };

        v.start_session()?;
        //v.load_theory("smt")?;

        Ok(v)
    }

    fn start_session(&mut self) -> io::Result<()> {
        log::debug!("Staring HOL session");
        let mut args = isabelle_client::client::args::SessionBuildArgs::session("HOL");
        args.dirs = Some(vec![self.theory_root.clone()]);
        args.include_sessions = vec![String::from("smt")];
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
}

impl ModelVerifier for ClientVerifier {
    fn check_model(&mut self, lemma: &Lemma) -> CheckResult {
        // Create temporary folder

        let session_id = self.session_id.clone();

        let mut theory = Theory::new("Validation", false);
        theory.add_theory_import("smt.Strings");
        theory.add_theory_import("smt.Core");
        theory.add_lemma(lemma.clone());

        let dir = PathBuf::from_str(&self.temp_dir).unwrap();
        match fs::File::create(dir.join("Validation.thy")) {
            Ok(th_file) => {
                if let Err(e) = th_file.write_all_at(theory.to_isabelle().as_bytes(), 0) {
                    panic!("{}", e)
                }
            }
            Err(e) => panic!("{}", e),
        }

        let path = dir.join("Validation");
        let path = path.to_str().unwrap();

        let mut args = UseTheoriesArgs::for_session(&session_id, &[path]);
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
            Ok(_ok) => (),
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
