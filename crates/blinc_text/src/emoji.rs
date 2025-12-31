//! Emoji detection and rendering utilities
//!
//! Provides functions to detect emoji characters in text and render them
//! as RGBA images for display.

use crate::rasterizer::{GlyphFormat, GlyphRasterizer, RasterizedGlyph};
use crate::registry::{FontRegistry, GenericFont};
use crate::shaper::TextShaper;
use crate::{Result, TextError};
use std::sync::{Arc, Mutex};

/// Check if a character is an emoji
///
/// This covers the main emoji Unicode ranges including:
/// - Emoticons (smileys, people)
/// - Symbols and pictographs
/// - Transport and map symbols
/// - Flags (regional indicators)
/// - Dingbats
/// - Miscellaneous symbols
///
/// # Examples
///
/// ```
/// use blinc_text::emoji::is_emoji;
///
/// assert!(is_emoji('😀'));
/// assert!(is_emoji('🎉'));
/// assert!(is_emoji('❤'));
/// assert!(!is_emoji('A'));
/// assert!(!is_emoji('中'));
/// ```
pub fn is_emoji(c: char) -> bool {
    let cp = c as u32;
    matches!(
        cp,
        // === MAIN EMOJI RANGES (Supplementary Multilingual Plane) ===
        // These are ALWAYS emoji - should use color emoji font

        // Emoticons (smileys, faces, gestures)
        0x1F600..=0x1F64F |
        // Miscellaneous Symbols and Pictographs
        0x1F300..=0x1F5FF |
        // Transport and Map Symbols
        0x1F680..=0x1F6FF |
        // Supplemental Symbols and Pictographs
        0x1F900..=0x1F9FF |
        // Symbols and Pictographs Extended-A
        0x1FA00..=0x1FA6F |
        // Symbols and Pictographs Extended-B
        0x1FA70..=0x1FAFF |
        // Regional Indicator Symbols (flags like 🇺🇸)
        0x1F1E0..=0x1F1FF |
        // Playing cards, Mahjong tiles, Dominos
        0x1F000..=0x1F0FF |
        // Enclosed Alphanumeric Supplement (🅰, 🅱, etc.)
        0x1F100..=0x1F1FF |
        // Geometric Shapes Extended
        0x1F780..=0x1F7FF |

        // === BASIC MULTILINGUAL PLANE - SELECTIVE EMOJI ===
        // These ranges contain a MIX of text and emoji characters.
        // We include specific characters that typically render as emoji.

        // Miscellaneous Symbols - common emoji characters only
        // (☀, ☁, ☂, ☃, ☄, ★, ☆, ☎, ☑, ☔, ☕, ☘, ☝, ☠, ☢, ☣, ☦, ☪, ☮, ☯, ☸, ☹, ☺, ♈-♓, ♠-♧, ♨, ♻, ♾, ♿)
        0x2600..=0x26FF |

        // Dingbats - SELECTIVE emoji characters only
        // NOT included: ✓ (U+2713), ✗ (U+2717) - these should render as text with color
        // Included: emoji-style dingbats that typically render in color
        0x2702 | // ✂ Scissors
        0x2705 | // ✅ White Heavy Check Mark (green)
        0x2708..=0x270D | // ✈✉✊✋✌✍
        0x270F | // ✏ Pencil
        0x2712 | // ✒ Black Nib
        0x2714 | // ✔ Heavy Check Mark
        0x2716 | // ✖ Heavy Multiplication X
        0x271D | // ✝ Latin Cross
        0x2721 | // ✡ Star of David
        0x2728 | // ✨ Sparkles
        0x2733..=0x2734 | // ✳✴
        0x2744 | // ❄ Snowflake
        0x2747 | // ❇ Sparkle
        0x274C | // ❌ Cross Mark
        0x274E | // ❎ Cross Mark Button
        0x2753..=0x2755 | // ❓❔❕
        0x2757 | // ❗ Heavy Exclamation Mark
        0x2763..=0x2764 | // ❣❤ Heart
        0x2795..=0x2797 | // ➕➖➗
        0x27A1 | // ➡ Right Arrow
        0x27B0 | // ➰ Curly Loop
        0x27BF | // ➿ Double Curly Loop

        // Miscellaneous Technical - watch, hourglass, keyboard, etc.
        // (⌚, ⌛, ⌨, ⏏, ⏩-⏺)
        0x231A..=0x231B | // Watch, Hourglass
        0x2328 | // Keyboard
        0x23CF | // Eject
        0x23E9..=0x23F3 | // Various media controls
        0x23F8..=0x23FA | // More media controls

        // Enclosed Alphanumerics - Ⓜ️ and other circled characters that are emoji
        0x24C2 | // Ⓜ️ (Metro)

        // Geometric Shapes - only specific emoji ones
        0x25AA..=0x25AB | // Small squares
        0x25B6 | // Play button
        0x25C0 | // Reverse button
        0x25FB..=0x25FE | // Medium/small squares

        // Miscellaneous Symbols and Arrows - specific emoji
        0x2B05..=0x2B07 | // Left/up/down arrows
        0x2B1B..=0x2B1C | // Large squares
        0x2B50 | // Star
        0x2B55 | // Circle

        // Specific other emoji characters
        0x203C | // Double exclamation
        0x2049 | // Exclamation question
        0x2139 | // Information
        0x2194..=0x2199 | // Arrows
        0x21A9..=0x21AA | // Arrows with hook
        0x2934..=0x2935 | // Curved arrows

        // CJK special emoji
        0x3030 | // Wavy dash
        0x303D | // Part alternation mark
        0x3297 | // Circled Ideograph Congratulation
        0x3299   // Circled Ideograph Secret

        // Note: We intentionally EXCLUDE the following ranges as they should render
        // as text with the user's specified color, not as color emoji:
        // - Arrows (0x2190..=0x21FF) - ←, →, ↑, ↓ should be text
        // - Mathematical Operators (0x2200..=0x22FF) - ∞, ≠, ≤, ≥ should be text
        // - Letterlike Symbols (0x2100..=0x214F) - ™, ℠ should be text
        // - Currency Symbols (0x20A0..=0x20CF) - €, £, ¥ should be text
        // - General Punctuation (0x2000..=0x206F) - should be text
        // - Copyright ©, Registered ® (0x00A9, 0x00AE) - should be text
    )
}

