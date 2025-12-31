//! Font loading and management
//!
//! Provides font parsing via ttf-parser and font metric extraction.

use crate::{Result, TextError};
use std::sync::Arc;

/// Font data that can be either owned or memory-mapped.
///
/// This avoids copying 180MB+ font files when using memory-mapped sources.
/// Uses the same trait object type as fontdb for zero-copy compatibility.
#[derive(Clone)]
pub enum FontData {
    /// Owned data (Vec wrapped in Arc for cheap cloning)
    Owned(Arc<Vec<u8>>),
    /// Shared data from fontdb (memory-mapped or binary) - avoids copy
    Shared(Arc<dyn AsRef<[u8]> + Send + Sync>),
}

impl FontData {
    /// Create from owned data
    pub fn from_vec(data: Vec<u8>) -> Self {
        FontData::Owned(Arc::new(data))
    }

    /// Create from shared data (memory-mapped or fontdb binary)
    ///
    /// This is the key optimization - accepts fontdb's Arc directly without copying.
    pub fn from_mapped(data: Arc<dyn AsRef<[u8]> + Send + Sync>) -> Self {
        FontData::Shared(data)
    }

    /// Get the raw bytes
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            FontData::Owned(data) => data.as_ref(),
            FontData::Shared(data) => data.as_ref().as_ref(),
        }
    }
}

impl std::fmt::Debug for FontData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FontData::Owned(data) => write!(f, "FontData::Owned({} bytes)", data.len()),
            FontData::Shared(data) => {
                write!(f, "FontData::Shared({} bytes)", data.as_ref().as_ref().len())
            }
        }
    }
}

/// Font weight variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FontWeight {
    Thin = 100,
    ExtraLight = 200,
    Light = 300,
    #[default]
    Regular = 400,
    Medium = 500,
    SemiBold = 600,
    Bold = 700,
    ExtraBold = 800,
    Black = 900,
}

impl FontWeight {
    /// Convert from numeric weight (100-900)
    pub fn from_number(weight: u16) -> Self {
        match weight {
            0..=149 => FontWeight::Thin,
            150..=249 => FontWeight::ExtraLight,
            250..=349 => FontWeight::Light,
            350..=449 => FontWeight::Regular,
            450..=549 => FontWeight::Medium,
            550..=649 => FontWeight::SemiBold,
            650..=749 => FontWeight::Bold,
            750..=849 => FontWeight::ExtraBold,
            _ => FontWeight::Black,
        }
    }

    /// Get numeric weight value
    pub fn to_number(self) -> u16 {
        self as u16
    }
}

/// Font style (normal or italic)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

/// Font metrics in font units (typically 1000 or 2048 units per em)
#[derive(Debug, Clone, Copy)]
pub struct FontMetrics {
    /// Units per em (typically 1000 or 2048)
    pub units_per_em: u16,
    /// Ascender (distance from baseline to top of tallest glyph)
    pub ascender: i16,
    /// Descender (distance from baseline to bottom, typically negative)
    pub descender: i16,
    /// Line gap (additional spacing between lines)
    pub line_gap: i16,
    /// Capital letter height
    pub cap_height: Option<i16>,
    /// x-height (height of lowercase 'x')
    pub x_height: Option<i16>,
    /// Underline position (typically negative)
    pub underline_position: Option<i16>,
    /// Underline thickness
    pub underline_thickness: Option<i16>,
}

impl FontMetrics {
    /// Calculate line height in font units
    pub fn line_height(&self) -> i16 {
        self.ascender - self.descender + self.line_gap
    }

    /// Scale a value from font units to pixels
    pub fn scale(&self, value: i16, font_size: f32) -> f32 {
        value as f32 * font_size / self.units_per_em as f32
    }

    /// Get ascender in pixels
    pub fn ascender_px(&self, font_size: f32) -> f32 {
        self.scale(self.ascender, font_size)
    }

    /// Get descender in pixels (typically negative)
    pub fn descender_px(&self, font_size: f32) -> f32 {
        self.scale(self.descender, font_size)
    }

    /// Get line height in pixels
    pub fn line_height_px(&self, font_size: f32) -> f32 {
        self.scale(self.line_height(), font_size)
    }
}

/// A parsed font face
pub struct FontFace {
    /// Raw font data (kept alive for ttf-parser) - can be owned or memory-mapped
    data: FontData,
    /// Face index within the font file (for TTC files)
    face_index: u32,
    /// Font metrics
    metrics: FontMetrics,
    /// Number of glyphs in the font
    glyph_count: u16,
    /// Font family name
    family_name: String,
    /// Font weight
    weight: FontWeight,
    /// Font style
    style: FontStyle,
}

impl FontFace {
    /// Load a font from raw TTF/OTF data (uses face index 0)
    pub fn from_data(data: Vec<u8>) -> Result<Self> {
        Self::from_data_with_index(data, 0)
    }

    /// Load a font from raw TTF/OTF data with a specific face index
    ///
    /// For TTC (TrueType Collection) files, different indices represent different
    /// font faces (e.g., Regular, Bold, Italic variants).
    pub fn from_data_with_index(data: Vec<u8>, face_index: u32) -> Result<Self> {
        let font_data = FontData::from_vec(data);
        Self::from_font_data(font_data, face_index)
    }

