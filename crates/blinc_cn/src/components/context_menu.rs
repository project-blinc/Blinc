//! Context Menu component for right-click menus
//!
//! A themed context menu that appears at a specific position (usually mouse coordinates).
//! Uses the overlay system for proper positioning and dismissal.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     div()
//!         .w(400.0)
//!         .h(300.0)
//!         .bg(theme.color(ColorToken::Surface))
//!         .on_click(|event_ctx| {
//!             // Use mouse_x/mouse_y from EventContext for absolute screen position
//!             cn::context_menu()
//!                 .at(event_ctx.mouse_x, event_ctx.mouse_y)
//!                 .item("Cut", || println!("Cut"))
//!                 .item("Copy", || println!("Copy"))
//!                 .item("Paste", || println!("Paste"))
//!                 .separator()
//!                 .item("Delete", || println!("Delete"))
//!                 .show();
//!         })
//! }
//!
//! // With keyboard shortcuts displayed
//! cn::context_menu()
//!     .at(x, y)
//!     .item_with_shortcut("Cut", "Ctrl+X", || {})
//!     .item_with_shortcut("Copy", "Ctrl+C", || {})
//!     .item_with_shortcut("Paste", "Ctrl+V", || {})
//!
//! // Disabled items
//! cn::context_menu()
//!     .at(x, y)
//!     .item("Undo", || {})
//!     .item_disabled("Redo")  // No action available
//!
//! // Submenus (nested menus)
//! cn::context_menu()
//!     .at(x, y)
//!     .item("Open", || {})
//!     .submenu("Recent Files", |sub| {
//!         sub.item("file1.rs", || {})
//!            .item("file2.rs", || {})
//!     })
//! ```

use std::sync::Arc;

use blinc_animation::AnimationPreset;
use blinc_core::context_state::BlincContextState;
use blinc_core::{Color, State};
use blinc_layout::element::CursorStyle;
use blinc_layout::motion::motion_derived;
use blinc_layout::overlay_state::get_overlay_manager;
use blinc_layout::prelude::*;
use blinc_layout::stateful::{ButtonState, Stateful};
use blinc_layout::widgets::overlay::{OverlayHandle, OverlayManagerExt};
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

use crate::button::use_button_state;

/// A menu item in the context menu
#[derive(Clone)]
pub struct ContextMenuItem {
    /// Display label
    label: String,
    /// Optional keyboard shortcut display
    shortcut: Option<String>,
    /// Optional icon SVG
    icon: Option<String>,
    /// Click handler
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Whether this item is disabled
    disabled: bool,
    /// Whether this is a separator (ignores other fields)
    is_separator: bool,
    /// Submenu items (if this is a submenu trigger)
    submenu: Option<Vec<ContextMenuItem>>,
}

impl std::fmt::Debug for ContextMenuItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextMenuItem")
            .field("label", &self.label)
            .field("shortcut", &self.shortcut)
            .field("icon", &self.icon.is_some())
            .field("disabled", &self.disabled)
            .field("is_separator", &self.is_separator)
            .field("submenu", &self.submenu.as_ref().map(|s| s.len()))
            .finish()
    }
}

impl ContextMenuItem {
    /// Create a new menu item
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            shortcut: None,
            icon: None,
            on_click: None,
            disabled: false,
            is_separator: false,
            submenu: None,
        }
    }

    /// Create a separator
    pub fn separator() -> Self {
        Self {
            label: String::new(),
            shortcut: None,
            icon: None,
            on_click: None,
            disabled: false,
            is_separator: true,
            submenu: None,
        }
    }

    /// Set the click handler
    pub fn on_click<F>(mut self, f: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_click = Some(Arc::new(f));
        self
    }

    /// Set a keyboard shortcut hint
    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// Set an icon (SVG string)
    pub fn icon(mut self, svg: impl Into<String>) -> Self {
        self.icon = Some(svg.into());
        self
    }

    /// Mark as disabled
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    /// Set submenu items
    pub fn submenu(mut self, items: Vec<ContextMenuItem>) -> Self {
        self.submenu = Some(items);
        self
    }

    // =========================================================================
    // Accessors for use by other components (like DropdownMenu)
    // =========================================================================

    /// Get the label
    pub fn get_label(&self) -> &str {
        &self.label
    }

    /// Get the shortcut if any
    pub fn get_shortcut(&self) -> Option<&str> {
        self.shortcut.as_deref()
    }

    /// Get the icon SVG if any
    pub fn get_icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }

    /// Check if this item is disabled
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Check if this is a separator
    pub fn is_separator(&self) -> bool {
        self.is_separator
    }

    /// Check if this item has a submenu
    pub fn has_submenu(&self) -> bool {
        self.submenu.is_some()
    }

    /// Get the click handler (clones the Arc)
    pub fn get_on_click(&self) -> Option<Arc<dyn Fn() + Send + Sync>> {
        self.on_click.clone()
    }
}

