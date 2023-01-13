mod checker;
mod convert;

use clap::{command, ArgGroup, Parser};
use env_logger::{builder, Builder};
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

    let iformula = converter.convert_fm(BufReader::new(File::open(cli.smt).unwrap()));
    info!("📝 Converted SMT formula to Isabelle");

    let imodel = converter.convert_model(model.as_bytes());
    info!("📝 Converted SMT model to Isabelle");

    info!("💡 Checking model with Isabelle");
    let res = checker::check_model(&iformula, &imodel, &cli.throot);
    match res {
        checker::CheckResult::OK => {
            info!("✅ Model is valid");
            println!("sat")
        }
        checker::CheckResult::FailedUnknown => {
            info!("⚠️ Unknown result");
            println!("unknown")
        }
        checker::CheckResult::FailedInvalid => {
            info!("🚨 Model is not valid");
            println!("unsat")
        }
    }
}

fn init_logger() {
    let mut builder = Builder::from_default_env();
    builder
        .format(|buf, record| writeln!(buf, "[{}] {}", record.level(), record.args()))
        .filter(None, LevelFilter::Info)
        .init();
}
