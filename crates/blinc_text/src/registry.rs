//! Font registry for system font discovery and caching
//!
//! Uses fontdb to discover and load system fonts by name or generic category.

use crate::font::FontFace;
use crate::{Result, TextError};
use fontdb::{Database, Family, Query, Source, Weight, Style, Stretch};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Generic font category for fallback
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GenericFont {
    /// Default system UI font
    #[default]
    System,
    /// Monospace font for code
    Monospace,
    /// Serif font
    Serif,
    /// Sans-serif font
    SansSerif,
}

/// Font registry that discovers and caches system fonts
pub struct FontRegistry {
    /// fontdb database containing all system fonts
    db: Database,
    /// Cached FontFace instances (Some = found, None = not found)
    faces: FxHashMap<String, Option<Arc<FontFace>>>,
}

impl FontRegistry {
    /// Create a new font registry and load system fonts
    pub fn new() -> Self {
        let mut db = Database::new();

        // Load all system fonts
        db.load_system_fonts();

        let mut registry = Self {
            db,
            faces: FxHashMap::default(),
        };

        // Preload all generic font categories at startup
        registry.preload_generic_fonts();

        registry
    }

    /// Preload all generic font categories
    fn preload_generic_fonts(&mut self) {
        for generic in [
            GenericFont::System,
            GenericFont::SansSerif,
            GenericFont::Serif,
            GenericFont::Monospace,
        ] {
            if let Err(e) = self.load_generic(generic) {
                tracing::warn!("Failed to preload generic font {:?}: {:?}", generic, e);
            }
        }
    }

    /// Preload specific fonts by name (call at startup for fonts your app uses)
    pub fn preload_fonts(&mut self, names: &[&str]) {
        for name in names {
            match self.load_font(name) {
                Ok(_) => tracing::debug!("Preloaded font: {}", name),
                Err(_) => tracing::debug!("Font not available: {}", name),
            }
        }
    }

    /// Load a font by name (e.g., "Fira Code", "Inter", "Arial")
    pub fn load_font(&mut self, name: &str) -> Result<Arc<FontFace>> {
        // Check cache first (includes failed lookups as None)
        if let Some(cached) = self.faces.get(name) {
            return cached.clone().ok_or_else(|| {
                TextError::FontLoadError(format!("Font '{}' not found (cached)", name))
            });
        }

        // Query fontdb for the font by family name
        // Request normal weight (400), normal style, normal stretch
        let query = Query {
            families: &[Family::Name(name)],
            weight: Weight::NORMAL,
            style: Style::Normal,
            stretch: Stretch::Normal,
        };

        let id = match self.db.query(&query) {
            Some(id) => id,
            None => {
                self.faces.insert(name.to_string(), None);
                return Err(TextError::FontLoadError(format!("Font '{}' not found", name)));
            }
        };

        // Get the font data
        let face = self.load_face_by_id(id)?;
        let face = Arc::new(face);

        // Cache it
        self.faces.insert(name.to_string(), Some(Arc::clone(&face)));

        Ok(face)
    }

    /// Load a font face by fontdb ID
    fn load_face_by_id(&self, id: fontdb::ID) -> Result<FontFace> {
        // Get the face source info
        let (src, face_index) = self.db.face_source(id)
            .ok_or_else(|| TextError::FontLoadError("Font source not found".to_string()))?;

        // Load the font data
        let data = match src {
            Source::File(path) => {
                std::fs::read(&path).map_err(|e| {
                    TextError::FontLoadError(format!("Failed to read font file {:?}: {}", path, e))
                })?
            }
            Source::Binary(arc) => {
                arc.as_ref().as_ref().to_vec()
            }
            Source::SharedFile(_path, data) => {
                data.as_ref().as_ref().to_vec()
            }
        };

        // Create FontFace with the correct index
        FontFace::from_data_with_index(data, face_index)
    }

    /// Load a generic font category
    pub fn load_generic(&mut self, generic: GenericFont) -> Result<Arc<FontFace>> {
        let cache_key = format!("__generic_{:?}", generic);

        // Check cache first (includes failed lookups as None)
        if let Some(cached) = self.faces.get(&cache_key) {
            return cached.clone().ok_or_else(|| {
                TextError::FontLoadError(format!("Generic font {:?} not found (cached)", generic))
            });
        }

        // Map GenericFont to fontdb Family
        let family = match generic {
            GenericFont::System => Family::SansSerif,
            GenericFont::Monospace => Family::Monospace,
            GenericFont::Serif => Family::Serif,
            GenericFont::SansSerif => Family::SansSerif,
        };

        // Query fontdb
        let query = Query {
            families: &[family],
            weight: Weight::NORMAL,
            style: Style::Normal,
            stretch: Stretch::Normal,
        };

        let id = match self.db.query(&query) {
            Some(id) => id,
            None => {
                self.faces.insert(cache_key, None);
                return Err(TextError::FontLoadError(format!("Generic font {:?} not found", generic)));
            }
        };

        let face = self.load_face_by_id(id)?;
        let face = Arc::new(face);

        // Cache it
        self.faces.insert(cache_key, Some(Arc::clone(&face)));

        Ok(face)
    }

