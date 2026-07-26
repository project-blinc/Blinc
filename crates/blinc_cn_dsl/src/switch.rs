//! `cn.Switch` — on/off toggle bound to a signal.

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

use crate::bridge::bool_state;

/// `cn.Switch(checked, label?, size?, disabled?)` — a toggle.
///
/// Props (DSL surface):
/// - `checked: Reactive<bool>` — bind a `signal` here and the switch
///   shares it: flipping the switch writes the signal, and setting the
///   signal moves the switch. A literal just seeds the initial state.
/// - `label: string` — optional text beside the switch.
/// - `size: string` — `"small"` / `"sm"`, `"medium"` / `"md"`
///   (default), `"large"` / `"lg"`.
/// - `disabled: bool` — non-interactive, dimmed.
///
/// `on_color` / `off_color` / `thumb_color` / `spring` are omitted:
/// colours belong in CSS via `.cn-switch`, and the spring takes a Rust
/// config the FFI has no shape for.
#[extern_widget(namespace = "cn", name = "Switch")]
pub struct CnSwitch {
    pub checked: Reactive<bool>,
    pub label: String,
    pub size: String,
    pub disabled: bool,
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::Switch>,
}

impl CnSwitch {
    fn get_or_build(&self) -> &blinc_cn::Switch {
        self.built.get_or_init(|| self.to_cn_widget())
    }

    fn to_cn_widget(&self) -> blinc_cn::Switch {
        let size = match self.size.as_str() {
            "small" | "sm" => blinc_cn::SwitchSize::Small,
            "large" | "lg" => blinc_cn::SwitchSize::Large,
            _ => blinc_cn::SwitchSize::Medium,
        };
        let state = bool_state(&self.checked);
        let mut s = blinc_cn::switch(&state).size(size);
        if !self.label.is_empty() {
            s = s.label(self.label.clone());
        }
        if self.disabled {
            s = s.disabled(true);
        }
        s.build_component()
    }
}

impl ElementBuilder for CnSwitch {
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
    // `event_handlers` especially: without it the switch never toggles.
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
