//! Font registry for system font discovery and caching
//!
//! Uses fontdb to discover and load system fonts by name or generic category.
//! Uses memory mapping to avoid loading large fonts (like 180MB Apple Color Emoji) fully into RAM.
//!
//! # Memory Optimization
//!
//! The registry uses lazy loading to minimize memory usage:
//! - Only known essential system fonts are loaded at startup (by path)
//! - Full system font scan is deferred until a font lookup fails
//! - Emoji/symbol fonts are loaded lazily on first use

use crate::font::{FontData, FontFace};
use crate::{Result, TextError};
use fontdb::{Database, Family, Query, Source, Stretch, Style, Weight};
use rustc_hash::FxHashMap;
use std::path::Path;
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
    /// Emoji font (color emoji)
    Emoji,
    /// Symbol font (Unicode symbols, arrows, math, etc.)
    Symbol,
}

/// Known system font paths for each platform
/// These are loaded directly without scanning all system fonts
#[cfg(target_os = "macos")]
const KNOWN_FONT_PATHS: &[&str] = &[
    // System UI fonts
    "/System/Library/Fonts/SFNS.ttf",      // SF Pro (System)
    "/System/Library/Fonts/SFNSMono.ttf",  // SF Mono
    "/System/Library/Fonts/Helvetica.ttc", // Helvetica
    "/System/Library/Fonts/Times.ttc",     // Times (Serif)
    "/System/Library/Fonts/Menlo.ttc",     // Menlo (Monospace with symbols)
    "/System/Library/Fonts/Monaco.ttf",    // Monaco (Monospace)
    // Common user fonts
    "/Library/Fonts/Arial.ttf",
    "/Library/Fonts/Georgia.ttf",
];

#[cfg(target_os = "windows")]
const KNOWN_FONT_PATHS: &[&str] = &[
    "C:\\Windows\\Fonts\\segoeui.ttf", // Segoe UI (System)
    "C:\\Windows\\Fonts\\consola.ttf", // Consolas (Monospace)
    "C:\\Windows\\Fonts\\arial.ttf",   // Arial (Sans-serif)
    "C:\\Windows\\Fonts\\times.ttf",   // Times New Roman (Serif)
    "C:\\Windows\\Fonts\\cour.ttf",    // Courier New (Monospace)
];

#[cfg(target_os = "android")]
const KNOWN_FONT_PATHS: &[&str] = &[
    // Android system fonts
    "/system/fonts/Roboto-Regular.ttf",
    "/system/fonts/Roboto-Bold.ttf",
    "/system/fonts/Roboto-Medium.ttf",
    "/system/fonts/RobotoMono-Regular.ttf",
    "/system/fonts/DroidSans.ttf",
    "/system/fonts/DroidSansMono.ttf",
    "/system/fonts/DroidSerif-Regular.ttf",
    "/system/fonts/NotoSansCJK-Regular.ttc",
];

#[cfg(target_os = "ios")]
const KNOWN_FONT_PATHS: &[&str] = &[
    // iOS system fonts - Core directory (most reliable)
    "/System/Library/Fonts/Core/SFUI.ttf",           // SF UI (system font)
    "/System/Library/Fonts/Core/SFUIMono.ttf",       // SF Mono
    "/System/Library/Fonts/Core/SFUIItalic.ttf",     // SF Italic
    "/System/Library/Fonts/Core/Helvetica.ttc",      // Helvetica
    "/System/Library/Fonts/Core/HelveticaNeue.ttc",  // Helvetica Neue
    "/System/Library/Fonts/Core/Avenir.ttc",         // Avenir
    "/System/Library/Fonts/Core/AvenirNext.ttc",     // Avenir Next
    "/System/Library/Fonts/Core/Courier.ttc",        // Courier
    "/System/Library/Fonts/Core/CourierNew.ttf",     // Courier New
    // CoreUI fonts
    "/System/Library/Fonts/CoreUI/Menlo.ttc",        // Menlo (monospace)
    "/System/Library/Fonts/CoreUI/SFUIRounded.ttf",  // SF Rounded
    // CoreAddition fonts
    "/System/Library/Fonts/CoreAddition/Georgia.ttf",
    "/System/Library/Fonts/CoreAddition/Arial.ttf",
    "/System/Library/Fonts/CoreAddition/ArialBold.ttf",
    "/System/Library/Fonts/CoreAddition/Verdana.ttf",
    "/System/Library/Fonts/CoreAddition/TimesNewRomanPS.ttf",
];

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "android", target_os = "ios")))]
const KNOWN_FONT_PATHS: &[&str] = &[
    // Linux common paths
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    // Noto fonts
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoMono-Regular.ttf",
];

