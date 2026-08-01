//! `cn.Accordion` — a section carries its own label, and it expands.
//!
//! Height across a fold, not structure: a wrapper can build and report
//! correctly while never moving.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use std::sync::{Arc, Mutex};

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        let s = blinc_animation::AnimationScheduler::new();
        blinc_animation::set_global_scheduler(s.handle());
        blinc_layout::render_state::set_global_scheduler(s.handle());
        Box::leak(Box::new(s));
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

fn compiled(src: &str, name: &str) -> BlincDsl {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(src, name).expect("compile");
    dsl
}

/// A laid-out tree, kept alive so its node ids stay meaningful.
fn laid_out(dsl: &BlincDsl) -> RenderTree {
    let host = div().w(400.0).h(600.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 600.0);
    tree
}

fn texts(tree: &RenderTree) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root().expect("root")];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
        {
            out.push(t.content.clone());
        }
        stack.extend(tree.layout_tree.children(id).iter().copied());
    }
    out
}

/// Height of the section headed by `label`: the highest node whose
/// subtree holds that header and none of `siblings`. Section height is
/// what separates an open section from a shut one — structure is
/// identical either way, since a fold animates from the open bounds and
/// so keeps its body mounted.
fn section_height(tree: &RenderTree, label: &str, siblings: &[&str]) -> f32 {
    let mut stack = vec![(tree.root().expect("root"), Vec::new())];
    let mut path_to_label = None;
    while let Some((id, path)) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
            && t.content == label
        {
            path_to_label = Some(path.clone());
            break;
        }
        let mut child_path = path.clone();
        child_path.push(id);
        for child in tree.layout_tree.children(id) {
            stack.push((child, child_path.clone()));
        }
    }
    let path = path_to_label.unwrap_or_else(|| panic!("no header named {label}"));
    // Root-first: the outermost node that has stopped sharing a subtree
    // with the other sections is this section.
    for id in path {
        let below = texts_under(tree, id);
        if below.iter().any(|t| t == label) && !below.iter().any(|t| siblings.contains(&t.as_str()))
        {
            return tree
                .layout_tree
                .get_layout(id)
                .expect("laid out")
                .size
                .height;
        }
    }
    panic!("no node isolates the {label} section")
}

fn texts_under(tree: &RenderTree, root: blinc_layout::LayoutNodeId) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
        {
            out.push(t.content.clone());
        }
        stack.extend(tree.layout_tree.children(id).iter().copied());
    }
    out
}

/// Every section renders its own header, and bodies are mounted even
/// when shut — a fold animates from the open bounds, so the content has
/// to exist to be measured.
#[test]
fn each_item_renders_its_label_and_body() {
    let dsl = compiled(
        r#"view {
             cn.Accordion {
               cn.AccordionItem(label = "Shipping") {
                 cn.Label("ships in two days")
               }
               cn.AccordionItem(label = "Returns") {
                 cn.Label("thirty day window")
               }
             }
           }"#,
        "acc_pairs.blinc",
    );
    let found = texts(&laid_out(&dsl));
    for want in [
        "Shipping",
        "Returns",
        "ships in two days",
        "thirty day window",
    ] {
        assert!(found.iter().any(|t| t == want), "{want} renders: {found:?}");
    }
}

/// The assertion the parallel-list version could not make: the label and
/// the body that expands under it are declared together, so an `open`
/// section is taller than its shut sibling in the SAME program.
#[test]
fn an_open_section_is_taller_than_a_shut_one() {
    let dsl = compiled(
        r#"view {
             cn.Accordion {
               cn.AccordionItem(label = "Shut") {
                 cn.Label("folded body")
               }
               cn.AccordionItem(label = "Open", open = true) {
                 cn.Label("expanded body")
               }
             }
           }"#,
        "acc_open.blinc",
    );
    let tree = laid_out(&dsl);
    let (shut, open) = (
        section_height(&tree, "Shut", &["Open"]),
        section_height(&tree, "Open", &["Shut"]),
    );
    assert!(
        open > shut,
        "the section marked open must be taller: shut={shut}, open={open}"
    );
}

/// A loose child has no header to sit under, so it is dropped rather
/// than rendered headless.
#[test]
fn a_child_that_is_not_an_item_is_dropped() {
    let dsl = compiled(
        r#"view {
             cn.Accordion {
               cn.AccordionItem(label = "Kept") { cn.Label("body") }
               cn.Label("loose")
             }
           }"#,
        "acc_loose.blinc",
    );
    let found = texts(&laid_out(&dsl));
    assert!(found.iter().any(|t| t == "Kept"), "the section renders");
    assert!(
        !found.iter().any(|t| t == "loose"),
        "the loose child is dropped: {found:?}"
    );
}

/// An item on its own has nothing to fold it, so it shows its content
/// rather than rendering as an empty header.
#[test]
fn an_item_outside_an_accordion_renders_unfolded() {
    let dsl = compiled(
        r#"view {
             cn.AccordionItem(label = "Standalone") {
               cn.Label("still visible")
             }
           }"#,
        "acc_standalone.blinc",
    );
    let found = texts(&laid_out(&dsl));
    assert!(
        found.iter().any(|t| t == "still visible"),
        "the body renders: {found:?}"
    );
}

/// A bound `open` drives the section from outside: writing the signal
/// folds it, which is the half a plain `bool` prop cannot do.
#[test]
fn a_bound_open_folds_the_section_when_the_signal_is_written() {
    let dsl = compiled(
        r#"signal shown: bool = true

           view {
             cn.Accordion {
               cn.AccordionItem(label = "Bound", open = shown) {
                 Div { Text("body under the bound section") }
               }
               cn.AccordionItem(label = "Other") {
                 Div { Text("something else") }
               }
             }
           }"#,
        "acc_bound.blinc",
    );

    let open = section_height(&laid_out(&dsl), "Bound", &["Other"]);
    dsl.set_signal_bool("shown", false);
    let shut = section_height(&laid_out(&dsl), "Bound", &["Other"]);

    assert!(
        shut < open,
        "writing the signal must fold the section: open={open}, then {shut}"
    );
}
