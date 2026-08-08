//! The playground's collision, as a test.
//!
//! `main.blinc` owned `page: string` for which tab the sidebar showed.
//! A pagination demo in `navigation.blinc` declared `signal page: i32`
//! for its page number. They were one signal, so clicking a page number
//! navigated the app — and nothing warned, because the second
//! declaration simply found the first and adopted it.
//!
//! The differing TYPES are what make this detectable at all: a shared
//! entry keeps whichever type was minted first, so the i32 reads would
//! be looking at a String signal.
use blinc_dsl_core::BlincDsl;
use blinc_layout::tree::LayoutTree;
use std::fs;

/// Both tests declare a bare `page`, which is one process-global key.
/// Run concurrently they mint over each other, so they take turns.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn project(dir: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("blinc_sigscope_{dir}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create project dir");
    for (name, src) in files {
        fs::write(root.join(name), src).expect("write module");
    }
    root
}

fn render(dsl: &BlincDsl) {
    let widget = dsl.view_widget();
    let mut tree = LayoutTree::new();
    let _ = widget.build(&mut tree);
}

/// The entry exports its `page`; an imported module declares its own.
/// Writing one must not move the other.
#[test]
fn an_imported_modules_page_is_not_the_entrys_page() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = project(
        "collide",
        &[
            (
                "inner.blinc",
                r#"
signal page: i32 = 1
component Inner() {
    view { Div(class="i") { Text("{page}") } }
}"#,
            ),
            (
                "main.blinc",
                r#"
import { Inner } from "./inner"
export signal page: string = "forms"
view { Inner() }"#,
            ),
        ],
    );

    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_project(&root.join("main.blinc"), &root)
        .expect("compile project");
    render(&dsl);

    // The entry's is exported, so it keeps its bare name and its type.
    assert_eq!(
        blinc_runtime::signal::lookup("page").map(|(_, ty)| ty),
        Some(blinc_runtime::signal::SignalType::String),
        "the exported entry signal keeps the bare name",
    );

    // The module's is its own, under a qualified key, with its own type.
    let inner = blinc_runtime::signal::lookup("inner$$page");
    assert!(
        inner.is_some(),
        "the imported module's `page` is a separate signal",
    );
    assert_eq!(
        inner.map(|(_, ty)| ty),
        Some(blinc_runtime::signal::SignalType::I32),
        "and keeps its OWN type — a shared entry would have held String",
    );
}

/// Writing the module's signal leaves the entry's alone. This is the
/// behaviour the playground lost: a page click wrote the sidebar.
#[test]
fn writing_the_modules_page_does_not_navigate_the_entry() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = project(
        "isolate",
        &[
            (
                "inner2.blinc",
                r#"
signal page: i32 = 1
component Inner2() {
    view { Div(class="i") { Text("{page}") } }
}"#,
            ),
            (
                "main.blinc",
                r#"
import { Inner2 } from "./inner2"
export signal page: string = "forms"
view { Inner2() }"#,
            ),
        ],
    );

    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_project(&root.join("main.blinc"), &root)
        .expect("compile project");
    render(&dsl);

    blinc_runtime::signal::set_i32("inner2$$page", 7);
    assert_eq!(
        blinc_runtime::signal::get_str("page").as_deref(),
        Some("forms"),
        "the sidebar stayed where it was",
    );
}

/// A MODULE can export its own signal, and then a host reaches it by
/// bare name — the recording side has to agree with the lookup side, or
/// a module's own export silently does nothing.
#[test]
fn a_module_can_export_its_own_signal() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = project(
        "modexport",
        &[
            (
                "inner3.blinc",
                r#"
export signal shared_count: i32 = 4
component Inner3() {
    view { Div(class="i") { Text("{shared_count}") } }
}"#,
            ),
            (
                "main.blinc",
                r#"
import { Inner3 } from "./inner3"
export signal page: string = "forms"
view { Inner3() }"#,
            ),
        ],
    );

    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_project(&root.join("main.blinc"), &root)
        .expect("compile project");
    render(&dsl);

    assert_eq!(
        blinc_runtime::signal::get_i32("shared_count"),
        Some(4),
        "a module's own export is reachable by bare name",
    );
    assert!(
        blinc_runtime::signal::lookup("inner3$$shared_count").is_none(),
        "and is NOT also minted under a qualified key",
    );
}
