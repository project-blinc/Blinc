//! Text shaping using rustybuzz (HarfBuzz)
//!
//! Converts text strings into positioned glyph sequences with proper
//! kerning, ligatures, and OpenType feature support.

use crate::font::{FontData, FontFace};
use rustybuzz::{Direction, Face, Language, Script, ShapePlan, UnicodeBuffer};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A shaped glyph with position information
#[derive(Debug, Clone, Copy)]
pub struct ShapedGlyph {
    /// Glyph ID in the font
    pub glyph_id: u16,
    /// Character this glyph represents (for debugging/fallback)
    pub codepoint: char,
    /// X offset from current position (for combining marks, etc.)
    pub x_offset: i32,
    /// Y offset from current position
    pub y_offset: i32,
    /// Horizontal advance to next glyph position
    pub x_advance: i32,
    /// Vertical advance (typically 0 for horizontal text)
    pub y_advance: i32,
    /// Index in the original string (cluster)
    pub cluster: u32,
    /// Which face this glyph is drawn from. `Primary` until fallback
    /// layout substitutes another.
    pub face: crate::fallback::FaceChoice,
}

/// Result of shaping a text string
#[derive(Debug, Clone)]
pub struct ShapedText {
    /// Shaped glyphs in visual order
    pub glyphs: Vec<ShapedGlyph>,
    /// Total advance width in font units
    pub total_advance: i32,
    /// Font size used for shaping reference
    pub font_size: f32,
    /// Units per em from the font
    pub units_per_em: u16,
}

impl ShapedText {
    /// Get total width in pixels
    pub fn width_px(&self) -> f32 {
        self.total_advance as f32 * self.font_size / self.units_per_em as f32
    }

    /// Scale a font-unit value to pixels
    pub fn scale(&self, value: i32) -> f32 {
        value as f32 * self.font_size / self.units_per_em as f32
    }
}

/// Text shaper using HarfBuzz via rustybuzz
///
/// Parsing a face for rustybuzz and planning how to shape for it are the cost of shaping a short
/// string, so the shaper keeps the face and its plans for each font it is asked to shape with.
pub struct TextShaper {
    faces: Mutex<HashMap<FaceKey, Arc<KeptFace>>>,
}

/// A face by its id and the bits of the `wght` value set on it, if any.
type FaceKey = (u64, Option<u32>);

/// A rustybuzz face, and the plans made for it.
struct KeptFace {
    // Declared before `_data`, which it borrows: it is dropped first.
    face: Face<'static>,
    plans: Mutex<HashMap<PlanKey, Arc<ShapePlan>>>,
    _data: FontData,
}

/// What a shape plan is made for, within one face.
#[derive(Clone, PartialEq, Eq, Hash)]
struct PlanKey {
    direction: Direction,
    script: Option<Script>,
    language: Option<Language>,
}

/// Faces kept before they are dropped to start again; one font and weight is one.
const MAX_FACES: usize = 64;
/// Plans kept for one face; a font, direction, script and language is one.
const MAX_PLANS: usize = 128;

impl KeptFace {
    fn new(font_face: &FontFace) -> Option<Self> {
        let data = font_face.font_data().clone();
        let bytes = data.as_bytes();
        // SAFETY: the bytes belong to the `Arc` that `_data` holds, which is not moved or
        // changed for as long as it lives; the face is dropped before it, and is only lent out
        // for the length of a borrow of this value.
        let bytes: &'static [u8] =
            unsafe { std::slice::from_raw_parts(bytes.as_ptr(), bytes.len()) };
        let mut face = Face::from_slice(bytes, font_face.face_index())?;
        // A variable face carries every weight in one file, so the axis has to be set or it
        // shapes at its default instance. Advances differ per weight, so this has to happen
        // before shaping, not only before rasterizing.
        if let Some(w) = font_face.variation_weight() {
            face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), w);
        }
        Some(Self {
            face,
            plans: Mutex::new(HashMap::new()),
            _data: data,
        })
    }

    /// Shape `buffer` with the plan kept for its segment properties, making it once.
    fn shape(&self, mut buffer: UnicodeBuffer) -> rustybuzz::GlyphBuffer {
        buffer.guess_segment_properties();
        // The buffer reports no script as unknown; both are shaped as the default.
        let key = PlanKey {
            direction: buffer.direction(),
            script: Some(buffer.script()).filter(|s| *s != rustybuzz::script::UNKNOWN),
            language: buffer.language(),
        };
        let plan = {
            let mut plans = self.plans.lock().unwrap_or_else(|e| e.into_inner());
            match plans.get(&key) {
                Some(plan) => Arc::clone(plan),
                None => {
                    if plans.len() >= MAX_PLANS {
                        plans.clear();
                    }
                    let plan = Arc::new(ShapePlan::new(
                        &self.face,
                        key.direction,
                        key.script,
                        key.language.as_ref(),
                        &[],
                    ));
                    plans.insert(key, Arc::clone(&plan));
                    plan
                }
            }
        };
        rustybuzz::shape_with_plan(&self.face, &plan, buffer)
    }
}

