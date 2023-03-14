mod checker;
mod convert;
mod lemma;

use crate::checker::ModelVerifier;
use clap::{command, ArgGroup, Parser};
use env_logger::Builder;
use log::info;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::io::{self, BufReader, Read};
use std::path::PathBuf;
use std::process::exit;
use std::str::FromStr;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[clap(group(ArgGroup::new("models").required(true).args(&["stdin", "model"])))]
struct Cli {
    smt: String,

    #[arg(long)]
    model: Option<String>,

    #[arg(long)]
    stdin: bool,

    #[arg(short = 'T')]
    throot: String,
}

fn main() {
    init_logger();
    let cli = Cli::parse();
    let raw_model = if cli.stdin {
        let mut stdin = io::stdin();
        let mut lines = String::new();
        stdin
            .read_to_string(&mut lines)
            .expect("Failed to read model from stdin");
        lines
    } else if let Some(m) = cli.model {
        fs::read_to_string(m).unwrap()
    } else {
        panic!("No model")
    };

    let model = if let Some(m) = sanitize_model(&raw_model) {
        log::debug!("Got model {}", m);
        m
    } else {
        log::debug!("unsat");
        exit(0);
    };

    let th_path = PathBuf::from_str(&cli.throot).unwrap();
    // Make absolute
    let th_path = fs::canonicalize(th_path).unwrap();

    let mut fm_str = String::new();
    BufReader::new(File::open(cli.smt).unwrap())
        .read_to_string(&mut fm_str)
        .expect("Failed to read formula");

    // Convert to smt 2.6
    fm_str = fm_str
        .replace("str.to.re", "str.to_re")
        .replace("str.in.re", "str.in_re");

    let spec_path = th_path.join("spec.json");
    let mut converter = convert::Converter::from_spec_file(&spec_path);
    let iformulas = match converter.convert(fm_str.as_bytes()) {
        Ok(f) => f,
        Err(e) => {
            println!("unknown: {}", e);
            exit(1);
        }
    };

    info!("📝 Converted SMT formula to Isabelle");

    let imodel = match converter.convert(model.as_bytes()) {
        Ok(f) => f,
        Err(e) => {
            println!("unknown: {}", e);
            exit(1);
        }
    };
    info!("📝 Converted SMT model to Isabelle");

    let undefined_vars: HashSet<String> = converter
        .get_vars_used()
        .difference(&converter.get_vars_defined())
        .cloned()
        .collect();
    if !undefined_vars.is_empty() {
        log::info!("Undefined variables: {:?}", undefined_vars);
        println!("unsat");
        exit(1);
    }

    let mut lemma = lemma::Lemma::new("validation");
    lemma.add_conclusions(&iformulas);
    lemma.add_premises(&imodel);

    info!("💡 Checking model with Isabelle");
    //let mut checker = checker::ClientVerifier::start_server(&cli.throot).unwrap();
    let mut checker = checker::BatchVerifier::new(th_path.to_str().unwrap());

    match checker.check_model(&lemma) {
        checker::CheckResult::OK => {
            println!("sat")
        }
        checker::CheckResult::FailedUnknown => {
            println!("unknown");
            exit(0)
        }
        checker::CheckResult::FailedInvalid => {
            println!("unsat");
            exit(0);
        }
    }
}

fn init_logger() {
    let mut builder = Builder::from_default_env();
    builder
        .format(|buf, record| writeln!(buf, "[{}] {}", record.level(), record.args()))
        .init();
}

fn sanitize_model(model: &str) -> Option<String> {
    let mut model = model.trim().to_owned();

    if model.starts_with("unsat") {
        return None;
    }

    // Unwrap model from 'sat(...)'
    if model.starts_with("sat\n(") {
        model = model
            .strip_prefix("sat")
            .unwrap()
            .trim()
            .strip_prefix('(')
            .unwrap()
            .strip_suffix(')')
            .unwrap()
            .trim()
            .to_owned()
    } else {
        return None;
    }

    // Remove additional 'model' prefix older z3 version produce
    if model.starts_with("model") {
        model = model.strip_prefix("model").unwrap().trim().to_owned();
    };
    Some(model)
}

mod tests {
    use super::sanitize_model;

    #[test]
    fn test_sanitize_model_unsat() {
        let model = "unsat".to_owned();
        assert_eq!(sanitize_model(&model), None);
    }

    #[test]
    fn test_sanitize_model_sat() {
        let model = "sat\n((define-fun x () Int 1))".to_owned();
        assert_eq!(
            sanitize_model(&model),
            Some("(define-fun x () Int 1)".to_owned())
        );
    }

    #[test]
    fn test_sanitize_model_sat_old_z3() {
        let model = "sat\n(model (define-fun x () Int 1))".to_owned();
        assert_eq!(
            sanitize_model(&model),
            Some("(define-fun x () Int 1)".to_owned())
        );
    }

    #[test]
    fn test_sanitize_model_empty_string() {
        let model = "".to_owned();
        assert_eq!(sanitize_model(&model), None);
    }
}
