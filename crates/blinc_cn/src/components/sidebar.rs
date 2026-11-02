//! Sidebar component with animated expand/collapse
//!
//! A navigation rail that collapses to its icons. The rail carries
//! `data-state="expanded"` or `"collapsed"`, and its width is the
//! `.cn-sidebar` rules': `--cn-sidebar-width` (200px) open and
//! `--cn-sidebar-collapsed-width` (41px) shut, eased by a `width` transition
//! over the theme's normal duration. The content beside it is laid out again
//! each frame of that transition, so it moves with the rail. What the rail
//! holds stays laid out as it is, clipped by the narrowing width: labels fade
//! and group titles fold away by rule.
//!
//! Items carry `data-active` while they are the selection.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Basic sidebar
//! let is_collapsed = ctx.use_signal(false);
//!
//! cn::sidebar(is_collapsed)
//!     .item("Home", home_icon, || println!("Home clicked"))
//!     .item("Settings", settings_icon, || println!("Settings clicked"))
//!     .item("Profile", profile_icon, || println!("Profile clicked"))
//!
//! // With sections
//! cn::sidebar(is_collapsed)
//!     .section("Main")
//!         .item("Home", icon, || {})
//!         .item("Explore", icon, || {})
//!     .section("Account")
//!         .item("Profile", icon, || {})
//!         .item("Settings", icon, || {})
//! ```
//!
//! Widths are rules:
//!
//! ```css
//! .wide-nav.cn-sidebar { width: 280px; }
//! .wide-nav.cn-sidebar[data-state="collapsed"] { width: 64px; }
//! ```

use std::cell::OnceCell;
use std::sync::Arc;

use blinc_core::reactive::Signal;
use blinc_core::reactive::{ReactiveGraph, computed};
use blinc_layout::InstanceKey;
use blinc_layout::div::{Div, ElementBuilder, ElementTypeId};
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::ThemeState;

/// Chevron left icon (collapse)
const CHEVRON_LEFT_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6"/></svg>"#;

/// Sidebar item definition
#[derive(Clone)]
pub struct SidebarItem {
    /// Display label
    label: String,
    /// Icon SVG string
    icon: String,
    /// Click handler
    on_click: Arc<dyn Fn() + Send + Sync>,
    /// Whether this item is active/selected
    is_active: bool,
}

impl SidebarItem {
    /// Create a new sidebar item
    pub fn new(
        label: impl Into<String>,
        icon: impl Into<String>,
        on_click: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            icon: icon.into(),
            on_click: Arc::new(on_click),
            is_active: false,
        }
    }

    /// Mark this item as active
    pub fn active(mut self, active: bool) -> Self {
        self.is_active = active;
        self
    }
}

/// Sidebar section for grouping items
#[derive(Clone)]
pub struct SidebarSection {
    /// Section title (shown when expanded)
    title: Option<String>,
    /// Items in this section
    items: Vec<SidebarItem>,
}

/// Sidebar component with animated expand/collapse
pub struct Sidebar {
    /// The rail alone, or the rail beside a content area.
    inner: Box<dyn ElementBuilder>,
}

