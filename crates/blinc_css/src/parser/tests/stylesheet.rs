//! Stylesheet bookkeeping: the hover-participant index.

use crate::element_style::*;
use crate::parser::*;

#[test]
fn participates_in_hover_recognizes_simple_id_class_and_complex() {
    let css = r#"
        #button:hover { background: red; }
        .card:hover { opacity: 0.8; }
        #parent:hover > .child { color: blue; }
        #plain { background: blue; }
        .untouched { color: white; }
    "#;
    let sheet = Stylesheet::parse(css).unwrap();

    assert!(sheet.participates_in_hover("button"));
    assert!(sheet.participates_in_hover("card"));
    // `#parent:hover > .child` — the hover is on #parent, so
    // #parent is a participant; .child is not (its segment has
    // no :hover).
    assert!(sheet.participates_in_hover("parent"));
    assert!(!sheet.participates_in_hover("child"));
    assert!(!sheet.participates_in_hover("plain"));
    assert!(!sheet.participates_in_hover("untouched"));
}

#[test]
fn insert_with_state_updates_hover_participants() {
    let mut sheet = Stylesheet::new();
    assert!(!sheet.participates_in_hover("dynamic"));
    sheet.insert_with_state("dynamic", ElementState::Hover, ElementStyle::default());
    assert!(sheet.participates_in_hover("dynamic"));
    // A non-hover state insertion shouldn't register the
    // identifier as a hover participant.
    sheet.insert_with_state("active-only", ElementState::Active, ElementStyle::default());
    assert!(!sheet.participates_in_hover("active-only"));
}
