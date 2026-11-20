//! Text shaping using HarfRust
//!
//! Converts text strings into positioned glyph sequences with proper
//! kerning, ligatures, and OpenType feature support.

use crate::font::{FontData, FontFace};
use harfrust::{
    Buffer, Direction, Feature, Font, Language, Script, ShapeOptions, ShapePlan, ShaperFont,
};
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

/// Text shaper that keeps an owned HarfRust font and its default shape plans per face.
pub struct TextShaper {
    faces: Mutex<HashMap<FaceKey, Arc<KeptFace>>>,
}

/// A face by its id and the bits of the selected `wght` value.
type FaceKey = (u64, Option<u32>);

struct KeptFace {
    font: Font,
    plans: Mutex<HashMap<PlanKey, Arc<ShapePlan>>>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct PlanKey {
    direction: Direction,
    script: Option<Script>,
    language: Option<Language>,
}

const MAX_FACES: usize = 64;
const MAX_PLANS: usize = 128;

impl KeptFace {
    fn new(face: &FontFace) -> Option<Self> {
        let data: Arc<dyn AsRef<[u8]> + Send + Sync> = match face.font_data() {
            FontData::Owned(data) => data.clone(),
            FontData::Shared(data) => data.clone(),
        };
        // HarfRust retains the shared bytes, including a native memory map.
        let mut font = Font::new(data, face.face_index())?;
        if let Some(weight) = face.variation_weight() {
            font = font
                .instance_builder()
                .variations([("wght", weight)])
                .build();
        }
        Some(Self {
            font,
            plans: Mutex::new(HashMap::new()),
        })
    }

    fn shape(&self, mut buffer: Buffer, features: &[Feature]) -> Option<Buffer> {
        buffer.guess_segment_properties();
        let key = PlanKey {
            direction: buffer.direction(),
            script: buffer.script().filter(|s| *s != Script::UNKNOWN),
            language: buffer.language().cloned(),
        };
        let plan = if features.is_empty() {
            let mut plans = self.plans.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(plan) = plans.get(&key) {
                Arc::clone(plan)
            } else {
                if plans.len() >= MAX_PLANS {
                    plans.clear();
                }
                let plan = Arc::new(ShapePlan::new(
                    &self.font,
                    key.direction,
                    key.script,
                    key.language.as_ref(),
                    &[],
                ));
                plans.insert(key, Arc::clone(&plan));
                plan
            }
        } else {
            // Feature ranges belong to this call, not the default-plan cache.
            Arc::new(ShapePlan::new(
                &self.font,
                key.direction,
                key.script,
                key.language.as_ref(),
                features,
            ))
        };
        harfrust::shape(
            &ShaperFont::new(&self.font),
            &mut buffer,
            ShapeOptions::new().plan(Some(&plan)).features(features),
        )
        .ok()?;
        Some(buffer)
    }
}

impl TextShaper {
    pub fn new() -> Self {
        Self {
            faces: Mutex::new(HashMap::new()),
        }
    }

    fn kept_face(&self, face: &FontFace) -> Option<Arc<KeptFace>> {
        let key = (face.id(), face.variation_weight().map(f32::to_bits));
        let mut faces = self.faces.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(kept) = faces.get(&key) {
            return Some(Arc::clone(kept));
        }
        let kept = Arc::new(KeptFace::new(face)?);
        if faces.len() >= MAX_FACES {
            faces.clear();
        }
        faces.insert(key, Arc::clone(&kept));
        Some(kept)
    }

    /// Shape a segment with inferred direction, script and language.
    pub fn shape(&self, text: &str, face: &FontFace, font_size: f32) -> ShapedText {
        self.shape_with_features(text, face, font_size, &[])
    }

    /// Shape a segment with call-scoped OpenType feature settings.
    pub fn shape_with_features(
        &self,
        text: &str,
        face: &FontFace,
        font_size: f32,
        features: &[Feature],
    ) -> ShapedText {
        let Some(kept) = self.kept_face(face) else {
            return self.fallback_shape(text, face, font_size);
        };
        let mut buffer = Buffer::new();
        buffer.push_str(text);
        let Some(output) = kept.shape(buffer, features) else {
            return self.fallback_shape(text, face, font_size);
        };
        let mut total_advance = 0;
        let glyphs = output
            .glyph_infos()
            .iter()
            .zip(output.glyph_positions())
            .map(|(info, pos)| {
                total_advance += pos.x_advance;
                ShapedGlyph {
                    glyph_id: info.glyph_id as u16,
                    codepoint: text
                        .get(info.cluster as usize..)
                        .and_then(|s| s.chars().next())
                        .unwrap_or('\u{FFFD}'),
                    x_offset: pos.x_offset,
                    y_offset: pos.y_offset,
                    x_advance: pos.x_advance,
                    y_advance: pos.y_advance,
                    cluster: info.cluster,
                    face: crate::fallback::FaceChoice::Primary,
                }
            })
            .collect();
        ShapedText {
            glyphs,
            total_advance,
            font_size,
            units_per_em: face.metrics().units_per_em,
        }
    }

    fn fallback_shape(&self, text: &str, face: &FontFace, font_size: f32) -> ShapedText {
        let mut total_advance = 0;
        let glyphs = text
            .char_indices()
            .map(|(cluster, c)| {
                let glyph_id = face.glyph_id(c).unwrap_or(0);
                let advance = face.glyph_advance(glyph_id).unwrap_or(500) as i32;
                total_advance += advance;
                ShapedGlyph {
                    glyph_id,
                    codepoint: c,
                    x_offset: 0,
                    y_offset: 0,
                    x_advance: advance,
                    y_advance: 0,
                    cluster: cluster as u32,
                    face: crate::fallback::FaceChoice::Primary,
                }
            })
            .collect();
        ShapedText {
            glyphs,
            total_advance,
            font_size,
            units_per_em: face.metrics().units_per_em,
        }
    }
}

impl Default for TextShaper {
    fn default() -> Self {
        Self::new()
    }
}
