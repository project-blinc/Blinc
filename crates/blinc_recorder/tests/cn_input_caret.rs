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

/// The drawn width, right edge and caret position of a cn input holding `text`.
fn input_with(text: &str) -> (f32, f32, f32) {
    let mut ui = Headless::new(480, 100).unwrap();
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Light,
    );
    blur_all_text_inputs();
    let data = text_input_data();
    {
        let mut d = data.lock().unwrap();
        d.value = text.to_string();
        d.cursor = text.chars().count();
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
    let (mut text_box, mut caret_x) = (None, None);
    let mut stack = vec![tree.root().unwrap()];
    while let Some(id) = stack.pop() {
        if let (Some(node), Some(b)) = (tree.get_render_node(id), tree.get_absolute_bounds(id)) {
            if matches!(&node.element_type, ElementType::Text(t) if t.content == text) {
                text_box = Some((b.width, b.x + b.width));
            }
            if b.width == 2.0
                && let Some(Transform::Affine2D(a)) = &node.props.transform
            {
                caret_x = Some(b.x + a.elements[4]);
            }
        }
        stack.extend(tree.layout_tree.children(id));
    }
    let (width, right) = text_box.expect("text run");
    (width, right, caret_x.expect("caret"))
}

/// A skin tone, a joined family and a flag are one emoji each. Laid out per code point the
/// modifier took a character of space and drew nothing, and a typed `👋🏾` left a gap before the
/// next word.
#[test]
fn an_emoji_sequence_takes_the_room_of_one_emoji() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let (single, _, _) = input_with("\u{4f60}\u{597d}\u{1F44B}hello");
    for sequence in [
        "\u{1F44B}\u{1F3FE}",
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
        "\u{1F1F3}\u{1F1EC}",
    ] {
        let text = format!("\u{4f60}\u{597d}{sequence}hello");
        let (width, right, caret) = input_with(&text);
        assert!(
            (width - single).abs() < 1.0,
            "{sequence:?} makes the text {width}px wide, one emoji makes it {single}px"
        );
        assert!(
            (caret - right).abs() < 1.0,
            "the caret is at {caret} and the drawn text ends at {right}"
        );
    }
}
