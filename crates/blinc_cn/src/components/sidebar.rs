//! Sidebar component with animated expand/collapse
//!
//! A collapsible sidebar navigation component that uses LayoutAnimation
//! for smooth width transitions.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//! use blinc_core::use_state_keyed;
//!
//! // Basic sidebar
//! let is_collapsed = use_state_keyed("sidebar_collapsed", || false);
//!
//! cn::sidebar(&is_collapsed)
//!     .item("Home", home_icon, || println!("Home clicked"))
//!     .item("Settings", settings_icon, || println!("Settings clicked"))
//!     .item("Profile", profile_icon, || println!("Profile clicked"))
//!
//! // With custom widths
//! cn::sidebar(&is_collapsed)
//!     .expanded_width(280.0)
//!     .collapsed_width(64.0)
//!     .item("Dashboard", icon, || {})
//!
//! // With sections
//! cn::sidebar(&is_collapsed)
//!     .section("Main")
//!         .item("Home", icon, || {})
//!         .item("Explore", icon, || {})
//!     .section("Account")
//!         .item("Profile", icon, || {})
//!         .item("Settings", icon, || {})
//! ```

use std::cell::OnceCell;
use std::sync::Arc;

use blinc_core::State;
use blinc_core::reactive::{ReactiveGraph, computed};
use blinc_layout::InstanceKey;
use blinc_layout::div::{Div, ElementBuilder, ElementTypeId};
use blinc_layout::element::CursorStyle;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::visual_animation::VisualAnimationConfig;
use blinc_theme::{ColorToken, ThemeState};

