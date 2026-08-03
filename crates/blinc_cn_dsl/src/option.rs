//! `cn.Option` — one choice inside a widget that offers a list of them.

use std::cell::OnceCell;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::{Div, ElementBuilder, div};

/// `cn.Option(value = "pro", label = "Pro")` — one choice inside a
/// [`crate::select::CnSelect`].
///
/// ```dsl,ignore
/// cn.Select(value = plan, placeholder = "Pick a plan") {
///     cn.Option(value = "free", label = "Free")
///     cn.Option(value = "pro", label = "Pro")
///     cn.Option(value = "team", label = "Team", disabled = true)
/// }
/// ```
///
/// Not `cn.SelectItem`: a combobox offers the same value/label/disabled
/// choice, and naming one per parent would leave two widgets that differ
/// only in which bracket they sit inside.
///
/// The value sits on the option that carries it, so the two cannot drift
/// apart and reordering means moving one line. The parent reads this
/// through `as_any` and draws the row itself, so `build` only runs when
/// the option is NOT inside one.
#[extern_widget(namespace = "cn", name = "Option")]
pub struct CnOption {
    /// What the bound signal reads while this choice is picked.
    /// Defaults to the label, which is right until two options share one
    /// or a label is reworded.
    pub value: String,
    /// Text shown in the list, and on the trigger once picked.
    pub label: String,
    /// Visible in the list but not selectable.
    pub disabled: bool,
    /// Only built when this option renders outside a parent.
    #[skip]
    fallback: OnceCell<Div>,
}

impl CnOption {
    /// This choice's identity: `value` when given, else the label.
    pub(crate) fn option_value(&self) -> String {
        if self.value.is_empty() {
            self.label.clone()
        } else {
            self.value.clone()
        }
    }

    /// The text to show, falling back to the value so an option with
    /// only a value still reads as something.
    pub(crate) fn option_label(&self) -> String {
        if self.label.is_empty() {
            self.value.clone()
        } else {
            self.label.clone()
        }
    }

    /// Just the label. An option outside a list has nothing to be
    /// selected against and no state to write.
    fn get_or_build(&self) -> &Div {
        ::blinc_layout::build_once::build_once(&self.fallback, || {
            tracing::warn!(
                label = %self.label,
                "cn.Option outside a widget that offers choices — rendering as a plain label",
            );
            div().child(blinc_cn::label(self.option_label()))
        })
    }
}

impl ElementBuilder for CnOption {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    /// What lets the parent read this choice.
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
