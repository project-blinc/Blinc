//! `cn.Checkbox` — checked/unchecked box bound to a signal.

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

use crate::bridge::bool_state;

/// `cn.Checkbox(checked, label?, size?, disabled?)` — a checkbox.
///
/// Props (DSL surface):
/// - `checked: Reactive<bool>` — bind a `signal` here and the checkbox
///   shares it: ticking the box writes the signal, and setting the
///   signal ticks the box. A literal just seeds the initial state.
/// - `label: string` — optional text beside the box.
/// - `size: string` — `"small"` / `"sm"`, `"medium"` / `"md"`
///   (default), `"large"` / `"lg"`.
/// - `disabled: bool` — non-interactive, dimmed.
///
/// The colour props (`checked_color`, `border_color`, ...) are omitted:
/// those belong in CSS via `.cn-checkbox`.
#[extern_widget(namespace = "cn", name = "Checkbox")]
pub struct CnCheckbox {
    pub checked: Reactive<bool>,
    pub label: String,
    pub size: String,
    pub disabled: bool,
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::Checkbox>,
}

impl CnCheckbox {
    fn get_or_build(&self) -> &blinc_cn::Checkbox {
        ::blinc_layout::build_once::build_once(&self.built, || self.to_cn_widget())
    }

    fn to_cn_widget(&self) -> blinc_cn::Checkbox {
        let size = match self.size.as_str() {
            "small" | "sm" => blinc_cn::CheckboxSize::Small,
            "large" | "lg" => blinc_cn::CheckboxSize::Large,
            _ => blinc_cn::CheckboxSize::Medium,
        };
        let state = bool_state(&self.checked);
        let mut s = blinc_cn::checkbox(&state).size(size);
        if !self.label.is_empty() {
            s = s.label(self.label.clone());
        }
        if self.disabled {
            s = s.disabled(true);
        }
        s.build_component()
    }
}

impl ElementBuilder for CnCheckbox {
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
    // `event_handlers` especially: without it the box never ticks.
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

    // Intrinsic size lives here: a textarea's rows-derived height and an
    // input's height are set on the taffy style, so a wrapper that does
    // not forward it hides the size from every builder-tree reader.
    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }
}
