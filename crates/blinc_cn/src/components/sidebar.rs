//! Sidebar component with animated expand/collapse
//!
//! A collapsible sidebar navigation component that uses LayoutAnimation
//! for smooth width transitions.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Basic sidebar
//! let is_collapsed = use_state(|| false);
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
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use blinc_core::State;
use blinc_layout::div::{Div, ElementBuilder, ElementTypeId};
use blinc_layout::element::CursorStyle;
use blinc_layout::prelude::*;
use blinc_layout::stateful::{stateful_with_key, ButtonState, NoState};
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::visual_animation::VisualAnimationConfig;
use blinc_layout::InstanceKey;
use blinc_theme::{ColorToken, ThemeState};

/// Chevron left icon (collapse)
const CHEVRON_LEFT_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6"/></svg>"#;

/// Chevron right icon (expand)
const CHEVRON_RIGHT_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"#;

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
    inner: Stateful<NoState>,
}

impl Sidebar {
    fn from_builder(builder: &SidebarBuilder) -> Self {
        let theme = ThemeState::get();
        let surface = theme.color(ColorToken::Surface);
        let border = theme.color(ColorToken::Border);
        let text_primary = theme.color(ColorToken::TextPrimary);
        let text_secondary = theme.color(ColorToken::TextSecondary);
        let text_tertiary = theme.color(ColorToken::TextTertiary);
        let primary = theme.color(ColorToken::Primary);

        let key = builder.key.get().to_string();
        let sections = builder.sections.clone();
        let show_toggle = builder.show_toggle;
        let content_builder = builder.content_builder.clone();

        // Single source of truth: the collapsed state from parent
        let is_collapsed = builder.is_collapsed.get();

        // Create stateful container that rebuilds when collapsed state changes
        let container_key = format!("{}_container", key.clone());
        // let is_collapsed_for_container = is_collapsed.clone();

        let stateful_container = stateful_with_key::<NoState>(&container_key)
            .deps([builder.is_collapsed.signal_id()])
            .on_state(move |ctx| {
                let collapsed = ctx.use_signal("collapsed", || is_collapsed);

                let mut sections = sections.clone();

                
                // Layout animation keys for smooth width transitions
                let layout_anim_key = format!("{}_layout", key.clone());
                let content_anim_key = format!("{}_content", key.clone());
                let sidebar_anim_key = format!("{}_sidebar_container", key.clone());

                let sidebar_content = div().flex_col().h_full().overflow_clip().animate_bounds(
                    VisualAnimationConfig::size()
                        .with_key(&sidebar_anim_key)
                        .clip_to_animated()
                        .snappy(),
                );

                // // Convert pixel widths to layout units (divide by 4)
                let mut toggle_btn = div();
                // Toggle button at the top
                if show_toggle {
                    let is_collapsed_for_state = collapsed.clone();
                    let is_collapsed_for_click = collapsed.clone();
                    let toggle_key = format!("{}_toggle", ctx.key());

                    toggle_btn = toggle_btn.child(
                        stateful_with_key::<ButtonState>(&toggle_key)
                            .deps([collapsed.signal_id()])
                            .on_state(move |ctx| {
                                let state = ctx.state();
                                let theme = ThemeState::get();
                                let collapsed_inner = is_collapsed_for_state.get();

                                let bg = match state {
                                    ButtonState::Hovered | ButtonState::Pressed => {
                                        theme.color(ColorToken::SecondaryHover).with_alpha(0.5)
                                    }
                                    _ => blinc_core::Color::TRANSPARENT,
                                };

                                let icon = if collapsed_inner {
                                    CHEVRON_RIGHT_SVG
                                } else {
                                    CHEVRON_LEFT_SVG
                                };

                                // Match item styling: w_fit, flex_row, same padding
                                let toggle_anim_key = format!("{}_anim", ctx.key());
                                div()
                                    .w_fit()
                                    .flex_row()
                                    .items_center()
                                    .gap(3.0)
                                    .px(3.0)
                                    .py(2.0)
                                    .bg(bg)
                                    .cursor(CursorStyle::Pointer)
                                    .animate_bounds(
                                        VisualAnimationConfig::size()
                                            .with_key(&toggle_anim_key)
                                            .clip_to_animated()
                                            .snappy(),
                                    )
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .self_end()
                                            .child(svg(icon).size(18.0, 18.0).color(text_secondary))
                                            .pointer_events_none(),
                                    )
                            })
                            .on_click(move |_| {
                                // is_collapsed_for_click.set(!current);
                                is_collapsed_for_click.update(|c| !c);
                            }),
                    );
                }

                // Sections and items container
                // Uses w_fit() so width is determined by children content
                // Children conditionally render icon-only (collapsed) or icon+label (expanded)
                // The infrastructure fix ensures children laid out at larger size during collapse
                let mut items_container = div()
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
                    items_container = items_container.child(toggle_btn);
                }

                let active_menu: State<Option<SidebarItem>> = ctx.use_signal("active_menu", || None);
                for (section_idx, section) in sections.iter_mut().enumerate() {
                    // Section title - animate height to 0 when collapsed
                    if let Some(ref title) = section.title {
                        let is_collapsed = collapsed.get();
                        let title_anim_key = format!("{}_section_{}_title", ctx.key(), section_idx);

                        // Always render title, but height animates to 0 when collapsed
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
                            .when(!is_collapsed, |d| {
                                d.px(3.0).py(2.0).child(
                                    text(&title.to_uppercase())
                                        .size(11.0)
                                        .color(text_tertiary)
                                        .weight(FontWeight::SemiBold)
                                        .no_cursor()
                                        .no_wrap(),
                                )
                            });
                        // .when(is_collapsed, |d| d.p(0.0).h(0.0).w(0.0));

                        items_container = items_container.child(title_div);
                    }

                    
                    // Items - conditionally render icon-only (collapsed) or icon+label (expanded)
                    for (item_idx, item) in section.items.iter_mut().enumerate() {
                        let item_key = format!("{}_item_{}_{}", ctx.key(), section_idx, item_idx);
                        let item_label = item.label.clone();
                        let item_icon = item.icon.clone();
                        let mut item_is_active = item.is_active;
                        if item_is_active {
                            item.is_active = false; // clear from loop
                        } 
                        if let Some(active_item) = active_menu.get() {
                            item_is_active = active_item.label == item.label;
                        }
                        
                        let item_on_click = item.on_click.clone();
                        let collapsed_for_item = collapsed.clone();

                        let active_menu_for_trigger = active_menu.clone();
                        let item_for_trigger = item.clone();

                        let item_element = stateful_with_key::<ButtonState>(&item_key)
                            .deps([collapsed.signal_id()])
                            .on_state(move |ctx| {
                                let state = ctx.state();
                                let theme = ThemeState::get();
                                let is_collapsed = collapsed_for_item.get();


                                let (bg, icon_color, text_col) = if item_is_active {
                                    (primary.with_alpha(0.15), primary, text_primary)
                                } else {
                                    match state {
                                        ButtonState::Hovered | ButtonState::Pressed => (
                                            theme.color(ColorToken::SecondaryHover).with_alpha(0.5),
                                            primary,
                                            text_primary
                                        ),
                                        _ => (
                                            blinc_core::Color::TRANSPARENT,
                                            text_secondary,
                                            text_secondary,
                                        ),
                                    }
                                };

                                // Conditionally render: icon-only when collapsed, icon+label when expanded
                                // Animate position so items slide smoothly when section titles disappear
                                let item_anim_key = format!("{}_anim", ctx.key());
                                div()
                                    .w_fit()
                                    .h_fit()
                                    .flex_row()
                                    .items_center()
                                    .gap(3.0)
                                    .px(3.0)
                                    .py(2.0)
                                    .bg(bg)
                                    .cursor(CursorStyle::Pointer)
                                    .overflow_clip()
                                    .animate_bounds(
                                        VisualAnimationConfig::all()
                                            .with_key(&item_anim_key)
                                            .clip_to_animated()
                                            .snappy(),
                                    )
                                    .when(!is_collapsed, |d| {
                                        d.child(div().flex_shrink_0().child(
                                            svg(&item_icon).size(18.0, 18.0).color(icon_color),
                                        ))
                                        .child(
                                            div().child(
                                                text(&item_label)
                                                    .size(14.0)
                                                    .color(text_col)
                                                    .no_cursor()
                                                    .no_wrap(),
                                            ),
                                        )
                                    })
                                    .when(is_collapsed, |d| {
                                        d.child(div().flex_shrink_0().child(
                                            svg(&item_icon).size(18.0, 18.0).color(icon_color),
                                        ))
                                    })
                            })
                            .on_click(move |_| {
                                active_menu_for_trigger.update(|_| Some(item_for_trigger.clone()));
                                item_on_click();
                            });

                        items_container = items_container.child(item_element);
                    }
                }

                let sidebar_menu = sidebar_content.child(items_container);

                // If content builder is provided, wrap both in a flex-row container
                if let Some(ref content_fn) = content_builder {
                    let active = active_menu.get();
                    let main_content = content_fn(active);
                    // Wrap main content with flex_1 and animate_bounds for smooth expansion
                    // Use all() to animate both position (x changes when sidebar shrinks)
                    // and size (width grows when sidebar shrinks)
                    let content_wrapper = div()
                        .flex_1()
                        .h_full()
                        .overflow_clip()
                        .animate_bounds(
                            VisualAnimationConfig::all()
                                .with_key(&content_anim_key)
                                .clip_to_animated()
                                .snappy(),
                        )
                        .child(main_content);

                    // Outer container just needs flex-row layout
                    // (no animation needed - its bounds are w_full/h_full and don't change)
                    div()
                        .flex_row()
                        .w_full()
                        .h_full()
                        .child(sidebar_menu)
                        .child(content_wrapper)
                } else {
                    sidebar_menu
                }
            });

        Self {
            // Use flex_shrink_0 to prevent sidebar from being compressed in flex containers
            inner: stateful_container,
        }
    }
}

impl Deref for Sidebar {
    type Target = Stateful<NoState>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Sidebar {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
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
            built: OnceCell::new(),
        }
    }

    /// Get or build the component
    fn get_or_build(&self) -> &Sidebar {
        self.built.get_or_init(|| Sidebar::from_builder(self))
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

    /// Set the main content area that sits next to the sidebar
    ///
    /// When provided, the sidebar wraps both the sidebar menu and the main content
    /// in a shared container, enabling smooth coordinated animations during
    /// collapse/expand transitions.
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
}

/// Create a sidebar navigation component
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// let collapsed = use_state(|| false);
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
