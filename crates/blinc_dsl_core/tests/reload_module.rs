//! Editing an IMPORTED module must reach the window, and must not hang.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::ElementBuilder;
use std::io::Write;

fn write(path: &std::path::Path, body: &str) {
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(body.as_bytes()).unwrap();
}

fn nodes(dsl: &BlincDsl) -> usize {
    fn walk(e: &dyn ElementBuilder) -> usize {
        1 + e
            .children_builders()
            .iter()
            .map(|c| walk(c.as_ref()))
            .sum::<usize>()
    }
    walk(dsl.view_widget().as_ref())
}

#[test]
fn editing_an_imported_module_changes_the_view() {
    let dir = std::env::temp_dir().join("blinc_reload_module");
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("main.blinc");
    let part = dir.join("part.blinc");

    write(
        &entry,
        r#"import { Part } from "./part"
           view { Div { Part() } }"#,
    );
    write(&part, r#"component Part { view { Div { } } }"#);

    let dsl = BlincDsl::new().unwrap();
    dsl.compile_project(&entry, &dir).expect("compile");
    let before = nodes(&dsl);

    // Only the module changes; the entry is untouched, which is the
    // case that silently did nothing.
    write(
        &part,
        r#"component Part { view { Div { Div { } Div { } Div { } } } }"#,
    );
    let dsl = BlincDsl::reload_project(&entry, &dir, |_| Ok(())).expect("reload");
    let after = nodes(&dsl);

    println!("RELOAD nodes {before} -> {after}");
    assert!(
        after > before,
        "an edit to an imported module must reach the view: {before} -> {after}"
    );
}

/// Several reloads in a row, with a render between each: the shape a
/// live editing session actually produces.
#[test]
fn repeated_reloads_keep_rendering() {
    let dir = std::env::temp_dir().join("blinc_reload_repeat");
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("main.blinc");
    let part = dir.join("part.blinc");
    write(
        &entry,
        r#"import { Part } from "./part"
           view { Div { Part() } }"#,
    );

    for i in 1..=4 {
        let kids = "Div { } ".repeat(i);
        write(
            &part,
            &format!("component Part {{ view {{ Div {{ {kids} }} }} }}"),
        );
        let dsl = if i == 1 {
            let d = BlincDsl::new().unwrap();
            d.compile_project(&entry, &dir).expect("compile");
            d
        } else {
            BlincDsl::reload_project(&entry, &dir, |_| Ok(())).expect("reload")
        };
        let n = nodes(&dsl);
        println!("RELOAD round {i}: {n} nodes");
    }
}

/// A broken module fails the reload rather than half-applying it.
#[test]
fn a_broken_module_fails_the_recompile() {
    let dir = std::env::temp_dir().join("blinc_reload_broken_mod");
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("main.blinc");
    let part = dir.join("part.blinc");
    write(
        &entry,
        r#"import { Part } from "./part"
           view { Div { Part() } }"#,
    );
    write(&part, r#"component Part { view { Div { } } }"#);
    let dsl = BlincDsl::new().unwrap();
    dsl.compile_project(&entry, &dir).expect("compile");

    write(&part, r#"component Part { view { Div { "#);
    let r = BlincDsl::reload_project(&entry, &dir, |_| Ok(()));
    assert!(
        r.is_err(),
        "a broken module must fail the reload, leaving the caller on the instance it has"
    );
    assert_eq!(nodes(&dsl), 3, "the running instance is untouched");
}

/// Does a FRESH instance see the edited module?
#[test]
fn a_fresh_instance_sees_the_edited_module() {
    let dir = std::env::temp_dir().join("blinc_reload_fresh");
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("main.blinc");
    let part = dir.join("part.blinc");
    write(
        &entry,
        r#"import { Part } from "./part"
           view { Div { Part() } }"#,
    );
    write(&part, r#"component Part { view { Div { } } }"#);
    let one = BlincDsl::new().unwrap();
    one.compile_project(&entry, &dir).expect("compile");
    let before = nodes(&one);

    write(
        &part,
        r#"component Part { view { Div { Div { } Div { } Div { } } } }"#,
    );
    let two = BlincDsl::new().unwrap();
    two.compile_project(&entry, &dir).expect("compile");
    let after = nodes(&two);
    println!("FRESH INSTANCE nodes {before} -> {after}");
}