/// Check if a string contains any emoji characters
///
/// # Examples
///
/// ```
/// use blinc_text::emoji::contains_emoji;
///
/// assert!(contains_emoji("Hello 😀 World"));
/// assert!(contains_emoji("🎉"));
/// assert!(!contains_emoji("Hello World"));
/// assert!(!contains_emoji(""));
/// ```
pub fn contains_emoji(s: &str) -> bool {
    s.chars().any(is_emoji)
}

/// Count the number of emoji characters in a string
///
/// Note: This counts individual emoji codepoints, not grapheme clusters.
/// Composite emoji (like family emoji) may count as multiple.
pub fn count_emoji(s: &str) -> usize {
    s.chars().filter(|&c| is_emoji(c)).count()
}

/// Check if a character is a skin tone modifier
///
/// Skin tone modifiers are used to change the skin tone of human emoji.
pub fn is_skin_tone_modifier(c: char) -> bool {
    let cp = c as u32;
    matches!(cp, 0x1F3FB..=0x1F3FF)
}

/// Check if a character is a zero-width joiner
///
/// ZWJ is used to combine emoji into sequences (like family emoji).
pub fn is_zwj(c: char) -> bool {
    c == '\u{200D}'
}

/// Check if a character is a variation selector
///
/// Variation selectors modify how the preceding character should be displayed
/// (text style vs emoji style).
pub fn is_variation_selector(c: char) -> bool {
    let cp = c as u32;
    matches!(cp, 0xFE00..=0xFE0F)
}

/// Rendered emoji image data
#[derive(Debug, Clone)]
pub struct EmojiSprite {
    /// RGBA pixel data (4 bytes per pixel)
    pub data: Vec<u8>,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
}

/// Emoji renderer that rasterizes emoji characters as RGBA images
pub struct EmojiRenderer {
    font_registry: Arc<Mutex<FontRegistry>>,
    rasterizer: GlyphRasterizer,
    shaper: TextShaper,
}

impl EmojiRenderer {
    /// Create a new emoji renderer with a shared font registry
    pub fn with_registry(font_registry: Arc<Mutex<FontRegistry>>) -> Self {
        Self {
            font_registry,
            rasterizer: GlyphRasterizer::new(),
            shaper: TextShaper::new(),
        }
    }

    /// Create a new emoji renderer.
    ///
    /// Uses the global shared font registry to minimize memory usage.
    /// Apple Color Emoji alone is 180MB - sharing prevents loading it multiple times.
    pub fn new() -> Self {
        Self {
            font_registry: crate::global_font_registry(),
            rasterizer: GlyphRasterizer::new(),
            shaper: TextShaper::new(),
        }
    }

    /// Render an emoji character as an RGBA sprite
    ///
    /// Returns the emoji as RGBA pixel data that can be used as an image.
    /// The size parameter controls the font size used for rendering.
    pub fn render(&mut self, emoji: char, size: f32) -> Result<EmojiSprite> {
        // Get the emoji font
        let emoji_font = {
            let mut registry = self.font_registry.lock().unwrap();
            registry.load_generic(GenericFont::Emoji)?
        };

        // Get glyph ID for this emoji
        let glyph_id = emoji_font.glyph_id(emoji).ok_or_else(|| {
            TextError::GlyphNotFound(emoji)
        })?;

        if glyph_id == 0 {
            return Err(TextError::GlyphNotFound(emoji));
        }

        // Rasterize as color
        let rasterized = self.rasterizer.rasterize_color(&emoji_font, glyph_id, size)?;

        if rasterized.width == 0 || rasterized.height == 0 {
            return Err(TextError::GlyphNotFound(emoji));
        }

        Ok(EmojiSprite {
            data: rasterized.bitmap,
            width: rasterized.width,
            height: rasterized.height,
        })
    }

