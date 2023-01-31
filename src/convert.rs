use serde::{Deserialize, Serialize};

use std::{collections::HashMap, fs, path::PathBuf};

use smt2parser::{
    concrete::{Command, Constant},
    concrete::{QualIdentifier, Term},
    visitors::{FunctionDec, Identifier},
    *,
}; // 0.8.0

#[derive(Serialize, Deserialize, Clone)]
struct Spec {
    mapsto: Option<String>,
    assoc: Option<String>,
    chainable: bool,
}

impl Spec {
    fn is_left_assoc(&self) -> bool {
        match &self.assoc {
            Some(a) => a == "left",
            None => false,
        }
    }

    fn is_right_assoc(&self) -> bool {
        match &self.assoc {
            Some(a) => a == "right",
            None => false,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SpecDef {
    version: String,
    #[serde(rename = "smt-lib-version")]
    smt_lib_version: String,
    specs: HashMap<String, HashMap<String, Spec>>,
}

impl SpecDef {
    fn get_spec(&self, op: &str) -> Option<(String, Spec)> {
        for (th, specs) in self.specs.iter() {
            if let Some(spec) = specs.get(op) {
                return Some((th.clone(), spec.clone()));
            }
        }
        None
    }
}

pub struct Converter {
    spec: SpecDef,
}

impl Converter {
    pub fn new(spec_json: String) -> Self {
        let spec: SpecDef = match serde_json::from_str(&spec_json) {
            Ok(s) => s,
            Err(e) => panic!("{}", e),
        };
        Self { spec }
    }

    pub fn from_spec_file(spec_file: &PathBuf) -> Self {
        let spec = match fs::read_to_string(spec_file) {
            Ok(b) => b,
            Err(e) => panic!("Error loading {:?}: {}", spec_file.as_os_str(), e),
        };
        Converter::new(spec)
    }

    pub fn convert(&self, input: impl std::io::BufRead) -> Result<Vec<String>, String> {
        let stream = CommandStream::new(input, concrete::SyntaxBuilder, None);
        let commands = stream.collect::<Result<Vec<_>, _>>().unwrap();

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

    #[allow(unstable_name_collisions)]
    fn convert_fun_defines(&self, decl: &FunctionDec, term: &Term) -> Result<String, String> {
        Ok(format!("{} = {}", decl.name, self.convert_term(term)?))
    }

    #[allow(unused_variables)]
    fn convert_term(&self, t: &Term) -> Result<String, String> {
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

    fn convert_constant(&self, c: &Constant) -> Result<String, String> {
        let unsupported_chars = vec!['\\', '\'', '\"'];
        match c {
            Constant::Numeral(n) => Ok(format!("({}::int)", n)),
            Constant::Decimal(d) => Ok(format!("{}", d)),
            Constant::Hexadecimal(_) => todo!(),
            Constant::Binary(_) => todo!(),
            Constant::String(s) => {
                for bad in unsupported_chars {
                    if s.contains(bad) {
                        return Err(format!("Contains unsupported char {}: {}", bad, s));
                    }
                }

                Ok(format!("''{}''", s))
            }
        }
    }

    fn convert_identifier(&self, identifier: &QualIdentifier) -> Result<String, String> {
        let op = &self.identifier_name(identifier);
        match self.spec.get_spec(op) {
            Some(m) => match m.1.mapsto {
                Some(m) => Ok(m),
                None => Err(format!("Unsupported operation: {}", op)),
            },
            None => return Ok(op.clone()), // Variables
        }
    }

    fn identifier_name(&self, identifier: &QualIdentifier) -> String {
        match identifier {
            QualIdentifier::Simple { identifier } | QualIdentifier::Sorted { identifier, .. } => {
                match identifier {
                    Identifier::Simple { symbol } => symbol.0.to_string(),
                    Identifier::Indexed { .. } => todo!(),
                }
            }
        }
    }

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

    #[allow(unused_variables)]
    fn unroll_assoc_right(&self, identifier: &QualIdentifier, args: &[Term]) -> Term {
        unimplemented!()
    }

    fn convert_application(
        &self,
        identifier: &QualIdentifier,
        args: &Vec<Term>,
    ) -> Result<String, String> {
        let op = &self.identifier_name(identifier);
        let spec = match self.spec.get_spec(op) {
            Some(m) => m.1,
            None => return Err(format!("Unknown operation: {}", op)),
        };

        if spec.is_left_assoc() && args.len() > 2 {
            self.convert_term(&self.unroll_assoc_left(identifier, args))
        } else if spec.is_right_assoc() && args.len() > 2 {
            self.convert_term(&self.unroll_assoc_right(identifier, args))
        } else {
            let name = match spec.mapsto {
                Some(n) => n,
                None => return Err(format!("Unsupported operation: {}", op)),
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
