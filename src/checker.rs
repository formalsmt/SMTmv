use crate::lemma;
use fs_extra::dir::CopyOptions;
use isabelle_client::commands::UseTheoryArgs;
use isabelle_client::{AsyncResult, IsabelleClient};
use log::error;
use std::env::temp_dir;
use std::os::unix::prelude::FileExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::{fs, io};

pub enum CheckResult {
    OK,
    FailedUnknown,
    FailedInvalid,
}

pub trait ModelVerifier {
    fn check_model(&mut self, formula: &str, model: &str) -> CheckResult;
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
        let mut isablle_cmd = Command::new("isabelle");

        isablle_cmd
            .arg("process")
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .current_dir(dir)
            .arg("-T")
            .arg("Validation")
            .arg("-d")
            .arg(theory_root);

        let child = isablle_cmd
            .spawn()
            .expect("Failed to start Isabelle process");

        let output = child.wait_with_output().expect("Failed to run process");

        let stderr = String::from_utf8(output.stderr).expect("Failed to decode stderr");
        let stdout = String::from_utf8(output.stdout).expect("Failed to decode stdout");

        if output.status.success() {
            Ok(CheckResult::OK)
        } else if stdout.contains("Failed to finish proof") {
            if stdout.contains("1. False") {
                // Heuristic
                Ok(CheckResult::FailedInvalid)
            } else {
                Ok(CheckResult::FailedUnknown)
            }
        } else {
            error!("{}", stdout);
            Err(stderr)
        }
    }
}

impl ModelVerifier for BatchVerifier {
    fn check_model(&mut self, formula: &str, model: &str) -> CheckResult {
        // Create temporary folder
        let dir = make_dir();

        // Copy Isabelle theory files
        let mut options = CopyOptions::new();
        options.content_only = true;
        options.depth = 1;
        options.overwrite = true;

        //if let Err(e) = fs_extra::dir::copy(&theory_root, &dir, &options) {
        //    panic!("{}", e);
        //}

        // Create new theory file with lemma
        let th = lemma::lemma_auto(formula, model, "Validation");
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
                panic!("Failed to check model for formula {}: {}", formula, e);
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
        let (port, pass) = isabelle_client::server::run_server(Some("vmv_server"))?;
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
        let mut args = isabelle_client::commands::SessionBuildStartArgs::session("HOL");
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
    fn check_model(&mut self, formula: &str, model: &str) -> CheckResult {
        // Create temporary folder
        log::debug!(
            "Checking {} ==> {}",
            model.replace('\n', ""),
            formula.replace('\n', "")
        );

        let session_id = self.session_id.clone();

        let th = lemma::lemma_simp(formula, model, "Validation", &["QF_S".to_owned()]);
        let dir = PathBuf::from_str(&self.temp_dir).unwrap();
        match fs::File::create(dir.join("Validation.thy")) {
            Ok(th_file) => {
                if let Err(e) = th_file.write_all_at(th.as_bytes(), 0) {
                    panic!("{}", e)
                }
            }
            Err(e) => panic!("{}", e),
        }

        let path = dir.join("Validation");
        let path = path.to_str().unwrap();

        let mut args: UseTheoryArgs = UseTheoryArgs::for_session(&session_id, &[path]);
        args.master_dir = Some(self.theory_root.clone());
        args.nodes_status_delay = Some(-1.0);
        args.check_limit = Some(1);
        args.unicode_symbols = Some(true);

        match self
            .runtime
            .block_on(self.client.use_theories(&args))
            .unwrap()
        {
            AsyncResult::Error(e) => {
                log::warn!("Error proving theory: {:?}", e);
                CheckResult::FailedUnknown
            }
            AsyncResult::Failed(f) => {
                log::info!("Proving theory failed: {:?}", f.message);
                CheckResult::FailedUnknown
            }
            AsyncResult::Finished(f) => {
                if f.ok {
                    CheckResult::OK
                } else {
                    log::info!("Proving theory unsuccessful: {:?}", f.errors);
                    CheckResult::FailedUnknown
                }
            }
        }
    }
}

fn make_dir() -> PathBuf {
    let temp_dir = temp_dir().join("isabelle_checker");
    if let std::io::Result::Err(e) = fs::create_dir_all(&temp_dir) {
        panic!("{}", e)
    }
    temp_dir
}
