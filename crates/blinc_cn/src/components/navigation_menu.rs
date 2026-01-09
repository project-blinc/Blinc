//! Navigation Menu component for site navigation
//!
//! A horizontal navigation bar with dropdown menus that appear on hover.
//! Commonly used for main website navigation with mega-menu style dropdowns.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Simple navigation menu
//! cn::navigation_menu()
//!     .item("Home", || println!("Go home"))
//!     .trigger("Products")
//!         .content(|| {
//!             div().flex_col().gap(4.0)
//!                 .child(cn::navigation_link("Electronics", || {}))
//!                 .child(cn::navigation_link("Clothing", || {}))
//!         })
//!     .trigger("About")
//!         .content(|| div().child(text("About us content")))
//!
//! // With indicator animation
//! cn::navigation_menu()
//!     .show_indicator(true)
//!     .item("Home", || {})
//!     .trigger("Services")
//!         .content(|| services_panel())
//! ```

use std::cell::OnceCell;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use blinc_animation::AnimationPreset;
use blinc_core::context_state::BlincContextState;
use blinc_core::State;
use blinc_layout::div::{Div, ElementBuilder, ElementTypeId};
use blinc_layout::element::CursorStyle;
use blinc_layout::motion::motion_derived;
use blinc_layout::overlay_state::get_overlay_manager;
use blinc_layout::prelude::*;
use blinc_layout::stateful::{stateful_with_key, ButtonState, NoState};
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::overlay::{
    OverlayAnimation, OverlayHandle, OverlayKind, OverlayManagerExt,
};
use blinc_layout::InstanceKey;
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

use crate::button::reset_button_state;

/// Navigation menu item type
#[derive(Clone)]
enum NavMenuItem {
    /// Simple clickable link
    Link {
        label: String,
        on_click: Arc<dyn Fn() + Send + Sync>,
    },
    /// Trigger with dropdown content
    Trigger {
        label: String,
        content: Arc<dyn Fn() -> Div + Send + Sync>,
    },
}

/// Navigation menu component
pub struct NavigationMenu {
    inner: Div,
}