impl Sidebar {
    fn from_builder(builder: &SidebarBuilder) -> Self {
        let typography = ThemeState::get().typography();
        let key = builder.key.get().to_string();

        // The caller's signal, read and written directly, so the toggle's
        // write reaches whoever else reads it.
        let collapsed = builder.is_collapsed;
        let expanded = computed(move |g: &ReactiveGraph| !g.get(collapsed).unwrap_or(false));
        let shut = computed(move |g: &ReactiveGraph| g.get(collapsed).unwrap_or(false));

        // Keyed by the sidebar, so the selection survives rebuilds of
        // whatever holds it.
        let active_menu: Signal<Option<SidebarItem>> =
            blinc_core::context_state::BlincContextState::get()
                .use_signal_keyed(&format!("{key}_active_menu"), || None);

        let mut rail = div()
            .class("cn-sidebar")
            .attr_when("data-state", "expanded", &expanded)
            .attr_when("data-state", "collapsed", &shut)
            .flex_col()
            .flex_shrink_0()
            .h_full()
            .gap(0.5)
            .py(2.0)
            .overflow_clip();

        if builder.show_toggle {
            rail = rail.child(
                div()
                    .class("cn-sidebar-toggle")
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .self_start()
                    .w(28.0)
                    .h(28.0)
                    .mx(1.5)
                    .mb(1.5)
                    .child(
                        div()
                            .class("cn-sidebar-chevron")
                            .child(svg(CHEVRON_LEFT_SVG).size(16.0, 16.0))
                            .pointer_events_none(),
                    )
                    .on_click(move |_| collapsed.update(|c| !c)),
            );
        }

        for section in &builder.sections {
            if section.title.is_none() && section.items.is_empty() {
                continue;
            }
            let mut group = div().class("cn-sidebar-group").flex_col().gap(0.5);
            if let Some(title) = &section.title {
                group = group.child(
                    div()
                        .class("cn-sidebar-group-label")
                        .flex_shrink_0()
                        .h(26.0)
                        .pt(2.0)
                        .px(3.0)
                        .overflow_clip()
                        .child(text(title).size(typography.text_xs).no_wrap().no_cursor()),
                );
            }
            for item in &section.items {
                group = group.child(item_row(item, active_menu, typography.text_sm));
            }
            rail = rail.child(group);
        }

        for c in &builder.classes {
            rail = rail.class(c);
        }
        if let Some(ref id) = builder.user_id {
            rail = rail.id(id);
        }

        // The content sits beside the rail and is built once with it, so it
        // keeps its state across clicks and collapses.
        let inner: Box<dyn ElementBuilder> = match &builder.content_builder {
            None => Box::new(rail),
            Some(content_fn) => Box::new(
                div()
                    .class("cn-sidebar-layout")
                    .flex_row()
                    .w_full()
                    .h_full()
                    .overflow_clip()
                    .child(rail)
                    .child(
                        div()
                            .class("cn-sidebar-inset")
                            .flex_1()
                            .flex_col()
                            .min_w(0.0)
                            .overflow_clip()
                            .child(content_fn(active_menu.try_get().flatten())),
                    ),
            ),
        };

        Self { inner }
    }
}

/// One item: its icon and label. Active while it is the selection, or,
/// before anything is selected, when the builder marked it.
fn item_row(item: &SidebarItem, active_menu: Signal<Option<SidebarItem>>, font_size: f32) -> Div {
    let label = item.label.clone();
    let initially_active = item.is_active;
    let active = computed(
        move |g: &ReactiveGraph| match g.get(active_menu).flatten() {
            Some(selected) => selected.label == label,
            None => initially_active,
        },
    );
    let chosen = item.clone();
    div()
        .class("cn-sidebar-item")
        .attr_when("data-active", "", &active)
        .flex_row()
        .items_center()
        .flex_shrink_0()
        .gap(3.0)
        .px(3.0)
        .py(2.0)
        .child(
            div()
                .class("cn-sidebar-icon")
                .flex_shrink_0()
                .child(svg(&item.icon).size(16.0, 16.0))
                .pointer_events_none(),
        )
        .child(
            div()
                .class("cn-sidebar-label")
                .flex_row()
                .flex_shrink_0()
                .child(text(&item.label).size(font_size).no_wrap().no_cursor())
                .pointer_events_none(),
        )
        .on_click(move |_| {
            active_menu.set(Some(chosen.clone()));
            (chosen.on_click)();
        })
}

impl ElementBuilder for Sidebar {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
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

    fn visual_animation_config(
        &self,
    ) -> Option<blinc_layout::visual_animation::VisualAnimationConfig> {
        self.inner.visual_animation_config()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }
}

/// Content builder function type
type ContentBuilderFn = Arc<dyn Fn(Option<SidebarItem>) -> Div + Send + Sync>;

/// Builder for sidebar component
pub struct SidebarBuilder {
    key: InstanceKey,
    is_collapsed: Signal<bool>,
    sections: Vec<SidebarSection>,
    show_toggle: bool,
    /// Optional main content area that sits next to the sidebar
    content_builder: Option<ContentBuilderFn>,
    /// User-added CSS classes
    classes: Vec<std::sync::Arc<str>>,
    /// User-set element ID
    user_id: Option<String>,
    built: OnceCell<Sidebar>,
}