/// Font registry that discovers and caches system fonts
pub struct FontRegistry {
    /// fontdb database containing system fonts
    db: Database,
    /// Cached FontFace instances (Some = found, None = not found)
    faces: FxHashMap<String, Option<Arc<FontFace>>>,
    /// Whether full system font scan has been performed
    system_fonts_loaded: bool,
}

impl FontRegistry {
    /// Create a new font registry with lazy loading
    ///
    /// Only loads known essential system fonts by path.
    /// Full system font scan is deferred until needed.
    pub fn new() -> Self {
        let mut db = Database::new();

        // Load only known essential fonts by path (fast, minimal memory)
        let mut loaded_count = 0;
        for path in KNOWN_FONT_PATHS {
            if Path::new(path).exists() {
                db.load_font_file(path).ok();
                loaded_count += 1;
            }
        }
        tracing::debug!("Loaded {} known system fonts", loaded_count);

        // Debug: list loaded fonts
        for face in db.faces() {
            for family in face.families.iter() {
                tracing::debug!(
                    "Font loaded: '{}' (family='{}', weight={:?}, style={:?})",
                    face.post_script_name,
                    family.0,
                    face.weight,
                    face.style
                );
            }
        }

        Self {
            db,
            faces: FxHashMap::default(),
            system_fonts_loaded: false,
        }
        // Note: We don't preload generic fonts here anymore.
        // They'll be loaded on first use. This avoids triggering a full
        // system font scan at startup.
    }

    /// Load a font from raw data (e.g., embedded or bundled fonts)
    ///
    /// This is useful for loading fonts that aren't in the standard system paths,
    /// such as app-bundled fonts or fonts loaded via CoreText on iOS.
    ///
    /// Returns the number of font faces loaded from the data.
    pub fn load_font_data(&mut self, data: Vec<u8>) -> usize {
        let before = self.db.faces().count();
        self.db.load_font_data(data);
        let after = self.db.faces().count();
        let loaded = after - before;
        if loaded > 0 {
            tracing::debug!("Loaded {} font faces from data", loaded);
        }
        loaded
    }

    /// Ensure all system fonts are loaded (lazy initialization)
    ///
    /// Called automatically when a font lookup fails.
    /// This scans all system font directories which can be slow.
    fn ensure_system_fonts_loaded(&mut self) {
        if self.system_fonts_loaded {
            return;
        }

        tracing::debug!("Loading all system fonts (lazy scan)...");
        self.db.load_system_fonts();
        self.system_fonts_loaded = true;
        tracing::debug!("System fonts loaded: {} faces", self.db.faces().count());
    }

    /// Preload essential generic font categories at startup.
    ///
    /// Emoji and Symbol fonts are NOT preloaded - they're loaded lazily on first use.
    /// This saves ~180MB of memory if emoji aren't used (Apple Color Emoji is huge).
    fn preload_generic_fonts(&mut self) {
        // Only preload essential text fonts - NOT emoji/symbol
        // Emoji font is 180MB+ on macOS, so lazy loading saves significant memory
        for generic in [
            GenericFont::System,
            GenericFont::SansSerif,
            GenericFont::Serif,
            GenericFont::Monospace,
            // GenericFont::Emoji,  // Lazy loaded on first emoji use
            // GenericFont::Symbol, // Lazy loaded on first symbol use
        ] {
            if let Err(e) = self.load_generic(generic) {
                tracing::warn!("Failed to preload generic font {:?}: {:?}", generic, e);
            }
        }
    }

    /// Preload specific fonts by name with all available variants
    /// (call at startup for fonts your app uses)
    ///
    /// This discovers and loads all variants (bold, italic, etc.) of each font.
    /// Note: This may trigger a full system font scan if the font isn't found
    /// in the known fonts.
    pub fn preload_fonts(&mut self, names: &[&str]) {
        for name in names {
            if self.has_font(name) {
                self.preload_font_family(name);
                tracing::debug!("Preloaded font family with all variants: {}", name);
            } else {
                tracing::debug!("Font not available: {}", name);
            }
        }
    }