impl TextShaper {
    /// Create a new text shaper
    pub fn new() -> Self {
        Self {
            faces: Mutex::new(HashMap::new()),
        }
    }

    /// The face kept for `font_face`, parsed on the first ask. `None` when it does not parse.
    fn kept_face(&self, font_face: &FontFace) -> Option<Arc<KeptFace>> {
        let key = (
            font_face.id(),
            font_face.variation_weight().map(f32::to_bits),
        );
        let mut faces = self.faces.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(kept) = faces.get(&key) {
            return Some(Arc::clone(kept));
        }
        let kept = Arc::new(KeptFace::new(font_face)?);
        if faces.len() >= MAX_FACES {
            faces.clear();
        }
        faces.insert(key, Arc::clone(&kept));
        Some(kept)
    }

    /// Shape a text string using the given font
    pub fn shape(&self, text: &str, font_face: &FontFace, font_size: f32) -> ShapedText {
        let Some(kept) = self.kept_face(font_face) else {
            // Fallback: return basic glyph sequence without shaping
            return self.fallback_shape(text, font_face, font_size);
        };

        // Create and fill the Unicode buffer
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);

        // Shape the buffer
        let output = kept.shape(buffer);

        // Extract glyph information
        let glyph_infos = output.glyph_infos();
        let glyph_positions = output.glyph_positions();

        let mut glyphs = Vec::with_capacity(glyph_infos.len());
        let mut total_advance = 0i32;

        for (info, pos) in glyph_infos.iter().zip(glyph_positions.iter()) {
            // Find the original character for this cluster
            let codepoint = text
                .char_indices()
                .find(|(i, _)| *i as u32 == info.cluster)
                .map(|(_, c)| c)
                .unwrap_or('\u{FFFD}');

            glyphs.push(ShapedGlyph {
                glyph_id: info.glyph_id as u16,
                codepoint,
                x_offset: pos.x_offset,
                y_offset: pos.y_offset,
                x_advance: pos.x_advance,
                y_advance: pos.y_advance,
                cluster: info.cluster,
                face: crate::fallback::FaceChoice::Primary,
            });

            total_advance += pos.x_advance;
        }

        ShapedText {
            glyphs,
            total_advance,
            font_size,
            units_per_em: font_face.metrics().units_per_em,
        }
    }

    /// Fallback shaping when rustybuzz fails
    fn fallback_shape(&self, text: &str, font_face: &FontFace, font_size: f32) -> ShapedText {
        let mut glyphs = Vec::new();
        let mut total_advance = 0i32;

        for (cluster, c) in text.char_indices() {
            let glyph_id = font_face.glyph_id(c).unwrap_or(0);
            let advance = font_face.glyph_advance(glyph_id).unwrap_or(500) as i32;

            glyphs.push(ShapedGlyph {
                glyph_id,
                codepoint: c,
                x_offset: 0,
                y_offset: 0,
                x_advance: advance,
                y_advance: 0,
                cluster: cluster as u32,
                face: crate::fallback::FaceChoice::Primary,
            });

            total_advance += advance;
        }

        ShapedText {
            glyphs,
            total_advance,
            font_size,
            units_per_em: font_face.metrics().units_per_em,
        }
    }

    /// Shape with specific OpenType features enabled/disabled
    pub fn shape_with_features(
        &self,
        text: &str,
        font_face: &FontFace,
        font_size: f32,
        features: &[rustybuzz::Feature],
    ) -> ShapedText {
        let Some(kept) = self.kept_face(font_face) else {
            return self.fallback_shape(text, font_face, font_size);
        };

        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);

        let output = rustybuzz::shape(&kept.face, features, buffer);

        let glyph_infos = output.glyph_infos();
        let glyph_positions = output.glyph_positions();

        let mut glyphs = Vec::with_capacity(glyph_infos.len());
        let mut total_advance = 0i32;

        for (info, pos) in glyph_infos.iter().zip(glyph_positions.iter()) {
            let codepoint = text
                .char_indices()
                .find(|(i, _)| *i as u32 == info.cluster)
                .map(|(_, c)| c)
                .unwrap_or('\u{FFFD}');

            glyphs.push(ShapedGlyph {
                glyph_id: info.glyph_id as u16,
                codepoint,
                x_offset: pos.x_offset,
                y_offset: pos.y_offset,
                x_advance: pos.x_advance,
                y_advance: pos.y_advance,
                cluster: info.cluster,
                face: crate::fallback::FaceChoice::Primary,
            });

            total_advance += pos.x_advance;
        }

        ShapedText {
            glyphs,
            total_advance,
            font_size,
            units_per_em: font_face.metrics().units_per_em,
        }
    }
}

impl Default for TextShaper {
    fn default() -> Self {
        Self::new()
    }
}