impl NavigationMenu {
    fn from_builder(builder: &NavigationMenuBuilder) -> Self {
        let theme = ThemeState::get();
        let text_primary = theme.color(ColorToken::TextPrimary);
        let text_secondary = theme.color(ColorToken::TextSecondary);
        let surface = theme.color(ColorToken::Surface);
        let border = theme.color(ColorToken::Border);
        let radius = theme.radius(RadiusToken::Md);

        let key_base = builder.key.get().to_string();
        let items = builder.items.clone();
        let min_content_width = builder.min_content_width;

        // Centralized state management (following menubar pattern)
        // Track which trigger is currently open (None = none open)
        let active_menu: State<Option<usize>> =
            BlincContextState::get().use_state_keyed(&format!("{}_active", key_base), || None);
        // Track the current overlay handle
        let overlay_handle_state: State<Option<u64>> =
            BlincContextState::get().use_state_keyed(&format!("{}_handle", key_base), || None);

        // Create stateful container that rebuilds when active menu changes
        let container_key = format!("{}_container", key_base);
        let active_menu_for_container = active_menu.clone();

        let stateful_container = stateful_with_key::<NoState>(&container_key)
            .deps([active_menu.signal_id()])
            .on_state(move |_ctx| {
                let current_active = active_menu_for_container.get();
                let mut nav = div().flex_row().items_center().h_fit().gap(1.0);

                for (idx, item) in items.iter().enumerate() {
                    let item_key = format!("{}_{}", key_base, idx);

                    match item {
                        NavMenuItem::Link { label, on_click } => {
                            let label = label.clone();
                            let on_click = on_click.clone();
                            let active_menu_for_click = active_menu_for_container.clone();
                            let overlay_handle_for_click = overlay_handle_state.clone();

                            let link_item = stateful_with_key::<ButtonState>(&format!("{}_btn", item_key))
                                .on_state(move |ctx| {
                                    let state = ctx.state();
                                    let theme = ThemeState::get();

                                    let (bg, text_color) = match state {
                                        ButtonState::Hovered | ButtonState::Pressed => (
                                            theme.color(ColorToken::SecondaryHover).with_alpha(0.5),
                                            text_primary,
                                        ),
                                        _ => (blinc_core::Color::TRANSPARENT, text_secondary),
                                    };

                                    div()
                                        .flex_row()
                                        .items_center()
                                        .h_fit()
                                        .px(3.0)
                                        .py(2.0)
                                        .rounded(radius)
                                        .bg(bg)
                                        .cursor(CursorStyle::Pointer)
                                        .child(
                                            text(&label)
                                                .size(14.0)
                                                .medium()
                                                .color(text_color)
                                                .no_cursor()
                                                .pointer_events_none(),
                                        )
                                })
                                .on_click(move |_| {
                                    // Close any open dropdown immediately
                                    if let Some(handle_id) = overlay_handle_for_click.get() {
                                        let mgr = get_overlay_manager();
                                        mgr.close_immediate(OverlayHandle::from_raw(handle_id));
                                    }
                                    active_menu_for_click.set(None);
                                    on_click();
                                });

                            nav = nav.child(link_item);
                        }
                        NavMenuItem::Trigger { label, content } => {
                            let is_active = current_active == Some(idx);
                            let label = label.clone();
                            let content = content.clone();
                            let menu_key = format!("{}_menu_{}", key_base, idx);

                            // Clone states for different handlers
                            let active_menu_for_hover = active_menu_for_container.clone();
                            let overlay_handle_for_hover = overlay_handle_state.clone();
                            let overlay_handle_for_leave = overlay_handle_state.clone();

                            // Clone menu_key for different handlers
                            let menu_key_for_hover = menu_key.clone();

                            // For resetting other triggers' button states
                            let key_base_for_reset = key_base.clone();
                            let items_count = items.len();

                            let trigger_item = stateful_with_key::<ButtonState>(&format!("{}_btn", item_key))
                                .deps([active_menu_for_container.signal_id()])
                                .on_state(move |ctx| {
                                    let state = ctx.state();
                                    let theme = ThemeState::get();

                                    // Background: highlight when hovered, pressed, or active (menu open)
                                    let (bg, text_color) = if is_active
                                        || state == ButtonState::Hovered
                                        || state == ButtonState::Pressed
                                    {
                                        (
                                            theme.color(ColorToken::SecondaryHover).with_alpha(0.5),
                                            text_primary,
                                        )
                                    } else {
                                        (blinc_core::Color::TRANSPARENT, text_secondary)
                                    };

                                    // Chevron down icon
                                    let chevron = r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;

                                    div()
                                        .flex_row()
                                        .items_center()
                                        .h_fit()
                                        .gap(1.0)
                                        .px(3.0)
                                        .py(2.0)
                                        .rounded(radius)
                                        .bg(bg)
                                        .cursor(CursorStyle::Pointer)
                                        .child(
                                            text(&label)
                                                .size(14.0)
                                                .medium()
                                                .color(text_color)
                                                .no_cursor()
                                                .pointer_events_none(),
                                        )
                                        .child(div().pointer_events_none().child(svg(chevron).size(12.0, 12.0).color(text_color)))
                                })
                                .on_hover_enter(move |ctx| {
                                    let current_active = active_menu_for_hover.get();
                                    let mgr = get_overlay_manager();

                                    // Build the full motion key to check animation state
                                    let full_motion_key = format!("motion:navmenu_{}:child:0", menu_key_for_hover);

                                    // If this menu is already open, cancel any pending close
                                    if current_active == Some(idx) {
                                        if let Some(handle_id) = overlay_handle_for_hover.get() {
                                            let handle = OverlayHandle::from_raw(handle_id);
                                            if mgr.is_pending_close(handle) {
                                                mgr.hover_enter(handle);
                                            }
                                            // Also cancel exit animation if playing
                                            let motion = blinc_layout::selector::query_motion(&full_motion_key);
                                            if motion.is_exiting() {
                                                mgr.cancel_close(handle);
                                                motion.cancel_exit();
                                            }
                                        }
                                        return;
                                    }

                                    // Reset all other triggers' button states to clear lingering hover
                                    for i in 0..items_count {
                                        if i != idx {
                                            let other_key = format!("{}_{}_btn", key_base_for_reset, i);
                                            reset_button_state(&other_key);
                                        }
                                    }

                                    // Close all existing hover-based overlays immediately
                                    // This ensures only one menu is visible at a time when switching
                                    mgr.close_all_of(OverlayKind::Tooltip);

                                    // Calculate position (below the trigger)
                                    let x = ctx.bounds_x;
                                    let y = ctx.bounds_y + ctx.bounds_height + 4.0;

                                    // Show the hover-based dropdown
                                    let handle = show_navigation_dropdown(
                                        x,
                                        y,
                                        content.clone(),
                                        min_content_width,
                                        active_menu_for_hover.clone(),
                                        overlay_handle_for_hover.clone(),
                                        menu_key_for_hover.clone(),
                                        surface,
                                        border,
                                        radius,
                                    );

                                    overlay_handle_for_hover.set(Some(handle.id()));
                                    active_menu_for_hover.set(Some(idx));
                                })
                                .on_hover_leave(move |_| {
                                    // Start close delay when leaving trigger
                                    if let Some(handle_id) = overlay_handle_for_leave.get() {
                                        let mgr = get_overlay_manager();
                                        mgr.close_all_of(blinc_layout::widgets::overlay::OverlayKind::Tooltip);
                                        let handle = OverlayHandle::from_raw(handle_id);

                                        // Only start close if overlay is visible and not already closing
                                        if mgr.is_visible(handle) && !mgr.is_pending_close(handle) {
                                            mgr.hover_leave(handle);
                                        }
                                    }
                                });

                            nav = nav.child(trigger_item);
                        }
                    }
                }

                nav
            });

        Self {
            inner: div().child(stateful_container),
        }
    }
}

