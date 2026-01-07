//! Menubar component - horizontal menu bar with dropdown menus
//!
//! A themed horizontal menubar with multiple dropdown menus, like File, Edit, View.
//! Each menu item opens a dropdown with actions and submenus.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     cn::menubar()
//!         .menu("File", |m| {
//!             m.item("New", || println!("New"))
//!              .item_with_shortcut("Open", "Ctrl+O", || println!("Open"))
//!              .item_with_shortcut("Save", "Ctrl+S", || println!("Save"))
//!              .separator()
//!              .item("Exit", || println!("Exit"))
//!         })
//!         .menu("Edit", |m| {
//!             m.item_with_shortcut("Undo", "Ctrl+Z", || {})
//!              .item_with_shortcut("Redo", "Ctrl+Y", || {})
//!              .separator()
//!              .item_with_shortcut("Cut", "Ctrl+X", || {})
//!              .item_with_shortcut("Copy", "Ctrl+C", || {})
//!              .item_with_shortcut("Paste", "Ctrl+V", || {})
//!         })
//!         .menu("Help", |m| {
//!             m.item("About", || {})
//!         })
//! }
//! ```

use std::cell::OnceCell;
use std::sync::Arc;

use blinc_animation::AnimationPreset;
use blinc_core::context_state::BlincContextState;
use blinc_core::{Color, State};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, RenderProps};
use blinc_layout::motion::motion_derived;
use blinc_layout::overlay_state::get_overlay_manager;
use blinc_layout::prelude::*;
use blinc_layout::stateful::{ButtonState, Stateful};
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::hr::hr_with_bg;
use blinc_layout::widgets::overlay::{OverlayAnimation, OverlayHandle, OverlayManagerExt};
use blinc_layout::InstanceKey;
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

use crate::button::{reset_button_state, use_button_state};
use super::context_menu::{ContextMenuItem, SubmenuBuilder};

/// How menus are triggered to open
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuTriggerMode {
    /// Open menu on click (default)
    #[default]
    Click,
    /// Open menu on hover
    Hover,
}

/// Trigger type for a menubar menu
#[derive(Clone)]
pub enum MenubarTrigger {
    /// Simple text label
    Label(String),
    /// Custom trigger component (receives is_open state)
    Custom(Arc<dyn Fn(bool) -> Div + Send + Sync>),
}

impl std::fmt::Debug for MenubarTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MenubarTrigger::Label(s) => write!(f, "Label({:?})", s),
            MenubarTrigger::Custom(_) => write!(f, "Custom(...)"),
        }
    }
}

/// A single menu in the menubar (e.g., "File", "Edit")
#[derive(Clone)]
pub struct MenubarMenu {
    /// Trigger displayed in the menubar
    trigger: MenubarTrigger,
    /// Menu items in the dropdown
    items: Vec<ContextMenuItem>,
}

impl MenubarMenu {
    /// Create a new menubar menu with a text label
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            trigger: MenubarTrigger::Label(label.into()),
            items: Vec::new(),
        }
    }

    /// Create a new menubar menu with a custom trigger
    pub fn new_custom<F>(trigger: F) -> Self
    where
        F: Fn(bool) -> Div + Send + Sync + 'static,
    {
        Self {
            trigger: MenubarTrigger::Custom(Arc::new(trigger)),
            items: Vec::new(),
        }
    }

    /// Add a menu item
    pub fn item<F>(mut self, label: impl Into<String>, on_click: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.items
            .push(ContextMenuItem::new(label).on_click(on_click));
        self
    }

    /// Add a menu item with keyboard shortcut display
    pub fn item_with_shortcut<F>(
        mut self,
        label: impl Into<String>,
        shortcut: impl Into<String>,
        on_click: F,
    ) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.items.push(
            ContextMenuItem::new(label)
                .shortcut(shortcut)
                .on_click(on_click),
        );
        self
    }

    /// Add a menu item with icon
    pub fn item_with_icon<F>(
        mut self,
        label: impl Into<String>,
        icon_svg: impl Into<String>,
        on_click: F,
    ) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.items.push(
            ContextMenuItem::new(label)
                .icon(icon_svg)
                .on_click(on_click),
        );
        self
    }

    /// Add a disabled menu item
    pub fn item_disabled(mut self, label: impl Into<String>) -> Self {
        self.items.push(ContextMenuItem::new(label).disabled());
        self
    }

    /// Add a separator line
    pub fn separator(mut self) -> Self {
        self.items.push(ContextMenuItem::separator());
        self
    }

    /// Add a submenu
    pub fn submenu<F>(mut self, label: impl Into<String>, builder: F) -> Self
    where
        F: FnOnce(SubmenuBuilder) -> SubmenuBuilder,
    {
        let sub = builder(SubmenuBuilder::new_public());
        self.items
            .push(ContextMenuItem::new(label).submenu(sub.items()));
        self
    }
}

/// Styling options for menu triggers
#[derive(Clone, Debug)]
pub struct MenuTriggerStyle {
    /// Horizontal padding (default: 12.0)
    pub px: f32,
    /// Vertical padding (default: 8.0)
    pub py: f32,
    /// Font size (default: 14.0)
    pub font_size: f32,
    /// Hover/active background color (default: theme SecondaryHover with 0.65 alpha)
    pub hover_bg: Option<Color>,
    /// Border radius (default: theme RadiusToken::Sm)
    pub radius: Option<f32>,
}

impl Default for MenuTriggerStyle {
    fn default() -> Self {
        Self {
            px: 12.0,
            py: 8.0,
            font_size: 14.0,
            hover_bg: None,
            radius: None,
        }
    }
}

impl MenuTriggerStyle {
    /// Create a new trigger style with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set horizontal padding
    pub fn px(mut self, px: f32) -> Self {
        self.px = px;
        self
    }