    /// Render an emoji string (may contain multi-codepoint emoji like flags)
    ///
    /// For single emoji characters, use `render()` instead as it's more efficient.
    pub fn render_string(&mut self, emoji_str: &str, size: f32) -> Result<EmojiSprite> {
        // Get the emoji font
        let emoji_font = {
            let mut registry = self.font_registry.lock().unwrap();
            registry.load_generic(GenericFont::Emoji)?
        };

        // Shape the emoji string to handle multi-codepoint sequences
        let shaped = self.shaper.shape(emoji_str, &emoji_font, size);

        if shaped.glyphs.is_empty() {
            return Err(TextError::GlyphNotFound(emoji_str.chars().next().unwrap_or(' ')));
        }

        // For now, just render the first glyph
        // TODO: Handle multi-glyph emoji sequences properly
        let glyph = &shaped.glyphs[0];
        let rasterized = self.rasterizer.rasterize_color(&emoji_font, glyph.glyph_id, size)?;

        if rasterized.width == 0 || rasterized.height == 0 {
            return Err(TextError::GlyphNotFound(emoji_str.chars().next().unwrap_or(' ')));
        }

        Ok(EmojiSprite {
            data: rasterized.bitmap,
            width: rasterized.width,
            height: rasterized.height,
        })
    }
}

impl Default for EmojiRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emoji_detection() {
        // Common emoji
        assert!(is_emoji('😀'));
        assert!(is_emoji('🎉'));
        assert!(is_emoji('❤'));
        assert!(is_emoji('✅')); // U+2705 - should be emoji (green checkmark)
        assert!(is_emoji('🚀'));
        assert!(is_emoji('🌍'));

        // Not emoji - these are text dingbats that should use text color
        assert!(!is_emoji('A'));
        assert!(!is_emoji('中'));
        assert!(!is_emoji(' '));
        assert!(!is_emoji('1'));
        assert!(!is_emoji('@'));
        assert!(!is_emoji('✓')); // U+2713 - check mark (text style)
        assert!(!is_emoji('✗')); // U+2717 - ballot x (text style)
    }

    #[test]
    fn test_contains_emoji() {
        assert!(contains_emoji("Hello 😀 World"));
        assert!(contains_emoji("🎉"));
        assert!(contains_emoji("Start 🚀 End"));
        assert!(!contains_emoji("Hello World"));
        assert!(!contains_emoji(""));
        assert!(!contains_emoji("Plain text only"));
    }

    #[test]
    fn test_count_emoji() {
        assert_eq!(count_emoji("😀😀😀"), 3);
        assert_eq!(count_emoji("Hello 😀 World 🎉"), 2);
        assert_eq!(count_emoji("No emoji here"), 0);
        assert_eq!(count_emoji(""), 0);
    }

    #[test]
    fn test_skin_tone_modifier() {
        assert!(is_skin_tone_modifier('\u{1F3FB}')); // Light skin tone
        assert!(is_skin_tone_modifier('\u{1F3FF}')); // Dark skin tone
        assert!(!is_skin_tone_modifier('😀'));
        assert!(!is_skin_tone_modifier('A'));
    }

    #[test]
    fn test_zwj() {
        assert!(is_zwj('\u{200D}'));
        assert!(!is_zwj('😀'));
        assert!(!is_zwj(' '));
    }

    #[test]
    fn test_variation_selector() {
        assert!(is_variation_selector('\u{FE0F}')); // Emoji presentation
        assert!(is_variation_selector('\u{FE0E}')); // Text presentation
        assert!(!is_variation_selector('😀'));
    }

    #[test]
    fn test_flag_emoji() {
        // Regional indicator symbols (used for flags)
        assert!(is_emoji('\u{1F1FA}')); // Regional indicator U
        assert!(is_emoji('\u{1F1F8}')); // Regional indicator S
    }

    #[test]
    fn test_dingbats() {
        assert!(is_emoji('✂')); // Scissors
        assert!(is_emoji('✈')); // Airplane
        assert!(is_emoji('✉')); // Envelope
    }

    #[test]
    fn test_miscellaneous_symbols() {
        assert!(is_emoji('☀')); // Sun
        assert!(is_emoji('☁')); // Cloud
        assert!(is_emoji('☂')); // Umbrella
    }
}