/// Builder for creating context menus
pub struct ContextMenuBuilder {
    /// Position x coordinate
    x: f32,
    /// Position y coordinate
    y: f32,
    /// Menu items
    items: Vec<ContextMenuItem>,
    /// Minimum width
    min_width: f32,
}

impl ContextMenuBuilder {
    /// Create a new context menu builder
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            items: Vec::new(),
            min_width: 180.0,
        }
    }

    /// Set the position where the menu should appear
    pub fn at(mut self, x: f32, y: f32) -> Self {
        self.x = x;
        self.y = y;
        self
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

    /// Add a menu item with keyboard shortcut
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
        let sub = builder(SubmenuBuilder::new());
        self.items
            .push(ContextMenuItem::new(label).submenu(sub.items));
        self
    }

    /// Add a raw menu item
    pub fn add_item(mut self, item: ContextMenuItem) -> Self {
        self.items.push(item);
        self
    }

    /// Set minimum width
    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    /// Show the context menu
    pub fn show(self) -> OverlayHandle {
        let theme = ThemeState::get();
        let bg = theme.color(ColorToken::Surface);
        let border = theme.color(ColorToken::Border);
        let text_color = theme.color(ColorToken::TextPrimary);
        let text_secondary = theme.color(ColorToken::TextSecondary);
        let text_tertiary = theme.color(ColorToken::TextTertiary);
        let surface_elevated = theme.color(ColorToken::SurfaceElevated);
        let radius = theme.radius(RadiusToken::Md);

        // Use same sizing as Select dropdown for consistency
        let font_size = 14.0; // Medium size font
        let padding = 12.0; // Medium size padding

        let items = self.items;
        let width = self.min_width;
        let x = self.x;
        let y = self.y;

        // Store overlay handle for closing from menu items
        let handle_key = format!("_context_menu_handle_{}_{}", x as i32, y as i32);
        let overlay_handle_state: State<Option<u64>> =
            BlincContextState::get().use_state_keyed(&handle_key, || None);
        let handle_state_for_content = overlay_handle_state.clone();
        // Create a key for the menu content
        let menu_key = format!("_context_menu_{}_{}", x as i32, y as i32);

        let mgr = get_overlay_manager();

        // Create a unique motion key for this context menu instance
        // The motion is on the child of the wrapper div, so we need ":child:0" suffix
        let motion_key_str = format!("ctxmenu_{}", menu_key);
        let motion_key_with_child = format!("{}:child:0", motion_key_str);

        // Use dropdown() instead of context_menu() to get transparent backdrop
        // that dismisses on click outside (same as Select component)
        let handle = mgr
            .dropdown()
            .at(x, y)
            .dismiss_on_escape(true)
            .motion_key(&motion_key_with_child)
            .content(move || {
                build_menu_content(
                    &items,
                    width,
                    &handle_state_for_content,
                    &motion_key_str,
                    bg,
                    border,
                    text_color,
                    text_secondary,
                    text_tertiary,
                    surface_elevated,
                    radius,
                    font_size,
                    padding,
                )
            })
            .show();

        overlay_handle_state.set(Some(handle.id()));
        handle
    }
}

impl Default for ContextMenuBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for submenu items
pub struct SubmenuBuilder {
    items: Vec<ContextMenuItem>,
}