    /// Set vertical padding
    pub fn py(mut self, py: f32) -> Self {
        self.py = py;
        self
    }

    /// Set font size
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// Set hover background color
    pub fn hover_bg(mut self, color: Color) -> Self {
        self.hover_bg = Some(color);
        self
    }

    /// Set border radius
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }
}

/// Builder for menubar component
pub struct MenubarBuilder {
    /// Menus in the menubar
    menus: Vec<MenubarMenu>,
    /// Trigger mode (click or hover)
    trigger_mode: MenuTriggerMode,
    /// Trigger styling options
    trigger_style: MenuTriggerStyle,
    /// Unique instance key
    key: InstanceKey,
    /// Built component cache
    built: OnceCell<Menubar>,
}

impl std::fmt::Debug for MenubarBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenubarBuilder")
            .field("menus", &self.menus.len())
            .finish()
    }
}

impl MenubarBuilder {
    /// Create a new menubar builder
    #[track_caller]
    pub fn new() -> Self {
        Self {
            menus: Vec::new(),
            trigger_mode: MenuTriggerMode::default(),
            trigger_style: MenuTriggerStyle::default(),
            key: InstanceKey::new("menubar"),
            built: OnceCell::new(),
        }
    }

    /// Set the trigger mode for opening menus
    ///
    /// - `MenuTriggerMode::Click` (default): Menus open when clicked
    /// - `MenuTriggerMode::Hover`: Menus open when hovered
    pub fn trigger_mode(mut self, mode: MenuTriggerMode) -> Self {
        self.trigger_mode = mode;
        self
    }

    /// Set the trigger style for menu triggers
    ///
    /// # Example
    ///
    /// ```ignore
    /// cn::menubar()
    ///     .trigger_style(MenuTriggerStyle::new().px(16.0).py(10.0).font_size(16.0))
    ///     .menu("File", |m| { ... })
    /// ```
    pub fn trigger_style(mut self, style: MenuTriggerStyle) -> Self {
        self.trigger_style = style;
        self
    }

    /// Add a menu to the menubar
    ///
    /// # Example
    ///
    /// ```ignore
    /// cn::menubar()
    ///     .menu("File", |m| {
    ///         m.item("New", || {})
    ///          .item("Open", || {})
    ///     })
    /// ```
    pub fn menu<F>(mut self, label: impl Into<String>, builder: F) -> Self
    where
        F: FnOnce(MenubarMenu) -> MenubarMenu,
    {
        let menu = builder(MenubarMenu::new(label));
        self.menus.push(menu);
        self
    }

    /// Add a menu with a custom trigger component to the menubar
    ///
    /// The trigger function receives a boolean indicating whether the menu is open.
    ///
    /// # Example
    ///
    /// ```ignore
    /// cn::menubar()
    ///     .menu_custom(
    ///         |is_open| {
    ///             cn::button(if is_open { "▼ Actions" } else { "▶ Actions" })
    ///                 .variant(ButtonVariant::Ghost)
    ///         },
    ///         |m| m.item("Action 1", || {}).item("Action 2", || {}),
    ///     )
    /// ```
    pub fn menu_custom<T, F>(mut self, trigger: T, builder: F) -> Self
    where
        T: Fn(bool) -> Div + Send + Sync + 'static,
        F: FnOnce(MenubarMenu) -> MenubarMenu,
    {
        let menu = builder(MenubarMenu::new_custom(trigger));
        self.menus.push(menu);
        self
    }

    /// Get or build the component
    fn get_or_build(&self) -> &Menubar {
        self.built.get_or_init(|| self.build_component())
    }