    /// Load a font by name (e.g., "Fira Code", "Inter", "Arial")
    pub fn load_font(&mut self, name: &str) -> Result<Arc<FontFace>> {
        self.load_font_with_style(name, 400, false)
    }

    /// Load a font by name with specific weight and italic style
    ///
    /// # Arguments
    /// * `name` - Font family name (e.g., "Fira Code", "Inter")
    /// * `weight` - Font weight (100-900, where 400 is normal, 700 is bold)
    /// * `italic` - Whether to load italic variant
    pub fn load_font_with_style(
        &mut self,
        name: &str,
        weight: u16,
        italic: bool,
    ) -> Result<Arc<FontFace>> {
        // Create cache key that includes weight and style
        let cache_key = format!("{}:w{}:{}", name, weight, if italic { "i" } else { "n" });

        // Check cache first (includes failed lookups as None)
        if let Some(cached) = self.faces.get(&cache_key) {
            return cached.clone().ok_or_else(|| {
                TextError::FontLoadError(format!(
                    "Font '{}' (weight={}, italic={}) not found (cached)",
                    name, weight, italic
                ))
            });
        }

        // Try to find the font (may need to load system fonts lazily)
        let id = self.find_font_id(name, weight, italic);

        // If not found in known fonts, try loading all system fonts
        let id = match id {
            Some(id) => id,
            None if !self.system_fonts_loaded => {
                // Lazy load all system fonts and retry
                self.ensure_system_fonts_loaded();
                match self.find_font_id(name, weight, italic) {
                    Some(id) => id,
                    None => {
                        self.faces.insert(cache_key.clone(), None);
                        return Err(TextError::FontLoadError(format!(
                            "Font '{}' (weight={}, italic={}) not found",
                            name, weight, italic
                        )));
                    }
                }
            }
            None => {
                self.faces.insert(cache_key.clone(), None);
                return Err(TextError::FontLoadError(format!(
                    "Font '{}' (weight={}, italic={}) not found",
                    name, weight, italic
                )));
            }
        };

        // Get the font data
        let face = self.load_face_by_id(id)?;
        let face = Arc::new(face);

        // Cache it
        self.faces.insert(cache_key, Some(Arc::clone(&face)));

        Ok(face)
    }

    /// Find a font ID by name, weight, and italic style
    fn find_font_id(&self, name: &str, weight: u16, italic: bool) -> Option<fontdb::ID> {
        // Query fontdb for the font by family name with requested weight/style
        let query = Query {
            families: &[Family::Name(name)],
            weight: Weight(weight),
            style: if italic { Style::Italic } else { Style::Normal },
            stretch: Stretch::Normal,
        };

        if let Some(id) = self.db.query(&query) {
            return Some(id);
        }

        // Try with Oblique if Italic wasn't found
        if italic {
            let oblique_query = Query {
                families: &[Family::Name(name)],
                weight: Weight(weight),
                style: Style::Oblique,
                stretch: Stretch::Normal,
            };
            if let Some(id) = self.db.query(&oblique_query) {
                return Some(id);
            }
        }

        None
    }

