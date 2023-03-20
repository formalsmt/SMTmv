use std::fmt::{Display, Formatter};

/// Error type
#[derive(Debug)]
pub enum Error {
    /// The SMT-LIB function is not supported by the Isabelle SMT theories.
    Unsupported(String),
    /// Error while parsing the model.
    ParseError(smt2parser::Error),
    /// Error while checking the lemma in Isabelle.
    IsabelleError,
    /// Other error.
    Other(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unsupported(s) => write!(f, "Unsupported SMT-LIB function: {}", s),
            Error::ParseError(e) => write!(f, "Parse error: {}", e),
            Error::Other(s) => write!(f, "Error: {}", s),
            Error::IsabelleError => {
                write!(f, "Isabelle failed to check proof (see logs for details)")
            }
        }
    }
}
