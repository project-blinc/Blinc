//! Pagination component for navigating pages
//!
//! Displays page navigation controls with previous/next buttons and page numbers.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Basic pagination
//! cn::pagination()
//!     .total_pages(10)
//!     .current_page(page_state.clone())
//!     .on_page_change(|page| println!("Go to page {}", page))
//!
//! // With custom visible pages
//! cn::pagination()
//!     .total_pages(100)
//!     .current_page(page_state.clone())
//!     .visible_pages(7)
//!     .show_first_last(true)
//! ```

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use blinc_core::State;
use blinc_core::reactive::{ReactiveGraph, computed};
use blinc_layout::InstanceKey;
use blinc_layout::div::{Div, ElementBuilder, ElementTypeId};
use blinc_layout::element::CursorStyle;
use blinc_layout::prelude::*;
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

/// Chevron left SVG
const CHEVRON_LEFT_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6"/></svg>"#;

/// Chevron right SVG
const CHEVRON_RIGHT_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"#;

/// Double chevron left (first page) SVG
const CHEVRONS_LEFT_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m11 17-5-5 5-5"/><path d="m18 17-5-5 5-5"/></svg>"#;

/// Double chevron right (last page) SVG
const CHEVRONS_RIGHT_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 17 5-5-5-5"/><path d="m13 17 5-5-5-5"/></svg>"#;

/// Ellipsis SVG
const ELLIPSIS_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/></svg>"#;

/// Pagination size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaginationSize {
    /// Small pagination (28px buttons)
    Small,
    /// Default size (32px buttons)
    #[default]
    Medium,
    /// Large pagination (40px buttons)
    Large,
}

impl PaginationSize {
    fn button_size(&self) -> f32 {
        match self {
            PaginationSize::Small => 24.0,
            PaginationSize::Medium => 32.0,
            PaginationSize::Large => 40.0,
        }
    }

    fn font_size(&self) -> f32 {
        match self {
            PaginationSize::Small => 12.0,
            PaginationSize::Medium => 14.0,
            PaginationSize::Large => 16.0,
        }
    }

    fn icon_size(&self) -> f32 {
        match self {
            PaginationSize::Small => 12.0,
            PaginationSize::Medium => 16.0,
            PaginationSize::Large => 20.0,
        }
    }

    fn gap(&self) -> f32 {
        match self {
            PaginationSize::Small => 4.0,
            PaginationSize::Medium => 4.0,
            PaginationSize::Large => 8.0,
        }
    }
}

/// Pagination component
pub struct Pagination {
    inner: Div,
}

