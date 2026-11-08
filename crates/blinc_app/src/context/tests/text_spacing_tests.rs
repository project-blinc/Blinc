use super::main_text_clip_tests::context;
use super::*;

/// Letter spacing is drawn in device pixels, scaled with the font size. Left in points it drew a
/// glyph run narrower than it measured at 2x and 3x, and a caret measured in points fell inside
/// the last characters.
#[test]
#[ignore] // Requires GPU
fn letter_spacing_scales_with_the_font_at_device_density() {
    let mut c = context(true, 1);
    let ui = div()
        .w(200.0)
        .h(60.0)
        .child(text("hello w").size(13.0).letter_spacing(-0.35).no_wrap());
    let mut t = RenderTree::from_element(&ui);
    t.set_scale_factor(3.0);
    t.compute_layout(200.0, 60.0);
    let (texts, _, _) = c.collect_render_elements(&t);
    let run = texts
        .iter()
        .find(|t| t.content == "hello w")
        .expect("text run");
    assert_eq!(run.font_size, 39.0);
    assert!(
        (run.letter_spacing - (-1.05)).abs() < 1e-4,
        "letter spacing {} at 3x",
        run.letter_spacing
    );
}
