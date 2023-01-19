mod checker;
mod convert;
mod lemma;

use crate::checker::ModelVerifier;
use clap::{command, ArgGroup, Parser};
use env_logger::Builder;
use log::{info, LevelFilter};
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
    let mut model = if cli.stdin {
        let mut stdin = io::stdin();
        let mut lines = String::new();
        stdin.read_to_string(&mut lines);
        lines
    } else if let Some(m) = cli.model {
        fs::read_to_string(m).unwrap()
    } else {
        panic!("No model")
    };
    model = model.trim().to_string();
    if model.starts_with("sat\n(") {
        model = model
            .strip_prefix("sat")
            .unwrap()
            .trim()
            .strip_prefix("(")
            .unwrap()
            .strip_suffix(")")
            .unwrap()
            .trim()
            .to_string()
    } else if model.starts_with("unsat") {
        exit(0);
    }

    let th_path = PathBuf::from_str(&cli.throot).unwrap();
    let spec_path = th_path.join("spec.json");
    let converter = convert::Converter::from_spec_file(&spec_path);

    let mut fm_str = String::new();
    BufReader::new(File::open(cli.smt).unwrap())
        .read_to_string(&mut fm_str)
        .expect("Failed to read formula");

    // Convert to smt 2.6
    fm_str = fm_str
        .replace("str.to.re", "str.to_re")
        .replace("str.in.re", "str.in_re");

    let iformulas = converter.convert_fm(fm_str.as_bytes());
    let n = iformulas.len();
    info!("📝 Converted SMT formula to Isabelle");

    let imodel = converter.convert_model(model.as_bytes());
    info!("📝 Converted SMT model to Isabelle");

    info!("💡 Checking model with Isabelle");
    let mut checker = checker::ClientVerifier::start_server(&cli.throot).unwrap();
    for (i, fm) in iformulas.iter().enumerate() {
        match checker.check_model(&fm, &imodel) {
            checker::CheckResult::OK => {
                info!("({}/{}) is valid", i, n);
            }
            checker::CheckResult::FailedUnknown => {
                info!("⚠️ ({}/{}) Unknown result", i, n);
                println!("unknown");
                exit(0)
            }
            checker::CheckResult::FailedInvalid => {
                info!("🚨 ({}/{}) Model is not valid", i, n);
                println!("unsat");
                exit(0);
            }
        }
    }
    println!("sat")
}

fn init_logger() {
    let mut builder = Builder::from_default_env();
    builder
        .format(|buf, record| writeln!(buf, "[{}] {}", record.level(), record.args()))
        .init();
}
