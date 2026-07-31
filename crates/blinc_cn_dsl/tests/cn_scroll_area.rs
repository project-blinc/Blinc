//! `cn.ScrollArea` — the first container wrapper with internal
//! structure.
//!
//! The assertions are on GEOMETRY, not node counts or tree depth. Both
//! of those passed against an earlier version that rendered the rows
//! sideways and dropped one, because they measured the `LayoutTree` the
//! wrapper touched rather than the `RenderTree` the app builds by
//! recursing through `children_builders()`.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(blinc_theme::ThemeState::init_default);
}

fn compiled(src: &str, name: &str) -> BlincDsl {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(src, name).expect("compile");
    dsl
}

/// Absolute origins of every text node, since `location` is
/// parent-relative.
fn text_origins(src: &str, name: &str) -> Vec<(f32, f32)> {
    let dsl = compiled(src, name);
    let host = div().w(400.0).h(200.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 200.0);

    let mut out = Vec::new();
    let mut stack = vec![(tree.root().expect("root"), 0.0f32, 0.0f32)];
    while let Some((id, px, py)) = stack.pop() {
        let (mut x, mut y) = (px, py);
        if let Some(l) = tree.layout_tree.get_layout(id) {
            x += l.location.x;
            y += l.location.y;
        }
        if let Some(node) = tree.get_render_node(id)
            && matches!(&node.element_type, ElementType::Text(_))
        {
            out.push((x, y));
        }
        for &c in tree.layout_tree.children(id).iter() {
            stack.push((c, x, y));
        }
    }
    out
}

/// Every row renders. An earlier version dropped all but the first,
/// because the scroll widget takes a single content child and the rest
/// became siblings of its scrollbar.
#[test]
fn every_row_renders() {
    let origins = text_origins(
        r#"view { cn.ScrollArea(h = 80.0, w = 240.0) { Text("one") Text("two") Text("three") } }"#,
        "sa_all.blinc",
    );
    assert_eq!(origins.len(), 3, "all three rows render: {origins:?}");
}

/// Rows stack. Same y meant they laid out in a row, which is what the
/// playground showed.
#[test]
fn rows_stack_vertically() {
    let mut origins = text_origins(
        r#"view { cn.ScrollArea(h = 80.0, w = 240.0) { Text("one") Text("two") } }"#,
        "sa_stack.blinc",
    );
    origins.sort_by(|a, b| a.1.total_cmp(&b.1));
    assert_ne!(
        origins[0].1, origins[1].1,
        "rows must differ in y: {origins:?}"
    );
    assert_eq!(
        origins[0].0, origins[1].0,
        "and share an x, since they stack rather than flow: {origins:?}"
    );
}

/// Content taller than the box still renders every row — the rows are
/// inside a scrolling viewport, not clipped away by it.
#[test]
fn content_taller_than_the_box_still_renders() {
    let origins = text_origins(
        r#"view { cn.ScrollArea(h = 40.0, w = 240.0) { Text("a") Text("b") Text("c") Text("d") } }"#,
        "sa_overflow.blinc",
    );
    assert_eq!(origins.len(), 4, "all four rows exist: {origins:?}");
}

/// Every string prop is accepted, and an unknown value falls back
/// rather than failing.
#[test]
fn scalar_props_and_their_fallbacks() {
    assert_eq!(
        text_origins(
            r#"view { cn.ScrollArea(direction = "both", scrollbar = "hover", size = "large", h = 60.0) { Text("a") } }"#,
            "sa_props.blinc",
        )
        .len(),
        1
    );
    assert_eq!(
        text_origins(
            r#"view { cn.ScrollArea(direction = "sideways", scrollbar = "maybe", size = "huge", h = 60.0) { Text("a") } }"#,
            "sa_unknown.blinc",
        )
        .len(),
        1,
        "a typo costs the styling, not the content"
    );
}

/// The viewport honours `h`. Content taller than it must not stretch
/// the box: if it does, nothing overflows and nothing scrolls.
#[test]
fn the_viewport_honours_its_height() {
    let dsl = compiled(
        r#"view { cn.ScrollArea(h = 80.0, w = 240.0) { Text("a") Text("b") Text("c") Text("d") Text("e") Text("f") } }"#,
        "sa_bounded.blinc",
    );
    let host = div().w(400.0).h(600.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 600.0);

    // Tallest node that is not the host or the view root: the scroll
    // area's own box.
    let root = tree.root().expect("root");
    let view_root = *tree.layout_tree.children(root).first().expect("view root");
    let shell = *tree
        .layout_tree
        .children(view_root)
        .first()
        .expect("scroll area");
    let h = tree
        .layout_tree
        .get_layout(shell)
        .expect("laid out")
        .size
        .height;
    assert!(
        (h - 80.0).abs() < 1.0,
        "the scroll box must stay at its declared height, got {h}"
    );
}
