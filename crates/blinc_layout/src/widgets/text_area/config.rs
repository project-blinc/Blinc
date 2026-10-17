//! How a text area is sized and coloured.

use super::*;

/// TextArea configuration
#[derive(Clone)]
pub struct TextAreaConfig {
    /// Placeholder text shown when empty
    pub placeholder: String,
    /// Width of the text area (can be overridden by cols)
    pub width: f32,
    /// Height of the text area (can be overridden by rows)
    pub height: f32,
    /// Number of visible rows (overrides height if set)
    pub rows: Option<usize>,
    /// Number of visible columns/character width (overrides width if set)
    pub cols: Option<usize>,
    /// Font size
    pub font_size: f32,
    /// Line height multiplier
    pub line_height: f32,
    /// Approximate character width in ems (for cols calculation)
    pub char_width_ratio: f32,
    /// Text color
    pub text_color: Color,
    /// Placeholder text color
    pub placeholder_color: Color,
    /// Background color
    pub bg_color: Color,
    /// Hovered background color
    pub hover_bg_color: Color,
    /// Focused background color
    pub focused_bg_color: Color,
    /// Border color
    pub border_color: Color,
    /// Hovered border color
    pub hover_border_color: Color,
    /// Focused border color
    pub focused_border_color: Color,
    /// Border width
    pub border_width: f32,
    /// Corner radius
    pub corner_radius: f32,
    /// Horizontal padding
    pub padding_x: f32,
    /// Vertical padding
    pub padding_y: f32,
    /// Cursor color
    pub cursor_color: Color,
    /// Selection color
    pub selection_color: Color,
    /// Whether the text area is disabled
    pub disabled: bool,
    /// Maximum character count (0 = unlimited)
    pub max_length: usize,
    /// Whether text wraps at container bounds (default: true)
    /// When true, long lines wrap to the next visual line.
    /// When false, content scrolls horizontally.
    pub wrap: bool,
    /// Whether `width` was asked for by the caller (`w`, `cols`, `size`,
    /// `text_size`) rather than measured after layout.
    ///
    /// `width` doubles as the text-wrapping width, so the layout bounds
    /// callback keeps it in sync with the box the widget actually got.
    /// Emitting that measured value as a fixed style width is what must
    /// not happen: it equals the parent's available width by
    /// construction, and a child exactly filling its parent tips taffy
    /// into sizing the ancestor by min-content instead of its children.
    /// Unset, the wrapper stays auto-width and stretches, which is both
    /// correct on the first frame and immune to that edge.
    pub width_is_explicit: bool,
}

impl Default for TextAreaConfig {
    fn default() -> Self {
        let theme = ThemeState::get();
        Self {
            placeholder: String::new(),
            width: 300.0,
            width_is_explicit: false,
            height: 120.0,
            rows: None,
            cols: None,
            font_size: 14.0,
            line_height: 1.4,
            char_width_ratio: 0.6,
            text_color: theme.color(ColorToken::TextPrimary),
            placeholder_color: theme.color(ColorToken::TextTertiary),
            bg_color: theme.color(ColorToken::InputBg),
            hover_bg_color: theme.color(ColorToken::InputBgHover),
            focused_bg_color: theme.color(ColorToken::InputBgFocus),
            border_color: theme.color(ColorToken::BorderSecondary),
            hover_border_color: theme.color(ColorToken::BorderHover),
            focused_border_color: theme.color(ColorToken::BorderFocus),
            border_width: 1.5,
            corner_radius: 8.0,
            padding_x: 12.0,
            padding_y: 10.0,
            cursor_color: theme.color(ColorToken::Accent),
            selection_color: theme.color(ColorToken::Selection),
            disabled: false,
            max_length: 0,
            // Default to wrapping since we now have visual lines computed for proper
            // cursor tracking. Visual lines are computed in the callback and used for
            // both rendering and cursor positioning to ensure they match.
            wrap: true,
        }
    }
}

impl TextAreaConfig {
    /// Calculate the effective width based on cols or explicit width
    pub fn effective_width(&self) -> f32 {
        if let Some(cols) = self.cols {
            let char_width = self.font_size * self.char_width_ratio;
            cols as f32 * char_width + self.padding_x * 2.0 + self.border_width * 2.0
        } else {
            self.width
        }
    }

    /// Calculate the effective height based on rows or explicit height
    pub fn effective_height(&self) -> f32 {
        if let Some(rows) = self.rows {
            let single_line_height = self.font_size * self.line_height;
            rows as f32 * single_line_height + self.padding_y * 2.0 + self.border_width * 2.0
        } else {
            self.height
        }
    }
}
