use std::collections::HashSet;

use itertools::Itertools;

#[derive(Default, Clone, Debug)]
pub struct Lemma {
    name: String,
    premises: Vec<String>,
    conclusions: Vec<String>,
    simps: HashSet<String>,
}

impl Lemma {
    pub fn new(name: &str) -> Self {
        // Here 'assms' uses the premises for substitutions and 'eval_nat_numeral' is required to implicitly convert natural numerals to (Suc (Suc ... 0))
        Self {
            name: name.to_owned(),
            //"eval_nat_numeral"
            simps: HashSet::from_iter(vec!["assms"].into_iter().map(str::to_string)),
            ..Default::default()
        }
    }

    pub fn add_premise(&mut self, premise: &str) -> &mut Self {
        self.premises.push(premise.to_owned());
        self
    }

    pub fn add_premises(&mut self, premises: &[String]) -> &mut Self {
        for p in premises {
            self.add_premise(p);
        }
        self
    }

    pub fn add_conclusion(&mut self, conclusion: &str) -> &mut Self {
        self.conclusions.push(conclusion.to_owned());
        self
    }

    pub fn add_conclusions(&mut self, conclusions: &[String]) -> &mut Self {
        for c in conclusions {
            self.add_conclusion(c);
        }
        self
    }

    pub fn to_isabelle(self) -> String {
        let template = "
lemma ?name: assumes ?model shows \"?formula\"
    apply(auto simp add: ?simps)
    done
";

        let premises = format!(
            "{}",
            self.premises
                .into_iter()
                .map(|p| format!("\"{}\"", p))
                .intersperse(" and ".to_string())
                .collect::<String>()
        );
        let conclusion: String = self
            .conclusions
            .into_iter()
            .intersperse(" \\<and> ".to_string())
            .collect();

        let simps: String = self
            .simps
            .iter()
            .cloned()
            .intersperse(" ".to_string())
            .collect();

        template
            .replace("?name", &self.name)
            .replace("?model", &premises)
            .replace("?formula", &conclusion)
            .replace("?simps", &simps)
    }

    fn split_conclusion(self) -> Vec<Lemma> {
        let mut builders = vec![];

        for (i, con) in self.conclusions.iter().enumerate() {
            let name = format!("{}_{}", self.name.clone(), i);
            let mut sl = Lemma::new(&name);
            sl.add_premises(&self.premises).add_conclusion(con);

            builders.push(sl);
        }
        builders
    }
}

#[derive(Default)]
pub struct Theory {
    name: String,
    imports: Vec<String>,
    split_lemmata: bool,
    lemmata: Vec<String>,
}

impl Theory {
    pub fn new(name: &str, split: bool) -> Self {
        Self {
            name: name.to_owned(),
            split_lemmata: split,
            ..Default::default()
        }
    }

    pub fn add_theory_import(&mut self, imports: &str) -> &mut Self {
        self.imports.push(imports.to_owned());
        self
    }

    pub fn add_lemma(&mut self, builder: Lemma) {
        if self.split_lemmata {
            for lem in builder.split_conclusion() {
                self.lemmata.push(lem.to_isabelle());
            }
        } else {
            self.lemmata.push(builder.to_isabelle())
        }
    }

    pub fn to_isabelle(&self) -> String {
        let mut theory = String::new();

        // Header
        theory += &format!("theory {}\n", self.name);
        // Imports
        theory += "\timports ";
        if self.imports.is_empty() {
            theory += " Main\n";
        } else {
            for i in &self.imports {
                theory += &i;
                theory += " ";
            }
            theory += "\n";
        }
        theory += "begin\n\n";

        // Lemmata
        for lemma in &self.lemmata {
            theory += &lemma;
            theory += "\n";
        }

        theory += "end\n";
        theory
    }
}
