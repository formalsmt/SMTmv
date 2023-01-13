use serde::{Deserialize, Serialize};

use std::{collections::HashMap, fs, path::PathBuf};

use itertools::Itertools;
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
            Some(a) => *a == String::from("left"),
            None => false,
        }
    }

    fn is_right_assoc(&self) -> bool {
        match &self.assoc {
            Some(a) => *a == String::from("right"),
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
    fn map_op(&self, op: &str) -> Option<String> {
        self.get_mapping(op)?.1.mapsto.clone()
    }

    fn get_mapping(&self, op: &str) -> Option<(String, Spec)> {
        for (th, mappings) in self.specs.iter() {
            if let Some(spec) = mappings.get(op) {
                return Some((th.clone(), spec.clone()));
            }
        }
        return None;
    }

    fn get_mapping_th(&self, th: &str, op: &str) -> Option<Spec> {
        self.specs.get(th)?.get(op).cloned()
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
            Err(e) => panic!("{}", e),
        };
        Converter::new(spec)
    }

    pub fn convert_fm(&self, input: impl std::io::BufRead) -> String {
        let stream = CommandStream::new(input, concrete::SyntaxBuilder, None);
        let commands = stream.collect::<Result<Vec<_>, _>>().unwrap();
        let asserts: Vec<Term> = commands
            .iter()
            .filter_map(|c| match c {
                Command::Assert { term } => Some(term.clone()),
                _ => None,
            })
            .collect();
        self.convert_assertions(&asserts)
    }

    pub fn convert_model(self, input: impl std::io::BufRead) -> String {
        let stream = CommandStream::new(input, concrete::SyntaxBuilder, None);
        let commands = stream.collect::<Result<Vec<_>, _>>().unwrap();
        let defines: Vec<(FunctionDec, Term)> = commands
            .iter()
            .filter_map(|c| match c {
                Command::DefineFun { sig, term } => Some((sig.clone(), term.clone())),
                _ => None,
            })
            .collect();
        self.convert_fun_defines(&defines)
    }

    fn convert_fun_defines(&self, defs: &[(FunctionDec, Term)]) -> String {
        defs.iter()
            .map(|(decl, v)| format!("{} = {}", decl.name, self.convert_term(v)))
            .intersperse(" \n\\<and> ".to_string())
            .collect()
    }

    fn convert_assertions(&self, assertions: &[Term]) -> String {
        let mut res = "".to_string();
        let n = assertions.len();
        for (i, term) in assertions.iter().enumerate() {
            res += &self.convert_term(term);
            if i + 1 < n {
                res += " \n\\<and> "
            }
        }
        res
    }

    fn convert_term(&self, t: &Term) -> String {
        match t {
            Term::Constant(c) => self.convert_constant(c),
            Term::QualIdentifier(i) => self.identifier_name(i),
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

    fn convert_constant(&self, c: &Constant) -> String {
        match c {
            Constant::Numeral(n) => format!("{}", n),
            Constant::Decimal(d) => format!("{}", d),
            Constant::Hexadecimal(h) => todo!(),
            Constant::Binary(_) => todo!(),
            Constant::String(s) => format!("(of_list ''{}'')", s),
        }
    }

    fn identifier_name(&self, identifier: &QualIdentifier) -> String {
        match identifier {
            QualIdentifier::Simple { identifier } | QualIdentifier::Sorted { identifier, .. } => {
                match identifier {
                    Identifier::Simple { symbol } => format!("{}", symbol.0),
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
                println!("{}", arg);
                term = Term::Application {
                    qual_identifier: identifier.clone(),
                    arguments: vec![term, arg.clone()],
                };
            }
            println!("in: {}\nout: {}", args.len(), term);
            term
        } else {
            Term::Application {
                qual_identifier: identifier.clone(),
                arguments: args.clone(),
            }
        }
    }

    fn unroll_assoc_right(&self, identifier: &QualIdentifier, args: &Vec<Term>) -> Term {
        unimplemented!()
    }

    fn convert_application(&self, identifier: &QualIdentifier, args: &Vec<Term>) -> String {
        let op = &self.identifier_name(identifier);
        let spec = match self.spec.get_mapping(&op) {
            Some(m) => m.1,
            None => panic!("Unknown operation: {}", op),
        };

        if spec.is_left_assoc() && args.len() > 2 {
            self.convert_term(&self.unroll_assoc_left(identifier, args))
        } else if spec.is_right_assoc() && args.len() > 2 {
            self.convert_term(&self.unroll_assoc_right(identifier, args))
        } else {
            let name = match spec.mapsto {
                Some(n) => n,
                None => panic!("Unsupported operation: {}", op),
            };
            let mut s = if args.len() <= 1 {
                format!("({} ", name)
            } else {
                format!("(({}) ", name)
            };
            for t in args {
                s += " ";
                s += &self.convert_term(t);
            }
            s += ")";
            s
        }
    }
}
