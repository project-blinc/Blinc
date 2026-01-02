//! Badge component for status indicators
//!
//! Small labeled indicators for status, counts, or categories.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Default badge
//! cn::badge("New")
//!
//! // Variant badges
//! cn::badge("Success").variant(BadgeVariant::Success)
//! cn::badge("Warning").variant(BadgeVariant::Warning)
//! cn::badge("Error").variant(BadgeVariant::Destructive)
//!
//! // Outline badge
//! cn::badge("Draft").variant(BadgeVariant::Outline)
//! ```

use std::ops::{Deref, DerefMut};

use blinc_core::Color;
use blinc_layout::div::{Div, ElementBuilder, ElementTypeId};
use blinc_layout::prelude::*;
use blinc_theme::{ColorToken, RadiusToken, SpacingToken, ThemeState, TypographyToken};

/// Badge visual variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeVariant {
    /// Default badge - primary color
    #[default]
    Default,
    /// Secondary badge - muted
    Secondary,
    /// Success badge - green
    Success,
    /// Warning badge - yellow/orange
    Warning,
    /// Destructive badge - red
    Destructive,
    /// Outline badge - border only
    Outline,
}

impl BadgeVariant {
    fn background(&self, theme: &ThemeState) -> Color {
        match self {
            BadgeVariant::Default => theme.color(ColorToken::Primary),
            BadgeVariant::Secondary => theme.color(ColorToken::Secondary),
            BadgeVariant::Success => theme.color(ColorToken::Success),
            BadgeVariant::Warning => theme.color(ColorToken::Warning),
            BadgeVariant::Destructive => theme.color(ColorToken::Error),
            BadgeVariant::Outline => Color::TRANSPARENT,
        }
    }

    fn foreground(&self, theme: &ThemeState) -> Color {
        match self {
            BadgeVariant::Default
            | BadgeVariant::Secondary
            | BadgeVariant::Success
            | BadgeVariant::Warning
            | BadgeVariant::Destructive => theme.color(ColorToken::TextInverse),
            BadgeVariant::Outline => theme.color(ColorToken::TextPrimary),
        }
    }

    fn border(&self, theme: &ThemeState) -> Option<Color> {
        match self {
            BadgeVariant::Outline => Some(theme.color(ColorToken::Border)),
            _ => None,
        }
    }
}

/// Badge component for status indicators
///
/// Implements `Deref` to `Div` for full customization.
pub struct Badge {
    inner: Div,
    label: String,
    variant: BadgeVariant,
}

impl Badge {
    /// Create a new badge with text
    pub fn new(label: impl Into<String>) -> Self {
        Self::with_variant(label, BadgeVariant::default())
    }

    fn with_variant(label: impl Into<String>, variant: BadgeVariant) -> Self {
        let theme = ThemeState::get();
        let label = label.into();

        let bg = variant.background(&theme);
        let fg = variant.foreground(&theme);
        let border = variant.border(&theme);

        let padding_x = theme.spacing_value(SpacingToken::Space2_5); // 10px
        let padding_y = theme.spacing_value(SpacingToken::Space0_5); // 2px
        let radius = theme.radius(RadiusToken::Full); // Pill shape
        let font_size = theme.typography().get(TypographyToken::TextXs);

        let mut badge = div()
            .bg(bg)
            .padding_x_px(padding_x)
            .padding_y_px(padding_y)
            .rounded(radius)
            .items_center()
            .justify_center()
            .child(text(&label).size(font_size).color(fg).medium());

        if let Some(border_color) = border {
            badge = badge.border(1.0, border_color);
        }

        Self {
            inner: badge,
            label,
            variant,
        }
    }

    /// Set the badge variant
    pub fn variant(self, variant: BadgeVariant) -> Self {
        Self::with_variant(self.label, variant)
    }
}

impl Deref for Badge {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Badge {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for Badge {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(&self.inner)
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementBuilder::element_type_id(&self.inner)
    }
}

/// Create a badge with text
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// cn::badge("New")
///     .variant(BadgeVariant::Success)
/// ```
pub fn badge(label: impl Into<String>) -> Badge {
    Badge::new(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_theme() {
        let _ = ThemeState::try_get().unwrap_or_else(|| {
            ThemeState::init_default();
            ThemeState::get()
        });
    }

    #[test]
    fn test_badge_default() {
        init_theme();
        let _ = badge("Test");
    }

    #[test]
    fn test_badge_variants() {
        init_theme();
        let _ = badge("Default").variant(BadgeVariant::Default);
        let _ = badge("Secondary").variant(BadgeVariant::Secondary);
        let _ = badge("Success").variant(BadgeVariant::Success);
        let _ = badge("Warning").variant(BadgeVariant::Warning);
        let _ = badge("Error").variant(BadgeVariant::Destructive);
        let _ = badge("Outline").variant(BadgeVariant::Outline);
    }
}
