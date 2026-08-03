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
use blinc_layout::InstanceKey;
use blinc_layout::div::{Div, ElementBuilder, ElementTypeId};
use blinc_layout::element::CursorStyle;
use blinc_layout::prelude::*;
use blinc_layout::stateful::{NoState, stateful_with_key};
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
        let text_primary = theme.color(ColorToken::TextPrimary);
        let text_secondary = theme.color(ColorToken::TextSecondary);
        let text_tertiary = theme.color(ColorToken::TextTertiary);
        let primary = theme.color(ColorToken::Primary);
        let primary_hover = theme.color(ColorToken::PrimaryHover);
        let surface = theme.color(ColorToken::Surface);
        let surface_elevated = theme.color(ColorToken::SurfaceElevated);
        let border = theme.color(ColorToken::Border);
        let radius = theme.radius(RadiusToken::Md);

        let button_size = builder.size.button_size();
        let font_size = builder.size.font_size();
        let icon_size = builder.size.icon_size();
        let gap = builder.size.gap();

        let key = builder.key.get().to_string();
        let total_pages = builder.total_pages;
        let visible_pages = builder.visible_pages;
        let show_first_last = builder.show_first_last;
        let on_page_change = builder.on_page_change.clone();
        let page_state = builder.current_page.clone();

        // Create stateful container that rebuilds when page changes
        let container_key = format!("{}_container", key);
        let page_state_for_container = page_state.clone();

        let stateful_container = stateful_with_key::<NoState>(&container_key)
            .deps([page_state.signal_id()])
            .on_state(move |_ctx| {
                let current_page = page_state_for_container.get();
                let mut container = div()
                    .class("cn-pagination")
                    .flex_row()
                    .items_center()
                    .gap(gap);

                // Calculate visible page range
                let (start_page, end_page) =
                    calculate_page_range(current_page, total_pages, visible_pages);

                let show_start_ellipsis = start_page > 1;
                let show_end_ellipsis = end_page < total_pages;

                // First page button (if enabled)
                if show_first_last && total_pages > visible_pages {
                    let page_state_first = page_state_for_container.clone();
                    let on_change_first = on_page_change.clone();
                    let first_key = format!("{}_first", key);
                    let is_disabled = current_page == 1;

                    let first_btn = build_nav_button(
                        &first_key,
                        CHEVRONS_LEFT_SVG,
                        button_size,
                        icon_size,
                        radius,
                        is_disabled,
                        surface_elevated,
                        border,
                        text_secondary,
                        text_tertiary,
                        move || {
                            if !is_disabled {
                                page_state_first.set(1);
                                if let Some(ref cb) = on_change_first {
                                    cb(1);
                                }
                            }
                        },
                    );
                    container = container.child(first_btn);
                }

                // Previous button
                {
                    let page_state_prev = page_state_for_container.clone();
                    let on_change_prev = on_page_change.clone();
                    let prev_key = format!("{}_prev", key);
                    let is_disabled = current_page == 1;
                    let prev_page = (current_page - 1).max(1);

                    let prev_btn = build_nav_button(
                        &prev_key,
                        CHEVRON_LEFT_SVG,
                        button_size,
                        icon_size,
                        radius,
                        is_disabled,
                        surface_elevated,
                        border,
                        text_secondary,
                        text_tertiary,
                        move || {
                            if !is_disabled {
                                page_state_prev.set(prev_page);
                                if let Some(ref cb) = on_change_prev {
                                    cb(prev_page);
                                }
                            }
                        },
                    );
                    container = container.child(prev_btn);
                }

                // Start ellipsis
                if show_start_ellipsis {
                    container = container.child(
                        div()
                            .w(button_size)
                            .h(button_size)
                            .items_center()
                            .justify_center()
                            .child(
                                svg(ELLIPSIS_SVG)
                                    .size(icon_size, icon_size)
                                    .color(text_tertiary),
                            ),
                    );
                }

                // Page number buttons
                for page in start_page..=end_page {
                    let page_state_num = page_state_for_container.clone();
                    let on_change_num = on_page_change.clone();
                    let page_key = format!("{}_page_{}", key, page);
                    let is_current = page == current_page;

                    let page_btn = build_page_button(
                        &page_key,
                        page,
                        is_current,
                        button_size,
                        font_size,
                        radius,
                        primary,
                        primary_hover,
                        surface_elevated,
                        border,
                        text_primary,
                        text_secondary,
                        move || {
                            if !is_current {
                                page_state_num.set(page);
                                if let Some(ref cb) = on_change_num {
                                    cb(page);
                                }
                            }
                        },
                    );
                    container = container.child(page_btn);
                }

                // End ellipsis
                if show_end_ellipsis {
                    container = container.child(
                        div()
                            .w(button_size)
                            .h(button_size)
                            .items_center()
                            .justify_center()
                            .child(
                                svg(ELLIPSIS_SVG)
                                    .size(icon_size, icon_size)
                                    .color(text_tertiary),
                            ),
                    );
                }

                // Next button
                {
                    let page_state_next = page_state_for_container.clone();
                    let on_change_next = on_page_change.clone();
                    let next_key = format!("{}_next", key);
                    let is_disabled = current_page == total_pages;
                    let next_page = (current_page + 1).min(total_pages);

                    let next_btn = build_nav_button(
                        &next_key,
                        CHEVRON_RIGHT_SVG,
                        button_size,
                        icon_size,
                        radius,
                        is_disabled,
                        surface_elevated,
                        border,
                        text_secondary,
                        text_tertiary,
                        move || {
                            if !is_disabled {
                                page_state_next.set(next_page);
                                if let Some(ref cb) = on_change_next {
                                    cb(next_page);
                                }
                            }
                        },
                    );
                    container = container.child(next_btn);
                }

                // Last page button (if enabled)
                if show_first_last && total_pages > visible_pages {
                    let page_state_last = page_state_for_container.clone();
                    let on_change_last = on_page_change.clone();
                    let last_key = format!("{}_last", key);
                    let is_disabled = current_page == total_pages;

                    let last_btn = build_nav_button(
                        &last_key,
                        CHEVRONS_RIGHT_SVG,
                        button_size,
                        icon_size,
                        radius,
                        is_disabled,
                        surface_elevated,
                        border,
                        text_secondary,
                        text_tertiary,
                        move || {
                            if !is_disabled {
                                page_state_last.set(total_pages);
                                if let Some(ref cb) = on_change_last {
                                    cb(total_pages);
                                }
                            }
                        },
                    );
                    container = container.child(last_btn);
                }

                container
            });

        let mut inner = div().child(stateful_container);
        for c in &builder.classes {
            inner = inner.class(c);
        }
        if let Some(ref id) = builder.user_id {
            inner = inner.id(id);
        }

        Self { inner }
    }
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

