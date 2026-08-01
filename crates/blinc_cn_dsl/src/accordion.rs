//! `cn.Accordion` — labelled sections, one or many open at a time.

use std::cell::OnceCell;
use std::sync::Mutex;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::ElementBuilder;

use crate::accordion_item::CnAccordionItem;

/// `cn.Accordion { cn.AccordionItem(label = "…") { … } }` — sections
/// that fold, with a spring-driven height animation.
///
/// ```dsl,ignore
/// cn.Accordion(multi = true) {
///     cn.AccordionItem(label = "Shipping", open = true) {
///         Div { Text("ships in two days") }
///     }
///     cn.AccordionItem(label = "Returns", open = shown) {
///         Div { Text("thirty day window") }
///     }
/// }
/// ```
///
/// Each section names itself, so the accordion reads its children rather
/// than being told about them separately. A child that is not a
/// `cn.AccordionItem` is dropped with a warning: the widget draws its own
/// headers and has nowhere to put a loose element.
///
/// Bodies are handed over as recipes rather than as built elements,
/// because the widget rebuilds a section's content on every fold. See
/// `crate::shared_child` for what that costs and why it is sound.
#[extern_widget(namespace = "cn", name = "Accordion")]
pub struct CnAccordion {
    /// Allow several sections open at once. Default is one at a time,
    /// where opening a section folds the previous one.
    pub multi: bool,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Built once, consuming `children`.
    #[skip]
    shell: OnceCell<blinc_cn::Accordion>,
}

impl CnAccordion {
    fn get_or_build(&self) -> &blinc_cn::Accordion {
        // Built outside the cell: the widget runs its stateful body
        // during construction, which re-enters here.
        if let Some(built) = self.shell.get() {
            return built;
        }
        let built = self.make();
        let _ = self.shell.set(built);
        self.shell.get().expect("just set")
    }

    fn make(&self) -> blinc_cn::Accordion {
        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));

        let mut b = blinc_cn::accordion();
        if self.multi {
            b = b.multi_open();
        }
        // The widget takes one initially-open key, so on a multi-open
        // accordion the last `open` section is the one that starts open.
        let mut open = None;

        for child in children {
            let Some(item) = child
                .as_any()
                .and_then(|any| any.downcast_ref::<CnAccordionItem>())
            else {
                tracing::warn!(
                    "cn.Accordion: child is not a cn.AccordionItem — dropped; \
                     wrap it in cn.AccordionItem(label = \"…\") to give it a header",
                );
                continue;
            };
            let key = item.section_key();
            let body = crate::shared_child::body_recipe(item.take_children());
            match item.bound_state() {
                // A bound section owns its state, so it carries its own
                // initial value and needs no `default_open`.
                Some(state) => b = b.item_with_state(key, item.label.clone(), state, body),
                None => {
                    if item.starts_open() {
                        open = Some(key.clone());
                    }
                    b = b.item(key, item.label.clone(), body);
                }
            }
        }

        if let Some(key) = open {
            b = b.default_open(key);
        }
        b.build_component()
    }
}

impl ElementBuilder for CnAccordion {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    /// The shell's children, which now hold the sections.
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
