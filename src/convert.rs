#![allow(unused_imports)]
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
};

use smt2parser::{
    concrete::{Command, Constant},
    concrete::{QualIdentifier, Term},
    visitors::{FunctionDec, Identifier},
    *,
}; // 0.8.0

use crate::error::Error;

/// The specification to map an SMT-LIB function to Isabelle/HOL using the Isabelle SMT theories.
#[derive(Serialize, Deserialize, Clone)]
struct Spec {
    mapsto: Option<String>,
    assoc: Option<String>,
    chainable: bool,
}

impl Spec {
    /// Returns true iff the SMT-LIB function is declared `left-assoc`.
    fn is_left_assoc(&self) -> bool {
        match &self.assoc {
            Some(a) => a == "left",
            None => false,
        }
    }

    /// Returns true iff the SMT-LIB function is declared `left-assoc`.
    fn is_right_assoc(&self) -> bool {
        match &self.assoc {
            Some(a) => a == "right",
            None => false,
        }
    }
}

/// The specification to map SMT-LIB functions to Isabelle/HOL using the Isabelle SMT theories.
#[derive(Serialize, Deserialize)]
struct SpecDef {
    version: String,
    #[serde(rename = "smt-lib-version")]
    smt_lib_version: String,
    specs: HashMap<String, HashMap<String, Spec>>,
}

impl SpecDef {
    /// Returns the name of the Isabelle/HOL function to use for the given SMT-LIB function.
    fn get_spec(&self, op: &str) -> Option<(String, Spec)> {
        for (th, specs) in self.specs.iter() {
            if let Some(spec) = specs.get(op) {
                return Some((th.clone(), spec.clone()));
            }
        }
        None
    }
}

/// A converter from SMT-LIB to Isabelle/HOL.
pub struct Converter {
    spec: SpecDef,
    vars_used: HashSet<String>,
    vars_defined: HashSet<String>,
}

impl Converter {
    /// Creates a new converter from the given specification.
    pub fn new(spec_json: String) -> Result<Self, Error> {
        let spec: SpecDef = match serde_json::from_str(&spec_json) {
            Ok(s) => s,
            Err(e) => return Err(Error::Other(format!("{}", e))),
        };
        Ok(Self {
            vars_used: HashSet::new(),
            vars_defined: HashSet::new(),
            spec,
        })
    }

    /// Creates a new converter from the given specification file.
    pub fn from_spec_file(spec_file: &PathBuf) -> Result<Self, Error> {
        let spec = match fs::read_to_string(spec_file) {
            Ok(b) => b,
            Err(e) => {
                return Err(Error::Other(format!(
                    "Error loading {:?}: {}",
                    spec_file.as_os_str(),
                    e
                )))
            }
        };
        Converter::new(spec)
    }

    /// Returns the names of the variables used in the converted SMT-LIB formula.
    pub fn get_vars_used(&self) -> HashSet<String> {
        self.vars_used.clone()
    }

    /// Returns the names of the variables defined in the converted SMT-LIB formula.
    pub fn get_vars_defined(&self) -> HashSet<String> {
        self.vars_defined.clone()
    }

    /// Converts the given SMT-LIB formula to Isabelle/HOL.
    /// The results is a list of Isabelle/HOL terms that in conjunction are equivalent to the input formula.
    pub fn convert(&mut self, input: String) -> Result<Vec<String>, Error> {
        // Convert from smt 2.5 to smt 2.6
        let input = input
            .replace("str.to.re", "str.to_re")
            .replace("str.in.re", "str.in_re");

        let stream = CommandStream::new(input.as_bytes(), concrete::SyntaxBuilder, None);
        let commands = match stream.collect::<Result<Vec<_>, _>>() {
            Ok(c) => c,
            Err(e) => return Err(Error::ParseError(e)),
        };

        let mut converted = vec![];
        for c in &commands {
            if let Some(conv) = match c {
                Command::Assert { term } => Some(self.convert_term(term)?),
                Command::DefineFun { sig, term } => Some(self.convert_fun_defines(sig, term)?),
                _ => None,
            } {
                converted.push(conv);
            }
        }
        Ok(converted)
    }