impl Pagination {
    fn from_builder(builder: &PaginationBuilder) -> Self {
        let theme = ThemeState::get();
        let text_secondary = theme.color(ColorToken::TextSecondary);
        let text_tertiary = theme.color(ColorToken::TextTertiary);
        let border = theme.color(ColorToken::Border);
        let radius = theme.radius(RadiusToken::Md);

        let button_size = builder.size.button_size();
        let font_size = builder.size.font_size();
        let icon_size = builder.size.icon_size();
        let gap = builder.size.gap();

        let total_pages = builder.total_pages;
        let visible_pages = builder.visible_pages;
        let show_first_last = builder.show_first_last;
        let on_page_change = builder.on_page_change.clone();
        let page_state = builder.current_page.clone();

        // The row is built once: a keyed list of slots worked out from the
        // current page. A page that stays in view keeps its button; the
        // active and disabled looks are classes bound to the page.
        let page = page_state.clone();
        let slots = computed(move |g: &ReactiveGraph| {
            page_slots(page.read(g), total_pages, visible_pages, show_first_last)
        });

        let row = div()
            .class("cn-pagination")
            .flex_row()
            .items_center()
            .gap(gap)
            .for_each(
                slots,
                |slot: &Slot| *slot,
                move |slot: Slot| -> Div {
                    let page = page_state.clone();
                    let on_change = on_page_change.clone();
                    // Where a click on this slot goes from `current`, if anywhere.
                    let target = move |current: usize| -> Option<usize> {
                        match slot {
                            Slot::First => (current > 1).then_some(1),
                            Slot::Prev => (current > 1).then(|| current - 1),
                            Slot::Next => (current < total_pages).then(|| current + 1),
                            Slot::Last => (current < total_pages).then_some(total_pages),
                            Slot::Page(n) => (current != n).then_some(n),
                            Slot::StartEllipsis | Slot::EndEllipsis => None,
                        }
                    };
                    let go = {
                        let page = page.clone();
                        move || {
                            if let Some(to) = target(page.get()) {
                                page.set(to);
                                if let Some(ref cb) = on_change {
                                    cb(to);
                                }
                            }
                        }
                    };
                    let icon = match slot {
                        Slot::First => Some(CHEVRONS_LEFT_SVG),
                        Slot::Prev => Some(CHEVRON_LEFT_SVG),
                        Slot::Next => Some(CHEVRON_RIGHT_SVG),
                        Slot::Last => Some(CHEVRONS_RIGHT_SVG),
                        _ => None,
                    };
                    match slot {
                        Slot::StartEllipsis | Slot::EndEllipsis => div()
                            .w(button_size)
                            .h(button_size)
                            .items_center()
                            .justify_center()
                            .child(
                                svg(ELLIPSIS_SVG)
                                    .size(icon_size, icon_size)
                                    .color(text_tertiary),
                            ),
                        Slot::Page(n) => {
                            let active = {
                                let page = page.clone();
                                computed(move |g: &ReactiveGraph| page.read(g) == n)
                            };
                            let text_color = {
                                let page = page.clone();
                                computed(move |g: &ReactiveGraph| {
                                    if page.read(g) == n {
                                        ThemeState::get().color(ColorToken::TextInverse)
                                    } else {
                                        text_secondary
                                    }
                                })
                            };
                            div()
                                .class("cn-pagination-btn")
                                .class_when("cn-pagination-btn--active", &active)
                                .w(button_size)
                                .h(button_size)
                                .rounded(radius)
                                .items_center()
                                .justify_center()
                                .border(1.0, border)
                                .cursor(CursorStyle::Pointer)
                                .child(
                                    text(n.to_string())
                                        .size(font_size)
                                        .color(&text_color)
                                        .medium()
                                        .pointer_events_none()
                                        .no_cursor(),
                                )
                                .on_click(move |_| go())
                        }
                        _ => {
                            let disabled = {
                                let page = page.clone();
                                computed(move |g: &ReactiveGraph| target(page.read(g)).is_none())
                            };
                            let icon_color = {
                                let page = page.clone();
                                computed(move |g: &ReactiveGraph| {
                                    if target(page.read(g)).is_none() {
                                        text_tertiary.with_alpha(0.5)
                                    } else {
                                        text_secondary
                                    }
                                })
                            };
                            div()
                                .class("cn-pagination-btn")
                                .class_when("cn-pagination-btn--disabled", &disabled)
                                .w(button_size)
                                .h(button_size)
                                .rounded(radius)
                                .items_center()
                                .justify_center()
                                .border(1.0, border)
                                .cursor(CursorStyle::Pointer)
                                .child(
                                    svg(icon.unwrap_or(CHEVRON_LEFT_SVG))
                                        .size(icon_size, icon_size)
                                        .color(&icon_color),
                                )
                                .on_click(move |_| go())
                        }
                    }
                },
            );

        let mut inner = row;
        for c in &builder.classes {
            inner = inner.class(c);
        }
        if let Some(ref id) = builder.user_id {
            inner = inner.id(id);
        }

        Self { inner }
    }
}

/// A place in the pagination row.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Slot {
    First,
    Prev,
    StartEllipsis,
    Page(usize),
    EndEllipsis,
    Next,
    Last,
}

