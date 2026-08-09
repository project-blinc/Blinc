//! `cn.MenuItem` / `cn.MenuSeparator` — the rows of a menu.

use blinc_dsl_core::extern_widget;
use blinc_layout::div::{Div, ElementBuilder, div};
use std::cell::OnceCell;

/// `cn.MenuItem(label = "New", on_click = || …)` — one command in a
/// menu.
///
/// ```dsl,ignore
/// cn.DropdownMenu(label = "File") {
///     cn.MenuItem(label = "New", shortcut = "⌘N", on_click = || make())
///     cn.MenuItem(label = "Open", icon = "folder-open", on_click = || open())
///     cn.MenuSeparator()
///     cn.MenuItem(label = "Quit", disabled = true)
/// }
/// ```
///
/// Not `cn.Option`, which the select family uses: an option carries a
/// VALUE the parent selects between, while a menu item is a COMMAND
/// that runs and dismisses. Sharing one child type would mean a widget
/// with `value` and `on_click` where only one ever applies.
#[extern_widget(namespace = "cn", name = "MenuItem")]
pub struct CnMenuItem {
    /// Text of the row.
    pub label: String,
    /// Right-aligned hint, e.g. `"⌘N"`. Decorative: binding the key
    /// itself is the host's business.
    pub shortcut: String,
    /// Lucide icon name, or a complete `<svg>` string.
    pub icon: String,
    /// Visible but not selectable, and it runs nothing.
    pub disabled: bool,
    /// Runs when the row is chosen. Zero when omitted.
    pub on_click: i64,
    /// Only built when the row renders outside a menu.
    #[skip]
    fallback: OnceCell<Div>,
}

impl CnMenuItem {
    pub(crate) fn item_label(&self) -> String {
        self.label.clone()
    }

    /// The click handler as something the cn builder can call, or
    /// `None` when the author gave none.
    pub(crate) fn handler(&self) -> Option<impl Fn() + Send + Sync + 'static> {
        let ptr = self.on_click;
        (ptr != 0).then_some(move || {
            type ClosureFn = extern "C" fn();
            // SAFETY: Zyntax mints a zero-arg `extern "C" fn()` for a DSL
            // closure and hands the pointer across as `i64`.
            let func: ClosureFn = unsafe { std::mem::transmute(ptr) };
            func();
        })
    }

    fn get_or_build(&self) -> &Div {
        ::blinc_layout::build_once::build_once(&self.fallback, || {
            tracing::warn!(
                label = %self.label,
                "cn.MenuItem outside a menu — rendering as a plain label",
            );
            div().child(blinc_cn::label(self.item_label()))
        })
    }
}

impl ElementBuilder for CnMenuItem {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    /// What lets the menu read this row.
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

/// `cn.MenuSeparator()` — a rule between groups of rows.
#[extern_widget(namespace = "cn", name = "MenuSeparator")]
pub struct CnMenuSeparator {
    /// Only built when it renders outside a menu.
    #[skip]
    fallback: OnceCell<Div>,
}

impl CnMenuSeparator {
    fn get_or_build(&self) -> &Div {
        ::blinc_layout::build_once::build_once(&self.fallback, div)
    }
}

impl ElementBuilder for CnMenuSeparator {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.get_or_build().element_type_id()
    }
}