    /// Build the menubar component
    fn build_component(&self) -> Menubar {
        let theme = ThemeState::get();
        let bg = theme.color(ColorToken::Surface);
        let border = theme.color(ColorToken::Border);

        // State for tracking which menu is currently open (None = none open)
        let active_menu: State<Option<usize>> =
            BlincContextState::get().use_state_keyed(self.key.get(), || None);

        // State for overlay handle
        let overlay_handle_state: State<Option<u64>> =
            BlincContextState::get().use_state_keyed(&self.key.derive("handle"), || None);

        let menus = self.menus.clone();
        let key_base = self.key.get().to_string();
        let trigger_mode = self.trigger_mode;
        let trigger_style = self.trigger_style.clone();

        let mut menubar = div()
            .flex_row()
            .items_center()
            .h_fit()
            .bg(bg)
            .border_bottom(1.0, border);

        // Add each menu trigger
        for (idx, menu) in menus.iter().enumerate() {
            let menu_trigger = menu.trigger.clone();
            let menu_items = menu.items.clone();
            let menu_key = format!("{}_{}", key_base, idx);

            let active_menu_for_trigger = active_menu.clone();
            let active_menu_for_state = active_menu.clone();
            let active_menu_for_hover = active_menu.clone();
            let overlay_handle_for_trigger = overlay_handle_state.clone();
            let overlay_handle_for_show = overlay_handle_state.clone();
            let overlay_handle_for_hover = overlay_handle_state.clone();

            let button_state = use_button_state(&format!("{}_btn", menu_key));

            // Clone menu items and key for different handlers
            let menu_items_for_hover = menu_items.clone();
            let menu_key_for_hover = menu_key.clone();
            let menu_key_for_click = menu_key.clone();

            // Clone style values for closures
            let style_px = trigger_style.px;
            let style_py = trigger_style.py;
            let style_font_size = trigger_style.font_size;
            let style_hover_bg = trigger_style.hover_bg;
            let style_radius = trigger_style.radius;

            // Build the menu trigger button
            let mut trigger = Stateful::with_shared_state(button_state)
                .h_fit()
                .px(style_px / 4.0) // Convert to quarter units used by Stateful
                .py(style_py / 4.0)
                .cursor_pointer()
                .deps(&[active_menu.signal_id()])
                .on_state(move |state, container: &mut Div| {
                    let theme = ThemeState::get();
                    let is_active = active_menu_for_state.get() == Some(idx);

                    // Background: highlight when hovered, pressed, or active (menu open)
                    let item_bg = if is_active
                        || *state == ButtonState::Hovered
                        || *state == ButtonState::Pressed
                    {
                        style_hover_bg.unwrap_or_else(|| {
                            theme.color(ColorToken::SecondaryHover).with_alpha(0.65)
                        })
                    } else {
                        Color::TRANSPARENT
                    };

                    let radius = style_radius.unwrap_or_else(|| theme.radius(RadiusToken::Sm));

                    // Build trigger content based on trigger type
                    let trigger_content: Div = match &menu_trigger {
                        MenubarTrigger::Label(label) => {
                            let text_col = theme.color(ColorToken::TextPrimary);
                            div()
                                .flex_row()
                                .items_center()
                                .bg(item_bg)
                                .rounded(radius)
                                .px(2.0)
                                .py(1.0)
                                .child(
                                    text(label)
                                        .size(style_font_size)
                                        .color(text_col)
                                        .no_cursor()
                                        .pointer_events_none(),
                                )
                        }
                        MenubarTrigger::Custom(custom_fn) => {
                            // Call custom trigger with is_open state
                            div()
                                .flex_row()
                                .items_center()
                                .bg(item_bg)
                                .rounded(radius)
                                .child(custom_fn(is_active))
                        }
                    };

                    container.merge(trigger_content);
                });

            // Add click handler (used for Click mode, or to toggle in Hover mode)
            trigger = trigger.on_click(move |ctx| {
                let current_active = active_menu_for_trigger.get();
                let mgr = get_overlay_manager();

                // If this menu is already open, close it
                if current_active == Some(idx) {
                    if let Some(handle_id) = overlay_handle_for_trigger.get() {
                        mgr.close(OverlayHandle::from_raw(handle_id));
                    }
                    active_menu_for_trigger.set(None);
                    return;
                }

                // Close any existing menu
                if let Some(handle_id) = overlay_handle_for_trigger.get() {
                    let handle = OverlayHandle::from_raw(handle_id);
                    if !mgr.is_closing(handle) && !mgr.is_pending_close(handle) {
                        mgr.close(handle);
                    }
                }

                // Open this menu
                active_menu_for_trigger.set(Some(idx));

                // Calculate position (below the trigger)
                let x = ctx.bounds_x;
                let y = ctx.bounds_y + ctx.bounds_height + 4.0; // 4px offset

                // Show the dropdown
                let handle = show_menubar_dropdown(
                    x,
                    y,
                    &menu_items,
                    180.0, // min_width
                    overlay_handle_for_show.clone(),
                    active_menu_for_trigger.clone(),
                    menu_key_for_click.clone(),
                );

                overlay_handle_for_show.set(Some(handle.id()));
            });

            // Add hover handlers for Hover trigger mode
            if trigger_mode == MenuTriggerMode::Hover {
                // Clone for hover leave handler
                let overlay_handle_for_hover_leave = overlay_handle_state.clone();

                trigger = trigger.on_hover_enter(move |ctx| {
                    let current_active = active_menu_for_hover.get();
                    let mgr = get_overlay_manager();

                    // Build the full motion key to check animation state
                    let full_motion_key = format!("motion:menubar_{}:child:0", menu_key_for_hover);

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

                    // Close all existing hover-based overlays (menus) immediately
                    // This ensures only one menu is visible at a time when switching
                    mgr.close_all_of(blinc_layout::widgets::overlay::OverlayKind::Tooltip);

                    // Open this menu
                    active_menu_for_hover.set(Some(idx));

                    // Calculate position (below the trigger)
                    let x = ctx.bounds_x;
                    let y = ctx.bounds_y + ctx.bounds_height + 4.0;

                    // Show the hover-based dropdown
                    let handle = show_menubar_hover_dropdown(
                        x,
                        y,
                        &menu_items_for_hover,
                        180.0,
                        overlay_handle_for_hover.clone(),
                        active_menu_for_hover.clone(),
                        menu_key_for_hover.clone(),
                    );

                    overlay_handle_for_hover.set(Some(handle.id()));
                });

                // Add hover leave handler to start close delay
                trigger = trigger.on_hover_leave(move |_| {
                    if let Some(handle_id) = overlay_handle_for_hover_leave.get() {
                        let mgr = get_overlay_manager();
                        let handle = OverlayHandle::from_raw(handle_id);

                        // Only start close if overlay is visible and in Open state
                        if mgr.is_visible(handle) && !mgr.is_pending_close(handle) {
                            mgr.hover_leave(handle);
                        }
                    }
                });
            }

            menubar = menubar.child(trigger);
        }

        Menubar { inner: menubar }
    }
}

impl Default for MenubarBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Show the menubar dropdown overlay
fn show_menubar_dropdown(
    x: f32,
    y: f32,
    items: &[ContextMenuItem],
    min_width: f32,
    handle_state: State<Option<u64>>,
    active_menu_state: State<Option<usize>>,
    key: String,
) -> OverlayHandle {
    let theme = ThemeState::get();
    let bg = theme.color(ColorToken::Surface);
    let border = theme.color(ColorToken::Border);
    let text_color = theme.color(ColorToken::TextPrimary);
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let text_tertiary = theme.color(ColorToken::TextTertiary);
    let radius = theme.radius(RadiusToken::Md);
    let font_size = 14.0;
    let padding = 12.0;

    let items = items.to_vec();
    let item_count = items.len();

    let handle_state_for_content = handle_state.clone();
    let active_menu_for_content = active_menu_state.clone();
    let handle_state_for_close = handle_state.clone();
    let active_menu_for_close = active_menu_state.clone();

    let mgr = get_overlay_manager();

    // Motion key for animation
    let motion_key_str = format!("menubar_{}", key);
    let key_for_close = key.clone();
    // let motion_key_with_child = format!("{}:child:0", motion_key_str);

    let handle = mgr
        .dropdown()
        .at(x, y)
        .animation(OverlayAnimation::none()) // Instant show/hide
        .dismiss_on_escape(true)
        // .motion_key(&motion_key_with_child)
        .on_close(move || {
            active_menu_for_close.set(None);
            handle_state_for_close.set(None);
            // Reset all button states to clear lingering hover/pressed states
            for idx in 0..item_count {
                let item_key = format!("{}_item-{}", key_for_close, idx);
                reset_button_state(&item_key);
            }
        })
        .content(move || {
            build_menubar_dropdown_content(
                &items,
                min_width,
                &handle_state_for_content,
                &active_menu_for_content,
                &motion_key_str,
                bg,
                border,
                text_color,
                text_secondary,
                text_tertiary,
                radius,
                font_size,
                padding,
            )
        })
        .show();

    handle
}