    /// Find a generic font ID by family, weight, and italic style
    fn find_generic_font_id(
        &self,
        family: Family,
        weight: u16,
        italic: bool,
    ) -> Option<fontdb::ID> {
        tracing::debug!(
            "find_generic_font_id: querying family={:?}, weight={}, italic={}, db_faces={}",
            family,
            weight,
            italic,
            self.db.faces().count()
        );

        let query = Query {
            families: &[family],
            weight: Weight(weight),
            style: if italic { Style::Italic } else { Style::Normal },
            stretch: Stretch::Normal,
        };

        if let Some(id) = self.db.query(&query) {
            tracing::debug!("find_generic_font_id: found font id={:?}", id);
            return Some(id);
        }

        tracing::debug!("find_generic_font_id: no font found for family={:?}, trying named fonts", family);

        // Generic family queries may not match fonts loaded by path
        // Try common font names as fallback based on the generic family
        let fallback_names: &[&str] = match family {
            Family::SansSerif => &["Roboto", "SF Pro", "Helvetica", "Arial", "Noto Sans", "DejaVu Sans"],
            Family::Serif => &["Noto Serif", "Times New Roman", "Georgia", "DejaVu Serif"],
            Family::Monospace => &["Roboto Mono", "Droid Sans Mono", "SF Mono", "Menlo", "Consolas", "DejaVu Sans Mono"],
            _ => &[],
        };

        for name in fallback_names {
            let named_query = Query {
                families: &[Family::Name(name)],
                weight: Weight(weight),
                style: if italic { Style::Italic } else { Style::Normal },
                stretch: Stretch::Normal,
            };
            if let Some(id) = self.db.query(&named_query) {
                tracing::debug!("find_generic_font_id: found named font '{}' id={:?}", name, id);
                return Some(id);
            }
        }

        // Try with Oblique if Italic wasn't found
        if italic {
            let oblique_query = Query {
                families: &[family],
                weight: Weight(weight),
                style: Style::Oblique,
                stretch: Stretch::Normal,
            };
            if let Some(id) = self.db.query(&oblique_query) {
                return Some(id);
            }
        }

        None
    }

    /// Load the system emoji font
    ///
    /// Tries platform-specific emoji fonts:
    /// - macOS: Apple Color Emoji
    /// - Windows: Segoe UI Emoji
    /// - Linux: Noto Color Emoji, Noto Emoji, Twitter Color Emoji
    fn load_emoji_font(&mut self) -> Result<Arc<FontFace>> {
        let cache_key = "__generic_Emoji:w400:n".to_string();

        // Check cache first
        if let Some(cached) = self.faces.get(&cache_key) {
            return cached.clone().ok_or_else(|| {
                TextError::FontLoadError("Emoji font not found (cached)".to_string())
            });
        }

        // Platform-specific emoji and symbol font names to try
        // These fonts provide coverage for emoji and special Unicode symbols
        let emoji_fonts = if cfg!(target_os = "macos") {
            vec![
                "Apple Color Emoji", // Color emoji
                "Apple Symbols",     // Unicode symbols (arrows, math, etc.)
            ]
        } else if cfg!(target_os = "windows") {
            vec![
                "Segoe UI Emoji",  // Color emoji
                "Segoe UI Symbol", // Unicode symbols
                "Segoe UI",        // Additional symbol coverage
            ]
        } else {
            // Linux and others
            vec![
                "Noto Color Emoji",   // Color emoji
                "Noto Emoji",         // Monochrome emoji fallback
                "Noto Sans Symbols",  // Unicode symbols
                "Noto Sans Symbols2", // Additional symbols
                "Twitter Color Emoji",
                "EmojiOne Color",
                "JoyPixels",
                "DejaVu Sans", // Good Unicode coverage
            ]
        };

        for font_name in emoji_fonts {
            if let Ok(face) = self.load_font(font_name) {
                // Cache it under the generic emoji key
                self.faces
                    .insert(cache_key.clone(), Some(Arc::clone(&face)));
                tracing::debug!("Loaded emoji font: {}", font_name);
                return Ok(face);
            }
        }

        // No emoji font found
        self.faces.insert(cache_key, None);
        Err(TextError::FontLoadError(
            "No emoji font found on system".to_string(),
        ))
    }

    /// Load the system symbol font
    ///
    /// Tries platform-specific symbol fonts for Unicode symbols:
    /// - macOS: Menlo (has dingbats like ✓✗), Apple Symbols
    /// - Windows: Segoe UI Symbol
    /// - Linux: Noto Sans Symbols, DejaVu Sans
    fn load_symbol_font(&mut self) -> Result<Arc<FontFace>> {
        let cache_key = "__generic_Symbol:w400:n".to_string();

        // Check cache first
        if let Some(cached) = self.faces.get(&cache_key) {
            return cached.clone().ok_or_else(|| {
                TextError::FontLoadError("Symbol font not found (cached)".to_string())
            });
        }

        // Platform-specific symbol font names to try
        // Priority: fonts with good dingbat/symbol coverage (✓, ✗, etc.) first
        let symbol_fonts = if cfg!(target_os = "macos") {
            vec![
                "Menlo",         // Has ✓ ✗ ✔ ✖ and other dingbats
                "Lucida Grande", // Has ✓ and many symbols
                "Apple Symbols", // Unicode symbols (arrows, math, but NOT ✓✗)
            ]
        } else if cfg!(target_os = "windows") {
            vec![
                "Segoe UI Symbol", // Unicode symbols
                "Segoe UI",        // Additional symbol coverage
                "Arial Unicode MS",
            ]
        } else {
            // Linux and others
            vec![
                "DejaVu Sans",        // Good Unicode coverage including dingbats
                "Noto Sans Symbols",  // Unicode symbols
                "Noto Sans Symbols2", // Additional symbols
                "FreeSans",
            ]
        };

        for font_name in symbol_fonts {
            if let Ok(face) = self.load_font(font_name) {
                // Cache it under the generic symbol key
                self.faces
                    .insert(cache_key.clone(), Some(Arc::clone(&face)));
                tracing::debug!("Loaded symbol font: {}", font_name);
                return Ok(face);
            }
        }

        // No symbol font found
        self.faces.insert(cache_key, None);
        Err(TextError::FontLoadError(
            "No symbol font found on system".to_string(),
        ))
    }

