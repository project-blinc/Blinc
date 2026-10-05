//! Layout, wrapping and drawing must agree about fallback glyphs.
//!
//! A character the primary face lacks, or an emoji, is drawn from another
//! face whose advance is not the primary's. Layout used to know only the
//! primary's, so a run measured narrower or wider than it drew, lines wrapped
//! in the wrong places, and a caret landed off the glyph it should follow. The
//! renderer corrected its own positions afterwards, by accumulating an offset,
//! which layout and measurement never saw.
//!
//! The renderer is the reference: where it puts the next glyph is where the
//! layout has to say it goes.

use blinc_text::layout::{LayoutOptions, TextLayoutEngine};
use blinc_text::registry::{FontRegistry, GenericFont};
use blinc_text::{FallbackFaces, TextRenderer};

const SIZE: f32 = 16.0;

/// Primaries whose missing-glyph advance differs from the fallback's. With a
/// face whose notdef happens to be exactly 1em, the two agree by luck and the
/// defect cannot be seen, which is why SF Pro alone hid it.
const PRIMARIES: [GenericFont; 3] = [
    GenericFont::System,
    GenericFont::Monospace,
    GenericFont::Serif,
];

/// Emoji and symbols that take a fallback, and one the primary usually has but
/// the renderer still treats as emoji.
const GLYPHS: [char; 5] = ['\u{1F600}', '\u{2764}', '\u{26A1}', '\u{2605}', '\u{2603}'];

