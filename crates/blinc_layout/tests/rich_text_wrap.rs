//! Rich text wraps at its container, like plain text does.
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::rich_text;

const LONG: &str = "Press <b>Enter</b> to accept, <i>Escape</i> to cancel, or read \
                    the <a href='https://example.com'>manual</a> for the full list of \
                    shortcuts and their meanings in every mode.";

/// `items_start` matters: stretched to the cross axis the node reports the
/// container's height whatever the text does, which is how the earlier
/// version of this test passed without measuring anything.
fn measure(width: f32) -> (f32, f32) {
    let host = div()
        .w(width)
        .h(400.0)
        .flex_col()
        .items_start()
        .child(rich_text(LONG));
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(width, 400.0);

    let root = tree.root().expect("root");
    let child = tree.layout_tree.children(root)[0];
    let l = tree.layout_tree.get_layout(child).expect("layout");
    (l.size.width, l.size.height)
}

#[test]
fn a_bounded_container_gains_lines_instead_of_overflowing() {
    let (wide_w, wide_h) = measure(1200.0);
    let (narrow_w, narrow_h) = measure(200.0);

    assert!(wide_w > 500.0, "the sentence is genuinely long: {wide_w}");
    assert!(narrow_w <= 200.0, "and fits the container: {narrow_w}");
    assert!(
        narrow_h > wide_h,
        "the narrow container allocates more lines: {narrow_h} vs {wide_h}",
    );
}

/// Height should track the line count, not jump to some fixed multiple.
#[test]
fn height_grows_as_the_container_narrows() {
    let (_, at_600) = measure(600.0);
    let (_, at_300) = measure(300.0);
    let (_, at_150) = measure(150.0);

    assert!(
        at_150 > at_300 && at_300 > at_600,
        "monotonic: 150px={at_150}, 300px={at_300}, 600px={at_600}",
    );
}
