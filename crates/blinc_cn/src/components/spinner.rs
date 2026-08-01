//! Spinner component for loading indicators
//!
//! A circular loading indicator that renders as a static track ring with a
//! motion-bound, polygon-masked 180° arc rotating on top. The timeline is
//! constructed internally — callers just write `cn::spinner()`.
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
//!     .duration_ms(2000)
//! ```

use blinc_animation::{AnimatedTimeline, SharedAnimatedTimeline, get_scheduler};
use blinc_core::{ClipLength, ClipPath, Color};
use blinc_layout::InstanceKey;
use blinc_layout::div::{Div, ElementTypeId};
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::{ColorToken, ThemeState};
use std::cell::{OnceCell, RefCell};
use std::sync::{Arc, Mutex};
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
struct SpinnerConfig {
    size: SpinnerSize,
    color: Option<blinc_layout::binding::Reactive<Color>>,
    /// Faint full-circle ring drawn behind the rotating arc. Defaults
    /// to `ColorToken::Border` when `None`. Pass `Color::TRANSPARENT`
    /// (or override via `.track_color(...)`) to suppress the track.
    track_color: Option<blinc_layout::binding::Reactive<Color>>,
    duration_ms: u32,
    classes: Vec<Arc<str>>,
    user_id: Option<String>,
}

// `Reactive<T>` isn't `Clone` -- it carries type-erased handles -- but
// every variant's payload is.
impl Clone for SpinnerConfig {
    fn clone(&self) -> Self {
        Self {
            size: self.size,
            color: self
                .color
                .as_ref()
                .map(crate::reactive_props::clone_reactive),
            track_color: self
                .track_color
                .as_ref()
                .map(crate::reactive_props::clone_reactive),
            duration_ms: self.duration_ms,
            classes: self.classes.clone(),
            user_id: self.user_id.clone(),
        }
    }
}

impl Default for SpinnerConfig {
    fn default() -> Self {
        Self {
            size: SpinnerSize::default(),
            color: None,
            track_color: None,
            duration_ms: 1000,
            classes: Vec::new(),
            user_id: None,
        }
    }
}

/// Built spinner — wraps the composed inner `Div`. Created by
/// `SpinnerBuilder::get_or_build` on first ElementBuilder access.
pub struct Spinner {
    inner: Div,
}

impl Spinner {
    /// Materialise the spinner tree from accumulated config. `_key`
    /// is the call-site `InstanceKey`; we don't currently use it to
    /// persist the timeline across rebuilds (a fresh timeline on each
    /// rebuild just restarts the rotation from 0° — visually
    /// indistinguishable for a non-stop spinner).
    fn with_config(_key: InstanceKey, config: SpinnerConfig) -> Self {
        let theme = ThemeState::get();
        let diameter = config.size.diameter();
        let border_width = config.size.border_width();
        let half = diameter / 2.0;
        // Bound colours ride the property-binding path: `border_color`
        // registers a writer, so a set patches render props in place --
        // no rebuild, and the rotation keeps its phase instead of
        // snapping back to 0°.
        let spinner_color = config.color.unwrap_or_else(|| {
            blinc_layout::binding::Reactive::Const(theme.color(ColorToken::Primary))
        });
        let track_color = config.track_color.unwrap_or_else(|| {
            blinc_layout::binding::Reactive::Const(theme.color(ColorToken::Border))
        });
        let spinner_color_now = crate::reactive_props::current(&spinner_color);
        let track_color_now = crate::reactive_props::current(&track_color);

        // Internal timeline — one per built spinner. The scheduler
        // handle is global so multiple spinners share the same animation
        // pump without contention.
        let scheduler = get_scheduler();
        let timeline: SharedAnimatedTimeline =
            Arc::new(Mutex::new(AnimatedTimeline::new(scheduler)));

        let entry_id = timeline.lock().unwrap().configure(|t| {
            let id = t.add(0, config.duration_ms, 0.0, 360.0);
            t.set_loop(-1);
            t.start();
            id
        });

        // Polygon mask: rectangle covering the left half of the
        // bounding box. The polygon test runs in element-local coords,
        // so the mask rotates with the element under motion's
        // `rotate_timeline`, exposing a 180° arc that sweeps around.
        let polygon = ClipPath::Polygon {
            points: vec![
                (ClipLength::Px(0.0), ClipLength::Px(0.0)),
                (ClipLength::Px(half), ClipLength::Px(0.0)),
                (ClipLength::Px(half), ClipLength::Px(diameter)),
                (ClipLength::Px(0.0), ClipLength::Px(diameter)),
            ],
        };

        // Static track ring — full circle, faint colour. Sits in the
        // static cache; only the arc above rotates.
        let track = div()
            .class("cn-spinner__track")
            .absolute()
            .inset(0.0)
            .w(diameter)
            .h(diameter)
            .rounded(half)
            .border(border_width, track_color_now)
            .border_color(track_color);

        // Rotating arc: full-circle ring masked to 180° by polygon.
        let arc = motion().rotate_timeline(timeline.clone(), entry_id).child(
            div()
                .class("cn-spinner__arc")
                .w(diameter)
                .h(diameter)
                .rounded(half)
                .border(border_width, spinner_color_now)
                .border_color(spinner_color)
                .clip_path(polygon),
        );

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

    /// Set the rotating arc colour. Defaults to `ColorToken::Primary`.
    pub fn color(self, color: impl blinc_layout::binding::IntoReactive<Color>) -> Self {
        self.config.borrow_mut().color = Some(color.into_reactive());
        self
    }

    /// Override the faint full-circle track ring colour. Defaults
    /// to `ColorToken::Border`. Pass `Color::TRANSPARENT` to hide
    /// the track entirely (e.g. for a pure-arc spinner).
    pub fn track_color(self, color: impl blinc_layout::binding::IntoReactive<Color>) -> Self {
        self.config.borrow_mut().track_color = Some(color.into_reactive());
        self
    }

    /// Set the rotation duration in milliseconds (default: 1000ms).
    pub fn duration_ms(self, duration: u32) -> Self {
        self.config.borrow_mut().duration_ms = duration;
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

/// Create an animated spinner loading indicator.
///
/// The timeline is constructed internally — no need to pass one in.
/// Uses `#[track_caller]` for a stable per-call-site key.
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
/// cn::spinner()
///     .size(SpinnerSize::Large)
///     .duration_ms(2000)
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