/// The row for `current` of `total` pages, showing `visible` page numbers.
fn page_slots(current: usize, total: usize, visible: usize, first_last: bool) -> Vec<Slot> {
    let (start, end) = calculate_page_range(current, total, visible);
    let with_first_last = first_last && total > visible;
    let mut slots = Vec::new();
    if with_first_last {
        slots.push(Slot::First);
    }
    slots.push(Slot::Prev);
    if start > 1 {
        slots.push(Slot::StartEllipsis);
    }
    slots.extend((start..=end).map(Slot::Page));
    if end < total {
        slots.push(Slot::EndEllipsis);
    }
    slots.push(Slot::Next);
    if with_first_last {
        slots.push(Slot::Last);
    }
    slots
}

/// Calculate the range of page numbers to display
fn calculate_page_range(current: usize, total: usize, visible: usize) -> (usize, usize) {
    if total <= visible {
        return (1, total);
    }

    let half = visible / 2;
    let start = if current <= half + 1 {
        1
    } else if current >= total - half {
        total - visible + 1
    } else {
        current - half
    };

    let end = (start + visible - 1).min(total);
    (start, end)
}

impl Deref for Pagination {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Pagination {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for Pagination {
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

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }
}

/// Builder for pagination component
/// Where a pagination reads and writes its page number.
///
/// A caller holding a `State<usize>` passes it straight through. The
/// DSL has no usize signal — every number there is one `f64` — so it
/// hands over its own signal and this rounds at the boundary. Narrowing
/// to a widget-owned copy instead would mint a second id and leave
/// anything keyed on the first (a `deps` list, an animation key) stale.
#[derive(Clone)]
pub enum PageValue {
    /// A caller's own page counter.
    Usize(State<usize>),
    /// A whole-number signal. What the DSL binds: a page is an integer,
    /// so nothing is rounded on the way through.
    I32(State<i32>),
    /// A number signal of any precision, rounded to a page.
    Number(crate::reactive_props::NumberValue),
}

impl PageValue {
    /// The current page, 1-based. A number below 1 clamps, since page 0
    /// is not a page.
    pub fn get(&self) -> usize {
        match self {
            Self::Usize(s) => s.get(),
            Self::I32(s) => s.get().max(1) as usize,
            Self::Number(n) => n.get().round().max(1.0) as usize,
        }
    }

    /// Write it back in the caller's own representation.
    pub fn set(&self, page: usize) {
        match self {
            Self::Usize(s) => s.set(page),
            Self::I32(s) => s.set(page as i32),
            Self::Number(n) => n.set(page as f32),
        }
    }

    /// The current page, read through `g` so a computed follows it.
    pub fn read(&self, g: &ReactiveGraph) -> usize {
        use crate::reactive_props::NumberValue;
        match self {
            Self::Usize(s) => g.get(s.signal()).unwrap_or(1),
            Self::I32(s) => g.get(s.signal()).unwrap_or(1).max(1) as usize,
            Self::Number(NumberValue::F32(s)) => {
                g.get(s.signal()).unwrap_or(1.0).round().max(1.0) as usize
            }
            Self::Number(NumberValue::F64(s)) => {
                g.get(s.signal()).unwrap_or(1.0).round().max(1.0) as usize
            }
        }
    }

    /// What to subscribe to. One id either way.
    pub fn signal_id(&self) -> blinc_core::reactive::SignalId {
        match self {
            Self::Usize(s) => s.signal_id(),
            Self::I32(s) => s.signal_id(),
            Self::Number(n) => n.signal_id(),
        }
    }
}

pub struct PaginationBuilder {
    key: InstanceKey,
    total_pages: usize,
    current_page: PageValue,
    visible_pages: usize,
    show_first_last: bool,
    size: PaginationSize,
    on_page_change: Option<Arc<dyn Fn(usize) + Send + Sync>>,
    /// User-added CSS classes
    classes: Vec<std::sync::Arc<str>>,
    /// User-set element ID
    user_id: Option<String>,
    built: std::cell::OnceCell<Pagination>,
}

impl PaginationBuilder {
    /// Create a new pagination builder
    #[track_caller]
    /// Drive the page from a number signal of any precision, for a
    /// caller with no `State<usize>` to offer — the DSL, where every
    /// number is one `f64`.
    pub fn with_page_value(total_pages: usize, page: PageValue) -> Self {
        Self {
            key: InstanceKey::new("pagination"),
            total_pages,
            current_page: page,
            visible_pages: 5,
            show_first_last: false,
            size: PaginationSize::default(),
            on_page_change: None,
            classes: Vec::new(),
            user_id: None,
            built: std::cell::OnceCell::new(),
        }
    }

