//! Progress: how much of something is done, as a filled track.
//!
//! The bar's `flex-grow` is bound to the value, so it follows a Signal or
//! Computed in place and eases to each new value by the `transition` on
//! `.cn-progress-bar`. With no value it is indeterminate: a part of the
//! track pulses by the `cn-progress-pulse` keyframes.
//!
//! The colours, radius and motion are `.cn-progress` rules in `CN_STYLES`;
//! `--cn-progress-track` and `--cn-progress-bar` recolour one. Sizes are
//! layout and set here.
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! cn::progress(75.0)
//! cn::progress(done).size(ProgressSize::Large).w(300.0)
//! cn::progress_indeterminate()
//! ```

use blinc_core::Color;
use blinc_core::reactive::{Computed, ReactiveGraph, computed};
use blinc_layout::binding::{IntoReactive, Reactive};
use blinc_layout::prelude::*;
use std::sync::Arc;

/// How much of the track an indeterminate bar covers.
const INDETERMINATE_SHARE: f32 = 0.3;

/// Progress bar size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProgressSize {
    /// Small progress bar (4px height)
    Small,
    /// Medium progress bar (8px height)
    #[default]
    Medium,
    /// Large progress bar (12px height)
    Large,
}

impl ProgressSize {
    fn height(&self) -> f32 {
        match self {
            ProgressSize::Small => 4.0,
            ProgressSize::Medium => 8.0,
            ProgressSize::Large => 12.0,
        }
    }

    /// The `data-size` its rules key on.
    fn attr(&self) -> &'static str {
        match self {
            ProgressSize::Small => "sm",
            ProgressSize::Medium => "md",
            ProgressSize::Large => "lg",
        }
    }
}

/// Configuration for building a Progress bar. `value` is in `0..=100`;
/// `None` is indeterminate.
#[derive(Clone)]
struct ProgressConfig {
    value: Option<Reactive<f32>>,
    size: ProgressSize,
    width: f32,
    indicator_color: Option<Color>,
    track_color: Option<Color>,
    corner_radius: Option<f32>,
}

impl ProgressConfig {
    fn new(value: Option<Reactive<f32>>) -> Self {
        Self {
            value,
            size: ProgressSize::default(),
            width: 200.0,
            indicator_color: None,
            track_color: None,
            corner_radius: None,
        }
    }
}

/// The share of the track `percent` fills.
fn share(percent: f32) -> f32 {
    if percent.is_finite() {
        (percent / 100.0).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// The bar's share of the track, following `value` in place.
fn bound_share(value: Reactive<f32>) -> Reactive<f32> {
    match value {
        Reactive::Const(v) => Reactive::Const(share(v)),
        Reactive::Bound(state) => {
            let signal = state.signal();
            Reactive::Computed(computed(move |g: &ReactiveGraph| {
                share(g.get(signal).unwrap_or(0.0))
            }))
        }
        Reactive::Computed(source) => {
            let id = source.derived_id();
            let graph = source.graph();
            Reactive::Computed(Computed::mapped(
                id,
                Arc::new(move || source.try_get().map(share)),
                graph,
            ))
        }
    }
}

/// Styled Progress bar component
pub struct Progress {
    inner: Div,
}

impl Progress {
    fn from_config(config: ProgressConfig) -> Self {
        let height = config.size.height();
        let mut bar = div().class("cn-progress-bar").w(0.0).h_full();
        let mut track = div()
            .class("cn-progress")
            .attr("data-size", config.size.attr())
            .flex_row()
            .w(config.width)
            .h(height)
            .overflow_clip();
        bar = match config.value {
            Some(value) => bar.flex_grow_value(bound_share(value)),
            None => {
                track = track.attr("data-indeterminate", "");
                bar.flex_grow_value(INDETERMINATE_SHARE)
            }
        };

        // Per-instance overrides over the rules.
        if let Some(color) = config.indicator_color {
            bar = bar.bg(color);
        }
        if let Some(color) = config.track_color {
            track = track.bg(color);
        }
        if let Some(radius) = config.corner_radius {
            bar = bar.rounded(radius);
            track = track.rounded(radius);
        }

        Self {
            inner: track.child(bar),
        }
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.inner = self.inner.class(name);
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }
}

impl ElementBuilder for Progress {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_id(&self) -> Option<&str> {
        ElementBuilder::element_id(&self.inner)
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Builder for creating Progress components with fluent API
pub struct ProgressBuilder {
    config: ProgressConfig,
    built: std::cell::OnceCell<Progress>,
}

impl ProgressBuilder {
    /// A bar filled to `value`, in `0..=100`: a number, Signal or Computed.
    pub fn new(value: impl IntoReactive<f32>) -> Self {
        Self::with_value(Some(value.into_reactive()))
    }

    fn with_value(value: Option<Reactive<f32>>) -> Self {
        Self {
            config: ProgressConfig::new(value),
            built: std::cell::OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &Progress {
        ::blinc_layout::build_once::build_once(&self.built, || {
            Progress::from_config(self.config.clone())
        })
    }

    /// Set the progress bar size
    pub fn size(mut self, size: ProgressSize) -> Self {
        self.config.size = size;
        self
    }

    /// Set the width in pixels
    pub fn w(mut self, width: f32) -> Self {
        self.config.width = width;
        self
    }

    /// The bar's colour for this one, over `.cn-progress-bar`.
    pub fn indicator_color(mut self, color: impl Into<Color>) -> Self {
        self.config.indicator_color = Some(color.into());
        self
    }

    /// The track's colour for this one, over `.cn-progress`.
    pub fn track_color(mut self, color: impl Into<Color>) -> Self {
        self.config.track_color = Some(color.into());
        self
    }

    /// The corner radius for this one, over the rules.
    pub fn rounded(mut self, radius: f32) -> Self {
        self.config.corner_radius = Some(radius);
        self
    }

    /// Build the final Progress component
    pub fn build_component(self) -> Progress {
        Progress::from_config(self.config)
    }
}

impl ElementBuilder for ProgressBuilder {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.get_or_build().element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }
}

/// A progress bar filled to `value`, in `0..=100`: a number, or a Signal or
/// Computed it follows in place, easing to each new value.
///
/// ```ignore
/// cn::progress(75.0)
/// cn::progress(done).size(ProgressSize::Large).w(300.0)
/// ```
pub fn progress(value: impl IntoReactive<f32>) -> ProgressBuilder {
    ProgressBuilder::new(value)
}

/// A progress bar for work of unknown length: a part of the track pulses.
pub fn progress_indeterminate() -> ProgressBuilder {
    ProgressBuilder::with_value(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_size_heights() {
        assert_eq!(ProgressSize::Small.height(), 4.0);
        assert_eq!(ProgressSize::Medium.height(), 8.0);
        assert_eq!(ProgressSize::Large.height(), 12.0);
    }

    #[test]
    fn a_value_is_its_share_of_the_track_within_the_bounds() {
        assert_eq!(share(50.0), 0.5);
        assert_eq!(share(150.0), 1.0);
        assert_eq!(share(-10.0), 0.0);
        assert_eq!(share(f32::NAN), 0.0);
    }
}
