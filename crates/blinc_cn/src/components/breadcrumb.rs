//! Breadcrumb navigation component
//!
//! Shows the user's current location in a site hierarchy.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Simple breadcrumb
//! cn::breadcrumb()
//!     .item("Home", || println!("Go home"))
//!     .item("Products", || println!("Go products"))
//!     .item("Electronics", || {})  // Current page
//!
//! // With custom separator
//! cn::breadcrumb()
//!     .separator(">")
//!     .item("Home", || {})
//!     .item("Settings", || {})
//!
//! // With icons
//! cn::breadcrumb()
//!     .item_with_icon("Home", home_svg, || {})
//!     .item("Profile", || {})
//! ```

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use blinc_core::reactive::{ReactiveGraph, computed};
use blinc_layout::div::{Div, ElementBuilder, ElementTypeId};
use blinc_layout::element::CursorStyle;
use blinc_layout::prelude::*;
use blinc_layout::{InstanceKey, Interaction};
use blinc_theme::{ColorToken, ThemeState};

use crate::reactive_props::clone_reactive;
use blinc_layout::binding::{IntoReactive, Reactive};

/// Default separator SVG (chevron right)
const CHEVRON_RIGHT_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"#;

/// A single breadcrumb item
pub struct BreadcrumbItem {
    /// Display label
    label: Reactive<String>,
    /// Optional icon SVG
    icon: Option<String>,
    /// Click handler (None for current page)
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Clone for BreadcrumbItem {
    fn clone(&self) -> Self {
        Self {
            label: clone_reactive(&self.label),
            icon: self.icon.clone(),
            on_click: self.on_click.clone(),
        }
    }
}

impl BreadcrumbItem {
    /// Create a new breadcrumb item. A signal here replaces the label in
    /// place.
    pub fn new(label: impl IntoReactive<String>) -> Self {
        Self {
            label: label.into_reactive(),
            icon: None,
            on_click: None,
        }
    }

    /// Add an icon to the item
    pub fn icon(mut self, svg: impl Into<String>) -> Self {
        self.icon = Some(svg.into());
        self
    }

    /// Add a click handler
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_click = Some(Arc::new(handler));
        self
    }

    /// Check if this item is clickable
    fn is_clickable(&self) -> bool {
        self.on_click.is_some()
    }
}

/// Breadcrumb size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BreadcrumbSize {
    /// Small breadcrumb (12px font)
    Small,
    /// Default size (14px font)
    #[default]
    Medium,
    /// Large breadcrumb (16px font)
    Large,
}

impl BreadcrumbSize {
    fn font_size(&self) -> f32 {
        match self {
            BreadcrumbSize::Small => 12.0,
            BreadcrumbSize::Medium => 14.0,
            BreadcrumbSize::Large => 16.0,
        }
    }

    fn icon_size(&self) -> f32 {
        match self {
            BreadcrumbSize::Small => 12.0,
            BreadcrumbSize::Medium => 14.0,
            BreadcrumbSize::Large => 16.0,
        }
    }

    fn gap(&self) -> f32 {
        match self {
            BreadcrumbSize::Small => 4.0,
            BreadcrumbSize::Medium => 8.0,
            BreadcrumbSize::Large => 12.0,
        }
    }
}

/// Separator type for breadcrumb items
#[derive(Clone, Default)]
pub enum BreadcrumbSeparator {
    /// Chevron right icon (default)
    #[default]
    Chevron,
    /// Slash character
    Slash,
    /// Custom text separator
    Text(String),
    /// Custom SVG separator
    Svg(String),
}

/// Breadcrumb component
pub struct Breadcrumb {
    inner: Div,
}

impl Breadcrumb {
    fn from_builder(builder: &BreadcrumbBuilder) -> Self {
        let font_size = builder.size.font_size();
        let icon_size = builder.size.icon_size();
        let gap = builder.size.gap();
        let items_count = builder.items.len();

        let mut container = div()
            .class("cn-breadcrumb")
            .flex_row()
            .items_center()
            .gap(gap);

        // Colours and weight are the `.cn-breadcrumb-*` rules: a link
        // carries `data-link`, the current page `aria-current="page"`.
        for (idx, item) in builder.items.iter().enumerate() {
            let is_last = idx == items_count - 1;
            let mut item_div = div()
                .class("cn-breadcrumb-item")
                .flex_row()
                .items_center()
                .gap(4.0);
            if let Some(ref icon_svg) = item.icon {
                item_div = item_div.child(
                    div()
                        .self_center()
                        .child(svg(icon_svg).size(icon_size, icon_size)),
                );
            }
            item_div = item_div.child(
                div().self_center().child(
                    blinc_layout::text::Text::bound(clone_reactive(&item.label))
                        .size(font_size)
                        .no_cursor()
                        .no_wrap(),
                ),
            );
            if is_last {
                item_div = item_div.attr("aria-current", "page");
            } else if item.is_clickable() {
                let on_click = item.on_click.clone();
                item_div = item_div
                    .attr("data-link", "")
                    .cursor(CursorStyle::Pointer)
                    .on_click(move |_| {
                        if let Some(ref handler) = on_click {
                            handler();
                        }
                    });
            }
            container = container.child(item_div);

            if !is_last {
                let separator = div().class("cn-breadcrumb-separator").items_center();
                let separator = match &builder.separator {
                    BreadcrumbSeparator::Chevron => {
                        separator.child(svg(CHEVRON_RIGHT_SVG).size(icon_size, icon_size))
                    }
                    BreadcrumbSeparator::Slash => {
                        separator.child(text("/").size(font_size).no_wrap())
                    }
                    BreadcrumbSeparator::Text(s) => {
                        separator.child(text(s).size(font_size).no_wrap())
                    }
                    BreadcrumbSeparator::Svg(svg_str) => {
                        separator.child(svg(svg_str).size(icon_size, icon_size))
                    }
                };
                container = container.child(separator);
            }
        }

        Self { inner: container }
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.inner = self.inner.class(name);
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }
}

