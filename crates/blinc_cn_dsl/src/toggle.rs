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
///
/// Inside a [`crate::toggle_group::CnToggleGroup`] the same widget
/// becomes one option of a set:
///
/// ```dsl,ignore
/// cn.ToggleGroup(value = align) {
///     cn.Toggle(value = "left", label = "Left")
///     cn.Toggle(value = "center", label = "Center")
/// }
/// ```
///
/// There the group owns the selection, so `value` identifies the option
/// and `pressed` is ignored: one signal decides which chip is down. The
/// group also imposes its own `variant` and `size`, so a bar cannot end
/// up with mismatched chips.
#[extern_widget(namespace = "cn", name = "Toggle")]
pub struct CnToggle {
    /// Whether the toggle is down. Ignored inside a `cn.ToggleGroup`,
    /// which owns the selection for the whole set.
    pub pressed: Reactive<bool>,
    /// Which option this is, inside a `cn.ToggleGroup`. Defaults to the
    /// label. Unused on a standalone toggle, which answers to `pressed`.
    pub value: String,
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

    /// This option's identity inside a group: `value` when given, else
    /// the label.
    pub(crate) fn option_value(&self) -> String {
        if self.value.is_empty() {
            self.label.clone()
        } else {
            self.value.clone()
        }
    }

    /// The caption to print. Falls back to the value so an option with
    /// only a value still reads as something, but stays empty when an
    /// icon carries the meaning, so a chip is not captioned twice.
    fn option_label(&self) -> String {
        if !self.label.is_empty() {
            self.label.clone()
        } else if self.icon.is_empty() {
            self.value.clone()
        } else {
            String::new()
        }
    }

    /// This toggle as one option of a group.
    ///
    /// `variant` and `size` are deliberately absent: the group applies
    /// its own to every chip, so a bar cannot come out mismatched.
    pub(crate) fn to_cn_item(&self) -> blinc_cn::ToggleItem {
        let mut item = blinc_cn::toggle_item(self.option_value());
        let label = self.option_label();
        if !label.is_empty() {
            item = item.label(label);
        }
        if let Some(svg) = crate::icon::resolve("cn.Toggle", &self.icon, ICON_SIZE) {
            item = item.icon(svg);
        }
        if !self.aria_label.is_empty() {
            item = item.aria_label(self.aria_label.clone());
        }
        if self.disabled {
            item = item.disabled(true);
        }
        item
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

    /// What lets `cn.ToggleGroup` read this toggle as one of its
    /// options. Without it every child downcast misses and the group
    /// silently drops the lot.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
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