/// Chevron left icon (collapse)
const CHEVRON_LEFT_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6"/></svg>"#;

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
        let theme = ThemeState::get();
        let surface = theme.color(ColorToken::Surface);
        let border = theme.color(ColorToken::Border);
        let text_secondary = theme.color(ColorToken::TextSecondary);
        let text_tertiary = theme.color(ColorToken::TextTertiary);
        let primary = theme.color(ColorToken::Primary);

        let key = builder.key.get().to_string();
        let sections = builder.sections.clone();
        let show_toggle = builder.show_toggle;

        // Single source of truth: the caller's state, used directly.
        // A copy held inside the stateful would take the toggle's write
        // and never pass it on, leaving the caller's signal saying the
        // rail was open while it was shut.
        let collapsed = builder.is_collapsed.clone();

        let content_anim_key = format!("{key}_content");

        // Hoisted out of the rail's closure so the content area can read
        // it too. Keyed by the sidebar, so it is the same signal the rail
        // writes on a click, and it survives the rail's rebuilds.
        let active_menu: State<Option<SidebarItem>> =
            blinc_core::context_state::use_state_keyed(&format!("{key}_active_menu"), || None);
        // The rail is built once. Collapsing hides the labels and titles in
        // place, so the rail's width changes and its keyed animation follows
        // it; the selection moves the active class and icon colour.
        let container_key = format!("{}_container", key);
        let collapsed_sig = collapsed.signal();
        let expanded = computed(move |g: &ReactiveGraph| !g.get(collapsed_sig).unwrap_or(false));
        let active_sig = active_menu.signal();

        let layout_anim_key = format!("{}_layout", key);
        let sidebar_anim_key = format!("{}_sidebar_container", key);

        let sidebar_content = div().flex_col().h_full().overflow_clip().animate_bounds(
            VisualAnimationConfig::size()
                .with_key(&sidebar_anim_key)
                .clip_to_animated()
                .snappy(),
        );

        // Sections and items container
        // Uses w_fit() so width is determined by children content
        let mut items_container = div()
            .class("cn-sidebar")
            .flex_col()
            .border_right(1.0, border)
            .bg(surface)
            .h_full()
            .w_fit()
            .overflow_clip() // Critical for animation clipping
            .py(2.0)
            .animate_bounds(
                VisualAnimationConfig::all()
                    .with_key(&layout_anim_key)
                    .clip_to_animated()
                    .snappy(),
            );

        if show_toggle {
            let toggle_key = format!("{}_toggle", container_key);
            let collapsed_for_click = collapsed.clone();
            // The chevron points right while collapsed, left while open.
            let chevron_angle = computed(move |g: &ReactiveGraph| {
                if g.get(collapsed_sig).unwrap_or(false) {
                    180.0
                } else {
                    0.0
                }
            });
            items_container = items_container.child(
                div().child(
                    div()
                        .w_fit()
                        .flex_row()
                        .items_center()
                        .gap(3.0)
                        .px(3.0)
                        .py(2.0)
                        .cursor(CursorStyle::Pointer)
                        .animate_bounds(
                            VisualAnimationConfig::size()
                                .with_key(format!("{}_anim", toggle_key))
                                .clip_to_animated()
                                .snappy(),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .self_end()
                                .rotate_deg(&chevron_angle)
                                .child(svg(CHEVRON_LEFT_SVG).size(18.0, 18.0).color(text_secondary))
                                .pointer_events_none(),
                        )
                        .on_click(move |_| collapsed_for_click.update(|c| !c)),
                ),
            );
        }

        for (section_idx, section) in sections.iter().enumerate() {
            // Section title: always present, shown while the rail is open.
            if let Some(ref title) = section.title {
                let title_anim_key = format!("{}_section_{}_title", container_key, section_idx);
                let title_div = div()
                    .w_fit()
                    .h_fit()
                    .overflow_clip()
                    .animate_bounds(
                        VisualAnimationConfig::all()
                            .with_key(&title_anim_key)
                            .clip_to_animated()
                            .snappy(),
                    )
                    .when(&expanded, |d| {
                        d.child(
                            div().px(3.0).py(2.0).child(
                                text(title.to_uppercase())
                                    .size(theme.typography().text_xs)
                                    .color(text_tertiary)
                                    .weight(FontWeight::SemiBold)
                                    .no_cursor()
                                    .no_wrap(),
                            ),
                        )
                    });
                items_container = items_container.child(title_div);
            }

            for (item_idx, item) in section.items.iter().enumerate() {
                let item_key = format!("{}_item_{}_{}", container_key, section_idx, item_idx);
                // Active when it is the selection, or, before anything is
                // selected, when the builder marked it.
                let label = item.label.clone();
                let initially_active = item.is_active;
                let active = computed(move |g: &ReactiveGraph| match g.get(active_sig).flatten() {
                    Some(selected) => selected.label == label,
                    None => initially_active,
                });
                let icon_color = {
                    let label = item.label.clone();
                    computed(move |g: &ReactiveGraph| {
                        let is_active = match g.get(active_sig).flatten() {
                            Some(selected) => selected.label == label,
                            None => initially_active,
                        };
                        if is_active { primary } else { text_secondary }
                    })
                };

                let item_on_click = item.on_click.clone();
                let active_menu_for_click = active_menu.clone();
                let item_for_click = item.clone();

                let item_element = div()
                    .class("cn-sidebar-item")
                    .class_when("cn-sidebar-item--active", &active)
                    // `w_full` so the hover / active bg stretches across the
                    // full sidebar width; the parent is `w_fit` and takes its
                    // width from the widest item.
                    .w_full()
                    .h_fit()
                    .flex_row()
                    .items_center()
                    .gap(3.0)
                    .cursor(CursorStyle::Pointer)
                    .overflow_clip()
                    .animate_bounds(
                        VisualAnimationConfig::all()
                            .with_key(format!("{}_anim", item_key))
                            .clip_to_animated()
                            .snappy(),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .child(svg(&item.icon).size(18.0, 18.0).color(&icon_color)),
                    )
                    .child(
                        div().visible(&expanded).child(
                            text(&item.label)
                                .size(theme.typography().text_sm)
                                .no_cursor()
                                .no_wrap(),
                        ),
                    )
                    .on_click(move |_| {
                        active_menu_for_click.set(Some(item_for_click.clone()));
                        item_on_click();
                    });

                items_container = items_container.child(item_element);
            }
        }

        let stateful_container = sidebar_content.child(items_container);

        // Apply user classes and id
        let mut rail = stateful_container;
        for c in &builder.classes {
            rail = rail.class(c);
        }
        if let Some(ref id) = builder.user_id {
            rail = rail.id(id);
        }

        // The content area sits BESIDE the rail's stateful, never inside
        // it. Built into the rebuild, it was torn down and remade every
        // time the rail changed — on a toggle, and on every click,
        // since selecting a row is rail state too. Anything the content
        // owned went with it: scroll offsets, springs, and the state of
        // whatever region it holds.
        let inner: Box<dyn ElementBuilder> = match &builder.content_builder {
            None => Box::new(rail),
            Some(content_fn) => {
                let content_wrapper = div()
                    .flex_1()
                    .h_full()
                    .overflow_clip()
                    // Position and size both move as the rail's width
                    // changes, so the content tracks it rather than
                    // jumping once the rail's own animation lands.
                    .animate_bounds(
                        VisualAnimationConfig::all()
                            .with_key(&content_anim_key)
                            .clip_to_animated()
                            .snappy(),
                    )
                    // The selection, handed over rather than made the
                    // caller's problem: a convenience argument is the
                    // point of a builder like this.
                    .child(content_fn(active_menu.get()));
                Box::new(
                    div()
                        .flex_row()
                        .w_full()
                        .h_full()
                        .child(rail)
                        .child(content_wrapper),
                )
            }
        };

        Self { inner }
    }
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
    is_collapsed: State<bool>,
    collapsed_width: f32,
    expanded_width: f32,
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
    pub fn new(is_collapsed: &State<bool>) -> Self {
        Self {
            key: InstanceKey::new("sidebar"),
            is_collapsed: is_collapsed.clone(),
            collapsed_width: 64.0,
            expanded_width: 240.0,
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

    /// Set the collapsed width
    pub fn collapsed_width(mut self, width: f32) -> Self {
        self.collapsed_width = width;
        self
    }

    /// Set the expanded width
    pub fn expanded_width(mut self, width: f32) -> Self {
        self.expanded_width = width;
        self
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
    /// When provided, the sidebar wraps both the sidebar menu and the main content
    /// in a shared container, so the content tracks the rail's width as it
    /// animates rather than jumping once the animation lands.
    ///
    /// The content is built once, outside the rail's own rebuilds, and so
    /// keeps its state when a row is clicked or the rail collapses. It
    /// takes no argument for that reason: nothing outside the rebuild can
    /// react to which row is selected. Drive the content from a signal
    /// the click handlers write.
    ///
    /// # Example
    ///
    /// ```ignore
    /// cn::sidebar(&collapsed)
    ///     .item("Home", home_icon, || {})
    ///     .item("Settings", settings_icon, || {})
    ///     .content(|| {
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
/// use blinc_core::use_state_keyed;
///
/// let collapsed = use_state_keyed("sidebar_collapsed", || false);
///
/// // Home icon SVG
/// let home_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>"#;
///
/// // Settings icon SVG
/// let settings_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="..."/></svg>"#;
///
/// cn::sidebar(&collapsed)
///     .item("Home", home_icon, || println!("Home"))
///     .item("Settings", settings_icon, || println!("Settings"))
/// ```
#[track_caller]
pub fn sidebar(is_collapsed: &State<bool>) -> SidebarBuilder {
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
