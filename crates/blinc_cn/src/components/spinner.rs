//! Spinner component for loading indicators
//!
//! A circular loading indicator: a faint track ring with a half-circle arc
//! turning on top. The arc's colour and its turn are `.cn-spinner__arc` rules
//! (`@keyframes cn-spinner-turn`); the track's colour is `.cn-spinner__track`.
//! A different speed is a rule of your own:
//!
//! ```css
//! .slow-spinner .cn-spinner__arc { animation-duration: 2000ms; }
//! ```
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn loading_view() -> impl ElementBuilder {
//!     cn::spinner()
//! }
//!
//! // Customised
//! cn::spinner()
//!     .size(SpinnerSize::Large)
//!     .color(Color::BLUE)
//!     .class("slow-spinner")
//! ```

use blinc_core::{ClipLength, ClipPath, Color};
use blinc_layout::InstanceKey;
use blinc_layout::div::{Div, ElementTypeId};
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use std::cell::{OnceCell, RefCell};
use std::sync::Arc;
use taffy::Style;

/// Spinner size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpinnerSize {
    /// Small spinner (16px)
    Small,
    /// Medium spinner (24px)
    #[default]
    Medium,
    /// Large spinner (32px)
    Large,
}

impl SpinnerSize {
    fn diameter(&self) -> f32 {
        match self {
            SpinnerSize::Small => 16.0,
            SpinnerSize::Medium => 24.0,
            SpinnerSize::Large => 32.0,
        }
    }

    fn border_width(&self) -> f32 {
        match self {
            SpinnerSize::Small => 2.0,
            SpinnerSize::Medium => 2.5,
            SpinnerSize::Large => 3.0,
        }
    }
}

/// Internal configuration accumulated by `SpinnerBuilder` before the
/// inner element is materialised on first `ElementBuilder` access.
#[derive(Clone, Default)]
struct SpinnerConfig {
    size: SpinnerSize,
    color: Option<blinc_layout::binding::Reactive<Color>>,
    /// The faint full-circle ring behind the arc; `.cn-spinner__track`'s
    /// colour when `None`.
    track_color: Option<blinc_layout::binding::Reactive<Color>>,
    classes: Vec<Arc<str>>,
    user_id: Option<String>,
}

/// Built spinner — wraps the composed inner `Div`. Created by
/// `SpinnerBuilder::get_or_build` on first ElementBuilder access.
pub struct Spinner {
    inner: Div,
}

impl Spinner {
    /// Materialise the spinner tree from accumulated config.
    fn with_config(_key: InstanceKey, config: SpinnerConfig) -> Self {
        let diameter = config.size.diameter();
        let border_width = config.size.border_width();
        let half = diameter / 2.0;

        // A rectangle over the left half, in the arc's own coordinates, so
        // it turns with the arc and shows a half circle going round.
        let polygon = ClipPath::Polygon {
            points: vec![
                (ClipLength::Px(0.0), ClipLength::Px(0.0)),
                (ClipLength::Px(half), ClipLength::Px(0.0)),
                (ClipLength::Px(half), ClipLength::Px(diameter)),
                (ClipLength::Px(0.0), ClipLength::Px(diameter)),
            ],
        };

        // Colours are rules unless set on the builder; a bound colour patches
        // the border in place.
        let mut track = div()
            .class("cn-spinner__track")
            .absolute()
            .inset(0.0)
            .w(diameter)
            .h(diameter)
            .rounded(half)
            .border_width(border_width);
        if let Some(color) = config.track_color {
            track = track.border_color(color);
        }

        let mut arc = div()
            .class("cn-spinner__arc")
            .absolute()
            .inset(0.0)
            .w(diameter)
            .h(diameter)
            .rounded(half)
            .border_width(border_width)
            .clip_path(polygon);
        if let Some(color) = config.color {
            arc = arc.border_color(color);
        }
        let arc_layer = div()
            .absolute()
            .inset(0.0)
            .w(diameter)
            .h(diameter)
            .child(arc);

        // Total container size includes border-stroke padding so the
        // ring isn't clipped by the parent layout.
        let total = diameter + border_width;
        let mut spinner = div()
            .class("cn-spinner")
            .w(total)
            .h(total)
            .relative()
            .child(track)
            .child(arc_layer);

        for c in &config.classes {
            spinner = spinner.class(c.as_ref());
        }
        if let Some(ref id) = config.user_id {
            spinner = spinner.id(id);
        }

        Self { inner: spinner }
    }
}