impl SubmenuBuilder {
    fn new() -> Self {
        Self { items: Vec::new() }
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

    /// Add a menu item with keyboard shortcut
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

    /// Add a disabled menu item
    pub fn item_disabled(mut self, label: impl Into<String>) -> Self {
        self.items.push(ContextMenuItem::new(label).disabled());
        self
    }

    /// Add a separator
    pub fn separator(mut self) -> Self {
        self.items.push(ContextMenuItem::separator());
        self
    }

    /// Get the items from this submenu builder
    pub fn items(self) -> Vec<ContextMenuItem> {
        self.items
    }
}

impl SubmenuBuilder {
    /// Create a new submenu builder (public for use by other components)
    pub fn new_public() -> Self {
        Self::new()
    }
}

impl Default for SubmenuBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the menu content div
///
/// Uses the same layout pattern as the Select dropdown for consistency.
#[allow(clippy::too_many_arguments)]
fn build_menu_content(
    items: &[ContextMenuItem],
    width: f32,
    overlay_handle_state: &State<Option<u64>>,
    key: &str,
    bg: Color,
    border: Color,
    text_color: Color,
    text_secondary: Color,
    text_tertiary: Color,
    surface_elevated: Color,
    radius: f32,
    font_size: f32,
    padding: f32,
) -> Div {
    // Generate a unique ID for the menu based on the key
    let menu_id = key;

    let mut menu = div()
        .id(menu_id)
        .flex_col()
        .w(width)
        .bg(bg)
        .border(1.0, border)
        .rounded(radius)
        .shadow_lg()
        .overflow_clip()
        .h_fit();

    // Note: on_ready callback removed - overlay manager now uses initial size estimation
    // If accurate sizing is needed, register via ctx.query("context-menu-{handle}").on_ready(...)

    for (idx, item) in items.iter().enumerate() {
        if item.is_separator {
            // Separator line with explicit background to prevent alpha artifacts
            // during opacity animations
            menu = menu.child(hr_with_bg(bg));
        } else {
            // Regular menu item
            let item_label = item.label.clone();
            let item_shortcut = item.shortcut.clone();
            let item_icon = item.icon.clone();
            let item_disabled = item.disabled;
            let item_on_click = item.on_click.clone();
            let has_submenu = item.submenu.is_some();

            let handle_state_for_click = overlay_handle_state.clone();

            // Create a stable key for this item's button state
            let item_key = format!("{}_item-{}", key, idx);
            let button_state = use_button_state(&item_key);

            let item_text_color = if item_disabled {
                text_tertiary
            } else {
                text_color
            };

            let shortcut_color = text_secondary;

            // Build the stateful menu item row
            let row = Stateful::with_shared_state(button_state)
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
                     let item_bg = if (*state == ButtonState::Hovered || *state == ButtonState::Pressed) && !item_disabled {
                        theme.color(ColorToken::SecondaryHover).with_alpha(0.65)
                    } else {
                        bg
                    };

                    let text_color = if (*state == ButtonState::Hovered || *state == ButtonState::Pressed) && !item_disabled {
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
                        left_side = left_side.child(svg(icon_svg).size(16.0, 16.0).color(item_text_color));
                    }

                    left_side = left_side.child(
                        text(&item_label)
                            .size(font_size)
                            .color(text_color)
                            .no_cursor(),
                    );

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
                        Some(div().child(svg(chevron_right).size(12.0, 12.0).color(text_tertiary)))
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
                    if !item_disabled {
                        // Execute the callback
                        if let Some(ref cb) = item_on_click {
                            cb();
                        }

                        // Close the menu
                        if let Some(handle_id) = handle_state_for_click.get() {
                            let mgr = get_overlay_manager();
                            mgr.close(OverlayHandle::from_raw(handle_id));
                        }
                    }
                });

            menu = menu.child(row);
        }
    }

    // Wrap menu in motion container for enter/exit animations
    // Use motion_derived with the key so the overlay can trigger exit animation
    div().child(
        motion_derived(key)
            .enter_animation(AnimationPreset::context_menu_in(150))
            .exit_animation(AnimationPreset::context_menu_out(100))
            .child(menu),
    )
}

/// Create a context menu builder
///
/// # Example
///
/// ```ignore
/// cn::context_menu()
///     .at(event.x, event.y)
///     .item("Cut", || println!("Cut"))
///     .item("Copy", || println!("Copy"))
///     .separator()
///     .item("Paste", || println!("Paste"))
///     .show();
/// ```
pub fn context_menu() -> ContextMenuBuilder {
    ContextMenuBuilder::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_item_creation() {
        let item = ContextMenuItem::new("Test");
        assert_eq!(item.label, "Test");
        assert!(!item.disabled);
        assert!(!item.is_separator);
    }

    #[test]
    fn test_menu_item_with_shortcut() {
        let item = ContextMenuItem::new("Copy").shortcut("Ctrl+C");
        assert_eq!(item.shortcut, Some("Ctrl+C".to_string()));
    }

    #[test]
    fn test_separator() {
        let sep = ContextMenuItem::separator();
        assert!(sep.is_separator);
    }

    #[test]
    fn test_disabled_item() {
        let item = ContextMenuItem::new("Disabled").disabled();
        assert!(item.disabled);
    }

    #[test]
    fn test_builder_items() {
        let menu = ContextMenuBuilder::new()
            .item("Item 1", || {})
            .separator()
            .item("Item 2", || {});

        assert_eq!(menu.items.len(), 3);
        assert!(!menu.items[0].is_separator);
        assert!(menu.items[1].is_separator);
        assert!(!menu.items[2].is_separator);
    }

    #[test]
    fn test_builder_position() {
        let menu = ContextMenuBuilder::new().at(100.0, 200.0);
        assert_eq!(menu.x, 100.0);
        assert_eq!(menu.y, 200.0);
    }
}
