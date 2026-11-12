//! Shaping with a plan kept for a face gives what shaping with a plan made for the call gives.

use blinc_text::{FontFace, TextShaper};
use rustybuzz::{Face, UnicodeBuffer};

const NOTO: &[u8] =
    include_bytes!("../../../examples/blinc_component_demos/assets/NotoSans-Regular.ttf");

fn direct(face: &Face, text: &str) -> Vec<(u32, i32, i32, i32, i32)> {
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    let out = rustybuzz::shape(face, &[], buffer);
    out.glyph_infos()
        .iter()
        .zip(out.glyph_positions())
        .map(|(i, p)| (i.glyph_id, p.x_advance, p.y_advance, p.x_offset, p.y_offset))
        .collect()
}

#[test]
fn a_kept_plan_shapes_as_a_fresh_one_does() {
    let font = FontFace::from_data(NOTO.to_vec()).expect("Noto Sans parses");
    let face = Face::from_slice(font.data(), font.face_index()).unwrap();
    let shaper = TextShaper::new();
    // Latin, digits alone (no script of their own), empty, right to left, mixed, and a ligature.
    let texts = [
        "Row 12",
        "Row 13",
        "123",
        "",
        "   ",
        "\u{05E9}\u{05DC}\u{05D5}\u{05DD}",
        "\u{0645}\u{0631}\u{062D}\u{0628}\u{0627} abc",
        "office fi ffl",
        "Ελληνικά Кириллица",
    ];
    // Twice: the second round is answered from the plans the first made.
    for round in 0..2 {
        for text in texts {
            let kept: Vec<_> = shaper
                .shape(text, &font, 16.0)
                .glyphs
                .iter()
                .map(|g| (g.glyph_id as u32, g.x_advance, g.y_advance, g.x_offset, g.y_offset))
                .collect();
            assert_eq!(kept, direct(&face, text), "round {round}: {text:?}");
        }
    }
}

#[test]
fn two_faces_do_not_share_a_plan() {
    let a = FontFace::from_data(NOTO.to_vec()).unwrap();
    let b = FontFace::from_data(NOTO.to_vec()).unwrap();
    assert_ne!(a.id(), b.id());
}
