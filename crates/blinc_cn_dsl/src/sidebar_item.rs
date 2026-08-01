//! `cn.SidebarItem` — one navigation entry.

use blinc_dsl_core::extern_widget;
use blinc_layout::div::{ElementBuilder, div};

/// Icon size inside a sidebar item, matching what the widget draws.
pub(crate) const ICON_SIZE: f32 = 18.0;

/// `cn.SidebarItem(label = "Forms", icon = "square-pen", on_click = || …)`
/// — a row in a [`crate::sidebar::CnSidebar`].
///
/// ```dsl,ignore
/// cn.Sidebar(collapsed = shut) {
///     cn.SidebarItem(label = "Forms", icon = "square-pen", on_click = || {
///         page.set(0)
///     })
/// }
/// ```
///
/// `icon` names a Lucide icon; see [`crate::icon::resolve`] for the
/// escape hatch. The sidebar reads this through `as_any` and draws the
/// row itself, so an item outside one renders nothing.
#[extern_widget(namespace = "cn", name = "SidebarItem")]
pub struct CnSidebarItem {
    /// Row text. Hidden when the sidebar is collapsed, leaving the icon.
    pub label: String,
    /// Lucide icon name, or a complete `<svg>` string.
    pub icon: String,
    /// Start highlighted.
    ///
    /// An initial value only: the sidebar tracks the selected row itself
    /// and takes over the moment anything is clicked. Mark the row whose
    /// page the app starts on.
    pub active: bool,
    /// Zero when omitted. Fired on click, after the sidebar records the
    /// row as selected.
    pub on_click: i64,
    /// Nothing reads these — an item's content is its label and icon.
    /// Declared so a body block is a no-op rather than a parse error.
    #[children]
    pub children: Vec<Box<dyn ElementBuilder>>,
}

impl CnSidebarItem {
    /// The click handler, as something the cn builder can hold.
    ///
    /// Zyntax mints a zero-arg `extern "C" fn()` for a DSL closure and
    /// hands the pointer across as `i64`; signal writes inside it route
    /// through the host externs as usual.
    pub(crate) fn click_handler(&self) -> impl Fn() + Send + Sync + 'static {
        let ptr = self.on_click;
        move || {
            if ptr != 0 {
                type ClosureFn = extern "C" fn();
                let func: ClosureFn = unsafe { std::mem::transmute(ptr) };
                func();
            }
        }
    }
}

impl ElementBuilder for CnSidebarItem {
    /// Only reached outside a sidebar, where there is no navigation for
    /// a row to belong to.
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        tracing::warn!(
            label = %self.label,
            "cn.SidebarItem outside a cn.Sidebar — nothing to render it",
        );
        div().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        blinc_layout::RenderProps::default()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[]
    }

    /// What lets `cn.Sidebar` read the label, icon and handler.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}