/// Build a navigation button (prev/next/first/last)
#[allow(clippy::too_many_arguments)]
fn build_nav_button<F>(
    key: &str,
    icon_svg: &'static str,
    button_size: f32,
    icon_size: f32,
    radius: f32,
    is_disabled: bool,
    _surface_elevated: blinc_core::Color,
    border: blinc_core::Color,
    text_secondary: blinc_core::Color,
    text_tertiary: blinc_core::Color,
    on_click: F,
) -> impl ElementBuilder + use<F>
where
    F: Fn() + Send + Sync + 'static,
{
    let on_click = Arc::new(on_click);

    // Use plain div — no Stateful wrapper to avoid hover rebuild artifact.
    // All hover visuals handled by CSS .cn-pagination-btn:hover
    let icon_color = if is_disabled {
        text_tertiary.with_alpha(0.5)
    } else {
        text_secondary
    };

    let mut btn = div()
        .class("cn-pagination-btn")
        .w(button_size)
        .h(button_size)
        .rounded(radius)
        .items_center()
        .justify_center()
        .border(
            1.0,
            if is_disabled {
                border.with_alpha(0.5)
            } else {
                border
            },
        )
        .cursor(if is_disabled {
            CursorStyle::NotAllowed
        } else {
            CursorStyle::Pointer
        })
        .child(svg(icon_svg).size(icon_size, icon_size).color(icon_color));
    if is_disabled {
        btn = btn.class("cn-pagination-btn--disabled");
    }
    btn.on_click(move |_| {
        on_click();
    })
}

/// Build a page number button
#[allow(clippy::too_many_arguments)]
fn build_page_button<F>(
    key: &str,
    page: usize,
    is_current: bool,
    button_size: f32,
    font_size: f32,
    radius: f32,
    primary: blinc_core::Color,
    _primary_hover: blinc_core::Color,
    _surface_elevated: blinc_core::Color,
    border: blinc_core::Color,
    text_primary: blinc_core::Color,
    text_secondary: blinc_core::Color,
    on_click: F,
) -> impl ElementBuilder + use<F>
where
    F: Fn() + Send + Sync + 'static,
{
    let on_click = Arc::new(on_click);
    let page_str = page.to_string();

    // Use plain div — no Stateful wrapper to avoid hover rebuild artifact.
    // All hover visuals handled by CSS .cn-pagination-btn:hover
    let theme = ThemeState::get();
    let (bg, text_color, border_color) = if is_current {
        (primary, theme.color(ColorToken::TextInverse), primary)
    } else {
        (blinc_core::Color::TRANSPARENT, text_secondary, border)
    };

    let mut btn = div()
        .class("cn-pagination-btn")
        .w(button_size)
        .h(button_size)
        .rounded(radius)
        .items_center()
        .justify_center()
        .bg(bg)
        .border(1.0, border_color)
        .cursor(if is_current {
            CursorStyle::Default
        } else {
            CursorStyle::Pointer
        })
        .child(
            text(&page_str)
                .size(font_size)
                .color(text_color)
                .medium()
                .pointer_events_none()
                .no_cursor(),
        );
    if is_current {
        btn = btn.class("cn-pagination-btn--active");
    }
    btn.on_click(move |_| {
        on_click();
    })
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
