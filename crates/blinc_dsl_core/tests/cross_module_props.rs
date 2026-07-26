//! Prop values passed to a component imported from another `.blinc` module.
//!
//! `inject_imported_view_externs` synthesizes the imported view's extern
//! without a parameter list, so these tests pin the behaviour that arguments
//! still arrive: the component's real signature comes from its compiled
//! module, not from the synthesized declaration.
use blinc_dsl_core::BlincDsl;
use blinc_layout::tree::LayoutTree;
use std::fs;

fn project(dir: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("blinc_xmod_props_{dir}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create project dir");
    for (name, src) in files {
        fs::write(root.join(name), src).expect("write module");
    }
    root
}

/// Render once so view bodies execute; the assertions read signals the
/// component wrote while rendering.
fn render(dsl: &BlincDsl) {
    let widget = dsl.view_widget();
    let mut tree = LayoutTree::new();
    let _ = widget.build(&mut tree);
}

#[test]
fn single_prop_reaches_imported_component() {
    let root = project(
        "single",
        &[
            (
                "widgets.blinc",
                r#"
signal xmod_single: i32
component Echo(n: i32) {
    view { Div(class="e") { xmod_single.set(n) Text("x") } }
}"#,
            ),
            (
                "main.blinc",
                r#"
import { Echo } from "./widgets"
view { Echo(7) }"#,
            ),
        ],
    );
    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_project(&root.join("main.blinc"), &root)
        .expect("compile project");
    dsl.set_signal_i32("xmod_single", 0);
    render(&dsl);
    assert_eq!(dsl.get_signal_i32("xmod_single"), Some(7));
}

#[test]
fn multiple_props_keep_their_order() {
    let root = project(
        "multi",
        &[
            (
                "widgets.blinc",
                r#"
signal xmod_a: i32
signal xmod_b: i32
component Two(a: i32, b: i32) {
    view { Div(class="t") { xmod_a.set(a) xmod_b.set(b) Text("x") } }
}"#,
            ),
            (
                "main.blinc",
                r#"
import { Two } from "./widgets"
view { Two(3, 9) }"#,
            ),
        ],
    );
    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_project(&root.join("main.blinc"), &root)
        .expect("compile project");
    dsl.set_signal_i32("xmod_a", 0);
    dsl.set_signal_i32("xmod_b", 0);
    render(&dsl);
    assert_eq!(
        (dsl.get_signal_i32("xmod_a"), dsl.get_signal_i32("xmod_b")),
        (Some(3), Some(9)),
        "positional args must not be transposed across the module boundary"
    );
}

/// A widget body may mix side-effect statements with child widgets. The
/// side effect runs and the widget still becomes a child — the statement is
/// not pushed onto the child list, where its void result would abort codegen.
#[test]
fn side_effect_statements_in_a_widget_body() {
    let root = project(
        "sidefx",
        &[(
            "main.blinc",
            r#"
signal xmod_fx: i32
component C {
    view { Div(class="c") { xmod_fx.set(5) Text("x") } }
}
view { C() }"#,
        )],
    );
    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_project(&root.join("main.blinc"), &root)
        .expect("compile project");
    dsl.set_signal_i32("xmod_fx", 0);

    let widget = dsl.view_widget();
    let mut tree = LayoutTree::new();
    let root_node = widget.build(&mut tree);
    let mut count = 0;
    let mut stack = vec![root_node];
    while let Some(id) = stack.pop() {
        count += 1;
        stack.extend(tree.children(id));
    }

    assert_eq!(
        dsl.get_signal_i32("xmod_fx"),
        Some(5),
        "side effect must run"
    );
    assert_eq!(count, 3, "view root + root Div + the Text child");
}
