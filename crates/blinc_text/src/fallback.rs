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
//! - an emoji tries the emoji face, then the symbol face; anything else tries
//!   the symbol face only.

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
}

/// The fallback faces available to a layout.
///
/// Empty by default, which makes layout behave exactly as it did before
/// fallback was known to it: every glyph stays on the primary face.
#[derive(Clone, Default)]
pub struct FallbackFaces {
    symbol: Option<Arc<FontFace>>,
    emoji: Option<Arc<FontFace>>,
}

/// What a string would need, found without loading anything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FallbackNeeds {
    /// Some character is not in the primary face, or is an emoji.
    pub symbol: bool,
    /// Some character is an emoji.
    pub emoji: bool,
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

impl FallbackFaces {
    /// No fallback: every glyph stays on the primary face.
    pub fn none() -> Self {
        Self::default()
    }

    /// Fallback faces already in hand.
    pub fn new(symbol: Option<Arc<FontFace>>, emoji: Option<Arc<FontFace>>) -> Self {
        Self { symbol, emoji }
    }

    /// Whether there is any fallback face at all.
    pub fn is_empty(&self) -> bool {
        self.symbol.is_none() && self.emoji.is_none()
    }

    /// The face a choice names. `Primary` has none here: the caller has it.
    pub fn face(&self, choice: FaceChoice) -> Option<&Arc<FontFace>> {
        match choice {
            FaceChoice::Primary => None,
            FaceChoice::Symbol => self.symbol.as_ref(),
            FaceChoice::Emoji => self.emoji.as_ref(),
        }
    }

    /// A small number that changes when the set of faces does, for keying a
    /// layout cache: the same text lays out differently with and without them.
    pub fn signature(&self) -> u8 {
        u8::from(self.symbol.is_some()) | u8::from(self.emoji.is_some()) << 1
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
            }
            if needs.symbol && needs.emoji {
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
        Self {
            symbol: needs
                .symbol
                .then(|| registry.load_generic(GenericFont::Symbol).ok())
                .flatten(),
            emoji: needs
                .emoji
                .then(|| registry.load_generic(GenericFont::Emoji).ok())
                .flatten(),
        }
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

            let chain: &[FaceChoice] = if emoji {
                &[FaceChoice::Emoji, FaceChoice::Symbol]
            } else {
                &[FaceChoice::Symbol]
            };
            for &choice in chain {
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
            .finish()
    }
}
