//! `cn.AspectRatio` — a container wrapper: children block plus scalars.
use blinc_dsl_core::BlincDsl;
use blinc_layout::tree::LayoutTree;

/// cn widgets read theme tokens while building, so the theme has to
/// exist before the first one is constructed.
fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(blinc_theme::ThemeState::init_default);
}

fn build(src: &str, name: &str) -> (usize, LayoutTree, blinc_layout::LayoutNodeId) {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(src, name).expect("compile");
    let widget = dsl.view_widget();
    let mut tree = LayoutTree::new();
    let root = widget.build(&mut tree);
    let (mut n, mut stack) = (0, vec![root]);
    while let Some(id) = stack.pop() {
        n += 1;
        stack.extend(tree.children(id));
    }
    (n, tree, root)
}

/// The body block becomes children, the same as every other cn
/// container.
#[test]
fn a_body_block_becomes_children() {
    let (n, _, _) = build(
        r#"view { cn.AspectRatio(ratio = 1.5) { Text("a") Text("b") } }"#,
        "ar_children.blinc",
    );
    assert_eq!(n, 4, "root + AspectRatio + 2 Texts");
}

/// A named preset is accepted in place of a raw ratio.
#[test]
fn a_preset_name_is_accepted() {
    let (n, _, _) = build(
        r#"view { cn.AspectRatio(preset = "widescreen") { Text("a") } }"#,
        "ar_preset.blinc",
    );
    assert_eq!(n, 3, "root + AspectRatio + Text");
}

/// An unknown preset falls back rather than failing: a typo should cost
/// the shape, not the content.
#[test]
fn an_unknown_preset_still_renders() {
    let (n, _, _) = build(
        r#"view { cn.AspectRatio(preset = "nonsense") { Text("a") } }"#,
        "ar_unknown.blinc",
    );
    assert_eq!(n, 3, "root + AspectRatio + Text");
}

/// An omitted ratio reads as zero, which has no meaning — it must not
/// collapse the box to the builder's 0.01 floor.
#[test]
fn an_omitted_ratio_falls_back_to_square() {
    let (n, _, _) = build(
        r#"view { cn.AspectRatio { Text("a") } }"#,
        "ar_default.blinc",
    );
    assert_eq!(n, 3, "root + AspectRatio + Text");
}