    pub fn new(total_pages: usize, current_page: State<usize>) -> Self {
        Self {
            key: InstanceKey::new("pagination"),
            total_pages,
            current_page: PageValue::Usize(current_page),
            visible_pages: 5,
            show_first_last: false,
            size: PaginationSize::default(),
            on_page_change: None,
            classes: Vec::new(),
            user_id: None,
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the pagination
    fn get_or_build(&self) -> &Pagination {
        ::blinc_layout::build_once::build_once(&self.built, || Pagination::from_builder(self))
    }

    /// Set the number of visible page buttons
    pub fn visible_pages(mut self, count: usize) -> Self {
        self.visible_pages = count.max(3); // At least 3 visible pages
        self
    }

    /// Show first/last page buttons
    pub fn show_first_last(mut self, show: bool) -> Self {
        self.show_first_last = show;
        self
    }

    /// Set the size
    pub fn size(mut self, size: PaginationSize) -> Self {
        self.size = size;
        self
    }

    /// Set small size
    pub fn small(mut self) -> Self {
        self.size = PaginationSize::Small;
        self
    }

    /// Set large size
    pub fn large(mut self) -> Self {
        self.size = PaginationSize::Large;
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.classes.push(blinc_core::intern::intern(name.as_ref()));
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.user_id = Some(id.to_string());
        self
    }

    /// Set page change callback
    pub fn on_page_change<F>(mut self, handler: F) -> Self
    where
        F: Fn(usize) + Send + Sync + 'static,
    {
        self.on_page_change = Some(Arc::new(handler));
        self
    }
}

impl ElementBuilder for PaginationBuilder {
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

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }
}

/// Create a pagination component
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
/// use blinc_core::use_state_keyed;
///
/// let page = use_state_keyed("pagination_page", || 1usize);
///
/// cn::pagination(10, page.clone())
///     .visible_pages(7)
///     .show_first_last(true)
///     .on_page_change(|page| println!("Page: {}", page))
/// ```
#[track_caller]
pub fn pagination(total_pages: usize, current_page: State<usize>) -> PaginationBuilder {
    PaginationBuilder::new(total_pages, current_page)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_range_small_total() {
        // Total pages <= visible pages
        assert_eq!(calculate_page_range(1, 5, 7), (1, 5));
        assert_eq!(calculate_page_range(3, 5, 7), (1, 5));
    }

    #[test]
    fn test_page_range_at_start() {
        // Current page near start
        assert_eq!(calculate_page_range(1, 20, 5), (1, 5));
        assert_eq!(calculate_page_range(2, 20, 5), (1, 5));
        assert_eq!(calculate_page_range(3, 20, 5), (1, 5));
    }

    #[test]
    fn test_page_range_at_end() {
        // Current page near end
        assert_eq!(calculate_page_range(20, 20, 5), (16, 20));
        assert_eq!(calculate_page_range(19, 20, 5), (16, 20));
        assert_eq!(calculate_page_range(18, 20, 5), (16, 20));
    }

    #[test]
    fn test_page_range_middle() {
        // Current page in middle
        assert_eq!(calculate_page_range(10, 20, 5), (8, 12));
        assert_eq!(calculate_page_range(10, 20, 7), (7, 13));
    }

    #[test]
    fn test_pagination_sizes() {
        assert_eq!(PaginationSize::Small.button_size(), 24.0);
        assert_eq!(PaginationSize::Medium.button_size(), 32.0);
        assert_eq!(PaginationSize::Large.button_size(), 40.0);
    }
}