/// Show navigation dropdown content using hover_card for proper hover handling
#[allow(clippy::too_many_arguments)]
fn show_navigation_dropdown(
    x: f32,
    y: f32,
    content: Arc<dyn Fn() -> Div + Send + Sync>,
    min_width: f32,
    active_menu_state: State<Option<usize>>,
    overlay_handle_state: State<Option<u64>>,
    key: String,
    surface: blinc_core::Color,
    border: blinc_core::Color,
    radius: f32,
) -> OverlayHandle {
    use blinc_layout::widgets::overlay::AnchorDirection;

    // Clone states for different handlers
    let active_menu_for_close = active_menu_state.clone();
    let overlay_handle_for_close = overlay_handle_state.clone();
    let overlay_handle_for_hover = overlay_handle_state.clone();

    // Motion key for animations
    let motion_key = format!("navmenu_{}", key);

    let mgr = get_overlay_manager();

    // Use hover_card for transient hover-based overlay (like menubar)
    let handle = mgr
        .hover_card()
        .at(x, y)
        .anchor_direction(AnchorDirection::Bottom)
        .animation(OverlayAnimation::none()) // Instant show/hide for snappy feel
        .dismiss_on_escape(true)
        .on_close(move || {
            active_menu_for_close.set(None);
            overlay_handle_for_close.set(None);
        })
        .content(move || {
            let user_content = content();

            // Clone handle for hover handlers on content
            let handle_for_enter = overlay_handle_for_hover.clone();
            let handle_for_leave = overlay_handle_for_hover.clone();

            let panel = div()
                .min_w(min_width / 4.0)
                .bg(surface)
                .border(1.0, border)
                .rounded(radius)
                .shadow_lg()
                .overflow_clip()
                .py(1.0)
                .child(user_content)
                // Add hover handlers to cancel/start close when mouse enters/leaves content
                .on_hover_enter(move |_| {
                    if let Some(handle_id) = handle_for_enter.get() {
                        let mgr = get_overlay_manager();
                        let handle = OverlayHandle::from_raw(handle_id);
                        if mgr.is_pending_close(handle) {
                            mgr.hover_enter(handle); // Cancel close delay
                        }
                    }
                })
                .on_hover_leave(move |_| {
                    if let Some(handle_id) = handle_for_leave.get() {
                        let mgr = get_overlay_manager();
                        let handle = OverlayHandle::from_raw(handle_id);
                        if mgr.is_visible(handle) && !mgr.is_pending_close(handle) {
                            mgr.hover_leave(handle);
                        }
                    }
                });

            div().child(
                motion_derived(&motion_key)
                    .enter_animation(AnimationPreset::dropdown_in(150))
                    .exit_animation(AnimationPreset::dropdown_out(100))
                    .child(panel),
            )
        })
        .show();

    handle
}

impl Deref for NavigationMenu {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for NavigationMenu {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for NavigationMenu {
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
}

/// Builder for navigation menu
pub struct NavigationMenuBuilder {
    key: InstanceKey,
    items: Vec<NavMenuItem>,
    min_content_width: f32,
    built: OnceCell<NavigationMenu>,
}

impl NavigationMenuBuilder {
    /// Create a new navigation menu builder
    #[track_caller]
    pub fn new() -> Self {
        Self {
            key: InstanceKey::new("nav_menu"),
            items: Vec::new(),
            min_content_width: 50.0, // Scaled by 4x = 200px
            built: OnceCell::new(),
        }
    }

    /// Get or build the component
    fn get_or_build(&self) -> &NavigationMenu {
        self.built
            .get_or_init(|| NavigationMenu::from_builder(self))
    }

