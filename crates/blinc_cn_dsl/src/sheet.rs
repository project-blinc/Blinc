//! `cn.Sheet` — a panel that slides in from an edge.

use std::sync::Mutex;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::{Div, ElementBuilder};
use blinc_layout::widgets::overlay_stack::OverlayHandle;

/// `cn.Sheet(open = signal, side = "right", title = "…") { …body… }` —
/// an edge panel that follows a signal.
///
/// ```dsl,ignore
/// signal filters: bool = false
///
/// cn.Button("Filters", on_click = || filters.set(true))
/// cn.Sheet(open = filters, side = "right", title = "Filters") {
///     cn.Label("Anything at all")
/// }
/// ```
///
/// Same contract as `cn.Dialog`: the signal is the handle, and the
/// element draws nothing — see [`crate::modal`] for why it has to be
/// subscribed and why the live handle lives outside the widget.
#[extern_widget(namespace = "cn", name = "Sheet")]
pub struct CnSheet {
    /// Whether the sheet is out. Writing it opens and closes.
    pub open: Reactive<bool>,
    /// `left` / `right` (default) / `top` / `bottom`.
    pub side: String,
    /// `small` / `medium` (default) / `large` / `full`.
    pub size: String,
    /// Heading.
    pub title: String,
    /// Line under the heading.
    pub description: String,
    /// Draw the close button. Named for what it turns off, since an
    /// omitted bool reads as `false`.
    pub hide_close: bool,
    /// Fired when the sheet closes, however it was dismissed. Zero when
    /// omitted.
    pub on_close: i64,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
}

#[derive(Clone)]
struct SheetProps {
    open: blinc_core::reactive::State<bool>,
    side: Option<blinc_cn::SheetSide>,
    size: Option<blinc_cn::SheetSize>,
    title: String,
    description: String,
    hide_close: bool,
    on_close: i64,
    content: Option<std::sync::Arc<dyn Fn() -> Div + Send + Sync>>,
}

impl CnSheet {
    fn to_element(&self) -> blinc_layout::stateful::Stateful<()> {
        let open = crate::bridge::bool_state(&self.open);
        let props = self.props();
        crate::modal::watcher(open, move || props.show())
    }

    fn props(&self) -> SheetProps {
        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));
        SheetProps {
            open: crate::bridge::bool_state(&self.open),
            side: self.side(),
            size: self.size(),
            title: self.title.clone(),
            description: self.description.clone(),
            hide_close: self.hide_close,
            on_close: self.on_close,
            content: crate::modal::content_recipe(children),
        }
    }

    fn side(&self) -> Option<blinc_cn::SheetSide> {
        use blinc_cn::SheetSide as S;
        match self.side.as_str() {
            "" => None,
            "left" => Some(S::Left),
            "right" => Some(S::Right),
            "top" => Some(S::Top),
            "bottom" => Some(S::Bottom),
            other => {
                tracing::warn!(side = %other, "cn.Sheet: unknown side");
                None
            }
        }
    }

    fn size(&self) -> Option<blinc_cn::SheetSize> {
        use blinc_cn::SheetSize as S;
        match self.size.as_str() {
            "" => None,
            "small" | "sm" => Some(S::Small),
            "medium" | "md" => Some(S::Medium),
            "large" | "lg" => Some(S::Large),
            "full" => Some(S::Full),
            other => {
                tracing::warn!(size = %other, "cn.Sheet: unknown size");
                None
            }
        }
    }
}

impl SheetProps {
    fn show(&self) -> OverlayHandle {
        let mut s = blinc_cn::sheet();
        if let Some(side) = self.side {
            s = s.side(side);
        }
        if let Some(size) = self.size {
            s = s.size(size);
        }
        if !self.title.is_empty() {
            s = s.title(self.title.clone());
        }
        if !self.description.is_empty() {
            s = s.description(self.description.clone());
        }
        if self.hide_close {
            s = s.show_close(false);
        }
        if let Some(content) = self.content.clone() {
            s = s.content(move || content());
        }
        // Whatever dismissed it — close button, backdrop, Escape —
        // clears the signal, or the next frame would slide it back out.
        s = s.on_close(crate::modal::closing_handler(
            self.open.clone(),
            self.on_close,
        ));
        s.show()
    }
}

impl ElementBuilder for CnSheet {
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