impl SidebarBuilder {
    /// Create a new sidebar builder
    #[track_caller]
    pub fn new(is_collapsed: Signal<bool>) -> Self {
        Self {
            key: InstanceKey::new("sidebar"),
            is_collapsed,
            sections: vec![SidebarSection {
                title: None,
                items: Vec::new(),
            }],
            show_toggle: true,
            content_builder: None,
            classes: Vec::new(),
            user_id: None,
            built: OnceCell::new(),
        }
    }

    /// Get or build the component
    fn get_or_build(&self) -> &Sidebar {
        ::blinc_layout::build_once::build_once(&self.built, || Sidebar::from_builder(self))
    }

    /// Show or hide the toggle button
    pub fn show_toggle(mut self, show: bool) -> Self {
        self.show_toggle = show;
        self
    }

    /// Add a navigation item to the current section
    pub fn item<F>(mut self, label: impl Into<String>, icon: impl Into<String>, on_click: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        let item = SidebarItem::new(label, icon, on_click);
        if let Some(section) = self.sections.last_mut() {
            section.items.push(item);
        }
        self
    }

    /// Add an active navigation item
    pub fn item_active<F>(
        mut self,
        label: impl Into<String>,
        icon: impl Into<String>,
        on_click: F,
    ) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        let item = SidebarItem::new(label, icon, on_click).active(true);
        if let Some(section) = self.sections.last_mut() {
            section.items.push(item);
        }
        self
    }

    /// Start a new section with optional title
    pub fn section(mut self, title: impl Into<String>) -> Self {
        self.sections.push(SidebarSection {
            title: Some(title.into()),
            items: Vec::new(),
        });
        self
    }

    /// Start a new section without title
    pub fn section_untitled(mut self) -> Self {
        self.sections.push(SidebarSection {
            title: None,
            items: Vec::new(),
        });
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

    /// Set the main content area that sits next to the sidebar
    ///
    /// The rail and the content share a `.cn-sidebar-layout` row, the
    /// content in a `.cn-sidebar-inset` that takes the rest of the width and
    /// moves with the rail as it eases.
    ///
    /// The content is built once, with the rail, and keeps its state when a
    /// row is clicked or the rail collapses. It is handed the selection as
    /// of that build; drive what it shows from a signal the click handlers
    /// write.
    ///
    /// # Example
    ///
    /// ```ignore
    /// cn::sidebar(collapsed)
    ///     .item("Home", home_icon, || {})
    ///     .item("Settings", settings_icon, || {})
    ///     .content(|_| {
    ///         div()
    ///             .p(24.0)
    ///             .child(text("Main content area"))
    ///     })
    /// ```
    pub fn content<F>(mut self, builder: F) -> Self
    where
        F: Fn(Option<SidebarItem>) -> Div + Send + Sync + 'static,
    {
        self.content_builder = Some(Arc::new(builder));
        self
    }
}

impl ElementBuilder for SidebarBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
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

    fn visual_animation_config(
        &self,
    ) -> Option<blinc_layout::visual_animation::VisualAnimationConfig> {
        self.get_or_build().visual_animation_config()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }
}

/// Create a sidebar navigation component
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
/// let collapsed = ctx.use_signal(false);
///
/// // Home icon SVG
/// let home_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>"#;
///
/// // Settings icon SVG
/// let settings_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="..."/></svg>"#;
///
/// cn::sidebar(collapsed)
///     .item("Home", home_icon, || println!("Home"))
///     .item("Settings", settings_icon, || println!("Settings"))
/// ```
#[track_caller]
pub fn sidebar(is_collapsed: Signal<bool>) -> SidebarBuilder {
    SidebarBuilder::new(is_collapsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sidebar_item() {
        let item = SidebarItem::new("Test", "<svg></svg>", || {});
        assert_eq!(item.label, "Test");
        assert!(!item.is_active);

        let active_item = item.active(true);
        assert!(active_item.is_active);
    }
}
