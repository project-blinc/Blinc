//! `cn.Tab` — one tab, holding its own label and panel.

use std::cell::OnceCell;
use std::sync::Mutex;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::{Div, ElementBuilder, div};

/// `cn.Tab(value = "account", label = "Account") { … }` — a labelled
/// panel inside a [`crate::tabs::CnTabs`].
///
/// ```dsl,ignore
/// cn.Tabs(value = section) {
///     cn.Tab(value = "account", label = "Account", icon = "user") {
///         Div { Text("who you are") }
///     }
///     cn.Tab(value = "alerts", label = "Alerts", badge = "3") {
///         Div { Text("what we sent you") }
///     }
/// }
/// ```
///
/// The label sits on the tab that owns it, so the two cannot drift apart
/// and reordering tabs means moving one block.
///
/// The parent reads this through `as_any`, taking the label and the
/// panel and rendering neither itself: tabs draw their own strip and
/// swap their own panels, so what they want from a tab is the parts, not
/// a picture. `build` therefore only runs when the tab is NOT inside
/// `cn.Tabs`, where it falls back to the label above the panel.
#[extern_widget(namespace = "cn", name = "Tab")]
pub struct CnTab {
    /// What the bound signal reads while this tab is showing. Defaults
    /// to the label, which is right until two tabs share one or a label
    /// is reworded.
    pub value: String,
    /// Strip text.
    pub label: String,
    /// Lucide icon name, drawn before the label.
    pub icon: String,
    /// Small count beside the label.
    pub badge: String,
    /// Visible but not selectable.
    pub disabled: bool,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Only built when this tab renders outside `cn.Tabs`.
    #[skip]
    fallback: OnceCell<Div>,
}

impl CnTab {
    /// This tab's identity: `value` when given, else the label.
    pub(crate) fn tab_value(&self) -> String {
        if self.value.is_empty() {
            self.label.clone()
        } else {
            self.value.clone()
        }
    }

    /// The strip entry: everything except the panel.
    pub(crate) fn menu_item(&self) -> blinc_cn::TabMenuItem {
        let mut item = blinc_cn::tab_item(self.tab_value());
        if !self.label.is_empty() {
            item = item.label(self.label.clone());
        }
        if !self.icon.is_empty() {
            match crate::icon::resolve("cn.Tab", &self.icon, 16.0) {
                Some(svg) => item = item.icon(svg),
                None => tracing::warn!(icon = %self.icon, "cn.Tab: unknown icon"),
            }
        }
        if !self.badge.is_empty() {
            item = item.badge(self.badge.clone());
        }
        if self.disabled {
            item = item.disabled();
        }
        item
    }

    /// Take the panel, leaving the tab empty. The parent calls this once
    /// while building; a second call yields nothing, which is what keeps
    /// the panel from being mounted twice.
    pub(crate) fn take_children(&self) -> Vec<Box<dyn ElementBuilder>> {
        std::mem::take(&mut *self.children.lock().expect("children mutex"))
    }

    /// Label above panel. A tab outside `cn.Tabs` has no strip to sit in,
    /// so showing the content beats showing nothing.
    fn get_or_build(&self) -> &Div {
        ::blinc_layout::build_once::build_once(&self.fallback, || {
            tracing::warn!(
                label = %self.label,
                "cn.Tab outside a cn.Tabs — rendering as a plain section",
            );
            let mut body = div().w_full().flex_col();
            for child in self.take_children() {
                body = body.child_box(child);
            }
            div()
                .w_full()
                .flex_col()
                .child(blinc_cn::label(&self.label))
                .child(body)
        })
    }
}

impl ElementBuilder for CnTab {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    /// What lets `cn.Tabs` pair this label with this panel.
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
