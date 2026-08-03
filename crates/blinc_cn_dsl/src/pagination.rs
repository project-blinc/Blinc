//! `cn.Pagination` — page numbers bound to a signal.

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

/// `cn.Pagination(page = signal, total = 10.0)` — first / previous, a
/// window of numbers, next / last.
///
/// ```dsl,ignore
/// signal page: i32 = 1
///
/// cn.Pagination(page = page, total = 12.0, visible = 5.0, show_first_last = true)
/// ```
///
/// `page` binds both ways: clicking a number writes the signal, and
/// writing the signal moves the highlight. Pages are 1-based, so a
/// signal seeded at 0 reads as page 1.
///
/// No children: the numbers are derived from `total` rather than
/// declared, which is what separates this from the widgets whose
/// options are `cn.Option` children.
#[extern_widget(namespace = "cn", name = "Pagination")]
pub struct CnPagination {
    /// Which page is current, 1-based.
    pub page: Reactive<i32>,
    /// How many pages there are. Below 1 draws nothing.
    pub total: f64,
    /// How many numbers to show around the current one. Default 5.
    pub visible: f64,
    /// Add first / last buttons beside the arrows.
    pub show_first_last: bool,
    /// `small` / `medium` (default) / `large`.
    pub size: String,
    /// Built once.
    #[skip]
    shell: OnceCell<blinc_cn::PaginationBuilder>,
}

impl CnPagination {
    fn get_or_build(&self) -> &blinc_cn::PaginationBuilder {
        ::blinc_layout::build_once::build_once(&self.shell, || self.make())
    }

    fn make(&self) -> blinc_cn::PaginationBuilder {
        // A page is a whole number, so it crosses as one: no rounding,
        // and no float clamp standing in for "at least page 1". Handing
        // over the signal rather than a narrowed copy keeps one id, so
        // the binding writes back to what the author declared.
        let page = blinc_cn::PageValue::I32(crate::bridge::i32_state(&self.page));

        let total = self.total.max(0.0) as usize;
        let mut b = blinc_cn::PaginationBuilder::with_page_value(total, page).size(self.size());
        if self.visible > 0.0 {
            b = b.visible_pages(self.visible as usize);
        }
        if self.show_first_last {
            b = b.show_first_last(true);
        }
        b
    }

    fn size(&self) -> blinc_cn::PaginationSize {
        match self.size.as_str() {
            "small" | "sm" => blinc_cn::PaginationSize::Small,
            "large" | "lg" => blinc_cn::PaginationSize::Large,
            "" | "medium" | "md" => blinc_cn::PaginationSize::Medium,
            other => {
                tracing::warn!(size = %other, "cn.Pagination: unknown size");
                blinc_cn::PaginationSize::Medium
            }
        }
    }
}

impl ElementBuilder for CnPagination {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
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

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }
}