impl Deref for Breadcrumb {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Breadcrumb {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for Breadcrumb {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(&self.inner)
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementBuilder::element_type_id(&self.inner)
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Builder for breadcrumb navigation
pub struct BreadcrumbBuilder {
    key: InstanceKey,
    items: Vec<BreadcrumbItem>,
    separator: BreadcrumbSeparator,
    size: BreadcrumbSize,
    built: std::cell::OnceCell<Breadcrumb>,
}

impl BreadcrumbBuilder {
    /// Create a new breadcrumb builder
    #[track_caller]
    pub fn new() -> Self {
        Self {
            key: InstanceKey::new("breadcrumb"),
            items: Vec::new(),
            separator: BreadcrumbSeparator::default(),
            size: BreadcrumbSize::default(),
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the breadcrumb
    fn get_or_build(&self) -> &Breadcrumb {
        ::blinc_layout::build_once::build_once(&self.built, || Breadcrumb::from_builder(self))
    }

    /// Add a breadcrumb item
    pub fn item<F>(mut self, label: impl IntoReactive<String>, on_click: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.items
            .push(BreadcrumbItem::new(label).on_click(on_click));
        self
    }

    /// Add a breadcrumb item with an icon
    pub fn item_with_icon<F>(
        mut self,
        label: impl IntoReactive<String>,
        icon: impl Into<String>,
        on_click: F,
    ) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.items
            .push(BreadcrumbItem::new(label).icon(icon).on_click(on_click));
        self
    }

    /// Add a non-clickable item (typically the current page)
    pub fn current(mut self, label: impl IntoReactive<String>) -> Self {
        self.items.push(BreadcrumbItem::new(label));
        self
    }

    /// Add a non-clickable item with icon (typically the current page)
    pub fn current_with_icon(
        mut self,
        label: impl IntoReactive<String>,
        icon: impl Into<String>,
    ) -> Self {
        self.items.push(BreadcrumbItem::new(label).icon(icon));
        self
    }

    /// Set the separator type
    pub fn separator(mut self, sep: BreadcrumbSeparator) -> Self {
        self.separator = sep;
        self
    }

    /// Use slash separator
    pub fn slash_separator(mut self) -> Self {
        self.separator = BreadcrumbSeparator::Slash;
        self
    }

    /// Use custom text separator
    pub fn text_separator(mut self, text: impl Into<String>) -> Self {
        self.separator = BreadcrumbSeparator::Text(text.into());
        self
    }

    /// Set the size
    pub fn size(mut self, size: BreadcrumbSize) -> Self {
        self.size = size;
        self
    }

    /// Set small size
    pub fn small(mut self) -> Self {
        self.size = BreadcrumbSize::Small;
        self
    }

    /// Set large size
    pub fn large(mut self) -> Self {
        self.size = BreadcrumbSize::Large;
        self
    }
}

impl Default for BreadcrumbBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ElementBuilder for BreadcrumbBuilder {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.get_or_build().event_handlers()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.get_or_build().element_type_id()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }
}

/// Create a breadcrumb navigation component
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// cn::breadcrumb()
///     .item("Home", || println!("Navigate home"))
///     .item("Products", || println!("Navigate to products"))
///     .current("Laptop")  // Current page, not clickable
/// ```
#[track_caller]
pub fn breadcrumb() -> BreadcrumbBuilder {
    BreadcrumbBuilder::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_theme() {
        let _ = ThemeState::try_get().unwrap_or_else(|| {
            ThemeState::init_default();
            ThemeState::get()
        });
    }

    #[test]
    fn test_breadcrumb_basic() {
        init_theme();
        let _ = breadcrumb()
            .item("Home", || {})
            .item("Products", || {})
            .current("Item");
    }

    #[test]
    fn test_breadcrumb_with_icons() {
        init_theme();
        let home_icon = r#"<svg></svg>"#;
        let _ = breadcrumb()
            .item_with_icon("Home", home_icon, || {})
            .current("Page");
    }

    #[test]
    fn test_breadcrumb_separators() {
        init_theme();
        let _ = breadcrumb().slash_separator().item("A", || {}).current("B");

        let _ = breadcrumb()
            .text_separator(">")
            .item("A", || {})
            .current("B");
    }

    #[test]
    fn test_breadcrumb_sizes() {
        init_theme();
        let _ = breadcrumb().small().item("A", || {}).current("B");
        let _ = breadcrumb()
            .size(BreadcrumbSize::Large)
            .item("A", || {})
            .current("B");
    }
}