/// Show the menubar dropdown overlay using hover-based overlay (for Hover trigger mode)
fn show_menubar_hover_dropdown(
    x: f32,
    y: f32,
    items: &[ContextMenuItem],
    min_width: f32,
    handle_state: State<Option<u64>>,
    active_menu_state: State<Option<usize>>,
    key: String,
) -> OverlayHandle {
    let theme = ThemeState::get();
    let bg = theme.color(ColorToken::Surface);
    let border = theme.color(ColorToken::Border);
    let text_color = theme.color(ColorToken::TextPrimary);
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let text_tertiary = theme.color(ColorToken::TextTertiary);
    let radius = theme.radius(RadiusToken::Md);
    let font_size = 14.0;
    let padding = 12.0;

    let items = items.to_vec();
    let item_count = items.len();

    let handle_state_for_content = handle_state.clone();
    let active_menu_for_content = active_menu_state.clone();
    let handle_state_for_close = handle_state.clone();
    let active_menu_for_close = active_menu_state.clone();
    let handle_state_for_hover = handle_state.clone();

    let mgr = get_overlay_manager();

    let menu_key = key.clone();
    let key_for_close = key.clone();

    // Use hover_card() for transient hover-based overlay
    // No motion animation - menus show/hide instantly for snappy feel
    let handle = mgr
        .hover_card()
        .at(x, y)
        .anchor_direction(blinc_layout::widgets::overlay::AnchorDirection::Bottom)
        .animation(OverlayAnimation::none()) // Instant show/hide
        .dismiss_on_escape(true)
        .on_close(move || {
            active_menu_for_close.set(None);
            handle_state_for_close.set(None);
            // Reset all button states to clear lingering hover/pressed states
            for idx in 0..item_count {
                let item_key = format!("{}_item-{}", key_for_close, idx);
                reset_button_state(&item_key);
            }
        })
        .content(move || {
            build_menubar_hover_dropdown_content(
                &items,
                min_width,
                &handle_state_for_content,
                &active_menu_for_content,
                &handle_state_for_hover,
                &menu_key,
                bg,
                border,
                text_color,
                text_secondary,
                text_tertiary,
                radius,
                font_size,
                padding,
            )
        })
        .show();

    handle
}

