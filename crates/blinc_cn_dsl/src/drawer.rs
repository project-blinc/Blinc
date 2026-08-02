//! `cn.Drawer` — a navigation panel that slides in from a side.

use std::sync::Mutex;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::{Div, ElementBuilder};
use blinc_layout::widgets::overlay_stack::OverlayHandle;

/// `cn.Drawer(open = signal, side = "left", title = "…") { …body… }` —
/// a side panel that follows a signal.
///
/// ```dsl,ignore
/// signal nav: bool = false
///
/// cn.Button("Menu", on_click = || nav.set(true))
/// cn.Drawer(open = nav, title = "Navigation") {
///     cn.Label("Wherever you like")
/// }
/// ```
///
/// A drawer differs from a sheet in intent rather than mechanism: a
/// drawer is for navigation and opens from the left or right, a sheet
/// is for a task and can come from any edge.
///
/// Same contract as `cn.Dialog` — see [`crate::modal`].
#[extern_widget(namespace = "cn", name = "Drawer")]
pub struct CnDrawer {
    /// Whether the drawer is out. Writing it opens and closes.
    pub open: Reactive<bool>,
    /// `left` (default) / `right`. A drawer has no top or bottom.
    pub side: String,
    /// `narrow` / `medium` (default) / `wide`.
    pub size: String,
    /// Heading.
    pub title: String,
    /// Hide the close button. Named for what it turns off, since an
    /// omitted bool reads as `false`.
    pub hide_close: bool,
    /// Fired when the drawer closes, however it was dismissed. Zero
    /// when omitted.
    pub on_close: i64,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
}

#[derive(Clone)]
struct DrawerProps {
    open: blinc_core::reactive::State<bool>,
    side: Option<blinc_cn::DrawerSide>,
    size: Option<blinc_cn::DrawerSize>,
    title: String,
    hide_close: bool,
    on_close: i64,
    content: Option<std::sync::Arc<dyn Fn() -> Div + Send + Sync>>,
}

impl CnDrawer {
    fn to_element(&self) -> blinc_layout::stateful::Stateful<()> {
        let open = crate::bridge::bool_state(&self.open);
        let props = self.props();
        crate::modal::watcher(open, move || props.show())
    }

    fn props(&self) -> DrawerProps {
        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));
        DrawerProps {
            open: crate::bridge::bool_state(&self.open),
            side: self.side(),
            size: self.size(),
            title: self.title.clone(),
            hide_close: self.hide_close,
            on_close: self.on_close,
            content: crate::modal::content_recipe(children),
        }
    }

    fn side(&self) -> Option<blinc_cn::DrawerSide> {
        use blinc_cn::DrawerSide as S;
        match self.side.as_str() {
            "" => None,
            "left" => Some(S::Left),
            "right" => Some(S::Right),
            other => {
                tracing::warn!(side = %other, "cn.Drawer: unknown side — left or right");
                None
            }
        }
    }

    fn size(&self) -> Option<blinc_cn::DrawerSize> {
        use blinc_cn::DrawerSize as S;
        match self.size.as_str() {
            "" => None,
            "narrow" => Some(S::Narrow),
            "medium" | "md" => Some(S::Medium),
            "wide" => Some(S::Wide),
            other => {
                tracing::warn!(size = %other, "cn.Drawer: unknown size");
                None
            }
        }
    }
}

impl DrawerProps {
    fn show(&self) -> OverlayHandle {
        let mut d = blinc_cn::drawer();
        if let Some(side) = self.side {
            d = d.side(side);
        }
        if let Some(size) = self.size {
            d = d.size(size);
        }
        if !self.title.is_empty() {
            d = d.title(self.title.clone());
        }
        if self.hide_close {
            d = d.show_close(false);
        }
        if let Some(content) = self.content.clone() {
            d = d.child(move || content());
        }
        d = d.on_close(crate::modal::closing_handler(
            self.open.clone(),
            self.on_close,
        ));
        d.show()
    }
}

impl ElementBuilder for CnDrawer {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.to_element().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        blinc_layout::RenderProps::default()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[]
    }
}
