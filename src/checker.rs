use crate::error::Error;
use crate::lemma::{Lemma, Theory};
use isabelle_client::client::args::{PurgeTheoryArgs, UseTheoriesArgs};
use isabelle_client::client::{AsyncResult, IsabelleClient};
use isabelle_client::process;

use std::os::unix::prelude::FileExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{fs, io};

/// The result of a lemma checking
pub enum CheckResult {
    /// Proof checked successfully
    OK,
    /// Proof checking failed because of an unknown reason
    FailedUnknown,
    /// Proof checking failed because the proof is invalid (i.e. the lemma is false)
    FailedInvalid,
}

/// A trait for checking lemmas
pub trait LemmaChecker {
    /// Checks whether the given lemma is true
    fn check(&mut self, lemma: &Lemma) -> Result<CheckResult, Error>;
}

/// Checks a lemma using the Isabelle process in batch mode
pub struct BatchChecker {
    theory_root: String,
}

impl BatchChecker {
    pub fn new(theory_root: &str) -> Self {
        Self {
            theory_root: theory_root.to_string(),
        }
    }

    /// Runs Isabelle in batch mode and loads the theory containing the lemma to check.
    /// Returns the result based on the output of Isabelle.
    fn run_isabelle(&self, dir: &Path, theory_root: &str) -> Result<CheckResult, Error> {
        let mut options = process::OptionsBuilder::new();
        options
            .build_pide_reports(false)
            .pide_reports(false)
            .process_output_limit(1)
            .process_output_tail(1)
            .record_proofs(0)
            .parallel_proofs(0)
            .quick_and_dirty(true);

        let args = process::ProcessArgs {
            theories: vec!["Validation".to_owned()],
            session_dirs: vec![theory_root.to_owned()],
            logic: Some("smt".to_string()),
            options: options.into(),
        };

        log::info!("Checking lemma with Isabelle");
        let output = match tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(process::batch_process(&args, Some(&dir.to_owned())))
        {
            Ok(o) => o,
            Err(e) => {
                log::error!("Error running the Isabelle process:s {}", e.to_string());
                return Err(Error::IsabelleError);
            }
        };

        let stderr = String::from_utf8(output.stderr).expect("Failed to decode stderr");
        let stdout = String::from_utf8(output.stdout).expect("Failed to decode stdout");
        if output.status.success() {
            Ok(CheckResult::OK)
        } else if stdout.contains("Failed to finish proof") {
            log::debug!("Proof could not be finished: {}", stdout);
            if stdout.contains("1. False") {
                // Heuristic
                log::debug!("Lemma is invalid");
                Ok(CheckResult::FailedInvalid)
            } else {
                Ok(CheckResult::FailedUnknown)
            }
        } else {
            log::error!(
                "Isabelle process terminated with non-zero exit status\nSTDOUT:\n{}\n STDERR:\n{}",
                stdout,
                stderr
            );
            Err(Error::IsabelleError)
        }
    }
}

impl LemmaChecker for BatchChecker {
    fn check(&mut self, lemma: &Lemma) -> Result<CheckResult, Error> {
        // TODO: Check if that is still needed with the heap image
        // Create temporary folder
        let dir = make_dir();

        // Create new theory file with lemma
        let mut theory = Theory::new("Validation", false);
        theory.add_theory_import("smt.Strings");
        theory.add_theory_import("smt.Core");
        theory.add_lemma(lemma.clone());

        let th = theory.to_isabelle();

        match fs::File::create(dir.path().join("Validation.thy")) {
            Ok(th_file) => {
                if let Err(e) = th_file.write_all_at(th.as_bytes(), 0) {
                    panic!("{}", e)
                }
            }
            Err(e) => panic!("{}", e),
        }

        // Call isabelle
        self.run_isabelle(dir.path(), &self.theory_root)
    }
}

/// Verifies models using the Isabelle server.
/// When verifying multiple models, this is much faster than the batch verifier, because the servers keeps the image of the base theories loaded.
/// Uses the Isabelle server instance named 'smtmv_server' and creates it if it does not exist.
///
/// ## Warning
/// Currently, this should not be used because the server uses substantial amounts of memory that it does not seem to free after validating a model.
/// This causes the server to run out of memory after a few validation calls.
/// I don't know if this is a memory leak in the server or if its not properly used here.
///
/// Moreover, it currently does not check why a check failed.
/// It only returns either CheckResult::OK or CheckResult::FailedUnknown, but never CheckResult::FailedInvalid.
pub struct ClientChecker {
    /// The client for the Isabelle server
    client: IsabelleClient,
    /// The root directory of the Isabelle SMT theories
    theory_root: String,
    /// The session id on the server
    session_id: String,
    /// The runtime for the async client
    runtime: tokio::runtime::Runtime,
    /// The temporary directory for validation theory files
    temp_dir: String,
}

impl ClientChecker {
    /// Starts a new Isabelle server and connects to it.
    #[allow(unused)]
    pub fn start_server(theory_root: &str) -> io::Result<Self> {
        let server = isabelle_client::server::run_server(Some("smtmv_server"))?;
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

impl LemmaChecker for ClientChecker {
    fn check(&mut self, lemma: &Lemma) -> Result<CheckResult, Error> {
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
                // TODO: Check why, return FailedInvalid if possible
                log::warn!("Proving theory failed: {:?}", f.message);
                CheckResult::FailedUnknown
            }
            AsyncResult::Finished(f) => {
                if f.ok {
                    CheckResult::OK
                } else {
                    log::warn!("Could not check proof: {}", theory.to_isabelle());
                    // TODO: Check why, return FailedInvalid if possible
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

        Ok(result)
    }
}

fn make_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
    /*temp_dir().join("isabelle_checker");

    if let std::io::Result::Err(e) = fs::create_dir_all(&temp_dir) {
        panic!("{}", e)
    }
    temp_dir*/
}
