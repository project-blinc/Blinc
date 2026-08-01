//! `cn.SidebarSection` — a titled group of navigation entries.

use std::sync::Mutex;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::{ElementBuilder, div};

/// `cn.SidebarSection(title = "Widgets") { …items… }` — groups rows
/// under a heading that hides when the sidebar collapses.
///
/// ```dsl,ignore
/// cn.Sidebar(collapsed = shut) {
///     cn.SidebarSection(title = "Widgets") {
///         cn.SidebarItem(label = "Forms", icon = "square-pen")
///         cn.SidebarItem(label = "Feedback", icon = "bell")
///     }
/// }
/// ```
///
/// Items may also sit directly in the sidebar, which puts them in an
/// untitled group above the first section.
#[extern_widget(namespace = "cn", name = "SidebarSection")]
pub struct CnSidebarSection {
    /// Heading text. Empty makes the group untitled, which still
    /// separates it from what came before.
    pub title: String,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
}

impl CnSidebarSection {
    /// Take the rows, leaving the section empty. The sidebar calls this
    /// once while building.
    pub(crate) fn take_children(&self) -> Vec<Box<dyn ElementBuilder>> {
        std::mem::take(&mut *self.children.lock().expect("children mutex"))
    }
}

impl ElementBuilder for CnSidebarSection {
    /// Only reached outside a sidebar, where a group of navigation rows
    /// has nothing to navigate.
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        tracing::warn!(
            title = %self.title,
            "cn.SidebarSection outside a cn.Sidebar — nothing to render it",
        );
        div().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        blinc_layout::RenderProps::default()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[]
    }

    /// What lets `cn.Sidebar` read the title and the rows under it.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}
