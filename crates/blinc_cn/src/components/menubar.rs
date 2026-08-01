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

use blinc_core::context_state::BlincContextState;
use blinc_core::{Color, State};
use blinc_layout::InstanceKey;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, RenderProps};
use blinc_layout::overlay_state::overlay_stack;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::hr::hr;
use blinc_layout::widgets::overlay::AnchorDirection;
use blinc_layout::widgets::overlay_stack::{OverlayBuilder, OverlayHandle};
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

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
    /// User-added CSS classes
    classes: Vec<std::sync::Arc<str>>,
    /// User-set element ID
    user_id: Option<String>,
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
            classes: Vec::new(),
            user_id: None,
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

    /// Get or build the component
    fn get_or_build(&self) -> &Menubar {
        ::blinc_layout::build_once::build_once(&self.built, || self.build_component())
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
            .class("cn-menubar")
            .flex_row()
            .items_center()
            .h_fit()
            .px(4.0)
            .bg(bg)
            .border_bottom(1.0, border);

        // Add each menu trigger
        for (idx, menu) in menus.iter().enumerate() {
            let menu_trigger = menu.trigger.clone();
            let menu_items = menu.items.clone();
            let menu_key = format!("{}_{}", key_base, idx);

            let active_menu_for_trigger = active_menu.clone();
            let active_menu_for_hover = active_menu.clone();
            let overlay_handle_for_trigger = overlay_handle_state.clone();
            let overlay_handle_for_show = overlay_handle_state.clone();
            let overlay_handle_for_hover = overlay_handle_state.clone();

            // Clone menu items and key for different handlers
            let menu_items_for_hover = menu_items.clone();
            let menu_key_for_hover = menu_key.clone();
            let menu_key_for_click = menu_key.clone();

            // Clone style values for closures
            let style_px = trigger_style.px;
            let style_py = trigger_style.py;
            let style_font_size = trigger_style.font_size;
            let style_radius = trigger_style.radius;

            // Build the menu trigger as a plain div — no Stateful wrapper.
            // Stateful subtree rebuilds contaminate base_styles with hover
            // state, causing hover backgrounds to persist after mouse leaves.
            // CSS .cn-menubar-trigger:hover handles hover background.
            let radius = style_radius.unwrap_or_else(|| theme.radius(RadiusToken::Sm));

            let trigger_content: Div = match &menu_trigger {
                MenubarTrigger::Label(label) => {
                    let text_col = theme.color(ColorToken::TextPrimary);
                    div()
                        .flex_row()
                        .items_center()
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
                MenubarTrigger::Custom(custom_fn) => div()
                    .flex_row()
                    .items_center()
                    .rounded(radius)
                    .child(custom_fn(false)),
            };

            let mut trigger = trigger_content
                .class("cn-menubar-trigger")
                .h_fit()
                .px(style_px / 4.0)
                .py(style_py / 4.0)
                .cursor_pointer();

            // Add click handler (used for Click mode, or to toggle in Hover mode)
            trigger = trigger.on_click(move |ctx| {
                let current_active = active_menu_for_trigger.get();

                // Toggle off if same menu is open.
                if current_active == Some(idx) {
                    if let Some(handle_id) = overlay_handle_for_trigger.get() {
                        OverlayHandle::from_raw(handle_id).close();
                    }
                    active_menu_for_trigger.set(None);
                    return;
                }

                // Close any previously-open menu.
                if let Some(handle_id) = overlay_handle_for_trigger.get() {
                    OverlayHandle::from_raw(handle_id).close();
                }

                active_menu_for_trigger.set(Some(idx));

                let x = ctx.bounds_x;
                let y = ctx.bounds_y + ctx.bounds_height + 4.0;

                let handle = spawn_menubar_dropdown(
                    x,
                    y,
                    &menu_items,
                    180.0,
                    overlay_handle_for_show.clone(),
                    active_menu_for_trigger.clone(),
                    menu_key_for_click.clone(),
                    idx,
                    /* hover_mode */ false,
                );

                overlay_handle_for_show.set(Some(handle.raw()));
            });

            // Add hover handlers for Hover trigger mode
            if trigger_mode == MenuTriggerMode::Hover {
                // Clone for hover leave handler
                let overlay_handle_for_hover_leave = overlay_handle_state.clone();

                trigger = trigger.on_hover_enter(move |ctx| {
                    let current_active = active_menu_for_hover.get();

                    // If this menu is already open, revive any in-flight exit
                    // and cancel any pending close countdown.
                    if current_active == Some(idx) {
                        if let Some(handle_id) = overlay_handle_for_hover.get() {
                            let handle = OverlayHandle::from_raw(handle_id);
                            if let Ok(mut stack) = overlay_stack().lock() {
                                let exiting = stack
                                    .iter_bottom_up()
                                    .any(|e| e.handle == handle && e.exiting);
                                if exiting {
                                    stack.revive(handle);
                                } else {
                                    stack.handle_mouse_enter(handle);
                                }
                            }
                        }
                        return;
                    }

                    // Close previously-open menu so only one is visible at a time.
                    if let Some(handle_id) = overlay_handle_for_hover.get() {
                        OverlayHandle::from_raw(handle_id).close();
                    }

                    active_menu_for_hover.set(Some(idx));

                    let x = ctx.bounds_x;
                    let y = ctx.bounds_y + ctx.bounds_height + 4.0;

                    let handle = spawn_menubar_dropdown(
                        x,
                        y,
                        &menu_items_for_hover,
                        180.0,
                        overlay_handle_for_hover.clone(),
                        active_menu_for_hover.clone(),
                        menu_key_for_hover.clone(),
                        idx,
                        /* hover_mode */ true,
                    );

                    overlay_handle_for_hover.set(Some(handle.raw()));
                });

                // Start close countdown when leaving trigger; content's own
                // hover handlers cancel/restart the same countdown so the
                // dropdown stays open while the user is inside it.
                trigger = trigger.on_hover_leave(move |_| {
                    if let Some(handle_id) = overlay_handle_for_hover_leave.get() {
                        let handle = OverlayHandle::from_raw(handle_id);
                        if let Ok(mut stack) = overlay_stack().lock() {
                            stack.handle_mouse_leave(handle);
                        }
                    }
                });
            }

            menubar = menubar.child(trigger);
        }

        // Apply user classes and id
        for c in &self.classes {
            menubar = menubar.class(c);
        }
        if let Some(ref id) = self.user_id {
            menubar = menubar.id(id);
        }

        Menubar { inner: menubar }
    }
}

