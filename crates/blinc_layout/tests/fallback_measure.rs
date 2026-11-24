//! The measurer, which sizes every text box, must agree with the renderer
//! about text that is drawn through a fallback face.

#![cfg(feature = "text_measurer")]

use blinc_layout::div::GenericFont;
use blinc_layout::text_measure::{TextLayoutOptions, measure_text_with_options};
use blinc_text::TextRenderer;
use blinc_text::layout::LayoutOptions;
use blinc_text::registry::GenericFont as TextGeneric;

const SIZE: f32 = 16.0;

fn measured(generic: GenericFont, text: &str) -> f32 {
    let mut o = TextLayoutOptions::new();
    o.generic_font = generic;
    measure_text_with_options(text, SIZE, &o).width
}

fn drawn_x_of_last(r: &mut TextRenderer, g: TextGeneric, text: &str) -> Option<f32> {
    r.prepare_text_with_style(
        text,
        SIZE,
        [1.0; 4],
        &LayoutOptions::default(),
        None,
        g,
        400,
        false,
    )
    .ok()
    .and_then(|p| p.glyphs.last().map(|gl| gl.bounds[0]))
}

/// What one emoji adds to a measured run is what it adds to a drawn one.
/// Under a monospace primary the difference used to be 6px per emoji.
#[test]
fn a_measured_emoji_is_as_wide_as_a_drawn_one() {
    blinc_layout::text_measurer::init_text_measurer();
    let mut renderer = TextRenderer::new();

    for (generic, text_generic) in [
        (GenericFont::Monospace, TextGeneric::Monospace),
        (GenericFont::Serif, TextGeneric::Serif),
        (GenericFont::System, TextGeneric::System),
    ] {
        let (Some(base), Some(with)) = (
            drawn_x_of_last(&mut renderer, text_generic, "ab"),
            drawn_x_of_last(&mut renderer, text_generic, "a\u{1F600}b"),
        ) else {
            eprintln!("SKIP {generic:?}: renderer unavailable");
            continue;
        };
        let drawn = with - base;
        let measured_delta = measured(generic, "a\u{1F600}b") - measured(generic, "ab");
        assert!(
            (drawn - measured_delta).abs() < 0.5,
            "{generic:?}: an emoji draws {drawn:.2}px wide but measures {measured_delta:.2}px"
        );
    }
}

/// Text with nothing to substitute measures exactly as the plain layout
/// engine says, so the ASCII fast path changes nothing and costs nothing.
#[test]
fn ascii_measures_exactly_as_the_plain_engine_does() {
    use blinc_text::layout::TextLayoutEngine;
    use blinc_text::registry::FontRegistry;

    blinc_layout::text_measurer::init_text_measurer();
    let mut registry = FontRegistry::new();
    let Ok(primary) = registry.load_generic_with_style(TextGeneric::System, 400, false) else {
        return;
    };
    let engine = TextLayoutEngine::new();

    for text in ["The quick brown fox", "Hello, world", "0123456789"] {
        let plain = engine
            .layout(text, &primary, SIZE, &LayoutOptions::default())
            .width;
        let through_measurer = measured(GenericFont::System, text);
        assert!(
            (plain - through_measurer).abs() < 0.01,
            "{text:?}: engine {plain}, measurer {through_measurer}"
        );
    }
}

/// What a CJK character adds to a measured run is what it adds to a drawn one.
#[test]
fn a_measured_cjk_run_is_as_wide_as_a_drawn_one() {
    blinc_layout::text_measurer::init_text_measurer();
    let mut renderer = TextRenderer::new();
    let mut checked = 0;

    for (generic, text_generic) in [
        (GenericFont::Monospace, TextGeneric::Monospace),
        (GenericFont::Serif, TextGeneric::Serif),
        (GenericFont::System, TextGeneric::System),
    ] {
        let Some(base) = drawn_x_of_last(&mut renderer, text_generic, "ab") else {
            continue;
        };
        for text in ["a你b", "aこんにちはb", "a안녕b"] {
            let Some(with) = drawn_x_of_last(&mut renderer, text_generic, text) else {
                continue;
            };
            let drawn = with - base;
            let measured_delta = measured(generic, text) - measured(generic, "ab");
            assert!(
                (drawn - measured_delta).abs() < 0.5,
                "{generic:?} {text:?}: draws {drawn:.2}px wide but measures {measured_delta:.2}px"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "nothing was checked");
}

/// Text in a weight no text has used yet measures as wide as it draws. The
/// measurer used to find only faces already loaded, so a medium or bold
/// label was sized by the character estimate while the renderer, loading
/// the face, drew real glyphs: "Home" got a box narrower than itself and
/// wrapped, longer labels got boxes wider than themselves.
#[test]
fn a_weight_not_yet_loaded_measures_as_it_draws() {
    blinc_layout::text_measurer::init_text_measurer();
    let mut renderer = TextRenderer::new();
    for weight in [500u16, 600, 700] {
        for text in ["Home", "Products", "Services"] {
            // Measured first, as layout runs before the frame is drawn.
            let mut o = TextLayoutOptions::new();
            o.generic_font = GenericFont::System;
            o.font_weight = weight;
            let measured = measure_text_with_options(text, 14.0, &o).width;
            let Ok(drawn) = renderer.prepare_text_with_style(
                text,
                14.0,
                [1.0; 4],
                &LayoutOptions::default(),
                None,
                TextGeneric::System,
                weight,
                false,
            ) else {
                eprintln!("SKIP: renderer unavailable");
                return;
            };
            assert!(
                (measured - drawn.width).abs() < 0.5,
                "{text:?} at weight {weight} draws {:.1}px wide but measures {measured:.1}px",
                drawn.width
            );
        }
    }
}

/// A label given a weight after it was created reports the width it draws
/// at in that weight. The paint path compares that width with the label's
/// box to decide whether to wrap, so a width left over from the regular
/// weight wrapped light text that fitted, or failed to wrap bold text.
#[test]
fn a_label_reports_its_width_in_its_own_weight() {
    use blinc_layout::div::div;
    use blinc_layout::renderer::{ElementType, RenderTree};
    use blinc_layout::text::text;

    blinc_layout::text_measurer::init_text_measurer();
    let mut renderer = TextRenderer::new();
    let content = "Products and services";
    for (weight, label) in [
        (300u16, text(content).size(14.0).light()),
        (700, text(content).size(14.0).bold()),
    ] {
        let Ok(drawn) = renderer.prepare_text_with_style(
            content,
            14.0,
            [1.0; 4],
            &LayoutOptions::default(),
            None,
            TextGeneric::System,
            weight,
            false,
        ) else {
            eprintln!("SKIP: renderer unavailable");
            return;
        };
        let tree = RenderTree::from_element(&div().child(label));
        let node = tree.layout_tree.children(tree.root().unwrap())[0];
        let reported = match &tree.get_render_node(node).unwrap().element_type {
            ElementType::Text(t) => t.measured_width,
            _ => panic!("not a text node"),
        };
        assert!(
            (reported - drawn.width).abs() < 0.5,
            "at weight {weight} the label draws {:.1}px wide but reports {reported:.1}px",
            drawn.width
        );
    }
}