fn drawn_x_of_last(r: &mut TextRenderer, g: GenericFont, text: &str) -> Option<f32> {
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

/// The width layout predicts for a run that contains a fallback glyph must be
/// the distance the renderer really moves the next glyph.
#[test]
fn layout_width_matches_where_the_renderer_puts_the_next_glyph() {
    let mut renderer = TextRenderer::new();
    let mut registry = FontRegistry::new();
    let engine = TextLayoutEngine::new();
    let opts = LayoutOptions::default();
    let mut checked = 0;

    for generic in PRIMARIES {
        let Ok(primary) = registry.load_generic_with_style(generic, 400, false) else {
            eprintln!("SKIP {generic:?}: no face");
            continue;
        };
        let Some(base_drawn) = drawn_x_of_last(&mut renderer, generic, "ab") else {
            continue;
        };
        let base_laid = engine.layout("ab", &primary, SIZE, &opts).width;

        for c in GLYPHS {
            let text = format!("a{c}b");
            let Some(drawn) = drawn_x_of_last(&mut renderer, generic, &text) else {
                continue;
            };
            let faces = FallbackFaces::resolve(&mut registry, &primary, &text);
            if faces.is_empty() {
                // No fallback face on this machine: nothing to compare.
                continue;
            }
            let laid = engine
                .layout_with_fallbacks(&text, &primary, SIZE, &opts, &faces)
                .width;

            let drawn_delta = drawn - base_drawn;
            let laid_delta = laid - base_laid;
            assert!(
                (drawn_delta - laid_delta).abs() < 0.5,
                "{generic:?} {c:?}: drawn {drawn_delta:.2}px, layout says {laid_delta:.2}px"
            );
            checked += 1;
        }
    }
    eprintln!("checked {checked} primary/glyph pairs");
    assert!(checked > 0, "nothing was checked: no fallback faces here");
}

/// Without fallback faces, layout is exactly what it was. Callers that never
/// opt in see no change.
#[test]
fn layout_without_fallbacks_is_unchanged() {
    let mut registry = FontRegistry::new();
    let Ok(primary) = registry.load_generic_with_style(GenericFont::Monospace, 400, false) else {
        return;
    };
    let engine = TextLayoutEngine::new();
    let opts = LayoutOptions::default();
    let text = "a\u{1F600}b";

    let plain = engine.layout(text, &primary, SIZE, &opts).width;
    let none = engine
        .layout_with_fallbacks(text, &primary, SIZE, &opts, &FallbackFaces::none())
        .width;
    assert_eq!(plain, none);
}

/// Wrapping uses the substituted advances. Six emoji at 16px, in a 40px
/// column, break two per line; under the monospace primary's own narrower
/// advance they would pack four per line and the box would be too short.
#[test]
fn a_run_of_emoji_wraps_by_the_advance_it_is_drawn_with() {
    let mut registry = FontRegistry::new();
    let Ok(primary) = registry.load_generic_with_style(GenericFont::Monospace, 400, false) else {
        return;
    };
    // Distinct emoji: identical ones are checked separately below.
    let text: String = [
        '\u{1F600}',
        '\u{1F601}',
        '\u{1F602}',
        '\u{1F603}',
        '\u{1F604}',
        '\u{1F605}',
    ]
    .iter()
    .collect();
    let faces = FallbackFaces::resolve(&mut registry, &primary, &text);
    if faces.is_empty() {
        eprintln!("SKIP: no emoji face");
        return;
    }

    let engine = TextLayoutEngine::new();
    let opts = LayoutOptions {
        max_width: Some(40.0),
        line_break: blinc_text::LineBreakMode::Character,
        ..LayoutOptions::default()
    };
    let with = engine.layout_with_fallbacks(&text, &primary, SIZE, &opts, &faces);
    let without = engine.layout(&text, &primary, SIZE, &opts);

    eprintln!(
        "lines: {} with fallback, {} without",
        with.lines.len(),
        without.lines.len()
    );
    assert!(
        with.lines.len() > without.lines.len(),
        "the wider emoji should need more lines than the primary's advance gives"
    );
    for line in &with.lines {
        assert!(
            line.width <= 40.0 + 0.01,
            "a line overflowed: {}",
            line.width
        );
    }
}

/// A repeated emoji is drawn every time it appears. The skip rule that
/// catches the shaper's second report of one cluster must not eat characters
/// the author typed twice.
#[test]
fn repeated_emoji_are_each_drawn() {
    let mut renderer = TextRenderer::new();
    let opts = LayoutOptions::default();
    let count = |r: &mut TextRenderer, t: &str| {
        r.prepare_text_with_style(
            t,
            SIZE,
            [1.0; 4],
            &opts,
            None,
            GenericFont::System,
            400,
            false,
        )
        .map(|p| p.glyphs.len())
        .ok()
    };

    let Some(one) = count(&mut renderer, "a\u{1F600}b") else {
        return;
    };
    let two = count(&mut renderer, "a\u{1F600}\u{1F600}b").unwrap();
    let three = count(&mut renderer, "a\u{1F600}\u{1F600}\u{1F600}b").unwrap();
    assert_eq!(two, one + 1, "the second smiley was not drawn");
    assert_eq!(three, one + 2, "the third smiley was not drawn");
}

/// The thing the skip rule is for still works: a symbol with its variation
/// selector is one cluster and draws once.
#[test]
fn a_symbol_with_its_variation_selector_is_still_one_glyph() {
    let mut renderer = TextRenderer::new();
    let opts = LayoutOptions::default();
    let count = |r: &mut TextRenderer, t: &str| {
        r.prepare_text_with_style(
            t,
            SIZE,
            [1.0; 4],
            &opts,
            None,
            GenericFont::System,
            400,
            false,
        )
        .map(|p| p.glyphs.len())
        .ok()
    };
    let Some(plain) = count(&mut renderer, "a\u{2600}b") else {
        return;
    };
    let with_selector = count(&mut renderer, "a\u{2600}\u{FE0F}b").unwrap();
    assert_eq!(
        with_selector, plain,
        "the variation selector drew a glyph of its own"
    );
}

const CJK_TEXT: &str = "你好 こんにちは 안녕";

/// Called when this machine has no CJK face. Those tests have nothing to check
/// then and skip; CI sets `BLINC_REQUIRE_CJK` so a missing face fails there
/// instead of passing vacuously.
fn skip_without_cjk(what: &str) {
    assert!(
        std::env::var_os("BLINC_REQUIRE_CJK").is_none(),
        "{what}: no CJK face found, but BLINC_REQUIRE_CJK is set"
    );
    eprintln!("SKIP {what}: no CJK face on this machine");
}

/// Resolve the CJK faces for `CJK_TEXT`, or say the machine has none.
fn cjk_faces(registry: &mut FontRegistry, primary: &blinc_text::FontFace) -> Option<FallbackFaces> {
    let faces = FallbackFaces::resolve(registry, primary, CJK_TEXT);
    if faces.cjk_len() == 0 {
        skip_without_cjk("cjk_faces");
        return None;
    }
    Some(faces)
}

/// Han, kana and Hangul are all drawn from a CJK face and none is left as the
/// primary's missing-glyph box.
#[test]
fn cjk_text_takes_a_cjk_face_for_every_visible_glyph() {
    let mut registry = FontRegistry::new();
    let engine = TextLayoutEngine::new();
    let Ok(primary) = registry.load_generic_with_style(GenericFont::System, 400, false) else {
        return;
    };
    let Some(faces) = cjk_faces(&mut registry, &primary) else {
        return;
    };
    let layout =
        engine.layout_with_fallbacks(CJK_TEXT, &primary, SIZE, &LayoutOptions::default(), &faces);
    for glyph in layout.lines.iter().flat_map(|l| &l.glyphs) {
        if glyph.codepoint.is_whitespace() {
            continue;
        }
        assert!(
            matches!(glyph.face, blinc_text::FaceChoice::Cjk(_)),
            "{:?} was laid out with {:?}",
            glyph.codepoint,
            glyph.face
        );
        assert_ne!(
            glyph.glyph_id, 0,
            "{:?} is a missing-glyph box",
            glyph.codepoint
        );
    }
}

/// The width layout predicts for CJK text is where the renderer puts the next
/// glyph, under every primary.
#[test]
fn layout_width_matches_the_renderer_for_cjk() {
    let mut renderer = TextRenderer::new();
    let mut registry = FontRegistry::new();
    let engine = TextLayoutEngine::new();
    let opts = LayoutOptions::default();
    let mut checked = 0;

    for generic in PRIMARIES {
        let Ok(primary) = registry.load_generic_with_style(generic, 400, false) else {
            continue;
        };
        let Some(base_drawn) = drawn_x_of_last(&mut renderer, generic, "ab") else {
            continue;
        };
        let base_laid = engine.layout("ab", &primary, SIZE, &opts).width;

        for text in ["a你b", "aこb", "a안b", "a你好こんにちは안녕b"] {
            let Some(drawn) = drawn_x_of_last(&mut renderer, generic, text) else {
                continue;
            };
            let faces = FallbackFaces::resolve(&mut registry, &primary, text);
            if faces.cjk_len() == 0 {
                continue;
            }
            let laid = engine
                .layout_with_fallbacks(text, &primary, SIZE, &opts, &faces)
                .width;
            let drawn_delta = drawn - base_drawn;
            let laid_delta = laid - base_laid;
            assert!(
                (drawn_delta - laid_delta).abs() < 0.5,
                "{generic:?} {text:?}: drawn {drawn_delta:.2}px, layout says {laid_delta:.2}px"
            );
            checked += 1;
        }
    }
    eprintln!("checked {checked} CJK runs");
    if checked == 0 {
        skip_without_cjk("layout_width_matches_the_renderer_for_cjk");
    }
}

/// A face is only loaded for what the text needs: Latin text pulls in none, and
/// the lookup is by character so Hangul alone does not need the Han face first.
#[test]
fn no_cjk_face_is_loaded_for_text_that_has_no_cjk() {
    let mut registry = FontRegistry::new();
    let Ok(primary) = registry.load_generic_with_style(GenericFont::System, 400, false) else {
        return;
    };
    for text in ["hello", "héllo wörld", "a\u{1F600}b", "a\u{2605}b"] {
        let faces = FallbackFaces::resolve(&mut registry, &primary, text);
        assert!(faces.cjk_len() == 0, "{text:?} loaded a CJK face");
    }
}

/// No single CJK face covers every script, and the faces tried first need not
/// cover what a later one does. A character the first face lacks is drawn from
/// whichever resolved face has it.
#[test]
fn a_character_the_first_cjk_face_lacks_takes_a_later_one() {
    let mut registry = FontRegistry::new();
    let engine = TextLayoutEngine::new();
    let Ok(primary) = registry.load_generic_with_style(GenericFont::System, 400, false) else {
        return;
    };
    // Simplified-only Han, then Hangul: kana-first or Japanese faces lack the
    // first, Chinese faces the second.
    let text = "这长标 안녕";
    let faces = FallbackFaces::resolve(&mut registry, &primary, text);
    if faces.cjk_len() == 0 {
        skip_without_cjk("a_character_the_first_cjk_face_lacks_takes_a_later_one");
        return;
    }
    let layout =
        engine.layout_with_fallbacks(text, &primary, SIZE, &LayoutOptions::default(), &faces);
    let mut checked = 0;
    for glyph in layout.lines.iter().flat_map(|l| &l.glyphs) {
        let c = glyph.codepoint;
        if c.is_whitespace() {
            continue;
        }
        let drawable = (0..faces.cjk_len() as u8).any(|i| {
            faces
                .face(blinc_text::FaceChoice::Cjk(i))
                .is_some_and(|f| f.glyph_id(c).is_some_and(|g| g != 0))
        });
        if drawable {
            assert!(
                matches!(glyph.face, blinc_text::FaceChoice::Cjk(_)) && glyph.glyph_id != 0,
                "{c:?} has a CJK face that draws it but was laid out as {:?}",
                glyph.face
            );
            checked += 1;
        }
    }
    eprintln!("checked {checked} glyphs against {} faces", faces.cjk_len());
}

/// Each CJK glyph reaches the atlas with ink in it. Faces differ in outline
/// format (TrueType on one platform, CFF on another), so a face that lays out
/// fine can still rasterize to nothing.
#[test]
fn cjk_glyphs_are_rasterized_with_ink() {
    let mut registry = FontRegistry::new();
    let Ok(primary) = registry.load_generic_with_style(GenericFont::System, 400, false) else {
        return;
    };
    if cjk_faces(&mut registry, &primary).is_none() {
        return;
    }

    let mut renderer = TextRenderer::new();
    let Ok(prepared) = renderer.prepare_text_with_style(
        CJK_TEXT,
        24.0,
        [1.0; 4],
        &LayoutOptions::default(),
        None,
        GenericFont::System,
        400,
        false,
    ) else {
        panic!("a CJK run failed to rasterize");
    };

    let visible = CJK_TEXT.chars().filter(|c| !c.is_whitespace()).count();
    assert_eq!(prepared.glyphs.len(), visible, "a glyph was not drawn");

    let atlas = renderer.atlas();
    let (aw, ah) = atlas.dimensions();
    let pixels = atlas.pixels();
    let bpp = pixels.len() / (aw as usize * ah as usize);
    for (glyph, c) in prepared
        .glyphs
        .iter()
        .zip(CJK_TEXT.chars().filter(|c| !c.is_whitespace()))
    {
        // The rect is in atlas pixels, not normalised.
        let [u0, v0, u1, v1] = glyph.uv_bounds;
        let (x0, x1) = (u0 as usize, (u1.ceil() as usize).min(aw as usize));
        let (y0, y1) = (v0 as usize, (v1.ceil() as usize).min(ah as usize));
        let ink = (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|&(x, y)| pixels[(y * aw as usize + x) * bpp] > 0)
            .count();
        assert!(ink >= 20, "{c:?} rasterized with only {ink} inked pixels");
    }
}