    /// Convert a function definition to an Isabelle/HOL term.
    #[allow(unstable_name_collisions)]
    fn convert_fun_defines(&mut self, decl: &FunctionDec, term: &Term) -> Result<String, Error> {
        self.vars_defined.insert(decl.name.to_string());
        Ok(format!("{} = {}", decl.name, self.convert_term(term)?))
    }

    /// Convert a term to an Isabelle/HOL term.
    #[allow(unused_variables)]
    fn convert_term(&mut self, t: &Term) -> Result<String, Error> {
        match t {
            Term::Constant(c) => self.convert_constant(c),
            Term::QualIdentifier(i) => self.convert_identifier(i),
            Term::Application {
                qual_identifier,
                arguments,
            } => self.convert_application(qual_identifier, arguments),
            Term::Let { var_bindings, term } => todo!(),
            Term::Forall { vars, term } => todo!(),
            Term::Exists { vars, term } => todo!(),
            Term::Match { term, cases } => todo!(),
            Term::Attributes { term, attributes } => todo!(),
        }
    }

    /// Convert a constant to an Isabelle/HOL term.
    fn convert_constant(&self, c: &Constant) -> Result<String, Error> {
        match c {
            Constant::Numeral(n) => Ok(format!("({}::int)", n)),
            Constant::Decimal(d) => Ok(format!("{}", d)),
            Constant::Hexadecimal(_) => todo!(),
            Constant::Binary(_) => todo!(),
            Constant::String(s) => {
                let s = unicode_unescape(s, true)?;
                let mut as_char_list = String::from("[");
                for (i, c) in s.chars().enumerate() {
                    if i < s.len() - 1 {
                        as_char_list.push_str(&format!("{},", u32::from(c)));
                    } else {
                        as_char_list.push_str(&format!("{}", u32::from(c)));
                    }
                }
                as_char_list.push(']');
                Ok(as_char_list)
            }
        }
    }

    /// Convert an identifier to an Isabelle/HOL identifier.
    fn convert_identifier(&mut self, identifier: &QualIdentifier) -> Result<String, Error> {
        let op = &self.identifier_name(identifier);
        match self.spec.get_spec(op) {
            Some(m) => match m.1.mapsto {
                Some(m) => Ok(m),
                None => Err(Error::Unsupported(op.to_string())),
            },
            None => {
                // Variables
                self.vars_used.insert(op.clone());
                Ok(op.clone())
            }
        }
    }

    /// Retrieve the name of an identifier.
    fn identifier_name(&self, identifier: &QualIdentifier) -> String {
        match identifier {
            QualIdentifier::Simple { identifier } | QualIdentifier::Sorted { identifier, .. } => {
                match identifier {
                    Identifier::Simple { symbol } => symbol.0.to_string(),
                    Identifier::Indexed { .. } => todo!(), // Not needed for Strings
                }
            }
        }
    }

    /// Unrolls an n-ary `left-assoc` application to a series of binary applications.
    fn unroll_assoc_left(&self, identifier: &QualIdentifier, args: &Vec<Term>) -> Term {
        if args.len() >= 2 {
            let mut term = Term::Application {
                qual_identifier: identifier.clone(),
                arguments: vec![args[0].clone(), args[1].clone()],
            };
            for arg in args.iter().skip(2) {
                term = Term::Application {
                    qual_identifier: identifier.clone(),
                    arguments: vec![term, arg.clone()],
                };
            }
            term
        } else {
            Term::Application {
                qual_identifier: identifier.clone(),
                arguments: args.clone(),
            }
        }
    }

    /// Unrolls an n-ary `right-assoc` application to a series of binary applications.
    #[allow(unused_variables)]
    fn unroll_assoc_right(&self, identifier: &QualIdentifier, args: &[Term]) -> Term {
        unimplemented!()
    }

