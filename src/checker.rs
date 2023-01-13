use fs_extra::dir::CopyOptions;
use std::env::temp_dir;
use std::fs;
use std::os::unix::prelude::FileExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub enum CheckResult {
    OK,
    FailedUnknown,
    FailedInvalid,
}

fn lemma_simp(formula: &str, model: &str, th_name: &str) -> String {
    let tmpl = "
theory ?th_name      
    imports QF_S
begin

lemma validation: assumes asm:\"?model\" shows \"?formula\"
    apply(simp add: asm)
    done

end
    ";
    tmpl.replace("?model", model)
        .replace("?formula", formula)
        .replace("?th_name", th_name)
        .trim()
        .to_string()
}

fn make_dir() -> PathBuf {
    let tempdir = temp_dir().join("isabelle_checker");
    if let std::io::Result::Err(e) = fs::create_dir_all(&tempdir) {
        panic!("{}", e)
    }
    tempdir
}

fn run_isabelle(dir: &PathBuf) -> CheckResult {
    let mut ths = vec![];
    for entry in fs::read_dir(dir).unwrap().filter_map(|f| f.ok()) {
        let path = entry.path();
        let extension = path.extension().and_then(|f| f.to_str()).clone();
        if extension == Some("thy") {
            let fnm = entry.file_name().to_str().unwrap().to_string();
            let fnm = fnm.strip_suffix(".thy").unwrap().to_string();
            ths.push(fnm);
        }
    }

    let mut isablle_cmd = Command::new("isabelle");

    isablle_cmd
        .arg("process")
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .current_dir(dir);

    for th in ths {
        isablle_cmd.arg("-T").arg(th);
    }

    let child = isablle_cmd
        .spawn()
        .expect("Failed to start Isabelle process");

    let output = child.wait_with_output().expect("Failed to run process");

    //let stderr = String::from_utf8(exit_code.stderr).expect("Failed to decode stderr");
    let stdout = String::from_utf8(output.stdout).expect("Failed to decode stdout");

    if output.status.success() {
        CheckResult::OK
    } else {
        println!("No successful");
        if stdout.contains("Failed to finish proof") {
            if stdout.contains("1. False") {
                // Heuristic
                CheckResult::FailedInvalid
            } else {
                CheckResult::FailedUnknown
            }
        } else {
            panic!("{}", stdout);
        }
    }
}

pub fn check_model(formula: &str, model: &str, theory_root: &str) -> CheckResult {
    // Create temporary folder
    let dir = make_dir();

    // Copy Isabelle theory files
    let mut options = CopyOptions::new();
    options.content_only = true;
    options.depth = 1;
    options.overwrite = true;

    if let Err(e) = fs_extra::dir::copy(&theory_root, &dir, &options) {
        panic!("{}", e);
    }

    // Create new theory file with lemma
    let th = lemma_simp(formula, model, "Validation");
    match fs::File::create(dir.join("Validation.thy")) {
        Ok(th_file) => {
            if let Err(e) = th_file.write_all_at(th.as_bytes(), 0) {
                panic!("{}", e)
            }
        }
        Err(e) => panic!("{}", e),
    }

    // Call isabelle
    run_isabelle(&dir)
}