    /// Load a font from FontData (can be owned or memory-mapped)
    ///
    /// This is the core loading function that avoids copying memory-mapped data.
    pub fn from_font_data(data: FontData, face_index: u32) -> Result<Self> {
        let face = ttf_parser::Face::parse(data.as_bytes(), face_index)
            .map_err(|e| TextError::FontParseError(format!("{:?}", e)))?;

        let metrics = FontMetrics {
            units_per_em: face.units_per_em(),
            ascender: face.ascender(),
            descender: face.descender(),
            line_gap: face.line_gap(),
            cap_height: face.capital_height(),
            x_height: face.x_height(),
            underline_position: face.underline_metrics().map(|m| m.position),
            underline_thickness: face.underline_metrics().map(|m| m.thickness),
        };

        // Extract font names
        let family_name = face
            .names()
            .into_iter()
            .find(|n| n.name_id == ttf_parser::name_id::FAMILY)
            .and_then(|n| n.to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        // Determine weight from OS/2 table
        let weight = face
            .tables()
            .os2
            .map(|os2| FontWeight::from_number(os2.weight().to_number()))
            .unwrap_or(FontWeight::Regular);

        // Determine style
        let style = if face.is_italic() {
            FontStyle::Italic
        } else if face.is_oblique() {
            FontStyle::Oblique
        } else {
            FontStyle::Normal
        };

        let glyph_count = face.number_of_glyphs();

        Ok(Self {
            data,
            face_index,
            metrics,
            glyph_count,
            family_name,
            weight,
            style,
        })
    }

    /// Load a font from a file path
    pub fn from_file(path: &std::path::Path) -> Result<Self> {
        let data = std::fs::read(path)
            .map_err(|e| TextError::FontLoadError(format!("Failed to read file: {}", e)))?;
        Self::from_data(data)
    }

    /// Get font metrics
    pub fn metrics(&self) -> &FontMetrics {
        &self.metrics
    }

    /// Get number of glyphs
    pub fn glyph_count(&self) -> u16 {
        self.glyph_count
    }

    /// Get font family name
    pub fn family_name(&self) -> &str {
        &self.family_name
    }

    /// Get font weight
    pub fn weight(&self) -> FontWeight {
        self.weight
    }

    /// Get font style
    pub fn style(&self) -> FontStyle {
        self.style
    }

    /// Get raw font data for shaping
    pub fn data(&self) -> &[u8] {
        self.data.as_bytes()
    }

    /// Get face index within the font file
    pub fn face_index(&self) -> u32 {
        self.face_index
    }

    /// Create a ttf-parser Face for glyph operations
    /// Note: This is slightly inefficient as it re-parses; consider caching if needed
    pub(crate) fn as_ttf_face(&self) -> Option<ttf_parser::Face<'_>> {
        ttf_parser::Face::parse(self.data.as_bytes(), self.face_index).ok()
    }

    /// Get glyph ID for a character
    pub fn glyph_id(&self, c: char) -> Option<u16> {
        self.as_ttf_face()
            .and_then(|face| face.glyph_index(c))
            .map(|id| id.0)
    }

    /// Check if the font has a glyph for a character
    ///
    /// Returns true if the font contains a glyph for the given character,
    /// false if it would need to use the notdef glyph or a fallback font.
    pub fn has_glyph(&self, c: char) -> bool {
        self.glyph_id(c).map(|id| id != 0).unwrap_or(false)
    }

    /// Get horizontal advance width for a glyph in font units
    pub fn glyph_advance(&self, glyph_id: u16) -> Option<u16> {
        self.as_ttf_face()
            .and_then(|face| face.glyph_hor_advance(ttf_parser::GlyphId(glyph_id)))
    }
}

impl std::fmt::Debug for FontFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontFace")
            .field("family_name", &self.family_name)
            .field("weight", &self.weight)
            .field("style", &self.style)
            .field("glyph_count", &self.glyph_count)
            .finish()
    }
}

/// A collection of font faces (different weights/styles of the same family)
pub struct Font {
    /// Font family name
    family: String,
    /// Font faces indexed by (weight, style)
    faces: Vec<FontFace>,
}

impl Font {
    /// Create a new font collection with a single face
    pub fn new(face: FontFace) -> Self {
        let family = face.family_name().to_string();
        Self {
            family,
            faces: vec![face],
        }
    }

    /// Add a font face to the collection
    pub fn add_face(&mut self, face: FontFace) {
        self.faces.push(face);
    }

    /// Get the best matching face for the given weight and style
    pub fn get_face(&self, weight: FontWeight, style: FontStyle) -> Option<&FontFace> {
        // First try exact match
        if let Some(face) = self
            .faces
            .iter()
            .find(|f| f.weight == weight && f.style == style)
        {
            return Some(face);
        }

        // Try matching style with closest weight
        let style_matches: Vec<_> = self.faces.iter().filter(|f| f.style == style).collect();
        if !style_matches.is_empty() {
            return style_matches
                .iter()
                .min_by_key(|f| (f.weight.to_number() as i32 - weight.to_number() as i32).abs())
                .copied();
        }

        // Fall back to any face with closest weight
        self.faces
            .iter()
            .min_by_key(|f| (f.weight.to_number() as i32 - weight.to_number() as i32).abs())
    }

    /// Get the default (regular) face
    pub fn default_face(&self) -> Option<&FontFace> {
        self.get_face(FontWeight::Regular, FontStyle::Normal)
            .or_else(|| self.faces.first())
    }

    /// Get font family name
    pub fn family(&self) -> &str {
        &self.family
    }
}
