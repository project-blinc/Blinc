//! `cn.ScrollArea` — a scrolling viewport with a styled scrollbar.

use std::cell::OnceCell;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::ElementBuilder;

/// `cn.ScrollArea(direction = "vertical") { … }` — content that scrolls
/// inside a fixed box.
///
/// ```dsl,ignore
/// cn.ScrollArea(direction = "vertical", scrollbar = "hover") {
///     cn.Label("row one")
///     cn.Label("row two")
/// }
/// ```
///
/// The box needs a height for there to be anything to scroll; that
/// rides the universal `Div`-style overlay surface (`h`, `w`, padding)
/// rather than per-widget props, matching every other cn wrapper.
///
/// Unknown values for the string props fall back to the cn default and
/// warn: a typo should cost the styling, not the content.
#[extern_widget(namespace = "cn", name = "ScrollArea")]
pub struct CnScrollArea {
    /// `vertical` (default) / `horizontal` / `both`.
    pub direction: String,
    /// `auto` (default) / `always` / `hover` / `never`. `never` still
    /// scrolls, it just draws no bar.
    pub scrollbar: String,
    /// Scrollbar thickness: `small` / `medium` (default) / `large`.
    pub size: String,
    #[children]
    pub children: Vec<Box<dyn ElementBuilder>>,
    /// Built once so `build()` and `render_props()` describe the same
    /// instance, and so the identity methods can borrow from it.
    #[skip]
    shell: OnceCell<blinc_cn::ScrollArea>,
}

impl CnScrollArea {
    fn get_or_build(&self) -> &blinc_cn::ScrollArea {
        self.shell.get_or_init(|| {
            let mut b = blinc_cn::scroll_area();
            if let Some(direction) = self.direction() {
                b = b.direction(direction);
            }
            if let Some(visibility) = self.scrollbar() {
                b = b.scrollbar(visibility);
            }
            if let Some(size) = self.size() {
                b = b.size(size);
            }
            b.build_final()
        })
    }

    fn direction(&self) -> Option<blinc_layout::widgets::scroll::ScrollDirection> {
        use blinc_layout::widgets::scroll::ScrollDirection as D;
        match self.direction.as_str() {
            "" => None,
            "vertical" => Some(D::Vertical),
            "horizontal" => Some(D::Horizontal),
            "both" => Some(D::Both),
            other => {
                tracing::warn!(direction = %other, "cn.ScrollArea: unknown direction");
                None
            }
        }
    }

    fn scrollbar(&self) -> Option<blinc_cn::ScrollbarVisibility> {
        use blinc_cn::ScrollbarVisibility as V;
        match self.scrollbar.as_str() {
            "" => None,
            "auto" => Some(V::Auto),
            "always" => Some(V::Always),
            "hover" => Some(V::Hover),
            "never" => Some(V::Never),
            other => {
                tracing::warn!(scrollbar = %other, "cn.ScrollArea: unknown scrollbar mode");
                None
            }
        }
    }

    fn size(&self) -> Option<blinc_cn::ScrollAreaSize> {
        use blinc_cn::ScrollAreaSize as S;
        match self.size.as_str() {
            "" => None,
            "small" => Some(S::Small),
            "medium" => Some(S::Medium),
            "large" => Some(S::Large),
            other => {
                tracing::warn!(size = %other, "cn.ScrollArea: unknown size");
                None
            }
        }
    }
}

impl ElementBuilder for CnScrollArea {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        // Same shape as `cn.Card`: build the shell, then parent the DSL
        // body to it, since the cn builder's `child` takes owned values
        // and this wrapper holds shared refs.
        let node = self.get_or_build().build(tree);
        for child in &self.children {
            let child_node = child.build(tree);
            tree.add_child(node, child_node);
        }
        node
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &self.children
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
