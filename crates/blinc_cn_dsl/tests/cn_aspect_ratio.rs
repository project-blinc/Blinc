//! `cn.AspectRatio` — container wrapper, geometry-checked.
//!
//! Assertions are on the laid-out box, not node counts: a wrapper can
//! report the right shape and still render nothing of the sort.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;

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

/// Width and height of the widget's own box.
fn box_size(src: &str, name: &str) -> (f32, f32) {
    let dsl = compiled(src, name);
    let host = div().w(600.0).h(600.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(600.0, 600.0);

    let root = tree.root().expect("root");
    let view_root = *tree.layout_tree.children(root).first().expect("view root");
    let shell = *tree
        .layout_tree
        .children(view_root)
        .first()
        .expect("the widget");
    let l = tree.layout_tree.get_layout(shell).expect("laid out");
    (l.size.width, l.size.height)
}

/// The point of the widget: a given width produces the height its ratio
/// implies, regardless of what the content would have measured.
#[test]
fn width_and_ratio_decide_the_height() {
    let (w, h) = box_size(
        r#"view { cn.AspectRatio(ratio = 2.0, w = 240.0) { cn.Skeleton(w = 20.0, h = 20.0) } }"#,
        "ar_ratio.blinc",
    );
    assert!((w - 240.0).abs() < 1.0, "width honoured, got {w}");
    assert!(
        (h - 120.0).abs() < 1.0,
        "height follows the 2:1 ratio, got {h} (content was 20 tall)"
    );
}

/// A named preset is the same statement spelled for a common case.
#[test]
fn a_preset_sets_the_same_shape() {
    let (w, h) = box_size(
        r#"view { cn.AspectRatio(preset = "square", w = 160.0) { cn.Skeleton(w = 10.0, h = 10.0) } }"#,
        "ar_preset.blinc",
    );
    assert!((w - h).abs() < 1.0, "square means equal sides, got {w}x{h}");
}

/// An unknown preset falls back to `ratio` rather than failing: a typo
/// should cost the shape, not the content.
#[test]
fn an_unknown_preset_falls_back_to_ratio() {
    let (w, h) = box_size(
        r#"view { cn.AspectRatio(preset = "nonsense", ratio = 2.0, w = 240.0) { cn.Skeleton(w = 10.0, h = 10.0) } }"#,
        "ar_unknown.blinc",
    );
    assert!(
        (h - 120.0).abs() < 1.0,
        "fell back to the 2:1 ratio, got {w}x{h}"
    );
}

/// An omitted ratio reads as zero, which has no meaning — it must not
/// collapse the box to the builder's 0.01 floor.
#[test]
fn an_omitted_ratio_is_square_not_a_sliver() {
    let (w, h) = box_size(
        r#"view { cn.AspectRatio(w = 120.0) { cn.Skeleton(w = 10.0, h = 10.0) } }"#,
        "ar_default.blinc",
    );
    assert!(
        (w - h).abs() < 1.0 && h > 1.0,
        "square, not a sliver, got {w}x{h}"
    );
}