    /// Load a font with fallback to generic category
    pub fn load_with_fallback(
        &mut self,
        name: Option<&str>,
        generic: GenericFont,
    ) -> Result<Arc<FontFace>> {
        // Try named font first
        if let Some(name) = name {
            // Check if we've already tried this font (avoid repeated warnings)
            let already_tried = self.faces.contains_key(name);

            tracing::trace!("load_with_fallback: name={}, already_tried={}, cache_size={}", name, already_tried, self.faces.len());

            if let Ok(face) = self.load_font(name) {
                return Ok(face);
            }

            // Only warn on the first failure for this font
            if !already_tried {
                tracing::warn!("Font '{}' not found, falling back to {:?}", name, generic);
            }
        }

        // Fall back to generic
        self.load_generic(generic)
    }

    /// Get cached font by name (doesn't load - for use during render)
    pub fn get_cached(&self, name: &str) -> Option<Arc<FontFace>> {
        self.faces.get(name).and_then(|opt| opt.clone())
    }

    /// Get cached generic font (doesn't load - for use during render)
    pub fn get_cached_generic(&self, generic: GenericFont) -> Option<Arc<FontFace>> {
        let cache_key = format!("__generic_{:?}", generic);
        self.faces.get(&cache_key).and_then(|opt| opt.clone())
    }

    /// Fast font lookup for rendering - only uses cache, never loads
    /// Returns the requested font if cached, or None if loading is needed
    pub fn get_for_render(&self, name: Option<&str>, generic: GenericFont) -> Option<Arc<FontFace>> {
        // Try named font from cache first
        if let Some(name) = name {
            // For named fonts, only return if we have that specific font cached
            // Return None to trigger loading if not found
            return self.get_cached(name);
        }

        // For generic-only requests, use cached generic font
        self.get_cached_generic(generic)
            .or_else(|| self.get_cached_generic(GenericFont::SansSerif))
    }

    /// List available font families on the system
    pub fn list_families(&self) -> Vec<String> {
        let mut families: Vec<String> = self.db.faces()
            .filter_map(|face| {
                face.families.first().map(|(name, _)| name.clone())
            })
            .collect();

        families.sort();
        families.dedup();
        families
    }

    /// Check if a font is available
    pub fn has_font(&self, name: &str) -> bool {
        let query = Query {
            families: &[Family::Name(name)],
            weight: Weight::NORMAL,
            style: Style::Normal,
            stretch: Stretch::Normal,
        };
        self.db.query(&query).is_some()
    }
}

impl Default for FontRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_generic_fonts() {
        let mut registry = FontRegistry::new();

        // Should be able to load generic fonts
        let sans = registry.load_generic(GenericFont::SansSerif);
        assert!(sans.is_ok(), "Should load sans-serif font");

