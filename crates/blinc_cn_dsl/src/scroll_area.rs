//! `cn.ScrollArea` — a scrolling viewport with a styled scrollbar.

use std::cell::{OnceCell, RefCell};

use blinc_dsl_core::extern_widget;
use blinc_layout::div::{ElementBuilder, div};

/// `cn.ScrollArea(direction = "vertical", h = 120.0) { … }` — content
/// that scrolls inside a bounded box.
///
/// ```dsl,ignore
/// cn.ScrollArea(scrollbar = "hover", h = 120.0) {
///     cn.Label("row one")
///     cn.Label("row two")
/// }
/// ```
///
/// `w` / `h` size the VIEWPORT, and they are real props rather than
/// the usual `Div` overlay: the overlay would size the outer wrapper
/// and leave the viewport at its 300x400 default, so the content would
/// never overflow and nothing would scroll. Omitted means that default.
///
/// Unknown values for the string props fall back to the cn default and
/// warn: a typo should cost the styling, not the content.
///
/// **The body is given to the cn builder, not parented afterwards.**
/// A render tree is built by recursing through `children_builders()`,
/// never through a builder's own `build`, so whatever this reports IS
/// the rendered structure. Reporting the body directly would make it
/// this node's children and flatten the scroll widget away — the
/// children would sit beside the viewport rather than inside it, take
/// the outer div's row layout, and all but the first would be dropped.
/// So the body goes into the widget and `children_builders` forwards to
/// the shell, which now contains it.
///
/// That is what the `RefCell` is for: `build` and `children_builders`
/// both take `&self`, and the children have to be MOVED into the
/// builder exactly once. `OnceCell` guarantees the once.
#[extern_widget(namespace = "cn", name = "ScrollArea")]
pub struct CnScrollArea {
    /// `vertical` (default) / `horizontal` / `both`.
    pub direction: String,
    /// `auto` (default) / `always` / `hover` / `never`. `never` still
    /// scrolls, it just draws no bar.
    pub scrollbar: String,
    /// Scrollbar thickness: `small` / `medium` (default) / `large`.
    pub size: String,
    /// Viewport width. Omitted (zero) keeps the cn default.
    pub w: f64,
    /// Viewport height — the bound that makes content scroll. Omitted
    /// (zero) keeps the cn default.
    pub h: f64,
    #[children]
    pub children: RefCell<Vec<Box<dyn ElementBuilder>>>,
    /// Built once, consuming `children`. Also keeps `build()` and
    /// `render_props()` describing the same instance.
    #[skip]
    shell: OnceCell<blinc_cn::ScrollArea>,
}

impl CnScrollArea {
    fn get_or_build(&self) -> &blinc_cn::ScrollArea {
        ::blinc_layout::build_once::build_once(&self.shell, || {
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
            // Zero is what an omitted prop reads as, and a zero-sized
            // viewport has no meaning, so it defers to the cn default.
            if self.w > 0.0 {
                b = b.w(self.w as f32);
            }
            if self.h > 0.0 {
                b = b.h(self.h as f32);
            }
            // One content child holding the whole body: the builder
            // takes a single `Div`, so the rows have to share a parent
            // to stack rather than compete with the scrollbar.
            let mut content = div().flex_col();
            for child in self.children.borrow_mut().drain(..) {
                content = content.child_box(child);
            }
            b.child(content).build_final()
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
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    /// The shell's children, which now hold the body. Reporting
    /// `self.children` here is what flattens the widget — see the type
    /// doc.
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