    /// Add a simple link item
    pub fn item<F>(mut self, label: impl Into<String>, on_click: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.items.push(NavMenuItem::Link {
            label: label.into(),
            on_click: Arc::new(on_click),
        });
        self
    }

    /// Add a trigger with dropdown content
    pub fn trigger<F>(mut self, label: impl Into<String>, content: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.items.push(NavMenuItem::Trigger {
            label: label.into(),
            content: Arc::new(content),
        });
        self
    }

    /// Set minimum width for dropdown content panels
    pub fn min_content_width(mut self, width: f32) -> Self {
        self.min_content_width = width;
        self
    }
}

impl Default for NavigationMenuBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ElementBuilder for NavigationMenuBuilder {
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
}

/// A navigation link component for use inside navigation menu content
pub struct NavigationLink {
    inner: Div,
}

impl NavigationLink {
    fn from_builder(builder: &NavigationLinkBuilder) -> Self {
        let theme = ThemeState::get();
        let text_primary = theme.color(ColorToken::TextPrimary);
        let text_secondary = theme.color(ColorToken::TextSecondary);
        let radius = theme.radius(RadiusToken::Sm);

        let label = builder.label.clone();
        let description = builder.description.clone();
        let on_click = builder.on_click.clone();
        let key = builder.key.get().to_string();

        let link = stateful_with_key::<ButtonState>(&key)
            .on_state(move |ctx| {
                let state = ctx.state();
                let theme = ThemeState::get();

                let bg = match state {
                    ButtonState::Hovered | ButtonState::Pressed => {
                        theme.color(ColorToken::SecondaryHover).with_alpha(0.5)
                    }
                    _ => blinc_core::Color::TRANSPARENT,
                };

                let mut content = div()
                    .flex_col()
                    .w_full()
                    .gap(1.0)
                    .px(3.0) // Horizontal padding on item
                    .py(2.0) // Vertical padding on item
                    .rounded(radius)
                    .bg(bg)
                    .cursor(CursorStyle::Pointer)
                    .child(
                        text(&label)
                            .size(14.0)
                            .medium()
                            .color(text_primary)
                            .no_cursor()
                            .pointer_events_none(),
                    );

                if let Some(ref desc) = description {
                    content = content.child(
                        text(desc)
                            .size(12.0)
                            .color(text_secondary)
                            .no_cursor()
                            .pointer_events_none(),
                    );
                }

                content
            })
            .on_click(move |_| {
                if let Some(ref cb) = on_click {
                    cb();
                }
            });

        Self {
            inner: div().child(link),
        }
    }
}

impl Deref for NavigationLink {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for NavigationLink {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for NavigationLink {
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
}

/// Builder for navigation link
pub struct NavigationLinkBuilder {
    key: InstanceKey,
    label: String,
    description: Option<String>,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    built: OnceCell<NavigationLink>,
}

impl NavigationLinkBuilder {
    /// Create a new navigation link builder
    #[track_caller]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            key: InstanceKey::new("nav_link"),
            label: label.into(),
            description: None,
            on_click: None,
            built: OnceCell::new(),
        }
    }

    /// Get or build the component
    fn get_or_build(&self) -> &NavigationLink {
        self.built
            .get_or_init(|| NavigationLink::from_builder(self))
    }

    /// Set description text
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set click handler
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_click = Some(Arc::new(handler));
        self
    }
}

impl ElementBuilder for NavigationLinkBuilder {
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
}

/// Create a navigation menu component
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// cn::navigation_menu()
///     .item("Home", || println!("Home clicked"))
///     .trigger("Products", || {
///         div().flex_col().gap(4.0)
///             .child(cn::navigation_link("Electronics").on_click(|| {}))
///             .child(cn::navigation_link("Clothing").on_click(|| {}))
///     })
///     .trigger("About", || {
///         div().child(text("About us"))
///     })
/// ```
#[track_caller]
pub fn navigation_menu() -> NavigationMenuBuilder {
    NavigationMenuBuilder::new()
}

/// Create a navigation link for use inside navigation menu content
///
/// # Example
///
/// ```ignore
/// cn::navigation_link("Products")
///     .description("Browse our product catalog")
///     .on_click(|| {})
/// ```
#[track_caller]
pub fn navigation_link(label: impl Into<String>) -> NavigationLinkBuilder {
    NavigationLinkBuilder::new(label)
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
    fn test_navigation_menu_basic() {
        init_theme();
        let _ = navigation_menu()
            .item("Home", || {})
            .trigger("Products", || div());
    }

    #[test]
    fn test_navigation_link() {
        init_theme();
        let _ = navigation_link("Test")
            .description("A test link")
            .on_click(|| {});
    }
}
