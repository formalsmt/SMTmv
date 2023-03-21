mod checker;
mod convert;
mod error;
mod lemma;
mod validation;

use clap::{command, ArgGroup, Parser};
use env_logger::Builder;

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
    /// Path to file containing the SMT formula
    smt: String,

    /// Path to file containing the model (must not be used with --stdin)
    #[arg(long)]
    model: Option<String>,

    /// Read model from stdin (must not be used with --model)
    #[arg(long)]
    stdin: bool,

    /// Path to the root of the theory directory
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
        log::error!("No model");
        exit(-1);
    };

    log::trace!("Received model: '{}'", raw_model);

    let th_path = PathBuf::from_str(&cli.throot).unwrap();
    // Make absolute
    let th_path = fs::canonicalize(th_path).unwrap();

    let mut fm_str = String::new();
    BufReader::new(File::open(cli.smt).unwrap())
        .read_to_string(&mut fm_str)
        .expect("Failed to read formula");

    log::info!("Starting validation");
    match validation::validate(raw_model, fm_str, &th_path) {
        Ok(validation::ValidationResult::Valid) => println!("valid"),
        Ok(validation::ValidationResult::Invalid) => println!("invalid"),
        Ok(validation::ValidationResult::Unknown) => println!("unknown"),
        Err(e) => {
            log::error!("Error: {}", e);
            exit(-1);
        }
    }
}

fn init_logger() {
    let mut builder = Builder::from_default_env();
    builder
        .format(|buf, record| writeln!(buf, "[{}] {}", record.level(), record.args()))
        .init();
}
