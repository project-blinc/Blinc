//! `cn.ScrollArea` — container wrapper: children block plus scalars.
use blinc_dsl_core::BlincDsl;
use blinc_layout::tree::LayoutTree;

/// cn widgets read theme tokens while building, so the theme has to
/// exist before the first one is constructed.
fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(blinc_theme::ThemeState::init_default);
}

fn node_count(src: &str, name: &str) -> usize {
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
    n
}

/// The body block becomes children.
#[test]
fn a_body_block_becomes_children() {
    assert!(
        node_count(
            r#"view { cn.ScrollArea { Text("a") Text("b") } }"#,
            "sa_children.blinc",
        ) >= 4,
        "root + the scroll shell + two Texts, plus whatever chrome the shell adds"
    );
}

/// Every string prop is accepted in its documented spelling.
#[test]
fn the_scalar_props_are_accepted() {
    assert!(
        node_count(
            r#"view { cn.ScrollArea(direction = "both", scrollbar = "hover", size = "large") { Text("a") } }"#,
            "sa_props.blinc",
        ) >= 3
    );
}

/// An unknown value falls back rather than failing.
#[test]
fn unknown_values_still_render() {
    assert!(
        node_count(
            r#"view { cn.ScrollArea(direction = "sideways", scrollbar = "maybe", size = "huge") { Text("a") } }"#,
            "sa_unknown.blinc",
        ) >= 3,
        "a typo costs the styling, not the content"
    );
}
