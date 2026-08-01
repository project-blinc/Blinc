//! `cn.AccordionItem` — one section, holding its own label and body.

use std::cell::OnceCell;
use std::sync::Mutex;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::{Div, ElementBuilder, div};

/// `cn.AccordionItem(label = "Shipping") { … }` — a labelled section
/// inside a [`crate::accordion::CnAccordion`].
///
/// ```dsl,ignore
/// cn.Accordion {
///     cn.AccordionItem(label = "Shipping") {
///         Div { Text("ships in two days") }
///     }
/// }
/// ```
///
/// The label sits on the section that owns it, which is the whole point:
/// the two halves cannot drift apart, and reordering sections is a
/// matter of moving one block.
///
/// The parent reads this through `as_any`, taking the label and the body
/// and rendering neither itself: an accordion draws its own headers and
/// folds its own bodies, so what it wants from a section is the parts,
/// not a picture. `build` therefore only runs when the item is NOT
/// inside an accordion, where it falls back to drawing the label above
/// the body unfolded.
#[extern_widget(namespace = "cn", name = "AccordionItem")]
pub struct CnAccordionItem {
    /// Header text. Also identifies the section unless `key` is given.
    pub label: String,
    /// Stable identity for the open/closed state. Defaults to `label`,
    /// which is right until two sections share a label or a label is
    /// reworded, at which point the section would forget whether it was
    /// open.
    pub key: String,
    /// Whether the section is expanded.
    ///
    /// Bind a signal and it works both ways, like `cn.Switch`'s
    /// `checked`: writing the signal folds or unfolds the section, and
    /// clicking the header writes back. A plain `true` is an initial
    /// value only, and on a single-open accordion the last such section
    /// wins, since only one can be open.
    pub open: Reactive<bool>,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Only built when this item renders outside an accordion.
    #[skip]
    fallback: OnceCell<Div>,
}

impl CnAccordionItem {
    /// This section's identity: `key` when given, else the label.
    pub(crate) fn section_key(&self) -> String {
        if self.key.is_empty() {
            self.label.clone()
        } else {
            self.key.clone()
        }
    }

    /// The caller's open state, for a section driven from a signal.
    ///
    /// Only a bound signal yields one. A literal has no signal to write
    /// back to, and a fresh one per build would forget the section's
    /// state on every rebuild, so those keep the accordion's internal
    /// keyed state and use [`starts_open`](Self::starts_open) as its
    /// seed. A computed is read the same way: it has nothing writable
    /// behind it either.
    pub(crate) fn bound_state(&self) -> Option<blinc_core::reactive::State<bool>> {
        match &self.open {
            Reactive::Signal(_) => Some(crate::bridge::bool_state(&self.open)),
            Reactive::Literal(_) | Reactive::Computed(_) => None,
        }
    }

    /// Initial expanded state, for the unbound case.
    pub(crate) fn starts_open(&self) -> bool {
        match &self.open {
            Reactive::Literal(v) => *v,
            Reactive::Computed(c) => c.try_get().unwrap_or(false),
            Reactive::Signal(_) => false,
        }
    }

    /// Take the body, leaving the item empty. The parent calls this
    /// once, while building; a second call yields nothing, which is what
    /// keeps the body from being mounted twice.
    pub(crate) fn take_children(&self) -> Vec<Box<dyn ElementBuilder>> {
        std::mem::take(&mut *self.children.lock().expect("children mutex"))
    }

    /// Label above body, unfolded. An item outside an accordion has
    /// nothing to fold it, so showing the content beats showing nothing.
    fn get_or_build(&self) -> &Div {
        if let Some(built) = self.fallback.get() {
            return built;
        }
        tracing::warn!(
            label = %self.label,
            "cn.AccordionItem outside a cn.Accordion — rendering unfolded",
        );
        let mut body = div().w_full().flex_col();
        for child in self.take_children() {
            body = body.child_box(child);
        }
        let built = div()
            .w_full()
            .flex_col()
            .child(blinc_cn::label(&self.label))
            .child(body);
        let _ = self.fallback.set(built);
        self.fallback.get().expect("just set")
    }
}

impl ElementBuilder for CnAccordionItem {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    /// What lets `cn.Accordion` pair this label with this body.
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