        let mono = registry.load_generic(GenericFont::Monospace);
        assert!(mono.is_ok(), "Should load monospace font");
    }

    #[test]
    fn test_list_families() {
        let registry = FontRegistry::new();
        let families = registry.list_families();
        assert!(!families.is_empty(), "Should find some fonts on the system");
        println!("Found {} font families", families.len());
    }

    #[test]
    fn test_menlo_font_loading() {
        let mut registry = FontRegistry::new();

        // Try to load Menlo
        match registry.load_font("Menlo") {
            Ok(font) => {
                println!("\n=== Menlo Font Info ===");
                println!("Family name: {}", font.family_name());
                println!("Face index: {}", font.face_index());
                println!("Weight: {:?}", font.weight());
                println!("Style: {:?}", font.style());
                println!("Glyph count: {}", font.glyph_count());

                // Test some glyph IDs
                for c in ['A', 'F', 'S', 'M', 'i', 'n', 'l'] {
                    if let Some(id) = font.glyph_id(c) {
                        println!("  '{}' -> glyph_id {}", c, id);
                    } else {
                        println!("  '{}' -> NOT FOUND", c);
                    }
                }
            }
            Err(e) => {
                println!("Failed to load Menlo: {:?}", e);
            }
        }
    }

    #[test]
    fn test_sf_mono_loading() {
        let mut registry = FontRegistry::new();

        // Try to load SF Mono
        match registry.load_font("SF Mono") {
            Ok(font) => {
                println!("\n=== SF Mono Font Info ===");
                println!("Family name: {}", font.family_name());
                println!("Face index: {}", font.face_index());
                println!("Weight: {:?}", font.weight());
                println!("Style: {:?}", font.style());
                println!("Glyph count: {}", font.glyph_count());

                // Test glyph IDs for "SF" - these should NOT be the same as "SI"
                println!("\nGlyph mappings:");
                for c in ['S', 'F', 'I', ' ', 'M', 'o', 'n'] {
                    if let Some(id) = font.glyph_id(c) {
                        println!("  '{}' (U+{:04X}) -> glyph_id {}", c, c as u32, id);
                    } else {
                        println!("  '{}' -> NOT FOUND", c);
                    }
                }
            }
            Err(e) => {
                println!("SF Mono not available: {:?}", e);
            }
        }
    }

    #[test]
    fn test_text_shaping() {
        use crate::shaper::TextShaper;

        let mut registry = FontRegistry::new();
        let shaper = TextShaper::new();

        // Load SF Mono
        let font = match registry.load_font("SF Mono") {
            Ok(f) => f,
            Err(_) => {
                // Fall back to monospace
                registry.load_generic(GenericFont::Monospace).expect("Should load monospace")
            }
        };

        println!("\n=== Testing text shaping for 'SF Mono' ===");
        println!("Using font: {} (face_index={})", font.family_name(), font.face_index());

        // Shape the text "SF"
        let shaped = shaper.shape("SF", &font, 24.0);

        println!("Shaped 'SF' -> {} glyphs:", shaped.glyphs.len());
        for (i, glyph) in shaped.glyphs.iter().enumerate() {
            println!("  [{}] glyph_id={}, x_advance={}, cluster={}",
                i, glyph.glyph_id, glyph.x_advance, glyph.cluster);
        }

        // The glyph IDs for 'S' and 'F' should be different
        if shaped.glyphs.len() >= 2 {
            let s_glyph = shaped.glyphs[0].glyph_id;
            let f_glyph = shaped.glyphs[1].glyph_id;
            println!("\nS glyph_id: {}, F glyph_id: {}", s_glyph, f_glyph);
            assert_ne!(s_glyph, f_glyph, "S and F should have different glyph IDs");
        }
    }

    #[test]
    fn test_full_text_rendering() {
        use crate::renderer::TextRenderer;
        use crate::layout::LayoutOptions;

        let mut renderer = TextRenderer::new();

        // Preload SF Mono
        renderer.preload_fonts(&["SF Mono"]);

        println!("\n=== Testing full text rendering for 'SF Mono' ===");

        // Prepare text through the full pipeline
        let options = LayoutOptions::default();
        let result = renderer.prepare_text_with_font(
            "SF Mono",
            24.0,
            [0.0, 0.0, 0.0, 1.0],
            &options,
            Some("SF Mono"),
            GenericFont::Monospace,
        );

        match result {
            Ok(prepared) => {
                println!("Prepared {} glyphs for 'SF Mono':", prepared.glyphs.len());
                for (i, glyph) in prepared.glyphs.iter().enumerate() {
                    println!("  [{}] bounds=[{:.1}, {:.1}, {:.1}, {:.1}], uv=[{:.4}, {:.4}, {:.4}, {:.4}]",
                        i, glyph.bounds[0], glyph.bounds[1], glyph.bounds[2], glyph.bounds[3],
                        glyph.uv_bounds[0], glyph.uv_bounds[1], glyph.uv_bounds[2], glyph.uv_bounds[3]);
                }
            }
            Err(e) => {
                println!("Error preparing text: {:?}", e);
            }
        }
    }

    #[test]
    fn test_fira_code_loading() {
        let mut registry = FontRegistry::new();

        // Try to load Fira Code
        match registry.load_font("Fira Code") {
            Ok(font) => {
                println!("\n=== Fira Code Font Info ===");
                println!("Family name: {}", font.family_name());
                println!("Face index: {}", font.face_index());
                println!("Weight: {:?}", font.weight());
                println!("Style: {:?}", font.style());
                println!("Glyph count: {}", font.glyph_count());

                // Test glyph IDs - specifically F and B which should be different
                println!("\nGlyph mappings:");
                for c in ['F', 'B', 'i', 'r', 'a', ' ', 'C', 'o', 'd', 'e'] {
                    if let Some(id) = font.glyph_id(c) {
                        println!("  '{}' (U+{:04X}) -> glyph_id {}", c, c as u32, id);
                    } else {
                        println!("  '{}' -> NOT FOUND", c);
                    }
                }
            }
            Err(e) => {
                println!("Fira Code not available: {:?}", e);
            }
        }
    }
}
