//! Braces are optional on a single-statement closure body, for both
//! arities.
//!
//! The one-parameter form already allowed it; the zero-arg form did
//! not, which is the shape almost every event handler uses.
use blinc_dsl_core::BlincDsl;
use blinc_layout::tree::LayoutTree;

fn count_of(src: &str, name: &str) -> usize {
    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_source(src, name).expect("compile");
    let widget = dsl.view_widget();
    let mut tree = LayoutTree::new();
    let root = widget.build(&mut tree);
    let (mut n, mut stack) = (0, vec![root]);
    while let Some(id) = stack.pop() {
        n += 1;
        stack.extend(tree.children(id));
    }
    n
}

/// `|| expr` — the handler shape, without braces.
#[test]
fn a_zero_arg_closure_needs_no_braces() {
    assert_eq!(
        count_of(
            r#"signal bl_a: i32 = 0
               view { Div(class="p") { Div(class="b", on_click = || bl_a.set(1)) } }"#,
            "bl_bare.blinc",
        ),
        3,
        "root + Div.p + Div.b"
    );
}

/// The braced form still parses as a block, not as a single statement
/// swallowed by the bare arm.
#[test]
fn the_braced_form_still_works() {
    assert_eq!(
        count_of(
            r#"signal bl_b: i32 = 0
               view { Div(class="p") { Div(class="b", on_click = || { bl_b.set(1) }) } }"#,
            "bl_braced.blinc",
        ),
        3,
    );
}

/// A braced body with several statements is unaffected — the bare arm
/// must not claim it.
#[test]
fn a_multi_statement_body_still_works() {
    assert_eq!(
        count_of(
            r#"signal bl_c: i32 = 0
               signal bl_d: i32 = 0
               view { Div(class="p") { Div(class="b", on_click = || { bl_c.set(1) bl_d.set(2) }) } }"#,
            "bl_multi.blinc",
        ),
        3,
    );
}
