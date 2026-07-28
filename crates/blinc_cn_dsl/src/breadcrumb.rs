//! `cn.Breadcrumb` — navigation trail. The first widget to take a
//! collection prop.

use std::cell::OnceCell;

use blinc_dsl_core::extern_widget;
use blinc_layout::div::ElementBuilder;

/// `cn.Breadcrumb(items, separator?, size?)` — a navigation trail.
///
/// Props (DSL surface):
/// - `items: Vec<String>` — the trail, root first. The LAST entry is
///   the current page: rendered non-clickable, matching the convention
///   every breadcrumb follows. An empty list renders an empty trail.
/// - `separator: string` — `"chevron"` (default), `"slash"`, or any
///   other value, which is used literally as the separator text.
/// - `size: string` — `"small"`, `"medium"` (default), `"large"`.
///
/// Per-item click handlers are not exposed yet. A DSL callback crosses
/// as a zero-arg function pointer, so telling the handlers apart needs
/// an index argument the FFI doesn't carry. Until then the non-current
/// items are inert.
#[extern_widget(namespace = "cn", name = "Breadcrumb")]
pub struct CnBreadcrumb {
    pub items: Vec<String>,
    pub separator: String,
    pub size: String,
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::BreadcrumbBuilder>,
}

impl CnBreadcrumb {
    fn get_or_build(&self) -> &blinc_cn::BreadcrumbBuilder {
        self.built.get_or_init(|| self.to_cn_builder())
    }

    fn to_cn_builder(&self) -> blinc_cn::BreadcrumbBuilder {
        let size = match self.size.as_str() {
            "small" | "sm" => blinc_cn::BreadcrumbSize::Small,
            "large" | "lg" => blinc_cn::BreadcrumbSize::Large,
            "" | "medium" | "md" => blinc_cn::BreadcrumbSize::Medium,
            other => {
                tracing::warn!(
                    size = %other,
                    "cn.Breadcrumb: unknown size — falling back to `medium`",
                );
                blinc_cn::BreadcrumbSize::Medium
            }
        };
        let separator = match self.separator.as_str() {
            "" | "chevron" => blinc_cn::BreadcrumbSeparator::Chevron,
            "slash" => blinc_cn::BreadcrumbSeparator::Slash,
            // Anything else is the separator itself, so `separator =
            // "→"` works without a new keyword per glyph.
            other => blinc_cn::BreadcrumbSeparator::Text(other.to_string()),
        };

        let mut b = blinc_cn::breadcrumb().size(size).separator(separator);
        let last = self.items.len().saturating_sub(1);
        for (i, label) in self.items.iter().enumerate() {
            if i == last {
                b = b.current(label.clone());
            } else {
                // No-op handler: `item` requires one, and per-item
                // callbacks can't cross the FFI yet.
                b = b.item(label.clone(), || {});
            }
        }
        b
    }
}

impl ElementBuilder for CnBreadcrumb {
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
