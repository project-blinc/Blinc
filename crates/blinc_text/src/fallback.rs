//! Font fallback, decided once at layout time.
//!
//! A glyph the primary face does not have, or an emoji, is drawn from a
//! fallback face (the emoji face first for emoji, then the symbol face). That
//! face's advance is not the primary's advance for the same character, so
//! unless layout knows about the substitution, everything built on layout
//! disagrees with what is drawn: the width of a run, where a line wraps, where
//! a caret stops.
//!
//! The renderer used to correct for this after layout, by accumulating an
//! offset while it walked the glyphs. Layout, wrapping and measurement never
//! saw it. This module moves the decision into the one place every path starts
//! from, right after shaping, so each of them reads the same advances.
//!
//! The rules are the renderer's, unchanged:
//! - whitespace, variation selectors and joiners draw nothing and are left
//!   alone;
//! - an emoji codepoint that repeats the one before it is a cluster duplicate
//!   the renderer skips, so it is left alone too;
//! - a codepoint needs a fallback if the primary has no glyph for it, or if it
//!   is an emoji;
//! - an emoji tries the emoji face, then the symbol face;
//! - a CJK character tries the CJK faces in order, then the symbol face;
//! - anything else tries the symbol face only.
//!
//! No single face covers Han, kana and Hangul, so the CJK choice is made per
//! character: the first candidate that has the glyph. A candidate is loaded
//! only when a character in the text needs it, since these fonts are tens of
//! megabytes.

use std::sync::Arc;

use crate::emoji::{is_emoji, is_variation_selector, is_zwj};
use crate::font::FontFace;
use crate::registry::{FontRegistry, GenericFont};
use crate::shaper::{ShapedText, TextShaper};

/// Which face a glyph is drawn from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum FaceChoice {
    /// The face the text was laid out in.
    #[default]
    Primary,
    /// The symbol fallback face.
    Symbol,
    /// The colour emoji fallback face.
    Emoji,
    /// The CJK fallback face at this position in the chain.
    Cjk(u8),
}

/// The fallback faces available to a layout.
///
/// Empty by default, which makes layout behave exactly as it did before
/// fallback was known to it: every glyph stays on the primary face.
#[derive(Clone, Default)]
pub struct FallbackFaces {
    symbol: Option<Arc<FontFace>>,
    emoji: Option<Arc<FontFace>>,
    /// CJK faces in the order they are tried. Usually one or two: whichever
    /// candidates the text needed.
    cjk: Vec<(&'static str, Arc<FontFace>)>,
}

/// What a string would need, found without loading anything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FallbackNeeds {
    /// Some character is not in the primary face, or is an emoji.
    pub symbol: bool,
    /// Some character is an emoji.
    pub emoji: bool,
    /// Some CJK character is not in the primary face.
    pub cjk: bool,
}

/// Whether a glyph is the shaper's second report of the glyph before it.
///
/// HarfBuzz can emit two glyphs for one cluster, such as a symbol and its
/// variation selector, and both carry the cluster's first codepoint. The
/// second draws nothing. It is NOT a repeat the author typed: `😀😀` is two
/// clusters, so two glyphs, two draws.
///
/// Matching on the codepoint alone, as this once did, also swallowed real
/// repeats: `a😀😀b` drew one smiley and left a blank where the other was.
/// The cluster is what tells them apart.
pub(crate) fn is_cluster_duplicate(
    prev_codepoint: char,
    prev_cluster: usize,
    codepoint: char,
    cluster: usize,
) -> bool {
    is_emoji(codepoint) && prev_codepoint == codepoint && prev_cluster == cluster
}

/// Whether `c` is Chinese, Japanese or Korean script or its punctuation:
/// ideographs, kana, Hangul, and the CJK symbols and full-width forms that
/// go with them.
pub fn is_cjk(c: char) -> bool {
    matches!(u32::from(c),
        0x2E80..=0x2FDF      // radicals
        | 0x3000..=0x303F    // symbols and punctuation
        | 0x3040..=0x30FF    // Hiragana, Katakana
        | 0x3100..=0x312F    // Bopomofo
        | 0x3130..=0x318F    // Hangul compatibility Jamo
        | 0x3190..=0x31FF    // Kanbun, Bopomofo extended, strokes, Katakana extension
        | 0x3200..=0x4DBF    // enclosed, compatibility, Extension A
        | 0x4E00..=0x9FFF    // unified ideographs
        | 0xA960..=0xA97F    // Hangul Jamo Extended-A
        | 0xAC00..=0xD7FF    // Hangul syllables and Jamo Extended-B
        | 0xF900..=0xFAFF    // compatibility ideographs
        | 0xFE30..=0xFE4F    // compatibility forms
        | 0xFF00..=0xFFEF    // half-width and full-width forms
        | 0x20000..=0x2FA1F  // Extensions B to F and the supplement
    )
}