impl ElementBuilder for Spinner {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Div
    }

    fn layout_style(&self) -> Option<&Style> {
        self.inner.layout_style()
    }

    fn element_classes(&self) -> &[Arc<str>] {
        self.inner.element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }
}

/// Fluent builder for `Spinner`. Implements `ElementBuilder` directly via
/// a lazy `OnceCell<Spinner>`, so the builder can be used inline as a
/// child — no terminal `build_final()` call needed. Same pattern as
/// `SliderBuilder` / `AvatarBuilder` / `CheckboxBuilder`.
pub struct SpinnerBuilder {
    key: InstanceKey,
    config: RefCell<SpinnerConfig>,
    built: OnceCell<Spinner>,
}

impl SpinnerBuilder {
    /// Create a new spinner builder. Uses `#[track_caller]` so each
    /// distinct call-site gets a stable, unique `InstanceKey`.
    #[track_caller]
    pub fn new() -> Self {
        Self {
            key: InstanceKey::new("spinner"),
            config: RefCell::new(SpinnerConfig::default()),
            built: OnceCell::new(),
        }
    }

    /// Set the spinner size.
    pub fn size(self, size: SpinnerSize) -> Self {
        self.config.borrow_mut().size = size;
        self
    }

    /// Set the turning arc's colour; `.cn-spinner__arc`'s otherwise.
    pub fn color(self, color: impl blinc_layout::binding::IntoReactive<Color>) -> Self {
        self.config.borrow_mut().color = Some(color.into_reactive());
        self
    }

    /// Set the track ring's colour; `.cn-spinner__track`'s otherwise. Pass
    /// `Color::TRANSPARENT` to hide the track.
    pub fn track_color(self, color: impl blinc_layout::binding::IntoReactive<Color>) -> Self {
        self.config.borrow_mut().track_color = Some(color.into_reactive());
        self
    }

    /// Add a CSS class for selector matching.
    pub fn class(self, name: impl AsRef<str>) -> Self {
        self.config
            .borrow_mut()
            .classes
            .push(blinc_core::intern::intern(name.as_ref()));
        self
    }

    /// Set the element ID for CSS selector matching.
    pub fn id(self, id: &str) -> Self {
        self.config.borrow_mut().user_id = Some(id.to_string());
        self
    }

    fn get_or_build(&self) -> &Spinner {
        ::blinc_layout::build_once::build_once(&self.built, || {
            let config = self.config.take();
            Spinner::with_config(self.key.clone(), config)
        })
    }
}

impl Default for SpinnerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ElementBuilder for SpinnerBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Div
    }

    fn layout_style(&self) -> Option<&Style> {
        self.get_or_build().layout_style()
    }

    fn element_classes(&self) -> &[Arc<str>] {
        self.get_or_build().element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }
}

/// A turning spinner for work in progress. Its turn and colours are rules.
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// fn loading() -> impl ElementBuilder {
///     cn::spinner()
/// }
///
/// // With customisation
/// cn::spinner().size(SpinnerSize::Large)
/// ```
#[track_caller]
pub fn spinner() -> SpinnerBuilder {
    SpinnerBuilder::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_size_values() {
        assert_eq!(SpinnerSize::Small.diameter(), 16.0);
        assert_eq!(SpinnerSize::Medium.diameter(), 24.0);
        assert_eq!(SpinnerSize::Large.diameter(), 32.0);
    }

    #[test]
    fn test_spinner_border_widths() {
        assert_eq!(SpinnerSize::Small.border_width(), 2.0);
        assert_eq!(SpinnerSize::Medium.border_width(), 2.5);
        assert_eq!(SpinnerSize::Large.border_width(), 3.0);
    }
}