/// Build the dropdown menu content for a menubar menu (hover mode - with hover handlers)
#[allow(clippy::too_many_arguments)]
fn build_menubar_hover_dropdown_content(
    items: &[ContextMenuItem],
    width: f32,
    overlay_handle_state: &State<Option<u64>>,
    _active_menu_state: &State<Option<usize>>,
    handle_state_for_hover: &State<Option<u64>>,
    key: &str,
    bg: Color,
    border: Color,
    text_color: Color,
    text_secondary: Color,
    text_tertiary: Color,
    radius: f32,
    font_size: f32,
    padding: f32,
) -> Div {
    let menu_id = key;

    // State for tracking open submenu
    let submenu_handle: State<Option<u64>> =
        BlincContextState::get().use_state_keyed(&format!("{}_submenu", key), || None);

    // Clone for hover handlers on the content
    let handle_state_for_enter = handle_state_for_hover.clone();
    let handle_state_for_leave = handle_state_for_hover.clone();

    let mut menu = div()
        .id(menu_id)
        .flex_col()
        .w(width)
        .bg(bg)
        .border(1.0, border)
        .rounded(radius)
        .shadow_lg()
        .overflow_clip()
        .h_fit()
        .py(1.0)
        // Add hover enter handler to cancel pending close when mouse enters dropdown
        .on_hover_enter(move |_| {
            if let Some(handle_id) = handle_state_for_enter.get() {
                let mgr = get_overlay_manager();
                let handle = OverlayHandle::from_raw(handle_id);
                if mgr.is_pending_close(handle) {
                    mgr.hover_enter(handle); // Cancel close delay
                }
            }
        })
        // Add hover leave handler to start close when mouse leaves dropdown
        .on_hover_leave(move |_| {
            if let Some(handle_id) = handle_state_for_leave.get() {
                let mgr = get_overlay_manager();
                let handle = OverlayHandle::from_raw(handle_id);
                if mgr.is_visible(handle) && !mgr.is_pending_close(handle) {
                    mgr.hover_leave(handle);
                }
            }
        });

    for (idx, item) in items.iter().enumerate() {
        if item.is_separator() {
            menu = menu.child(hr_with_bg(bg));
        } else {
            let item_label = item.get_label().to_string();
            let item_shortcut = item.get_shortcut().map(|s| s.to_string());
            let item_icon = item.get_icon().map(|s| s.to_string());
            let item_disabled = item.is_disabled();
            let item_on_click = item.get_on_click();
            let has_submenu = item.has_submenu();
            let submenu_items = item.get_submenu().cloned();

            // let handle_state_for_click = overlay_handle_state.clone();
            let submenu_handle_for_hover = submenu_handle.clone();
            let submenu_handle_for_leave = submenu_handle.clone();

            // Create a stable key for this item's button state
            let item_key = format!("{}_item-{}", key, idx);
            let submenu_key = format!("{}_sub-{}", key, idx);
            let button_state = use_button_state(&item_key);

            let item_text_color = if item_disabled {
                text_tertiary
            } else {
                text_color
            };

            let shortcut_color = text_secondary;

            // Build the stateful row element
            let mut row = Stateful::with_shared_state(button_state)
                .w_full()
                .h_fit()
                .py(padding / 4.0)
                .px(padding / 2.0)
                .bg(bg)
                .cursor(if item_disabled {
                    CursorStyle::NotAllowed
                } else {
                    CursorStyle::Pointer
                })
                .on_state(move |state, container: &mut Div| {
                    let theme = ThemeState::get();
                    // Apply hover background based on button state
                    let item_bg = if (*state == ButtonState::Hovered
                        || *state == ButtonState::Pressed)
                        && !item_disabled
                    {
                        theme.color(ColorToken::SecondaryHover).with_alpha(0.65)
                    } else {
                        bg
                    };

                    let text_col = if (*state == ButtonState::Hovered
                        || *state == ButtonState::Pressed)
                        && !item_disabled
                    {
                        theme.color(ColorToken::TextSecondary)
                    } else {
                        item_text_color
                    };

                    // Left side: icon + label
                    let mut left_side = div()
                        .w_fit()
                        .h_fit()
                        .flex_row()
                        .items_center()
                        .gap(padding / 4.0);

                    if let Some(ref icon_svg) = item_icon {
                        left_side =
                            left_side.child(svg(icon_svg).size(16.0, 16.0).color(item_text_color));
                    }

                    left_side = left_side
                        .child(
                            text(&item_label)
                                .size(font_size)
                                .color(text_col)
                                .no_cursor()
                                .pointer_events_none(),
                        )
                        .pointer_events_none();

                    // Right side: shortcut or submenu arrow
                    let right_side: Option<Div> = if let Some(ref shortcut) = item_shortcut {
                        Some(div().child(
                            text(shortcut)
                                .size(font_size - 2.0)
                                .color(shortcut_color)
                                .no_cursor(),
                        ))
                    } else if has_submenu {
                        let chevron_right = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"#;
                        Some(
                            div()
                                .child(svg(chevron_right).size(12.0, 12.0).color(text_tertiary))
                                .pointer_events_none(),
                        )
                    } else {
                        None
                    };

                    let mut row_content = div()
                        .w_full()
                        .h_fit()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .bg(item_bg)
                        .child(left_side);

                    if let Some(right) = right_side {
                        row_content = row_content.child(right);
                    }

                    container.merge(row_content);
                })
                .on_click(move |_| {
                    if !item_disabled && !has_submenu {
                        if let Some(ref cb) = item_on_click {
                            cb();
                        }

                        // Close all menus immediately (no animation delay)
                        let mgr = get_overlay_manager();
                        mgr.close_all_of(blinc_layout::widgets::overlay::OverlayKind::Tooltip);
                    }
                });

            // Add hover handlers for submenu items
            if has_submenu && !item_disabled {
                let submenu_items_for_hover = submenu_items.clone();
                let overlay_handle_for_submenu = overlay_handle_state.clone();
                let submenu_key_for_hover = submenu_key.clone();

                row = row.on_hover_enter(move |ctx| {
                    // Close any existing submenu
                    if let Some(handle_id) = submenu_handle_for_hover.get() {
                        let mgr = get_overlay_manager();
                        let handle = OverlayHandle::from_raw(handle_id);
                        if !mgr.is_closing(handle) && !mgr.is_pending_close(handle) {
                            mgr.close(handle);
                        }
                    }

                    // Show submenu to the right of this item
                    if let Some(ref items) = submenu_items_for_hover {
                        let x = ctx.bounds_x + ctx.bounds_width + 4.0;
                        let y = ctx.bounds_y;

                        let handle = show_menubar_submenu(
                            x,
                            y,
                            items,
                            160.0,
                            overlay_handle_for_submenu.clone(),
                            submenu_handle_for_hover.clone(),
                            submenu_key_for_hover.clone(),
                        );
                        submenu_handle_for_hover.set(Some(handle.id()));
                    }
                });
            } else {
                // When hovering a non-submenu item, close any open submenu
                row = row.on_hover_enter(move |_| {
                    if let Some(handle_id) = submenu_handle_for_leave.get() {
                        let mgr = get_overlay_manager();
                        let handle = OverlayHandle::from_raw(handle_id);
                        if !mgr.is_closing(handle) && !mgr.is_pending_close(handle) {
                            mgr.close(handle);
                        }
                    }
                });
            }

            menu = menu.child(row);
        }
    }

    // Return menu directly without motion wrapper to prevent lingering text
    div().child(menu)
}