    /// Load a font face by fontdb ID
    ///
    /// Uses memory mapping when available to avoid loading large fonts into RAM.
    fn load_face_by_id(&mut self, id: fontdb::ID) -> Result<FontFace> {
        // Try to get shared face data first (memory-mapped if from file)
        // This keeps the mmap alive via Arc, avoiding a copy of the 180MB emoji font
        //
        // SAFETY: We accept the risk of file changes on disk since fonts rarely change
        // and the benefit of not copying 180MB outweighs the risk
        //
        // Note: make_shared_face_data is only available when fontdb has "fs" and "memmap" features,
        // which are enabled by default. If they're disabled, this will fail to compile.
        if let Some((shared_data, face_index)) = unsafe { self.db.make_shared_face_data(id) } {
            let font_data = FontData::from_mapped(shared_data);
            return FontFace::from_font_data(font_data, face_index);
        }

        // Fallback: Get the face source info and load manually
        // This path is taken for Binary sources that don't come from files
        let (src, face_index) = self
            .db
            .face_source(id)
            .ok_or_else(|| TextError::FontLoadError("Font source not found".to_string()))?;

        // Load the font data - use FontData to avoid copying memory-mapped data
        let font_data = match src {
            Source::File(path) => {
                // File source - shouldn't reach here if make_shared_face_data worked
                tracing::warn!("Loading font via fs::read (mmap failed): {:?}", path);
                let data = std::fs::read(&path).map_err(|e| {
                    TextError::FontLoadError(format!("Failed to read font file {:?}: {}", path, e))
                })?;
                FontData::from_vec(data)
            }
            Source::Binary(arc) => {
                // Binary data - wrap the Arc directly without copying
                FontData::from_mapped(arc)
            }
            Source::SharedFile(_path, data) => {
                // Memory-mapped file - use directly without copying!
                // This is the key optimization for large fonts like Apple Color Emoji
                FontData::from_mapped(data)
            }
        };

        // Create FontFace with the memory-mapped or owned data
        FontFace::from_font_data(font_data, face_index)
    }

    /// Load a generic font category
    pub fn load_generic(&mut self, generic: GenericFont) -> Result<Arc<FontFace>> {
        self.load_generic_with_style(generic, 400, false)
    }

