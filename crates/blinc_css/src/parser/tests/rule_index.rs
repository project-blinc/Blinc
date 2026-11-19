//! A stylesheet lists the rules that can match an element without looking at them all.

use super::super::*;

fn sheet() -> Stylesheet {
    Stylesheet::parse(
        "\
        .a .row { color: red; }\n\
        #main .x { color: red; }\n\
        .b { color: red; }\n\
        div { color: red; }\n\
        [data-state=open] { color: red; }\n\
        .row[data-state=open] .cell { color: red; }\n\
        * { color: red; }\n\
        :not(.z) { color: red; }\n\
        .cell:hover { color: red; }\n\
        .a #main { color: red; }\n\
        ",
    )
    .expect("css")
}

fn candidates(
    sheet: &Stylesheet,
    id: Option<&str>,
    classes: &[&str],
    ty: Option<&str>,
) -> Vec<usize> {
    sheet.candidate_rules(id, classes, ty)
}

#[test]
fn an_element_gets_the_rules_filed_under_what_it_has_and_the_rest() {
    let sheet = sheet();
    let selectors: Vec<String> = sheet
        .complex_rules()
        .iter()
        .map(|(s, _)| format!("{s:?}"))
        .collect();
    assert!(!selectors.is_empty());
    // A bare element: only what asks for nothing in particular.
    let bare = candidates(&sheet, None, &[], Some("span"));
    // A cell: the rule that ends in `.cell`, and the rules that ask for nothing.
    let cell = candidates(&sheet, None, &["cell"], Some("span"));
    assert!(cell.len() > bare.len());
    // An element with the attribute gets the attribute's rules.
    let open = candidates(&sheet, None, &["data-state=open"], Some("span"));
    assert!(open.len() > bare.len());
    // A type brings its rules.
    let div = candidates(&sheet, None, &[], Some("div"));
    assert!(div.len() > bare.len());
    // An id brings its rules.
    let main = candidates(&sheet, Some("main"), &[], Some("span"));
    assert!(main.len() > bare.len());
}

#[test]
fn candidates_come_in_the_order_the_rules_were_written() {
    let sheet = sheet();
    let all = candidates(
        &sheet,
        Some("main"),
        &["cell", "b", "row", "data-state=open"],
        Some("div"),
    );
    assert!(all.windows(2).all(|w| w[0] < w[1]), "{all:?}");
}

#[test]
fn a_merged_stylesheet_indexes_the_rules_it_gained() {
    let mut sheet = Stylesheet::parse(".a { color: red; }").expect("css");
    let before = sheet.candidate_rules(None, &["late"], Some("span")).len();
    sheet.merge(Stylesheet::parse(".late { color: blue; }").expect("css"));
    let after = sheet.candidate_rules(None, &["late"], Some("span")).len();
    assert_eq!(after, before + 1);
}
