//! How a text input is sized and what its text is coloured.
//!
//! The box (fill, border, ring) is the `.blinc-input` rules of
//! [`crate::ua_styles::UA_STYLES`].

use super::*;

#[derive(Clone, Debug)]
pub struct TextInputConfig {
    pub width: f32,
    pub height: f32,
    pub use_full_width: bool,
    pub font_size: f32,
    pub text_color: Color,
    pub placeholder_color: Color,
    pub cursor_color: Color,
    pub selection_color: Color,
    pub corner_radius: f32,
    pub border_width: f32,
    pub padding_x: f32,
    pub placeholder: String,
    /// Horizontal alignment of the visible text inside the field.
    /// Default `Left`. `Center` is the canonical choice for
    /// numeric / OTP-style fields where the value should sit
    /// visually centred in a fixed-width cell.
    pub text_align: blinc_core::TextAlign,
}

impl Default for TextInputConfig {
    fn default() -> Self {
        let theme = ThemeState::get();
        Self {
            width: 200.0,
            height: 44.0,
            use_full_width: false,
            font_size: 16.0,
            text_color: theme.color(ColorToken::TextPrimary),
            placeholder_color: theme.color(ColorToken::TextTertiary),
            cursor_color: theme.color(ColorToken::Accent),
            selection_color: theme.color(ColorToken::Selection),
            corner_radius: 8.0,
            border_width: 1.5,
            padding_x: 12.0,
            placeholder: String::new(),
            text_align: blinc_core::TextAlign::Left,
        }
    }
}

// =============================================================================
// TextInput Widget
// =============================================================================

/// Callback type for on_change events
pub type OnChangeCallback = Arc<dyn Fn(&str) + Send + Sync>;
