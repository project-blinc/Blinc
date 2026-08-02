//! `cn.Slider` — a value picked by dragging.

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

/// `cn.Slider(value = signal, min = 0.0, max = 100.0)` — drag to choose
/// a number.
///
/// ```dsl,ignore
/// signal volume: f64 = 40.0
///
/// cn.Slider(value = volume, min = 0.0, max = 100.0, step = 5.0,
///           label = "Volume", show_value = true)
/// ```
///
/// `value` binds two ways: dragging writes the signal, and setting the
/// signal is read the next time the slider is built.
///
/// The widget keeps its position in an `f32` state of its own while the
/// DSL types every number as `f64`, so the bound signal is seeded at
/// build time and written back on every change. That is why a drag
/// shows up in the signal at once, and why a write from elsewhere lands
/// on the next build rather than mid-drag.
#[extern_widget(namespace = "cn", name = "Slider")]
pub struct CnSlider {
    /// The chosen number.
    pub value: Reactive<f64>,
    /// Lower bound. Omitted keeps the cn default.
    pub min: f64,
    /// Upper bound. Omitted keeps the cn default.
    pub max: f64,
    /// Granularity. Omitted keeps the cn default.
    pub step: f64,
    /// Text above the track.
    pub label: String,
    /// Print the current number beside the label.
    pub show_value: bool,
    /// Track width. Omitted keeps the cn default, which is the width
    /// the thumb's travel is laid out against — a track wider than that
    /// would map a range the thumb cannot reach.
    pub w: f64,
    /// `small` / `medium` (default) / `large`.
    pub size: String,
    /// Non-interactive, dimmed.
    pub disabled: bool,
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::SliderBuilder>,
}

impl CnSlider {
    fn get_or_build(&self) -> &blinc_cn::SliderBuilder {
        ::blinc_layout::build_once::build_once(&self.built, || self.to_cn_widget())
    }

    fn to_cn_widget(&self) -> blinc_cn::SliderBuilder {
        let state = crate::bridge::f32_state_from(&self.value);
        let mut s = blinc_cn::slider(&state).size(self.size());

        // Zero is what an omitted number prop reads as, and zero is a
        // meaningful bound — so `min` only overrides when `max` says a
        // range was given at all.
        if self.max > self.min {
            s = s.min(self.min as f32).max(self.max as f32);
        }
        if self.step > 0.0 {
            s = s.step(self.step as f32);
        }
        if !self.label.is_empty() {
            s = s.label(self.label.clone());
        }
        if self.show_value {
            s = s.show_value();
        }
        if self.w > 0.0 {
            s = s.w(self.w as f32);
        }
        if self.disabled {
            s = s.disabled(true);
        }

        // The widget writes its own `f32` state; the signal the source
        // declared is what everything else reads, so a drag has to
        // reach it too.
        if let Some(bound) = self.bound_signal() {
            s = s.on_change(move |v| bound.set(v as f64));
        }
        s
    }

    /// The signal behind `value`, when one was bound. A literal has
    /// nothing to write back to.
    fn bound_signal(&self) -> Option<blinc_core::reactive::Signal<f64>> {
        match &self.value {
            Reactive::Signal(s) => Some(*s),
            Reactive::Literal(_) | Reactive::Computed(_) => None,
        }
    }

    fn size(&self) -> blinc_cn::SliderSize {
        match self.size.as_str() {
            "small" | "sm" => blinc_cn::SliderSize::Small,
            "large" | "lg" => blinc_cn::SliderSize::Large,
            _ => blinc_cn::SliderSize::Medium,
        }
    }
}

impl ElementBuilder for CnSlider {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    // MUST forward — see `gotcha_element_builder_trait_forwarding`.
    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.get_or_build().event_handlers()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.get_or_build().element_type_id()
    }
}
