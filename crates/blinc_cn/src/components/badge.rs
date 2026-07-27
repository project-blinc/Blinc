//! Badge component for status indicators
//!
//! Small labeled indicators for status, counts, or categories. Two
//! axes of customization:
//!
//! * `BadgeVariant` — semantic colour (Default / Secondary / Success
//!   / Warning / Destructive).
//! * `BadgeStyle` — visual treatment (`Soft` is the default — pale
//!   tinted bg with same-hue text, matching the Alert component;
//!   `Solid` is the legacy filled style; `Outline` is border-only).
//!
//! Supports icons via `.icon(...)` and `.icon_position(...)`,
//! mirroring the Button API.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Default badge (Soft style, Default variant)
//! cn::badge("New")
//!
//! // Semantic variants — all default to Soft style
//! cn::badge("Success").variant(BadgeVariant::Success)
//! cn::badge("Warning").variant(BadgeVariant::Warning)
//! cn::badge("Error").variant(BadgeVariant::Destructive)
//!
//! // Style opt-ins
//! cn::badge("Solid").style(BadgeStyle::Solid)
//! cn::badge("Outline").style(BadgeStyle::Outline)
//!
//! // With icon — pass any ElementBuilder. CSS rules under
//! // `.cn-badge--{style}-{variant} svg` set the fill to the variant's
//! // colour automatically, so the icon stays in sync with the badge
//! // tint without the caller passing it through. Inline `.color(...)`
//! // on the icon still wins if you want a one-off override.
//! cn::badge("Shipped")
//!     .variant(BadgeVariant::Success)
//!     .icon(svg(CHECK_SVG).size(12.0, 12.0))
//! ```

use std::ops::{Deref, DerefMut};

use blinc_layout::div::{Div, ElementBuilder, ElementTypeId};
use blinc_layout::prelude::*;
use blinc_theme::ColorToken;
use blinc_theme::ThemeState;

pub use super::button::IconPosition;

/// Badge semantic colour variant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeVariant {
    /// Default - primary accent (blue)
    #[default]
    Default,
    /// Secondary - muted neutral
    Secondary,
    /// Success - green
    Success,
    /// Warning - yellow/orange
    Warning,
    /// Destructive - red
    Destructive,
}

/// Badge visual treatment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeStyle {
    /// Soft tinted background + same-hue text (matches Alert) — the
    /// modern default.
    #[default]
    Soft,
    /// Solid filled background + inverse text (the legacy style).
    Solid,
    /// Transparent background + variant-coloured border + variant-
    /// coloured text.
    Outline,
}

impl BadgeStyle {
    fn css_suffix(self) -> &'static str {
        match self {
            BadgeStyle::Soft => "soft",
            BadgeStyle::Solid => "solid",
            BadgeStyle::Outline => "outline",
        }
    }
}

impl BadgeVariant {
    fn css_suffix(self) -> &'static str {
        match self {
            BadgeVariant::Default => "default",
            BadgeVariant::Secondary => "secondary",
            BadgeVariant::Success => "success",
            BadgeVariant::Warning => "warning",
            BadgeVariant::Destructive => "destructive",
        }
    }
}

/// The theme colour `.cn-badge--{style}-{variant}` sets as `color`.
///
/// Mirrors the stylesheet: change one and change the other, or a bound
/// label drifts from a static one.
fn label_color_token(style: BadgeStyle, variant: BadgeVariant) -> ColorToken {
    match (style, variant) {
        (BadgeStyle::Solid, _) => ColorToken::TextInverse,
        (_, BadgeVariant::Default) => ColorToken::Primary,
        (BadgeStyle::Soft, BadgeVariant::Secondary) => ColorToken::TextSecondary,
        (BadgeStyle::Outline, BadgeVariant::Secondary) => ColorToken::TextPrimary,
        (_, BadgeVariant::Success) => ColorToken::Success,
        (_, BadgeVariant::Warning) => ColorToken::Warning,
        (_, BadgeVariant::Destructive) => ColorToken::Error,
    }
}

/// Badge component for status indicators.
///
/// Implements `Deref` to `Div` for full layout customisation. Use
/// `.variant(...)`, `.style(...)`, `.icon(...)` to configure.
pub struct Badge {
    inner: Div,
    label: String,
    variant: BadgeVariant,
    style: BadgeStyle,
    icon: Option<Box<dyn ElementBuilder>>,
    icon_position: IconPosition,
}

impl Badge {
    /// Create a new badge with text. Defaults: `BadgeVariant::Default`,
    /// `BadgeStyle::Soft`, no icon.
    pub fn new(label: impl Into<String>) -> Self {
        Self::rebuild(
            label.into(),
            BadgeVariant::default(),
            BadgeStyle::default(),
            None,
            IconPosition::default(),
        )
    }

