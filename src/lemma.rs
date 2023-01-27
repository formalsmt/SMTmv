use itertools::Itertools;

#[derive(Default, Clone, Debug)]
pub struct Lemma {
    name: String,
    premises: Vec<String>,
    conclusions: Vec<String>,
}

impl Lemma {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            ..Default::default()
        }
    }

    pub fn add_premise(&mut self, premise: &str) -> &mut Self {
        self.premises.push(premise.to_owned());
        self
    }

    pub fn add_premises(&mut self, premises: &[String]) -> &mut Self {
        self.premises.extend(premises.iter().cloned());
        self
    }

    pub fn add_conclusion(&mut self, conclusion: &str) -> &mut Self {
        self.conclusions.push(conclusion.to_owned());
        self
    }

    pub fn add_conclusions(&mut self, conclusions: &[String]) -> &mut Self {
        self.conclusions.extend(conclusions.iter().cloned());
        self
    }

    pub fn to_isabelle(self) -> String {
        let template = "
lemma ?name: assumes ?model shows \"?formula\"
    apply(simp add: assms)
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

        template
            .replace("?name", &self.name)
            .replace("?model", &premises)
            .replace("?formula", &conclusion)
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

#[allow(unstable_name_collisions)]
pub fn lemma_simp(formula: &str, model: &str, th_name: &str, imports: &[String]) -> String {
    let tmpl = "
theory ?th_name      
    imports ?imports
begin

lemma validation: assumes \"?model\" shows \"?formula\"
    apply(simp add: assms)
    done

end
    ";
    tmpl.replace("?model", model)
        .replace("?formula", formula)
        .replace("?th_name", th_name)
        .replace(
            "?imports",
            &imports
                .iter()
                .map(|t| format!("\"{}\"", t))
                .intersperse(" ".to_string())
                .collect::<String>(),
        )
        .trim()
        .to_string()
}

#[allow(unreachable_code, unused_variables)]
pub fn lemma_auto(formula: &str, model: &str, th_name: &str) -> String {
    unimplemented!();
    let tmpl = "
theory ?th_name      
    imports QF_S
begin

lemma validation: assumes asm:\"?model\" shows \"?formula\"
    apply(auto simp add: asm)
    done

end
    ";
    tmpl.replace("?model", model)
        .replace("?formula", formula)
        .replace("?th_name", th_name)
        .trim()
        .to_string()
}
