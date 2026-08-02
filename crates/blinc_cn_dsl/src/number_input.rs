//! `cn.NumberInput` — a number typed or stepped.

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

/// `cn.NumberInput(value = signal, min = 0.0, max = 10.0)` — a field
/// for a number, with steppers.
///
/// ```dsl,ignore
/// signal quantity: f64 = 1.0
///
/// cn.NumberInput(value = quantity, min = 1.0, max = 99.0, step = 1.0,
///                precision = 0.0)
/// ```
///
/// `value` binds two ways: typing or stepping writes the signal, and
/// setting the signal is read the next time the field is built. Same
/// arrangement as `cn.Slider` — the widget owns a state, and the change
/// callback keeps the bound signal level with it.
#[extern_widget(namespace = "cn", name = "NumberInput")]
pub struct CnNumberInput {
    /// The number.
    pub value: Reactive<f64>,
    /// Lower bound. Omitted keeps the cn default.
    pub min: f64,
    /// Upper bound. Omitted keeps the cn default.
    pub max: f64,
    /// How much a step moves. Omitted keeps the cn default.
    pub step: f64,
    /// Decimal places shown. Omitted keeps the cn default.
    pub precision: f64,
    /// Grey text when empty.
    pub placeholder: String,
    /// `small` / `medium` (default) / `large`.
    pub size: String,
    /// Field width. Omitted keeps the cn default.
    pub w: f64,
    /// Non-interactive, dimmed.
    pub disabled: bool,
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::NumberInputBuilder>,
}

impl CnNumberInput {
    fn get_or_build(&self) -> &blinc_cn::NumberInputBuilder {
        ::blinc_layout::build_once::build_once(&self.built, || self.to_cn_widget())
    }

    fn to_cn_widget(&self) -> blinc_cn::NumberInputBuilder {
        let state = crate::bridge::f64_state(&self.value);
        let mut n = blinc_cn::number_input(&state).size(self.size());

        // Zero is what an omitted number prop reads as and is also a
        // meaningful bound, so a range only applies when `max` says one
        // was given.
        if self.max > self.min {
            n = n.min(self.min).max(self.max);
        }
        if self.step > 0.0 {
            n = n.step(self.step);
        }
        if self.precision > 0.0 {
            n = n.precision(self.precision as usize);
        }
        if !self.placeholder.is_empty() {
            n = n.placeholder(self.placeholder.clone());
        }
        if self.w > 0.0 {
            n = n.w(self.w as f32);
        }
        if self.disabled {
            n = n.disabled(true);
        }
        if let Some(bound) = self.bound_signal() {
            n = n.on_change(move |v| bound.set(v));
        }
        n
    }

    /// The signal behind `value`, when one was bound. A literal has
    /// nothing to write back to.
    fn bound_signal(&self) -> Option<blinc_core::reactive::Signal<f64>> {
        match &self.value {
            Reactive::Signal(s) => Some(*s),
            Reactive::Literal(_) | Reactive::Computed(_) => None,
        }
    }

    fn size(&self) -> blinc_cn::InputSize {
        match self.size.as_str() {
            "small" | "sm" => blinc_cn::InputSize::Small,
            "large" | "lg" => blinc_cn::InputSize::Large,
            _ => blinc_cn::InputSize::Medium,
        }
    }
}

impl ElementBuilder for CnNumberInput {
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