/// Show a submenu overlay positioned to the right of the parent item (hover mode)
fn show_menubar_submenu(
    x: f32,
    y: f32,
    items: &[ContextMenuItem],
    min_width: f32,
    parent_handle_state: State<Option<u64>>,
    submenu_handle_state: State<Option<u64>>,
    key: String,
) -> OverlayHandle {
    let theme = ThemeState::get();
    let bg = theme.color(ColorToken::Surface);
    let border = theme.color(ColorToken::Border);
    let text_color = theme.color(ColorToken::TextPrimary);
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let text_tertiary = theme.color(ColorToken::TextTertiary);
    let radius = theme.radius(RadiusToken::Md);
    let font_size = 14.0;
    let padding = 12.0;

    let items = items.to_vec();
    let item_count = items.len();

    let submenu_handle_for_content = submenu_handle_state.clone();
    let parent_handle_for_content = parent_handle_state.clone();
    let submenu_handle_for_close = submenu_handle_state.clone();
    let parent_handle_for_hover = parent_handle_state.clone();

    let mgr = get_overlay_manager();

    let key_for_close = key.clone();

    // Use hover_card() for transient hover-based overlay (like main menu)
    let handle = mgr
        .hover_card()
        .at(x, y)
        .anchor_direction(blinc_layout::widgets::overlay::AnchorDirection::Right)
        .animation(OverlayAnimation::none()) // Instant show/hide
        .dismiss_on_escape(true)
        .on_close(move || {
            submenu_handle_for_close.set(None);
            // Reset all button states to clear lingering hover/pressed states
            for idx in 0..item_count {
                let item_key = format!("{}_item-{}", key_for_close, idx);
                reset_button_state(&item_key);
            }
        })
        .content(move || {
            build_menubar_submenu_content(
                &items,
                min_width,
                &parent_handle_for_content,
                &submenu_handle_for_content,
                &parent_handle_for_hover,
                &key,
                bg,
                border,
                text_color,
                text_secondary,
                text_tertiary,
                radius,
                font_size,
                padding,
            )
        })
        .show();

    handle
}

/// Build submenu content for menubar (recursive for nested submenus)
#[allow(clippy::too_many_arguments)]
fn build_menubar_submenu_content(
    items: &[ContextMenuItem],
    width: f32,
    parent_handle_state: &State<Option<u64>>,
    submenu_handle_state: &State<Option<u64>>,
    parent_menu_handle_state: &State<Option<u64>>,
    key: &str,
    bg: Color,
    border: Color,
    text_color: Color,
    text_secondary: Color,
    text_tertiary: Color,
    radius: f32,
    font_size: f32,
    padding: f32,
) -> Div {
    let menu_id = key;

    // State for tracking nested submenus
    let nested_submenu_handle: State<Option<u64>> =
        BlincContextState::get().use_state_keyed(&format!("{}_nested", key), || None);

    // Clone handles for hover handlers
    let submenu_handle_for_enter = submenu_handle_state.clone();
    let submenu_handle_for_leave = submenu_handle_state.clone();
    let parent_handle_for_enter = parent_menu_handle_state.clone();

    let mut menu = div()
        .id(menu_id)
        .flex_col()
        .w(width)
        .bg(bg)
        .border(1.0, border)
        .rounded(radius)
        .shadow_lg()
        .overflow_clip()
        .h_fit()
        .py(1.0)
        // When mouse enters submenu, cancel pending close on both submenu AND parent menu
        .on_hover_enter(move |_| {
            let mgr = get_overlay_manager();

            // Cancel pending close on submenu
            if let Some(handle_id) = submenu_handle_for_enter.get() {
                let handle = OverlayHandle::from_raw(handle_id);
                if mgr.is_pending_close(handle) {
                    mgr.hover_enter(handle);
                }
            }

            // Also cancel pending close on parent menu to keep it open
            if let Some(handle_id) = parent_handle_for_enter.get() {
                let handle = OverlayHandle::from_raw(handle_id);
                if mgr.is_pending_close(handle) {
                    mgr.hover_enter(handle);
                }
            }
        })
        // When mouse leaves submenu, start close countdown
        .on_hover_leave(move |_| {
            if let Some(handle_id) = submenu_handle_for_leave.get() {
                let mgr = get_overlay_manager();
                let handle = OverlayHandle::from_raw(handle_id);
                if mgr.is_visible(handle) && !mgr.is_pending_close(handle) {
                    mgr.hover_leave(handle);
                }
            }
        });

    for (idx, item) in items.iter().enumerate() {
        if item.is_separator() {
            menu = menu.child(hr_with_bg(bg));
        } else {
            let item_label = item.get_label().to_string();
            let item_shortcut = item.get_shortcut().map(|s| s.to_string());
            let item_icon = item.get_icon().map(|s| s.to_string());
            let item_disabled = item.is_disabled();
            let item_on_click = item.get_on_click();
            let has_submenu = item.has_submenu();
            let submenu_items = item.get_submenu().cloned();

            let parent_handle_for_click = parent_handle_state.clone();
            let submenu_handle_for_click = submenu_handle_state.clone();
            let nested_submenu_for_hover = nested_submenu_handle.clone();
            let nested_submenu_for_leave = nested_submenu_handle.clone();

            let item_key = format!("{}_item-{}", key, idx);
            let submenu_key = format!("{}_sub-{}", key, idx);
            let button_state = use_button_state(&item_key);

            let item_text_color = if item_disabled {
                text_tertiary
            } else {
                text_color
            };

            let shortcut_color = text_secondary;

            let mut row = Stateful::with_shared_state(button_state)
                .w_full()
                .h_fit()
                .py(padding / 4.0)
                .px(padding / 2.0)
                .bg(bg)
                .cursor(if item_disabled {
                    CursorStyle::NotAllowed
                } else {
                    CursorStyle::Pointer
                })
                .on_state(move |state, container: &mut Div| {
                    let theme = ThemeState::get();
                    let item_bg = if (*state == ButtonState::Hovered || *state == ButtonState::Pressed) && !item_disabled {
                        theme.color(ColorToken::SecondaryHover).with_alpha(0.65)
                    } else {
                        bg
                    };

                    let text_col = if (*state == ButtonState::Hovered || *state == ButtonState::Pressed) && !item_disabled {
                        theme.color(ColorToken::TextSecondary)
                    } else {
                        item_text_color
                    };

                    let mut left_side = div()
                        .w_fit()
                        .h_fit()
                        .flex_row()
                        .items_center()
                        .gap(padding / 4.0);

                    if let Some(ref icon_svg) = item_icon {
                        left_side = left_side.child(svg(icon_svg).size(16.0, 16.0).color(item_text_color));
                    }

                    left_side = left_side.child(
                        text(&item_label)
                            .size(font_size)
                            .color(text_col)
                            .no_cursor().pointer_events_none(),
                    ).pointer_events_none();

                    let right_side: Option<Div> = if let Some(ref shortcut) = item_shortcut {
                        Some(div().child(
                            text(shortcut)
                                .size(font_size - 2.0)
                                .color(shortcut_color)
                                .no_cursor(),
                        ))
                    } else if has_submenu {
                        let chevron_right = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"#;
                        Some(div().child(svg(chevron_right).size(12.0, 12.0).color(text_tertiary)).pointer_events_none())
                    } else {
                        None
                    };

                    let mut row_content = div()
                        .w_full()
                        .h_fit()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .bg(item_bg)
                        .child(left_side);

                    if let Some(right) = right_side {
                        row_content = row_content.child(right);
                    }

                    container.merge(row_content);
                })
                .on_click(move |_| {
                    if !item_disabled && !has_submenu {
                        if let Some(ref cb) = item_on_click {
                            cb();
                        }

                        // Close all menus immediately (no animation delay)
                        let mgr = get_overlay_manager();
                        mgr.close_all_of(blinc_layout::widgets::overlay::OverlayKind::Tooltip);
                    }
                });

            // Add hover handlers for submenu items
            if has_submenu && !item_disabled {
                let submenu_items_for_hover = submenu_items.clone();
                let parent_handle_for_submenu = parent_handle_state.clone();
                let submenu_key_for_hover = submenu_key.clone();

                row = row.on_hover_enter(move |ctx| {
                    // Close any existing nested submenu
                    if let Some(handle_id) = nested_submenu_for_hover.get() {
                        let mgr = get_overlay_manager();
                        let handle = OverlayHandle::from_raw(handle_id);
                        if !mgr.is_closing(handle) && !mgr.is_pending_close(handle) {
                            mgr.close(handle);
                        }
                    }

                    // Show new nested submenu
                    if let Some(ref items) = submenu_items_for_hover {
                        let x = ctx.bounds_x + ctx.bounds_width + 4.0;
                        let y = ctx.bounds_y;

                        let handle = show_menubar_submenu(
                            x,
                            y,
                            items,
                            160.0,
                            parent_handle_for_submenu.clone(),
                            nested_submenu_for_hover.clone(),
                            submenu_key_for_hover.clone(),
                        );
                        nested_submenu_for_hover.set(Some(handle.id()));
                    }
                });
            } else {
                // Close nested submenu when hovering non-submenu item
                row = row.on_hover_enter(move |_| {
                    if let Some(handle_id) = nested_submenu_for_leave.get() {
                        let mgr = get_overlay_manager();
                        let handle = OverlayHandle::from_raw(handle_id);
                        if !mgr.is_closing(handle) && !mgr.is_pending_close(handle) {
                            mgr.close(handle);
                        }
                    }
                });
            }

            menu = menu.child(row);
        }
    }

    // Return menu directly without motion wrapper to prevent jittering
    div().child(menu)
}

