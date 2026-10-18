//! A glyph of a PostScript (CFF) font rasterizes to pixels, not font units.

use blinc_text::font::FontFace;
use blinc_text::rasterizer::GlyphRasterizer;

/// The widest side of `c` rasterized at `size` px from the font at `path`.
fn side(path: &str, c: char, size: f32) -> Option<u32> {
    let data = std::fs::read(path).ok()?;
    let gid = ttf_parser::Face::parse(&data, 0).ok()?.glyph_index(c)?.0;
    let face = FontFace::from_data(data).ok()?;
    let glyph = GlyphRasterizer::new().rasterize(&face, gid, size).ok()?;
    Some(glyph.width.max(glyph.height))
}

#[test]
fn a_truetype_glyph_is_a_few_pixels_across() {
    let arial = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts/Arial.ttf");
    let side = side(arial, '3', 32.0).expect("the bundled Arial rasterizes");
    assert!((10..64).contains(&side), "a 32px glyph is {side}px across");
}

/// A CFF face, where the machine has one: macOS's SF Mono. Unhinted, swash
/// rasterized its glyphs a thousand pixels a side, and a few of them filled
/// the glyph atlas. No CFF font is committed, so elsewhere this is skipped.
#[test]
fn a_cff_glyph_is_a_few_pixels_across() {
    let sf_mono =
        "/System/Applications/Utilities/Terminal.app/Contents/Resources/Fonts/SF-Mono-Regular.otf";
    let Some(side) = side(sf_mono, '3', 32.0) else {
        eprintln!("skipped: no CFF font on this machine");
        return;
    };
    assert!((10..64).contains(&side), "a 32px glyph is {side}px across");
}