    /// Convert a function application to an Isabelle/HOL term.
    fn convert_application(
        &mut self,
        identifier: &QualIdentifier,
        args: &Vec<Term>,
    ) -> Result<String, Error> {
        let op = &self.identifier_name(identifier);
        let spec = match self.spec.get_spec(op) {
            Some(m) => m.1,
            None => return Err(Error::Unsupported(op.to_string())),
        };

        if spec.is_left_assoc() && args.len() > 2 {
            self.convert_term(&self.unroll_assoc_left(identifier, args))
        } else if spec.is_right_assoc() && args.len() > 2 {
            self.convert_term(&self.unroll_assoc_right(identifier, args))
        } else {
            let name = match spec.mapsto {
                Some(n) => n,
                None => return Err(Error::Unsupported(op.to_string())),
            };
            let mut s = if args.len() <= 1 {
                format!("({} ", name)
            } else {
                format!("(({}) ", name)
            };
            for t in args {
                s += " ";
                s += &self.convert_term(t)?;
            }
            s += ")";
            Ok(s)
        }
    }
}

/// Unescape a string literal as specified in the SMT-LIB standard.
/// If `legacy` is true, additionally unescapes unicode escape sequences in SMT-LIB 2.5 syntax (`\xAB` with A, B hex chars).
fn unicode_unescape(s: &str, legacy: bool) -> Result<String, Error> {
    let mut res = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('u') => match chars.next() {
                    Some('{') => {
                        let mut code = String::new();
                        #[allow(clippy::while_let_on_iterator)]
                        while let Some(n) = chars.next() {
                            if n == '}' {
                                break;
                            }
                            code.push(n);
                        }
                        let code = u32::from_str_radix(&code, 16).unwrap();
                        res.push(char::from_u32(code).unwrap());
                    }
                    Some(c) => {
                        let mut code = String::new();
                        code.push(c);
                        for _ in 0..4 {
                            code.push(chars.next().unwrap());
                        }
                        let code = u32::from_str_radix(&code, 16).unwrap();
                        res.push(char::from_u32(code).unwrap());
                    }
                    None => return Err(Error::Other("Invalid escape sequence".to_string())),
                },
                Some('x') if legacy => {
                    let mut code = String::new();
                    for _ in 0..2 {
                        code.push(chars.next().unwrap());
                    }
                    let code = u32::from_str_radix(&code, 16).unwrap();
                    res.push(char::from_u32(code).unwrap());
                }
                Some(c) => {
                    res.push(c);
                }
                None => return Err(Error::Other("Invalid escape sequence".to_string())),
            }
        } else {
            res.push(c);
        }
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::unicode_unescape;

    #[test]
    fn basic_unescapes() {
        assert_eq!(
            unicode_unescape("hello\\u{21}", false).unwrap(),
            "hello!".to_owned()
        );
        assert_eq!(
            unicode_unescape("\\u{1f600}", false).unwrap(),
            "😀".to_owned()
        );
        assert_eq!(
            unicode_unescape("\\u1f600", false).unwrap(),
            "😀".to_owned()
        );
    }

    #[test]
    fn smt25_unescapes() {
        assert_eq!(
            unicode_unescape("hello\\x21", true).unwrap(),
            "hello!".to_owned()
        );
        assert_eq!(unicode_unescape("\\x65", true).unwrap(), "e".to_owned());
    }

    #[test]
    #[should_panic]
    fn invalid_escape_sequence1() {
        unicode_unescape("\\u{}", false);
    }

    #[test]
    #[should_panic]
    fn tooshort_escape_sequence() {
        unicode_unescape("\\u12", false);
    }

    #[test]
    #[should_panic]
    fn nonhex_escape_sequence() {
        unicode_unescape("\\u{12g}", false);
    }

    #[test]
    #[should_panic]
    fn smt25_invalid() {
        assert_eq!(unicode_unescape("\\xFG", true).unwrap(), "e".to_owned());
    }
}