impl Default for MenubarBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Dropdown / submenu spawning
// =============================================================================

/// Push a top-level menubar dropdown onto the stack. `hover_mode` chooses
/// between click-outside-dismiss (Dropdown kind) and hover-leave-dismiss
/// (Tooltip kind) — the trigger uses click vs hover handlers accordingly.
#[allow(clippy::too_many_arguments)]
fn spawn_menubar_dropdown(
    x: f32,
    y: f32,
    items: &[ContextMenuItem],
    min_width: f32,
    handle_state: State<Option<u64>>,
    active_menu_state: State<Option<usize>>,
    key: String,
    menu_idx: usize,
    hover_mode: bool,
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
    let handle_state_for_close = handle_state.clone();
    let active_menu_for_close = active_menu_state.clone();

    let next_handle_id = overlay_stack()
        .lock()
        .ok()
        .map(|s| s.peek_next_handle_id())
        .unwrap_or(0);
    let menu_handle = OverlayHandle::from_raw(next_handle_id);

    // Per-menu submenu tracking (keyed off the dropdown's handle id).
    let child_submenu_state: State<Option<u64>> = BlincContextState::get()
        .use_state_keyed(&format!("menubar_child_sub_{}", next_handle_id), || None);
    let child_submenu_for_content = child_submenu_state.clone();
    let menu_key_for_content = key.clone();

    let builder = if hover_mode {
        // Tooltip's default mouse_leave_delay_ms is 0 — too tight for a
        // hover-anchored dropdown the user has to traverse into. Give them
        // ~300ms to cross the gap between trigger and content.
        OverlayBuilder::tooltip()
            .anchor_direction(AnchorDirection::Bottom)
            .dismissable_by_mouse_leave(true, 300)
    } else {
        OverlayBuilder::dropdown()
    };

    // Exact row-height estimate for the viewport clamp — same
    // shape as cn::context_menu / cn::dropdown_menu. font_size
    // and padding are the local 14 / 12 from above.
    let est_w = min_width + 20.0;
    let est_h = items.len() as f32 * (font_size + padding) + padding;

    let handle = builder
        .at(x, y)
        .size(est_w, est_h)
        .dismissable_by_escape(true)
        .on_close(move |_reason| {
            if active_menu_for_close.get() == Some(menu_idx) {
                active_menu_for_close.set(None);
            }
            handle_state_for_close.set(None);
        })
        .content(move || {
            build_menubar_menu_div(
                &items,
                min_width,
                menu_handle,
                /* is_root */ true,
                hover_mode,
                &menu_key_for_content,
                &child_submenu_for_content,
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

    debug_assert_eq!(
        handle.raw(),
        next_handle_id,
        "peek_next_handle_id was stale — concurrent push?"
    );

    handle
}

/// Push a submenu onto the stack, positioned to the right of its parent item.
/// `root_handle` is always the top-level dropdown so leaf-item clicks can
/// close the whole chain. `parent_submenu_state` is the state slot the parent
/// uses to track this child; we clear it on our own close.
#[allow(clippy::too_many_arguments)]
fn spawn_menubar_submenu(
    x: f32,
    y: f32,
    items: &[ContextMenuItem],
    min_width: f32,
    root_handle: OverlayHandle,
    parent_submenu_state: State<Option<u64>>,
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

    let next_handle_id = overlay_stack()
        .lock()
        .ok()
        .map(|s| s.peek_next_handle_id())
        .unwrap_or(0);
    let submenu_handle = OverlayHandle::from_raw(next_handle_id);

    let child_submenu_state: State<Option<u64>> = BlincContextState::get()
        .use_state_keyed(&format!("menubar_child_sub_{}", next_handle_id), || None);
    let child_submenu_for_content = child_submenu_state.clone();
    let menu_key_for_content = key.clone();

    let parent_submenu_for_close = parent_submenu_state.clone();

    // Submenus share `build_menubar_menu_div` so the row math
    // matches the top-level dropdown.
    let est_w = min_width + 20.0;
    let est_h = items.len() as f32 * (font_size + padding) + padding;

    let handle = OverlayBuilder::tooltip()
        .at(x, y)
        .size(est_w, est_h)
        .anchor_direction(AnchorDirection::Right)
        .dismissable_by_escape(true)
        // ~300ms grace so the cursor can cross from the parent row into the
        // submenu without the close-on-leave countdown firing first.
        .dismissable_by_mouse_leave(true, 300)
        .on_close(move |_reason| {
            // Pop ourselves from the parent's tracking slot if still pointing at us.
            if parent_submenu_for_close.get() == Some(submenu_handle.raw()) {
                parent_submenu_for_close.set(None);
            }
        })
        .content(move || {
            build_menubar_menu_div(
                &items,
                min_width,
                root_handle,
                /* is_root */ false,
                /* hover_mode */ true,
                &menu_key_for_content,
                &child_submenu_for_content,
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

    debug_assert_eq!(
        handle.raw(),
        next_handle_id,
        "peek_next_handle_id was stale — concurrent push?"
    );

    handle
}

/// Build a menubar dropdown / submenu's content div. Item clicks call
/// `root_handle.close()` so the whole chain dismisses. In hover_mode, the
/// menu container's own hover handlers cancel / restart the pending-close
/// countdown driven by Tooltip dismiss rules.
#[allow(clippy::too_many_arguments)]
fn build_menubar_menu_div(
    items: &[ContextMenuItem],
    width: f32,
    root_handle: OverlayHandle,
    is_root: bool,
    hover_mode: bool,
    key: &str,
    child_submenu_state: &State<Option<u64>>,
    bg: Color,
    border: Color,
    text_color: Color,
    text_secondary: Color,
    text_tertiary: Color,
    radius: f32,
    font_size: f32,
    padding: f32,
) -> Div {
    let _ = is_root; // reserved for future styling hooks
    let menu_id = key;

    let mut menu = div()
        .class("cn-menubar-content")
        .id(menu_id)
        .flex_col()
        .w(width)
        .bg(bg)
        .border(1.0, border)
        .rounded(radius)
        .lock_corner_shape()
        .shadow_lg()
        .overflow_clip()
        .h_fit();
    // No top/bottom padding — see dropdown_menu.rs for rationale.

    // In hover mode the dropdown stays open while the cursor sits inside it.
    // Tooltip dismiss rules drive the countdown; container hover handlers
    // cancel / restart via the held lock to avoid the re-entrant deadlock.
    if hover_mode {
        menu = menu
            .on_hover_enter(move |_| {
                if let Ok(mut stack) = overlay_stack().lock() {
                    let exiting = stack
                        .iter_bottom_up()
                        .any(|e| e.handle == root_handle && e.exiting);
                    if exiting {
                        stack.revive(root_handle);
                    } else {
                        stack.handle_mouse_enter(root_handle);
                    }
                }
            })
            .on_hover_leave(move |_| {
                if let Ok(mut stack) = overlay_stack().lock() {
                    stack.handle_mouse_leave(root_handle);
                }
            });
    }

    for (idx, item) in items.iter().enumerate() {
        if item.is_separator() {
            // See dropdown_menu.rs for rationale.
            menu = menu.child(hr());
        } else {
            let item_label = item.get_label().to_string();
            let item_shortcut = item.get_shortcut().map(|s| s.to_string());
            let item_icon = item.get_icon().map(|s| s.to_string());
            let item_disabled = item.is_disabled();
            let item_on_click = item.get_on_click();
            let has_submenu = item.has_submenu();
            let submenu_items = item.get_submenu().cloned();

            let child_submenu_for_hover_open = child_submenu_state.clone();
            let child_submenu_for_hover_close = child_submenu_state.clone();
            let submenu_key = format!("{}_sub-{}", key, idx);

            let item_text_color = if item_disabled {
                text_tertiary
            } else {
                text_color
            };
            let shortcut_color = text_secondary;
            let cursor = if item_disabled {
                CursorStyle::NotAllowed
            } else {
                CursorStyle::Pointer
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

            left_side = left_side
                .child(
                    text(&item_label)
                        .size(font_size)
                        .color(item_text_color)
                        .no_cursor()
                        .pointer_events_none(),
                )
                .pointer_events_none();

            let right_side: Option<Div> = if let Some(ref shortcut) = item_shortcut {
                Some(
                    div().child(
                        text(shortcut)
                            .size(font_size - 2.0)
                            .color(shortcut_color)
                            .monospace()
                            .no_cursor(),
                    ),
                )
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
                .child(left_side);
            if let Some(right) = right_side {
                row_content = row_content.child(right);
            }

            let mut row = row_content
                .class("cn-menubar-item")
                .w_full()
                .h_fit()
                // Fallback padding MUST equal the `.cn-menubar-item` CSS
                // (space-2 / space-3 = 8px / 12px). Class layout overrides
                // can drop on a rebuilt subtree (submenu open re-mints the
                // hovered row — see project_css_overrides_consistency); if
                // the fallback differs, that row's text visibly shifts
                // against its siblings.
                .py(2.0)
                .px(3.0)
                .cursor(cursor);
            if has_submenu {
                // CSS hook for the panel's `:has()` flicker
                // suppression rule. See cn_styles.rs's
                // `.cn-menubar-content:has(.cn-menubar-item--has-submenu:hover)`.
                row = row.class("cn-menubar-item--has-submenu");
            }
            row = row.on_click(move |_| {
                if !item_disabled && !has_submenu {
                    // Close root FIRST so `cb()`-pushed overlays (e.g.
                    // a confirm dialog) aren't immediately
                    // cascade-closed by the menubar's
                    // `UnwindFromBelow`. See cn::context_menu for the
                    // full rationale + verified trace.
                    root_handle.close();
                    if let Some(ref cb) = item_on_click {
                        cb();
                    }
                }
            });

            // Submenu hover-to-open at this level.
            if has_submenu && !item_disabled {
                let submenu_items_for_hover = submenu_items.clone();
                let submenu_key_for_hover = submenu_key.clone();

                row = row.on_hover_enter(move |ctx| {
                    // Close any existing child submenu at this level.
                    if let Some(handle_id) = child_submenu_for_hover_open.get() {
                        OverlayHandle::from_raw(handle_id).close();
                    }

                    if let Some(ref items) = submenu_items_for_hover {
                        let x = ctx.bounds_x + ctx.bounds_width + 4.0;
                        let y = ctx.bounds_y;

                        let handle = spawn_menubar_submenu(
                            x,
                            y,
                            items,
                            160.0,
                            root_handle,
                            child_submenu_for_hover_open.clone(),
                            submenu_key_for_hover.clone(),
                        );
                        child_submenu_for_hover_open.set(Some(handle.raw()));
                    }
                });
            } else {
                row = row.on_hover_enter(move |_| {
                    if let Some(handle_id) = child_submenu_for_hover_close.get() {
                        OverlayHandle::from_raw(handle_id).close();
                    }
                });
            }

            menu = menu.child(row);
        }
    }

    menu
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

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().inner.element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().inner.element_id()
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
            .menu("File", |m| {
                m.item("New", || {}).separator().item("Exit", || {})
            })
            .menu("Edit", |m| {
                m.item_with_shortcut("Undo", "Ctrl+Z", || {})
                    .item_with_shortcut("Redo", "Ctrl+Y", || {})
            });

        assert_eq!(mb.menus.len(), 2);
        let file_str = String::from("File");
        assert!(matches!(
            &mb.menus[0].trigger,
            MenubarTrigger::Label(file_str)
        ));
        assert_eq!(mb.menus[0].items.len(), 3); // New, separator, Exit
        let edit_str = String::from("Edit");
        assert!(matches!(
            &mb.menus[1].trigger,
            MenubarTrigger::Label(edit_str)
        ));
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
