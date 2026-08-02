//! `cn.Radio` — one option, holding its own value and label.

use std::cell::OnceCell;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::{Div, ElementBuilder, div};

/// `cn.Radio(value = "pro", label = "Pro")` — one choice inside a
/// [`crate::radio_group::CnRadioGroup`].
///
/// ```dsl,ignore
/// cn.RadioGroup(value = plan) {
///     cn.Radio(value = "free", label = "Free")
///     cn.Radio(value = "pro", label = "Pro")
/// }
/// ```
///
/// The value sits on the option that carries it, so the two cannot
/// drift apart and reordering options means moving one line.
///
/// The parent reads this through `as_any` and draws the dial itself,
/// which is why `build` only runs when the option is NOT inside a
/// group: there it falls back to its bare label.
#[extern_widget(namespace = "cn", name = "Radio")]
pub struct CnRadio {
    /// What the bound signal reads while this option is picked.
    /// Defaults to the label, which is right until two options share
    /// one or a label is reworded.
    pub value: String,
    /// Text beside the dial.
    pub label: String,
    /// Visible but not selectable.
    pub disabled: bool,
    /// Only built when this option renders outside a group.
    #[skip]
    fallback: OnceCell<Div>,
}

impl CnRadio {
    /// This option's identity: `value` when given, else the label.
    pub(crate) fn option_value(&self) -> String {
        if self.value.is_empty() {
            self.label.clone()
        } else {
            self.value.clone()
        }
    }

    /// The label to print, falling back to the value so an option with
    /// only a value still reads as something.
    pub(crate) fn option_label(&self) -> String {
        if self.label.is_empty() {
            self.value.clone()
        } else {
            self.label.clone()
        }
    }

    /// Just the label. An option outside a group has no dial to draw
    /// and nothing to be selected against.
    fn get_or_build(&self) -> &Div {
        ::blinc_layout::build_once::build_once(&self.fallback, || {
            tracing::warn!(
                label = %self.label,
                "cn.Radio outside a cn.RadioGroup — rendering as a plain label",
            );
            div().child(blinc_cn::label(self.option_label()))
        })
    }
}

impl ElementBuilder for CnRadio {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    /// What lets `cn.RadioGroup` read this option's value and label.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    // MUST forward — see `gotcha_element_builder_trait_forwarding`.
    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        Some(self.get_or_build().event_handlers())
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
