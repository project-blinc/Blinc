//! Rich text does not wrap — a known gap, pinned here.
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::rich_text;

fn measure(width: f32) -> (f32, f32) {
    let long = "Press <b>Enter</b> to accept, <i>Escape</i> to cancel, or read \
                the <a href='https://example.com'>manual</a> for the full list of \
                shortcuts and their meanings in every mode.";
    let host = div().w(width).h(400.0).child(rich_text(long));
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(width, 400.0);

    let root = tree.root().expect("root");
    let child = tree.layout_tree.children(root)[0];
    let l = tree.layout_tree.get_layout(child).expect("layout");
    (l.size.width, l.size.height)
}

/// A bounded container clamps the width but never gains a line.
///
/// `blinc_gpu::text::prepare_styled_text` hardcodes
/// `LineBreakMode::None` and passes no `max_width`, so `RichText`
/// ignores its own `wrap: true` — the plain-text path only disables
/// wrapping when `wrap` is false. Content wider than the container
/// overflows rather than breaking.
///
/// **When wrapping lands this test SHOULD fail.** Landing it alone is
/// not enough, though: `calculate_link_regions` measures prefix widths
/// as one continuous line, and both link hover and link clicking use
/// those x-ranges. Wrapping without per-line regions moves every link's
/// hit area to the wrong place. See
/// `gotcha_hover_cursor_six_gates` in memory.
#[test]
fn rich_text_does_not_wrap_yet() {
    let (wide_w, wide_h) = measure(1200.0);
    let (narrow_w, narrow_h) = measure(200.0);

    assert!(wide_w > 500.0, "the sentence is genuinely long: {wide_w}");
    assert_eq!(narrow_w, 200.0, "width clamps to the container");
    assert_eq!(
        narrow_h, wide_h,
        "but no second line is allocated — content overflows instead",
    );
}