/// CJK faces to try, in order, on this platform.
///
/// No one face covers every script, so these are tried per character and only
/// as far as the text needs. The first candidate wins a contested codepoint,
/// which for Han ideographs is a guess at the reader's locale: Simplified,
/// Traditional, Japanese and Korean share codepoints and draw them
/// differently. Of these lists only macOS has been exercised.
fn cjk_candidates() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        // PingFang is the system's own choice where the registry can see it.
        &[
            "PingFang SC",
            "Hiragino Sans GB",
            "Hiragino Sans",
            "Apple SD Gothic Neo",
            "Arial Unicode MS",
        ]
    } else if cfg!(target_os = "windows") {
        &[
            "Microsoft YaHei",
            "Yu Gothic",
            "Malgun Gothic",
            "Microsoft JhengHei",
            "SimSun",
        ]
    } else {
        &[
            "Noto Sans CJK SC",
            "Noto Sans CJK JP",
            "Noto Sans CJK KR",
            "Noto Sans CJK TC",
            "WenQuanYi Micro Hei",
            "Droid Sans Fallback",
        ]
    }
}

impl FallbackFaces {
    /// No fallback: every glyph stays on the primary face.
    pub fn none() -> Self {
        Self::default()
    }

    /// Fallback faces already in hand.
    pub fn new(symbol: Option<Arc<FontFace>>, emoji: Option<Arc<FontFace>>) -> Self {
        Self {
            symbol,
            emoji,
            cjk: Vec::new(),
        }
    }

    /// The same faces with these CJK faces added, in the order to try them.
    pub fn with_cjk(mut self, faces: Vec<(&'static str, Arc<FontFace>)>) -> Self {
        self.cjk = faces;
        self
    }

    /// Whether there is any fallback face at all.
    pub fn is_empty(&self) -> bool {
        self.symbol.is_none() && self.emoji.is_none() && self.cjk.is_empty()
    }

    /// The family name a CJK face was loaded under. Faces report no name of
    /// their own, so this is what tells two apart.
    pub fn cjk_name(&self, index: u8) -> Option<&'static str> {
        self.cjk.get(usize::from(index)).map(|(n, _)| *n)
    }

    /// How many CJK faces were resolved.
    pub fn cjk_len(&self) -> usize {
        self.cjk.len()
    }

    /// The face a choice names. `Primary` has none here: the caller has it.
    pub fn face(&self, choice: FaceChoice) -> Option<&Arc<FontFace>> {
        match choice {
            FaceChoice::Primary => None,
            FaceChoice::Symbol => self.symbol.as_ref(),
            FaceChoice::Emoji => self.emoji.as_ref(),
            FaceChoice::Cjk(i) => self.cjk.get(usize::from(i)).map(|(_, f)| f),
        }
    }

    /// A small number that changes when the set of faces does, for keying a
    /// layout cache: the same text lays out differently with and without them.
    pub fn signature(&self) -> u8 {
        u8::from(self.symbol.is_some())
            | u8::from(self.emoji.is_some()) << 1
            | (self.cjk.len().min(63) as u8) << 2
    }

    /// What `text` would need, without loading anything.
    ///
    /// ASCII is never looked up: every face a UI draws text in has it, and
    /// this runs on every measurement.
    pub fn needs(primary: &FontFace, text: &str) -> FallbackNeeds {
        let mut needs = FallbackNeeds::default();
        for c in text.chars().filter(|c| !c.is_ascii()) {
            if c.is_whitespace() || is_variation_selector(c) || is_zwj(c) {
                continue;
            }
            if is_emoji(c) {
                needs.symbol = true;
                needs.emoji = true;
            } else if !primary.has_glyph(c) {
                needs.symbol = true;
                needs.cjk |= is_cjk(c);
            }
            if needs.symbol && needs.emoji && needs.cjk {
                break;
            }
        }
        needs
    }

    /// The faces `text` needs, loaded from `registry` if they are not yet.
    ///
    /// The same laziness the renderer always had: the symbol face is small and
    /// loads when any character needs a fallback, and the emoji face, which is
    /// large, loads only when an emoji is actually present. Text with neither
    /// costs a scan of its non-ASCII characters and loads nothing.
    pub fn resolve(registry: &mut FontRegistry, primary: &FontFace, text: &str) -> Self {
        let needs = Self::needs(primary, text);
        let mut faces = Self {
            symbol: needs
                .symbol
                .then(|| registry.load_generic(GenericFont::Symbol).ok())
                .flatten(),
            emoji: needs
                .emoji
                .then(|| registry.load_generic(GenericFont::Emoji).ok())
                .flatten(),
            cjk: Vec::new(),
        };
        if needs.cjk {
            faces.cjk = Self::resolve_cjk(registry, primary, text);
        }
        faces
    }

