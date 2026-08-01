//! `cn.SidebarContent` — the main area beside the navigation.

use std::sync::Mutex;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::{ElementBuilder, div};

/// `cn.SidebarContent { … }` — what fills the space next to the rows.
///
/// ```dsl,ignore
/// cn.Sidebar(collapsed = shut) {
///     cn.SidebarItem(label = "Forms", icon = "square-pen", on_click = || page.set(0))
///     cn.SidebarContent {
///         with {
///             if page.get() == 0 { FormWidgets() } else { FeedbackWidgets() }
///         }
///     }
/// }
/// ```
///
/// Content given this way shares a container with the navigation, so it
/// widens in step with a collapsing sidebar rather than jumping once the
/// animation lands. Content placed beside `cn.Sidebar` in an ordinary
/// row still works and still reflows, it just does not animate with it.
///
/// It is built once, outside the rail's own rebuilds, so clicking a row
/// or collapsing the rail leaves it alone. Only a write it actually
/// reads moves it.
///
/// Put the page switch in a `with` region, as above: the region confines
/// the re-render to the content area, so the sidebar's own springs keep
/// whatever they had in flight.
#[extern_widget(namespace = "cn", name = "SidebarContent")]
pub struct CnSidebarContent {
    /// Handle for this area's scroller, from a `ref name: Scroll`
    /// declaration: `ref = pages`. Zero when omitted.
    ///
    /// The handle itself, not a name to look one up by, so two of these
    /// cannot collide however they are spelled.
    pub r#ref: i64,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
}

impl CnSidebarContent {
    /// Take the body, leaving the slot empty.
    pub(crate) fn take_children(&self) -> Vec<Box<dyn ElementBuilder>> {
        std::mem::take(&mut *self.children.lock().expect("children mutex"))
    }
}

impl ElementBuilder for CnSidebarContent {
    /// Only reached outside a sidebar, where there is no navigation for
    /// this to be the content of.
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        tracing::warn!("cn.SidebarContent outside a cn.Sidebar — nothing to render it");
        div().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        blinc_layout::RenderProps::default()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[]
    }

    /// What lets `cn.Sidebar` claim the body as its content area.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}
