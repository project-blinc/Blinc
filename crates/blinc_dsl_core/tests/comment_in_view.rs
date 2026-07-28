//! A commented-out line inside a view body must parse.
use blinc_dsl_core::BlincDsl;

#[test]
fn a_comment_inside_a_view_body_parses() {
    let dsl = BlincDsl::new().unwrap();
    let r = dsl.compile_source(
        r#"
        component M {
            view {
                Div {
                    // Div { }
                    Div { }
                }
            }
        }
        view { M() }
        "#,
        "comment.blinc",
    );
    println!("COMMENT IN VIEW: {:?}", r.as_ref().err());
    assert!(r.is_ok(), "a `//` comment inside a view body must parse");
}

/// The exact shape that failed: a commented-out `cn.*` call, with a
/// space after the slashes and a string literal inside it.
#[test]
fn a_commented_out_widget_call_parses() {
    let dsl = BlincDsl::new().unwrap();
    let r = dsl.compile_source(
        r#"
        component M2 {
            view {
                Div {
                    // cn.Badge("Another")
                    Div { }
                }
            }
        }
        view { M2() }
        "#,
        "comment2.blinc",
    );
    println!("COMMENTED WIDGET: {:?}", r.as_ref().err());
    assert!(r.is_ok(), "a commented-out widget call must parse");
}