    fn rebuild(
        label: String,
        variant: BadgeVariant,
        style: BadgeStyle,
        icon: Option<Box<dyn ElementBuilder>>,
        icon_position: IconPosition,
    ) -> Self {
        let variant_class = format!("cn-badge--{}-{}", style.css_suffix(), variant.css_suffix());

        // Content row: optional icon + label. The icon's colour
        // comes from CSS descendant rules
        // (`.cn-badge--{style}-{variant} svg { fill: ... }`), so we
        // don't tint it from Rust.
        let label_text = text(&label).medium();
        let badge_body = div()
            .class("cn-badge")
            .class(&variant_class)
            .items_center()
            .justify_center();
        let badge_body = match icon {
            Some(icon_el) => {
                let row = div().flex_row().items_center().justify_center().gap_px(4.0);
                let row = match icon_position {
                    IconPosition::Start => row.child_box(icon_el).child(label_text),
                    IconPosition::End => row.child(label_text).child_box(icon_el),
                };
                badge_body.child(row)
            }
            None => badge_body.child(label_text),
        };

        Self {
            inner: badge_body,
            label,
            variant,
            style,
            // The icon's been consumed into `inner`. Subsequent
            // builder methods re-call `rebuild` with `icon: None`,
            // which means `.icon(...)` should typically be last in
            // the chain (or the icon will be lost on the next
            // method call).
            icon: None,
            icon_position,
        }
    }

    /// Bind the chip's text to a reactive source.
    ///
    /// Text content has no property writer -- a changed label needs a
    /// new text node -- so a bound label wraps the chip in a
    /// `Stateful` subscribed to the source. A `Const` needs none of
    /// that and rebuilds in place, exactly as `new` does.
    pub fn reactive_label(self, label: blinc_layout::binding::Reactive<String>) -> Self {
        use crate::reactive_props::{clone_reactive, current, dep_signal};
        use blinc_layout::stateful::{ButtonState, stateful};

        let (variant, style, icon_position) = (self.variant, self.style, self.icon_position);
        let Some(sig) = dep_signal(&label) else {
            return Self::rebuild(current(&label), variant, style, None, icon_position);
        };

        // The class stays on a STABLE node outside the stateful, and
        // only the text is rebuilt inside it. The chip's whole
        // appearance -- background, border, radius, and the text colour
        // the label inherits -- comes from
        // `.cn-badge--{style}-{variant}`, and that cascade only reaches
        // content the stylesheet pass can see as a descendant. Building
        // the classed node inside the callback put it behind the
        // stateful boundary, so the label rendered in the default text
        // colour instead of the variant's.
        let src = clone_reactive(&label);
        let variant_class = format!("cn-badge--{}-{}", style.css_suffix(), variant.css_suffix());
        // The label's colour is set in Rust rather than left to the
        // cascade. A static chip's text is a plain child of the classed
        // node and inherits `color` from it; a rebuilt label is created
        // by the stateful's callback, so whether it carries the
        // variant's colour depends on a stylesheet pass having run over
        // it since. That is why the chip rendered in the default text
        // colour until some unrelated interaction happened to trigger
        // one. Reading the token here makes the rebuilt label
        // self-sufficient. The trade-off is that a CSS override of
        // `.cn-badge--{style}-{variant} { color }` no longer reaches a
        // BOUND label -- the same trade-off cn::Button already makes for
        // its own label.
        let fg = ThemeState::get().color(label_color_token(style, variant));
        // Same helper every other reactive text prop uses, so the
        // wrapper's sizing and the "set your own colour" rule live in
        // one place.
        let label_slot = crate::reactive_props::reactive_text(&src, move |t| t.medium().color(fg));
        Self {
            inner: div()
                .class("cn-badge")
                .class(&variant_class)
                .items_center()
                .justify_center()
                .child_box(label_slot),
            label: current(&label),
            variant,
            style,
            icon: None,
            icon_position,
        }
    }

    /// Set the badge variant (semantic colour).
    pub fn variant(self, variant: BadgeVariant) -> Self {
        Self::rebuild(
            self.label,
            variant,
            self.style,
            self.icon,
            self.icon_position,
        )
    }

    /// Set the badge visual treatment (Soft / Solid / Outline).
    pub fn style(self, style: BadgeStyle) -> Self {
        Self::rebuild(
            self.label,
            self.variant,
            style,
            self.icon,
            self.icon_position,
        )
    }

    /// Add a leading or trailing icon. Accepts any `ElementBuilder`
    /// — typically `svg(MY_SVG).size(w, h)`. The badge's CSS rules
    /// (`.cn-badge--{style}-{variant} svg { fill: var(--variant); }`)
    /// tint the icon to match the variant automatically; pass an
    /// inline `.color(...)` on the icon if you need to override
    /// for a one-off.
    pub fn icon<E>(self, icon: E) -> Self
    where
        E: ElementBuilder + 'static,
    {
        Self::rebuild(
            self.label,
            self.variant,
            self.style,
            Some(Box::new(icon)),
            self.icon_position,
        )
    }

    /// Position the icon before (default) or after the label.
    pub fn icon_position(self, position: IconPosition) -> Self {
        Self::rebuild(self.label, self.variant, self.style, self.icon, position)
    }

    /// Add a CSS class for selector matching.
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.inner = self.inner.class(name);
        self
    }

    /// Set the element ID for CSS selector matching.
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
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

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Create a badge with text. See [`Badge`] for the full API.
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
        let _ = badge("Destructive").variant(BadgeVariant::Destructive);
    }

    #[test]
    fn test_badge_styles() {
        init_theme();
        let _ = badge("Soft").style(BadgeStyle::Soft);
        let _ = badge("Solid").style(BadgeStyle::Solid);
        let _ = badge("Outline").style(BadgeStyle::Outline);
    }

    #[test]
    fn test_badge_icon() {
        init_theme();
        let _ = badge("Shipped")
            .variant(BadgeVariant::Success)
            .icon(text("✓"));
        let _ = badge("End")
            .icon(text("→"))
            .icon_position(IconPosition::End);
    }
}
