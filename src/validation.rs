use std::collections::HashSet;
use std::path::Path;

use crate::checker::LemmaChecker;
use crate::{checker, convert, lemma};

pub enum ValidationResult {
    Valid,
    Invalid,
    Unknown,
}

/// Validate model against formula
pub fn validate(
    smt_model: String,
    smt_formula: String,
    theory_path: &Path,
) -> Result<ValidationResult, String> {
    let smt_model = sanitize_model(&smt_model).unwrap();
    let spec_path = theory_path.join("spec.json");
    let mut converter = convert::Converter::from_spec_file(&spec_path);

    // Conjunction of assertions converted to Isabelle
    let formula = converter.convert(smt_formula)?;
    // Conjunction of equalities equivalent to the model, converted to Isabelle
    let model = converter.convert(smt_model)?;

    let undefined_vars: HashSet<String> = converter
        .get_vars_used()
        .difference(&converter.get_vars_defined())
        .cloned()
        .collect();
    if !undefined_vars.is_empty() {
        log::info!("Undefined variables: {:?}", undefined_vars);
        return Ok(ValidationResult::Invalid);
    }

    let mut lemma = lemma::Lemma::new("validation");
    lemma.add_conclusions(&formula);
    lemma.add_premises(&model);

    let mut checker = checker::BatchChecker::new(theory_path.to_str().unwrap());

    match checker.check(&lemma) {
        checker::CheckResult::OK => Ok(ValidationResult::Valid),
        checker::CheckResult::FailedUnknown => Ok(ValidationResult::Unknown),
        checker::CheckResult::FailedInvalid => Ok(ValidationResult::Invalid),
    }
}

/// Extract model from SMT output and sanitizes it.
/// If the models is not in valid SMT syntax, return None.
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
    #[allow(unused_imports)]
    use super::*;

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