    /// Load a generic font category with specific weight and italic style
    ///
    /// # Arguments
    /// * `generic` - Generic font category (System, Monospace, Serif, SansSerif)
    /// * `weight` - Font weight (100-900, where 400 is normal, 700 is bold)
    /// * `italic` - Whether to load italic variant
    pub fn load_generic_with_style(
        &mut self,
        generic: GenericFont,
        weight: u16,
        italic: bool,
    ) -> Result<Arc<FontFace>> {
        let cache_key = format!(
            "__generic_{:?}:w{}:{}",
            generic,
            weight,
            if italic { "i" } else { "n" }
        );

        // Check cache first (includes failed lookups as None)
        if let Some(cached) = self.faces.get(&cache_key) {
            return cached.clone().ok_or_else(|| {
                TextError::FontLoadError(format!(
                    "Generic font {:?} (weight={}, italic={}) not found (cached)",
                    generic, weight, italic
                ))
            });
        }

        // Map GenericFont to fontdb Family
        // For Emoji and Symbol, we try platform-specific fonts by name
        if generic == GenericFont::Emoji {
            return self.load_emoji_font();
        }
        if generic == GenericFont::Symbol {
            return self.load_symbol_font();
        }

        let family = match generic {
            GenericFont::System => Family::SansSerif,
            GenericFont::Monospace => Family::Monospace,
            GenericFont::Serif => Family::Serif,
            GenericFont::SansSerif => Family::SansSerif,
            GenericFont::Emoji => unreachable!(), // Handled above
            GenericFont::Symbol => unreachable!(), // Handled above
        };

        // Try to find the font (may need to load system fonts lazily)
        let id = self.find_generic_font_id(family, weight, italic);

        // If not found in known fonts, try loading all system fonts
        let id = match id {
            Some(id) => id,
            None if !self.system_fonts_loaded => {
                // Lazy load all system fonts and retry
                self.ensure_system_fonts_loaded();
                match self.find_generic_font_id(family, weight, italic) {
                    Some(id) => id,
                    None => {
                        self.faces.insert(cache_key.clone(), None);
                        return Err(TextError::FontLoadError(format!(
                            "Generic font {:?} (weight={}, italic={}) not found",
                            generic, weight, italic
                        )));
                    }
                }
            }
            None => {
                self.faces.insert(cache_key.clone(), None);
                return Err(TextError::FontLoadError(format!(
                    "Generic font {:?} (weight={}, italic={}) not found",
                    generic, weight, italic
                )));
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
        self.load_with_fallback_styled(name, generic, 400, false)
    }

    /// Load a font with fallback to generic category, with specific weight and style
    pub fn load_with_fallback_styled(
        &mut self,
        name: Option<&str>,
        generic: GenericFont,
        weight: u16,
        italic: bool,
    ) -> Result<Arc<FontFace>> {
        // Try named font first
        if let Some(name) = name {
            // Check if we've already tried this font (avoid repeated warnings)
            let cache_key = format!("{}:w{}:{}", name, weight, if italic { "i" } else { "n" });
            let already_tried = self.faces.contains_key(&cache_key);

            tracing::trace!(
                "load_with_fallback_styled: name={}, weight={}, italic={}, already_tried={}, cache_size={}",
                name,
                weight,
                italic,
                already_tried,
                self.faces.len()
            );

            if let Ok(face) = self.load_font_with_style(name, weight, italic) {
                return Ok(face);
            }

            // Only warn on the first failure for this font
            if !already_tried {
                tracing::warn!(
                    "Font '{}' (weight={}, italic={}) not found, falling back to {:?}",
                    name,
                    weight,
                    italic,
                    generic
                );
            }
        }

        // Fall back to generic with same style
        self.load_generic_with_style(generic, weight, italic)
    }

    /// Get cached font by name (doesn't load - for use during render)
    pub fn get_cached(&self, name: &str) -> Option<Arc<FontFace>> {
        // Legacy: check for normal weight/style first
        let cache_key = format!("{}:w400:n", name);
        self.faces.get(&cache_key).and_then(|opt| opt.clone())
    }

    /// Get cached font by name with specific weight and style
    pub fn get_cached_with_style(
        &self,
        name: &str,
        weight: u16,
        italic: bool,
    ) -> Option<Arc<FontFace>> {
        let cache_key = format!("{}:w{}:{}", name, weight, if italic { "i" } else { "n" });
        self.faces.get(&cache_key).and_then(|opt| opt.clone())
    }

    /// Get cached generic font (doesn't load - for use during render)
    pub fn get_cached_generic(&self, generic: GenericFont) -> Option<Arc<FontFace>> {
        // Legacy: check for normal weight/style first
        let cache_key = format!("__generic_{:?}:w400:n", generic);
        self.faces.get(&cache_key).and_then(|opt| opt.clone())
    }

    /// Get cached generic font with specific weight and style
    pub fn get_cached_generic_with_style(
        &self,
        generic: GenericFont,
        weight: u16,
        italic: bool,
    ) -> Option<Arc<FontFace>> {
        let cache_key = format!(
            "__generic_{:?}:w{}:{}",
            generic,
            weight,
            if italic { "i" } else { "n" }
        );
        self.faces.get(&cache_key).and_then(|opt| opt.clone())
    }

    /// Fast font lookup for rendering - only uses cache, never loads
    /// Returns the requested font if cached, or None if loading is needed
    pub fn get_for_render(
        &self,
        name: Option<&str>,
        generic: GenericFont,
    ) -> Option<Arc<FontFace>> {
        self.get_for_render_with_style(name, generic, 400, false)
    }

    /// Fast font lookup for rendering with specific weight and style
    pub fn get_for_render_with_style(
        &self,
        name: Option<&str>,
        generic: GenericFont,
        weight: u16,
        italic: bool,
    ) -> Option<Arc<FontFace>> {
        // Try named font from cache first
        if let Some(name) = name {
            // For named fonts, only return if we have that specific font cached
            // Return None to trigger loading if not found
            return self.get_cached_with_style(name, weight, italic);
        }

        // For generic-only requests, use cached generic font with style
        self.get_cached_generic_with_style(generic, weight, italic)
            .or_else(|| self.get_cached_generic_with_style(GenericFont::SansSerif, weight, italic))
    }

    /// Get the emoji font if available (cached)
    ///
    /// Returns the cached emoji font if it was successfully loaded during
    /// initialization, or None if no emoji font is available.
    pub fn get_emoji_font(&self) -> Option<Arc<FontFace>> {
        self.get_cached_generic(GenericFont::Emoji)
    }

    /// Check if a character needs an emoji font
    ///
    /// Returns true for emoji characters that typically need a color emoji font.
    pub fn needs_emoji_font(c: char) -> bool {
        crate::emoji::is_emoji(c)
    }

    /// List available font families on the system
    ///
    /// Note: This triggers a full system font scan if not already done.
    pub fn list_families(&mut self) -> Vec<String> {
        // Ensure all system fonts are loaded for a complete list
        self.ensure_system_fonts_loaded();

        let mut families: Vec<String> = self
            .db
            .faces()
            .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
            .collect();

        families.sort();
        families.dedup();
        families
    }

    /// Check if a font is available
    ///
    /// Note: This may trigger a full system font scan if the font
    /// isn't found in the initially loaded fonts.
    pub fn has_font(&mut self, name: &str) -> bool {
        let query = Query {
            families: &[Family::Name(name)],
            weight: Weight::NORMAL,
            style: Style::Normal,
            stretch: Stretch::Normal,
        };

        // Check in already loaded fonts first
        if self.db.query(&query).is_some() {
            return true;
        }

        // If not found and we haven't loaded all system fonts, try that
        if !self.system_fonts_loaded {
            self.ensure_system_fonts_loaded();
            return self.db.query(&query).is_some();
        }

        false
    }

    /// Preload all variants (weights and styles) of a font family
    ///
    /// This discovers all available variants of the font using fontdb
    /// and loads each one into the cache.
    pub fn preload_font_family(&mut self, name: &str) {
        // Find all faces that belong to this font family
        let face_ids: Vec<_> = self
            .db
            .faces()
            .filter(|face| {
                face.families
                    .iter()
                    .any(|(family_name, _)| family_name == name)
            })
            .map(|face| (face.id, face.weight.0, face.style))
            .collect();

        // Load each variant
        for (id, weight, style) in face_ids {
            let italic = matches!(style, Style::Italic | Style::Oblique);
            let cache_key = format!("{}:w{}:{}", name, weight, if italic { "i" } else { "n" });

            // Skip if already cached
            if self.faces.contains_key(&cache_key) {
                continue;
            }

            // Load the face
            match self.load_face_by_id(id) {
                Ok(face) => {
                    self.faces.insert(cache_key, Some(Arc::new(face)));
                }
                Err(e) => {
                    tracing::warn!("Failed to load font variant {}: {:?}", cache_key, e);
                    self.faces.insert(cache_key, None);
                }
            }
        }
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

        // Try to load generic fonts - may not be available in minimal CI environments
        let sans = registry.load_generic(GenericFont::SansSerif);
        let mono = registry.load_generic(GenericFont::Monospace);

        // At least one generic font should be available on most systems
        if sans.is_err() && mono.is_err() {
            println!("No generic fonts available - skipping test (CI environment)");
            return;
        }

        // If we have fonts, verify they loaded correctly
        if let Ok(font) = sans {
            println!("Loaded sans-serif: {}", font.family_name());
        }
        if let Ok(font) = mono {
            println!("Loaded monospace: {}", font.family_name());
        }
    }

    #[test]
    fn test_list_families() {
        let mut registry = FontRegistry::new();
        let families = registry.list_families();
        // May be empty in minimal CI environments without fonts
        println!("Found {} font families", families.len());
        if families.is_empty() {
            println!("No fonts found - likely minimal CI environment");
        }
    }

    #[test]
    fn test_check_cross_glyph_coverage() {
        let mut registry = FontRegistry::new();

        // Test characters: check and cross marks
        let test_chars = ['✓', '✗', '✔', '✖'];

        println!("\n=== Testing glyph coverage for check/cross marks ===\n");

        // Test common fonts
        let fonts_to_test = [
            "Arial",
            "Helvetica",
            "Helvetica Neue",
            "SF Pro",
            "SF Pro Text",
            "Menlo",
            "Monaco",
            "Lucida Grande",
            "Times New Roman",
            "Apple Symbols",
            "Apple Color Emoji",
        ];

        for font_name in fonts_to_test {
            match registry.load_font(font_name) {
                Ok(font) => {
                    print!("{:20} ", font_name);
                    for c in test_chars {
                        let glyph_id = font.glyph_id(c);
                        match glyph_id {
                            Some(id) if id > 0 => print!("'{}':✓({}) ", c, id),
                            _ => print!("'{}':✗     ", c),
                        }
                    }
                    println!();
                }
                Err(_) => {
                    println!("{:20} NOT AVAILABLE", font_name);
                }
            }
        }

        // Test generic fonts
        println!("\n--- Generic Fonts ---");

        if let Ok(font) = registry.load_generic(GenericFont::System) {
            print!("{:20} ", format!("System ({})", font.family_name()));
            for c in test_chars {
                let glyph_id = font.glyph_id(c);
                match glyph_id {
                    Some(id) if id > 0 => print!("'{}':✓({}) ", c, id),
                    _ => print!("'{}':✗     ", c),
                }
            }
            println!();
        }

        if let Ok(font) = registry.load_generic(GenericFont::Symbol) {
            print!("{:20} ", format!("Symbol ({})", font.family_name()));
            for c in test_chars {
                let glyph_id = font.glyph_id(c);
                match glyph_id {
                    Some(id) if id > 0 => print!("'{}':✓({}) ", c, id),
                    _ => print!("'{}':✗     ", c),
                }
            }
            println!();
        }
    }

    #[test]
    fn test_list_all_fonts_with_check() {
        let mut registry = FontRegistry::new();
        let families = registry.list_families();

        println!("\n=== Fonts with ✓ (U+2713) glyph ===\n");

        let mut fonts_with_check = Vec::new();
        for family in &families {
            if let Ok(font) = registry.load_font(family) {
                if let Some(gid) = font.glyph_id('✓') {
                    if gid > 0 {
                        fonts_with_check.push((family.clone(), gid));
                    }
                }
            }
        }

        for (name, gid) in &fonts_with_check {
            println!("{}: glyph_id={}", name, gid);
        }
        println!("\nTotal fonts with ✓: {}", fonts_with_check.len());
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

        // Try to load a font - SF Mono, then monospace, then any available
        let font = match registry.load_font("SF Mono") {
            Ok(f) => f,
            Err(_) => match registry.load_generic(GenericFont::Monospace) {
                Ok(f) => f,
                Err(_) => match registry.load_generic(GenericFont::SansSerif) {
                    Ok(f) => f,
                    Err(_) => {
                        println!("No fonts available - skipping test (CI environment)");
                        return;
                    }
                },
            },
        };

        println!("\n=== Testing text shaping ===");
        println!(
            "Using font: {} (face_index={})",
            font.family_name(),
            font.face_index()
        );

        // Shape the text "SF"
        let shaped = shaper.shape("SF", &font, 24.0);

        println!("Shaped 'SF' -> {} glyphs:", shaped.glyphs.len());
        for (i, glyph) in shaped.glyphs.iter().enumerate() {
            println!(
                "  [{}] glyph_id={}, x_advance={}, cluster={}",
                i, glyph.glyph_id, glyph.x_advance, glyph.cluster
            );
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
        use crate::layout::LayoutOptions;
        use crate::renderer::TextRenderer;

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