    /// The CJK faces `text` needs: candidates in order, loaded only until every
    /// CJK character the primary lacks is covered, and only kept if they cover
    /// at least one not already covered. A face a character does not need is
    /// never loaded.
    fn resolve_cjk(
        registry: &mut FontRegistry,
        primary: &FontFace,
        text: &str,
    ) -> Vec<(&'static str, Arc<FontFace>)> {
        let mut uncovered: Vec<char> = text
            .chars()
            .filter(|&c| !c.is_ascii() && is_cjk(c) && !is_emoji(c) && !primary.has_glyph(c))
            .collect();
        uncovered.sort_unstable();
        uncovered.dedup();

        let mut chosen: Vec<(&'static str, Arc<FontFace>)> = Vec::new();
        for &name in cjk_candidates() {
            if uncovered.is_empty() {
                break;
            }
            let Ok(face) = registry.load_font(name) else {
                continue;
            };
            let before = uncovered.len();
            uncovered.retain(|&c| face.glyph_id(c).is_none_or(|g| g == 0));
            if uncovered.len() < before {
                chosen.push((name, face));
            }
        }
        chosen
    }

    /// Substitute the fallback glyph and advance into `shaped`, in place.
    ///
    /// The advance is converted into the PRIMARY face's font units, because
    /// every consumer of a `ShapedText` scales by its `units_per_em`. A glyph
    /// that gets a fallback also records which face it is now from, so the
    /// renderer rasterises the right one.
    pub fn apply(&self, shaped: &mut ShapedText, primary: &FontFace, font_size: f32) {
        if self.is_empty() {
            return;
        }
        let shaper = TextShaper::new();
        let primary_upem = f32::from(shaped.units_per_em);

        for i in 0..shaped.glyphs.len() {
            let glyph = shaped.glyphs[i];
            let c = glyph.codepoint;

            if c.is_whitespace() || is_variation_selector(c) || is_zwj(c) {
                continue;
            }
            let emoji = is_emoji(c);
            // A cluster duplicate the renderer skips; it draws nothing, so
            // it must not change the advance either.
            if i > 0 {
                let prev = &shaped.glyphs[i - 1];
                if is_cluster_duplicate(
                    prev.codepoint,
                    prev.cluster as usize,
                    c,
                    glyph.cluster as usize,
                ) {
                    continue;
                }
            }

            let primary_has = glyph.glyph_id != 0 && primary.has_glyph(c);
            if primary_has && !emoji {
                continue;
            }

            // The faces to try, best first. CJK characters try the CJK faces
            // in order and then the symbol face, which can carry some of their
            // punctuation.
            let mut chain: Vec<FaceChoice> = Vec::with_capacity(self.cjk.len() + 2);
            if emoji {
                chain.extend([FaceChoice::Emoji, FaceChoice::Symbol]);
            } else {
                if is_cjk(c) {
                    chain.extend((0..self.cjk.len()).map(|i| FaceChoice::Cjk(i as u8)));
                }
                chain.push(FaceChoice::Symbol);
            }
            for choice in chain {
                let Some(face) = self.face(choice) else {
                    continue;
                };
                if face.glyph_id(c).is_none_or(|g| g == 0) {
                    continue;
                }
                let mut buf = [0u8; 4];
                let one = shaper.shape(c.encode_utf8(&mut buf), face, font_size);
                let Some(g) = one.glyphs.first() else {
                    continue;
                };

                let face_upem = f32::from(face.metrics().units_per_em);
                let slot = &mut shaped.glyphs[i];
                slot.glyph_id = g.glyph_id;
                slot.x_advance = (g.x_advance as f32 * primary_upem / face_upem).round() as i32;
                slot.face = choice;
                break;
            }
        }
    }
}

impl std::fmt::Debug for FallbackFaces {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FallbackFaces")
            .field("symbol", &self.symbol.is_some())
            .field("emoji", &self.emoji.is_some())
            .field("cjk", &self.cjk.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_covers_the_scripts_and_their_punctuation() {
        for c in [
            '你',
            '好',
            '漢',
            'あ',
            'ん',
            'カ',
            'ー',
            '안',
            '녕',
            '한',
            '、',
            '。',
            '「',
            '」',
            'ａ',
            '，',
            '\u{20000}',
        ] {
            assert!(is_cjk(c), "{c:?} should be CJK");
        }
    }

    #[test]
    fn cjk_excludes_latin_symbols_and_emoji() {
        for c in [
            'a',
            'Z',
            '0',
            ' ',
            'é',
            'Ω',
            'я',
            '★',
            '☃',
            '\u{1F600}',
            '\u{FE0F}',
        ] {
            assert!(!is_cjk(c), "{c:?} should not be CJK");
        }
    }

    #[test]
    fn signature_distinguishes_the_cjk_face_count() {
        let none = FallbackFaces::none();
        assert_eq!(none.signature(), 0);
        assert_eq!(FallbackFaces::none().with_cjk(Vec::new()).signature(), 0);
    }

    #[test]
    fn candidates_are_never_empty() {
        assert!(!cjk_candidates().is_empty());
    }
}