/// Build the dropdown menu content for a menubar menu
#[allow(clippy::too_many_arguments)]
fn build_menubar_dropdown_content(
    items: &[ContextMenuItem],
    width: f32,
    overlay_handle_state: &State<Option<u64>>,
    active_menu_state: &State<Option<usize>>,
    key: &str,
    bg: Color,
    border: Color,
    text_color: Color,
    text_secondary: Color,
    text_tertiary: Color,
    radius: f32,
    font_size: f32,
    padding: f32,
) -> Div {
    let menu_id = key;

    // State for tracking open submenu
    let submenu_handle: State<Option<u64>> =
        BlincContextState::get().use_state_keyed(&format!("{}_submenu", key), || None);

    let mut menu = div()
        .id(menu_id)
        .flex_col()
        .w(width)
        .bg(bg)
        .border(1.0, border)
        .rounded(radius)
        .shadow_lg()
        .overflow_clip()
        .h_fit()
        .py(1.0);

    for (idx, item) in items.iter().enumerate() {
        if item.is_separator() {
            menu = menu.child(hr_with_bg(bg));
        } else {
            let item_label = item.get_label().to_string();
            let item_shortcut = item.get_shortcut().map(|s| s.to_string());
            let item_icon = item.get_icon().map(|s| s.to_string());
            let item_disabled = item.is_disabled();
            let item_on_click = item.get_on_click();
            let has_submenu = item.has_submenu();
            let submenu_items = item.get_submenu().cloned();

            let handle_state_for_click = overlay_handle_state.clone();
            let submenu_handle_for_hover = submenu_handle.clone();
            let submenu_handle_for_leave = submenu_handle.clone();

            // Create a stable key for this item's button state
            let item_key = format!("{}_item-{}", key, idx);
            let submenu_key = format!("{}_sub-{}", key, idx);
            let button_state = use_button_state(&item_key);

            let item_text_color = if item_disabled {
                text_tertiary
            } else {
                text_color
            };

            let shortcut_color = text_secondary;

            // Build the stateful row element
            let mut row = Stateful::with_shared_state(button_state)
                .w_full()
                .h_fit()
                .py(padding / 4.0)
                .px(padding / 2.0)
                .bg(bg)
                .cursor(if item_disabled {
                    CursorStyle::NotAllowed
                } else {
                    CursorStyle::Pointer
                })
                .on_state(move |state, container: &mut Div| {
                    let theme = ThemeState::get();
                    // Apply hover background based on button state
                    let item_bg = if (*state == ButtonState::Hovered
                        || *state == ButtonState::Pressed)
                        && !item_disabled
                    {
                        theme.color(ColorToken::SecondaryHover).with_alpha(0.65)
                    } else {
                        bg
                    };

                    let text_col = if (*state == ButtonState::Hovered
                        || *state == ButtonState::Pressed)
                        && !item_disabled
                    {
                        theme.color(ColorToken::TextSecondary)
                    } else {
                        item_text_color
                    };

                    // Left side: icon + label
                    let mut left_side = div()
                        .w_fit()
                        .h_fit()
                        .flex_row()
                        .items_center()
                        .gap(padding / 4.0);

                    if let Some(ref icon_svg) = item_icon {
                        left_side =
                            left_side.child(svg(icon_svg).size(16.0, 16.0).color(item_text_color));
                    }

                    left_side = left_side
                        .child(
                            text(&item_label)
                                .size(font_size)
                                .color(text_col)
                                .no_cursor()
                                .pointer_events_none(),
                        )
                        .pointer_events_none();

                    // Right side: shortcut or submenu arrow
                    let right_side: Option<Div> = if let Some(ref shortcut) = item_shortcut {
                        Some(div().child(
                            text(shortcut)
                                .size(font_size - 2.0)
                                .color(shortcut_color)
                                .no_cursor(),
                        ))
                    } else if has_submenu {
                        let chevron_right = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"#;
                        Some(
                            div()
                                .child(svg(chevron_right).size(12.0, 12.0).color(text_tertiary))
                                .pointer_events_none(),
                        )
                    } else {
                        None
                    };

                    let mut row_content = div()
                        .w_full()
                        .h_fit()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .bg(item_bg)
                        .child(left_side);

                    if let Some(right) = right_side {
                        row_content = row_content.child(right);
                    }

                    container.merge(row_content);
                })
                .on_click(move |_| {
                    if !item_disabled && !has_submenu {
                        if let Some(ref cb) = item_on_click {
                            cb();
                        }

                        // Close the overlay
                        if let Some(handle_id) = handle_state_for_click.get() {
                            let mgr = get_overlay_manager();
                            mgr.close(OverlayHandle::from_raw(handle_id));
                        }
                    }
                });

            // Add hover handlers for submenu items
            if has_submenu && !item_disabled {
                let submenu_items_for_hover = submenu_items.clone();
                let overlay_handle_for_submenu = overlay_handle_state.clone();
                let submenu_key_for_hover = submenu_key.clone();

                row = row.on_hover_enter(move |ctx| {
                    // Close any existing submenu
                    if let Some(handle_id) = submenu_handle_for_hover.get() {
                        let mgr = get_overlay_manager();
                        let handle = OverlayHandle::from_raw(handle_id);
                        if !mgr.is_closing(handle) && !mgr.is_pending_close(handle) {
                            mgr.close(handle);
                        }
                    }

                    // Show submenu to the right of this item
                    if let Some(ref items) = submenu_items_for_hover {
                        let x = ctx.bounds_x + ctx.bounds_width + 4.0;
                        let y = ctx.bounds_y;

                        let handle = show_menubar_submenu(
                            x,
                            y,
                            items,
                            160.0,
                            overlay_handle_for_submenu.clone(),
                            submenu_handle_for_hover.clone(),
                            submenu_key_for_hover.clone(),
                        );
                        submenu_handle_for_hover.set(Some(handle.id()));
                    }
                });
            } else {
                // When hovering a non-submenu item, close any open submenu
                row = row.on_hover_enter(move |_| {
                    if let Some(handle_id) = submenu_handle_for_leave.get() {
                        let mgr = get_overlay_manager();
                        let handle = OverlayHandle::from_raw(handle_id);
                        if !mgr.is_closing(handle) && !mgr.is_pending_close(handle) {
                            mgr.close(handle);
                        }
                    }
                });
            }

            menu = menu.child(row);
        }
    }

    // // Wrap in motion for animation
    // div().child(
    //     motion_derived(key)
    //         .enter_animation(AnimationPreset::dropdown_in(150))
    //         .exit_animation(AnimationPreset::dropdown_out(100))
    //         .child(menu),
    // )
    div().child(menu)
}

