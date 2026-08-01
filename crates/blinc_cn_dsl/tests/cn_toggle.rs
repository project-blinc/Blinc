//! `cn.Toggle` — a button that stays down, bound to a signal.
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

fn laid_out(dsl: &BlincDsl) -> RenderTree {
    let host = div().w(400.0).h(200.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 200.0);
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

/// The whole surface in one program: labels render, variants and sizes
/// parse, and an icon-only toggle needs no label.
#[test]
fn the_toggle_surface_renders() {
    let dsl = compiled(
        r#"signal bold: bool = false
           signal italic: bool = true

           view {
             Div {
               cn.Toggle(pressed = bold, label = "Bold")
               cn.Toggle(pressed = italic, label = "Italic", variant = "outline",
                         size = "small")
               cn.Toggle(pressed = bold, icon = "bold", aria_label = "bold")
             }
           }"#,
        "toggle_surface.blinc",
    );
    let found = texts(&laid_out(&dsl));
    for want in ["Bold", "Italic"] {
        assert!(found.iter().any(|t| t == want), "{want} renders: {found:?}");
    }
}

/// `pressed` binds two ways: setting the signal moves the toggle, which
/// a literal-only prop could not express.
#[test]
fn setting_the_signal_moves_the_toggle() {
    let dsl = compiled(
        r#"signal bold: bool = false
           view { cn.Toggle(pressed = bold, label = "Bold") }"#,
        "toggle_bound.blinc",
    );

    let count_nodes = |tree: &RenderTree| {
        let mut n = 0;
        let mut stack = vec![tree.root().expect("root")];
        while let Some(id) = stack.pop() {
            n += 1;
            stack.extend(tree.layout_tree.children(id).iter().copied());
        }
        n
    };

    let before = laid_out(&dsl);
    let off_nodes = count_nodes(&before);
    let off_props = before
        .get_render_node(before.root().expect("root"))
        .map(|n| n.props.clone());
    assert!(off_props.is_some());

    dsl.set_signal_bool("bold", true);
    let after = laid_out(&dsl);

    // The label survives the flip: a rebuild that lost it would be a
    // different bug wearing the same clothes.
    assert!(
        texts(&after).iter().any(|t| t == "Bold"),
        "the toggle still reads Bold once pressed"
    );
    assert_eq!(
        count_nodes(&after),
        off_nodes,
        "and pressing changes appearance, not structure"
    );
}
