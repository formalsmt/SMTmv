use itertools::Itertools;

pub struct Generator {}

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