/// The built menubar component
pub struct Menubar {
    inner: Div,
}

impl std::fmt::Debug for Menubar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Menubar").finish()
    }
}

impl ElementBuilder for MenubarBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.get_or_build().inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().inner.layout_style()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.get_or_build().inner)
    }
}

/// Create a new menubar
///
/// # Example
///
/// ```ignore
/// cn::menubar()
///     .menu("File", |m| {
///         m.item("New", || {})
///          .item_with_shortcut("Open", "Ctrl+O", || {})
///     })
///     .menu("Edit", |m| {
///         m.item_with_shortcut("Undo", "Ctrl+Z", || {})
///          .item_with_shortcut("Redo", "Ctrl+Y", || {})
///     })
/// ```
#[track_caller]
pub fn menubar() -> MenubarBuilder {
    MenubarBuilder::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menubar_builder() {
        let mb = menubar()
            .menu("File", |m| m.item("New", || {}).separator().item("Exit", || {}))
            .menu("Edit", |m| {
                m.item_with_shortcut("Undo", "Ctrl+Z", || {})
                    .item_with_shortcut("Redo", "Ctrl+Y", || {})
            });

        assert_eq!(mb.menus.len(), 2);
        let file_str = String::from("File");
        assert!(matches!(&mb.menus[0].trigger, MenubarTrigger::Label(file_str)));
        assert_eq!(mb.menus[0].items.len(), 3); // New, separator, Exit
        let edit_str = String::from("Edit");
        assert!(matches!(&mb.menus[1].trigger,  MenubarTrigger::Label(edit_str)));
        assert_eq!(mb.menus[1].items.len(), 2);
    }

    #[test]
    fn test_menu_with_submenu() {
        let mb = menubar().menu("File", |m| {
            m.item("New", || {}).submenu("Recent", |sub| {
                sub.item("File 1", || {}).item("File 2", || {})
            })
        });

        assert_eq!(mb.menus[0].items.len(), 2);
        assert!(mb.menus[0].items[1].has_submenu());
    }
}
