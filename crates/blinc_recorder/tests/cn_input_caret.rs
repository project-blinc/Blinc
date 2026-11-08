#![cfg(feature = "headless")]
//! The caret of a cn input stands at the end of the text it follows. The cn stylesheet tracks
//! input text with a negative `letter-spacing`, and the caret is measured with the same spacing.

use blinc_cn::input;
use blinc_core::Transform;
use blinc_layout::div::div;
use blinc_layout::renderer::ElementType;
use blinc_layout::widgets::text_input::{blur_all_text_inputs, text_input_data};
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const TEXT: &str = "the quick brown fox jumps over the lazy dog";

#[test]
fn the_caret_stands_at_the_end_of_tracked_text() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(480, 100).unwrap();
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Light,
    );
    blur_all_text_inputs();
    let data = text_input_data();
    {
        let mut d = data.lock().unwrap();
        d.value = TEXT.to_string();
        d.cursor = TEXT.chars().count();
    }
    let bound = data.clone();
    ui.mount(move |_| {
        div()
            .w(480.0)
            .h(100.0)
            .items_center()
            .justify_center()
            .child(input(&bound).w(440.0))
    });
    ui.leave().settle();

    let tree = ui.tree();
    let (mut text_right, mut caret_x) = (None, None);
    let mut stack = vec![tree.root().unwrap()];
    while let Some(id) = stack.pop() {
        if let (Some(node), Some(b)) = (tree.get_render_node(id), tree.get_absolute_bounds(id)) {
            match &node.element_type {
                ElementType::Text(t) if t.content == TEXT => text_right = Some(b.x + b.width),
                _ => {}
            }
            // The caret is the 2px-wide box moved by its own translation.
            if b.width == 2.0
                && let Some(Transform::Affine2D(a)) = &node.props.transform
            {
                caret_x = Some(b.x + a.elements[4]);
            }
        }
        stack.extend(tree.layout_tree.children(id));
    }
    let (text_right, caret_x) = (text_right.expect("text run"), caret_x.expect("caret"));
    assert!(
        (caret_x - text_right).abs() < 1.0,
        "the caret is at {caret_x} and the drawn text ends at {text_right}"
    );
}
