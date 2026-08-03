//! `cn.RadioGroup` — a set of options, one picked at a time.

use std::cell::OnceCell;
use std::sync::Mutex;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

use crate::bridge::CallSiteId;
use crate::radio::CnRadio;

/// `cn.RadioGroup(value = signal) { cn.Radio(label = "…") }` — pick one.
///
/// ```dsl,ignore
/// signal plan: string = "free"
///
/// cn.RadioGroup(value = plan, label = "Plan", layout = "horizontal") {
///     cn.Radio(value = "free", label = "Free")
///     cn.Radio(value = "pro", label = "Pro")
///     cn.Radio(value = "team", label = "Team", disabled = true)
/// }
/// ```
///
/// `value` binds both ways: picking an option writes the signal, and
/// writing the signal moves the dial. Each option names itself, so the
/// group reads its children rather than being told about them
/// separately. A child that is not a `cn.Radio` is dropped with a
/// warning: the group draws its own dials and has nowhere to put a
/// loose element.
#[extern_widget(namespace = "cn", name = "RadioGroup")]
pub struct CnRadioGroup {
    /// Which option is picked. Bind a signal to drive it from
    /// elsewhere.
    pub value: Reactive<String>,
    /// Text above the options.
    pub label: String,
    /// `small` / `medium` (default) / `large`.
    pub size: String,
    /// `vertical` (default) or `horizontal`.
    pub layout: String,
    /// Dim every option and take the whole group out of reach.
    pub disabled: bool,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Where this group was written, which is what tells one group's
    /// options from another's. Every group is built from the one call
    /// site inside this wrapper, so the widget's own call-site identity
    /// would be the same for all of them.
    #[skip]
    call_site: CallSiteId,
    /// Built once, consuming `children`.
    #[skip]
    shell: OnceCell<blinc_cn::RadioGroup>,
}

impl CnRadioGroup {
    fn get_or_build(&self) -> &blinc_cn::RadioGroup {
        ::blinc_layout::build_once::build_once(&self.shell, || self.make())
    }

    fn make(&self) -> blinc_cn::RadioGroup {
        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));
        let state = crate::bridge::string_state(&self.value);

        let mut b = blinc_cn::radio_group(&state)
            .key(format!("cn-radio-group-{}", self.call_site.0))
            .size(self.size())
            .layout(self.layout());
        if !self.label.is_empty() {
            b = b.label(self.label.clone());
        }
        if self.disabled {
            b = b.disabled(true);
        }

        for child in children {
            let Some(option) = child.as_any().and_then(|any| any.downcast_ref::<CnRadio>()) else {
                tracing::warn!(
                    "cn.RadioGroup: child is not a cn.Radio — dropped; wrap it in \
                     cn.Radio(label = \"…\") to give it a dial",
                );
                continue;
            };
            let (value, label) = (option.option_value(), option.option_label());
            b = if option.disabled {
                b.option_disabled(value, label)
            } else {
                b.option(value, label)
            };
        }

        b.build_component()
    }

    fn size(&self) -> blinc_cn::RadioSize {
        match self.size.as_str() {
            "small" | "sm" => blinc_cn::RadioSize::Small,
            "large" | "lg" => blinc_cn::RadioSize::Large,
            _ => blinc_cn::RadioSize::Medium,
        }
    }

    fn layout(&self) -> blinc_cn::RadioLayout {
        match self.layout.as_str() {
            "horizontal" | "row" => blinc_cn::RadioLayout::Horizontal,
            "" | "vertical" | "column" => blinc_cn::RadioLayout::Vertical,
            other => {
                tracing::warn!(layout = %other, "cn.RadioGroup: unknown layout");
                blinc_cn::RadioLayout::Vertical
            }
        }
    }
}

impl ElementBuilder for CnRadioGroup {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    /// The shell's children, which now hold the dials.
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
