//! `cn.Toggle` — a two-state button bound to a signal.

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

use crate::bridge::bool_state;

/// `cn.Toggle(pressed = signal, label?, icon?, variant?, size?, disabled?)`
/// — a button that stays down.
///
/// ```dsl,ignore
/// signal bold: bool = false
///
/// cn.Toggle(pressed = bold, icon = "bold", variant = "outline")
/// ```
///
/// `pressed` binds two ways, the same as `cn.Switch`'s `checked`:
/// pressing the toggle writes the signal, and setting the signal moves
/// the toggle. A literal seeds the initial state.
///
/// A toggle differs from a switch in what it is for rather than in what
/// it does: a switch reads as a setting, a toggle as a button that
/// stays down. Toolbars want the second.
///
/// `icon` names a Lucide icon, as everywhere else — see
/// [`crate::icon`]. An icon-only toggle should carry an `aria_label`,
/// since there is no text for a screen reader to fall back on.
#[extern_widget(namespace = "cn", name = "Toggle")]
pub struct CnToggle {
    /// Whether the toggle is down.
    pub pressed: Reactive<bool>,
    /// Text inside the toggle. Omit for an icon-only one.
    pub label: String,
    /// Lucide icon name, or a complete `<svg>` string.
    pub icon: String,
    /// `default` (no border until pressed) or `outline` (bordered, for
    /// radio-style bars).
    pub variant: String,
    /// `small` / `sm`, `medium` / `md` (default), `large` / `lg`.
    pub size: String,
    /// Non-interactive, dimmed.
    pub disabled: bool,
    /// What a screen reader announces. Recommended for an icon-only
    /// toggle, which has no text to fall back on.
    pub aria_label: String,
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::ToggleBuilder>,
}

/// Icon size inside a toggle, matching what the widget draws.
const ICON_SIZE: f32 = 16.0;

impl CnToggle {
    fn get_or_build(&self) -> &blinc_cn::ToggleBuilder {
        ::blinc_layout::build_once::build_once(&self.built, || self.to_cn_widget())
    }

    fn to_cn_widget(&self) -> blinc_cn::ToggleBuilder {
        let state = bool_state(&self.pressed);
        let mut t = blinc_cn::toggle(&state)
            .size(self.size())
            .variant(self.variant());
        if !self.label.is_empty() {
            t = t.label(self.label.clone());
        }
        if let Some(svg) = crate::icon::resolve("cn.Toggle", &self.icon, ICON_SIZE) {
            t = t.icon(svg);
        }
        if !self.aria_label.is_empty() {
            t = t.aria_label(self.aria_label.clone());
        }
        if self.disabled {
            t = t.disabled(true);
        }
        t
    }

    fn size(&self) -> blinc_cn::ToggleSize {
        match self.size.as_str() {
            "small" | "sm" => blinc_cn::ToggleSize::Small,
            "large" | "lg" => blinc_cn::ToggleSize::Large,
            _ => blinc_cn::ToggleSize::Medium,
        }
    }

    fn variant(&self) -> blinc_cn::ToggleVariant {
        match self.variant.as_str() {
            "outline" => blinc_cn::ToggleVariant::Outline,
            "" | "default" => blinc_cn::ToggleVariant::Default,
            other => {
                tracing::warn!(variant = %other, "cn.Toggle: unknown variant");
                blinc_cn::ToggleVariant::Default
            }
        }
    }
}

impl ElementBuilder for CnToggle {
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
