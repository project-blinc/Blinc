//! The playground's collision, as a test.
//!
//! `main.blinc` owned `page: string` for which tab the sidebar showed.
//! A pagination demo in `navigation.blinc` declared `signal page: i32`
//! for its page number. They were one signal, so clicking a page number
//! navigated the app — and nothing warned, because the second
//! declaration simply found the first and adopted it.
//!
//! Two earlier attempts at scoping shipped broken, and every test they
//! came with passed, because those tests asserted on REGISTRY CONTENTS.
//! Keys can be right while the render path still resolves a name under
//! whichever module compiled last. So the load-bearing assertions here
//! read the RENDERED TREE.
use blinc_dsl_core::BlincDsl;
use blinc_layout::renderer::{ElementType, RenderTree};
use blinc_layout::tree::LayoutTree;
use std::fs;

/// These declare bare names in one process-global registry. Run
/// concurrently they mint over each other, so they take turns.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The property channel needs a context; without one a bound widget
/// builds with no value and the tree comes out empty.
fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_core::BlincContextState::init(
            blinc_core::reactive::global_graph(),
            std::sync::Arc::new(std::sync::Mutex::new(
                blinc_core::context_state::HookState::new(),
            )),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
    });
}

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

/// Every string a text node in the tree carries.
fn rendered_text(dsl: &BlincDsl) -> Vec<String> {
    let tree = RenderTree::from_element(&dsl.view_widget());
    let mut out = Vec::new();
    let Some(root) = tree.root() else {
        return out;
    };
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id) {
            match &node.element_type {
                ElementType::Text(t) => out.push(t.content.clone()),
                ElementType::StyledText(t) => out.push(t.content.clone()),
                _ => {}
            }
        }
        stack.extend(tree.layout_tree.children(id));
    }
    out
}

/// The load-bearing one: an imported module READS its own `n`.
///
/// Both files declare `n: i32`, with values that fall on opposite sides
/// of the module's own branch. Control flow is deliberate: it forces a
/// real value read through the `__signal_get_by_id_*` choke point,
/// where `Text("{n}")` would only hand over a binding handle and prove
/// nothing about which signal was read.
///
/// If the name resolves to the entry's signal — which is what both
/// earlier attempts did at render time — the tree shows `BIG`.
#[test]
fn an_imported_module_reads_its_own_signal() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = project(
        "ownvalue",
        &[
            (
                "counter.blinc",
                r#"
signal n: i32 = 7
component Counter() {
    view {
        Div(class="c") {
            if n.get() > 50 { Text("BIG") } else { Text("SMALL") }
        }
    }
}"#,
            ),
            (
                "main.blinc",
                r#"
import { Counter } from "./counter"
signal n: i32 = 99
view { Counter() }"#,
            ),
        ],
    );

    init();
    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_project(&root.join("main.blinc"), &root)
        .expect("compile project");

    let texts = rendered_text(&dsl);
    assert!(
        texts.iter().any(|t| t == "SMALL"),
        "the module's branch must read the module's own `n` (7); got {texts:?}",
    );
    assert!(
        !texts.iter().any(|t| t == "BIG"),
        "the entry's `n` (99) leaked into the module's branch; got {texts:?}",
    );
}

/// The entry keeps the bare key, an imported module gets a qualified
/// one, and the types stay apart. Differing types are what make the
/// original bug detectable: a shared entry keeps whichever was minted
/// first, so the i32 reads were looking at a String signal.
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
signal page: string = "forms"
view { Inner() }"#,
            ),
        ],
    );

    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_project(&root.join("main.blinc"), &root)
        .expect("compile project");
    render(&dsl);

    // Every file is a module, the entry included.
    assert_eq!(
        blinc_runtime::signal::lookup_exact("main.page").map(|(_, ty)| ty),
        Some(blinc_runtime::signal::SignalType::String),
        "the entry's signal is keyed by the entry's own file",
    );
    assert_eq!(
        blinc_runtime::signal::lookup_exact("inner.page").map(|(_, ty)| ty),
        Some(blinc_runtime::signal::SignalType::I32),
        "the module's `page` is separate and keeps its OWN type — \
         a shared entry would have held String",
    );

    // And the bare name now refuses to answer, which is the guarantee:
    // silently picking one is what made a page click drive the sidebar.
    assert!(
        matches!(
            blinc_runtime::signal::resolve("page"),
            Err(blinc_runtime::signal::ResolveError::Ambiguous(_)),
        ),
        "two modules declare `page`, so the bare name must not resolve",
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
signal page: string = "forms"
view { Inner2() }"#,
            ),
        ],
    );

    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_project(&root.join("main.blinc"), &root)
        .expect("compile project");
    render(&dsl);

    blinc_runtime::signal::set_i32("inner2.page", 7);
    assert_eq!(
        blinc_runtime::signal::get_str("main.page").as_deref(),
        Some("forms"),
        "the sidebar stayed where it was",
    );
}

/// A module path is its directories AND its file name, so two files of
/// the same name in different folders stay apart.
#[test]
fn a_module_path_includes_its_directories() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join("blinc_sigscope_nested");
    let _ = fs::remove_dir_all(&root);
    for dir in ["left", "right"] {
        fs::create_dir_all(root.join(dir)).expect("create module dir");
        fs::write(
            root.join(dir).join("panel.blinc"),
            format!(
                r#"
signal slot: i32 = {}
component {}Panel() {{
    view {{ Div(class="p") {{ Text("p") }} }}
}}"#,
                if dir == "left" { 1 } else { 2 },
                if dir == "left" { "Left" } else { "Right" },
            ),
        )
        .expect("write module");
    }
    fs::write(
        root.join("main.blinc"),
        r#"
import { LeftPanel } from "./left/panel"
import { RightPanel } from "./right/panel"
view { Div(class="r") { LeftPanel() RightPanel() } }"#,
    )
    .expect("write entry");

    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_project(&root.join("main.blinc"), &root)
        .expect("compile project");
    render(&dsl);

    assert_eq!(
        blinc_runtime::signal::lookup_exact("left.panel.slot").map(|(_, ty)| ty),
        Some(blinc_runtime::signal::SignalType::I32),
        "directories are part of the module path",
    );
    assert!(
        blinc_runtime::signal::lookup_exact("right.panel.slot").is_some(),
        "same file name in another folder is another module",
    );
    assert_eq!(blinc_runtime::signal::get_i32("left.panel.slot"), Some(1));
    assert_eq!(blinc_runtime::signal::get_i32("right.panel.slot"), Some(2));
}

/// A host still reaches a module's signal by bare name when only one
/// module declares it. Qualification is about keeping distinct signals
/// distinct, not about hiding them.
#[test]
fn a_unique_module_signal_is_reachable_unqualified() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = project(
        "modexport",
        &[
            (
                "inner3.blinc",
                r#"
signal shared_count: i32 = 4
component Inner3() {
    view { Div(class="i") { Text("{shared_count}") } }
}"#,
            ),
            (
                "main.blinc",
                r#"
import { Inner3 } from "./inner3"
signal page: string = "forms"
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
        "a lone module signal resolves from its bare name",
    );
    assert_eq!(
        blinc_runtime::signal::lookup_exact("inner3.shared_count").map(|(_, ty)| ty),
        Some(blinc_runtime::signal::SignalType::I32),
        "and is stored under the qualified key",
    );
}
