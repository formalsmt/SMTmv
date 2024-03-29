use std::collections::HashSet;
use std::path::Path;

use crate::checker::LemmaChecker;
use crate::error::Error;
use crate::{checker, convert, lemma};

/// Result of a validation
pub enum ValidationResult {
    /// Model is valid
    Valid,
    /// Model is invalid
    Invalid,
    /// Unable to determine validity
    Unknown,
}

/// Validate model against formula.
/// Returns `ValidationResult::Valid` if the model is valid, `ValidationResult::Invalid` if the model is invalid, and `ValidationResult::Unknown` if the validity cannot be determined.
/// Returns `Err` if the model or formula is not in valid SMT syntax.
pub fn validate(
    smt_model: String,
    smt_formula: String,
    theory_path: &Path,
) -> Result<ValidationResult, Error> {
    let smt_model = sanitize_model(&smt_model);
    let spec_path = theory_path.join("spec.json");
    log::debug!("Loading spec from {}", spec_path.display());
    let mut converter = convert::Converter::from_spec_file(&spec_path)?;

    // Conjunction of assertions converted to Isabelle
    let formula = converter.convert(smt_formula)?;
    log::info!("Converted formula");
    // Conjunction of equalities equivalent to the model, converted to Isabelle
    let model = converter.convert(smt_model)?;
    log::info!("Converted model");

    let undefined_vars: HashSet<String> = converter
        .get_vars_used()
        .difference(&converter.get_vars_defined())
        .cloned()
        .collect();
    if !undefined_vars.is_empty() {
        log::info!("Model does not assign all variables: {:?}", undefined_vars);
        return Ok(ValidationResult::Invalid);
    }

    let mut lemma = lemma::Lemma::new("validation");
    lemma.add_conclusions(&formula);
    lemma.add_premises(&model);
    log::info!("Generated lemma");
    log::debug!("{}", lemma.to_isabelle());

    let mut checker = checker::BatchChecker::new(theory_path.to_str().unwrap());
    //let mut checker = checker::ClientChecker::start_server(theory_path.to_str().unwrap()).unwrap();

    match checker.check(&lemma)? {
        checker::CheckResult::OK => Ok(ValidationResult::Valid),
        checker::CheckResult::FailedUnknown => Ok(ValidationResult::Unknown),
        checker::CheckResult::FailedInvalid => Ok(ValidationResult::Invalid),
    }
}

/// Extract model from SMT output and sanitizes it.
/// If the models is not in valid SMT syntax, return None.
fn sanitize_model(model: &str) -> String {
    let mut model = model.trim().to_owned();
    if model.matches("sat").count() > 1 {
        log::warn!("Multiple 'sat' in model, did you provide two models?");
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
    }

    // Remove additional 'model' prefix older z3 version produce
    if model.starts_with("model") {
        model = model.strip_prefix("model").unwrap().trim().to_owned();
    };
    model
}

mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_sanitize_model_sat() {
        let model = "sat\n((define-fun x () Int 1))".to_owned();
        assert_eq!(sanitize_model(&model), "(define-fun x () Int 1)".to_owned());
    }

    #[test]
    fn test_sanitize_model_sat_old_z3() {
        let model = "sat\n(model (define-fun x () Int 1))".to_owned();
        assert_eq!(sanitize_model(&model), "(define-fun x () Int 1)".to_owned());
    }
}
