//! Dropdown Menu component for button-triggered menus
//!
//! A themed dropdown menu that appears below (or above) a trigger element.
//! Similar to Context Menu but triggered by clicking a button rather than right-click.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     // Simple dropdown with string trigger
//!     cn::dropdown_menu("Options")
//!         .item("Edit", || println!("Edit"))
//!         .item("Duplicate", || println!("Duplicate"))
//!         .separator()
//!         .item("Delete", || println!("Delete"))
//!
//!     // Custom trigger (icon button)
//!     cn::dropdown_menu_custom(|open| {
//!         cn::button(if open { "Close" } else { "Open" })
//!             .variant(ButtonVariant::Ghost)
//!     })
//!     .item("Option 1", || {})
//!     .item("Option 2", || {})
//!
//!     // With keyboard shortcuts
//!     cn::dropdown_menu("File")
//!         .item_with_shortcut("New", "Ctrl+N", || {})
//!         .item_with_shortcut("Open", "Ctrl+O", || {})
//!         .item_with_shortcut("Save", "Ctrl+S", || {})
//!
//!     // With icons
//!     cn::dropdown_menu("Actions")
//!         .item_with_icon("Copy", copy_icon_svg, || {})
//!         .item_with_icon("Paste", paste_icon_svg, || {})
//!
//!     // Submenus
//!     cn::dropdown_menu("More")
//!         .submenu("Share", |sub| {
//!             sub.item("Email", || {})
//!                .item("Link", || {})
//!         })
//! }
//! ```

use std::cell::OnceCell;
use std::sync::Arc;

use blinc_animation::AnimationPreset;
use blinc_core::context_state::BlincContextState;
use blinc_core::{Color, State};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, ElementBounds, RenderProps};
use blinc_layout::motion::motion_derived;
use blinc_layout::overlay_state::get_overlay_manager;
use blinc_layout::prelude::*;
use blinc_layout::stateful::{stateful_with_key, ButtonState, Stateful};
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::hr::hr_with_bg;
use blinc_layout::widgets::overlay::{OverlayHandle, OverlayManagerExt};
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

/// Icon for chevron down
const CHEVRON_DOWN_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;

/// Icon for chevron up
const CHEVRON_UP_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m18 15-6-6-6 6"/></svg>"#;
use crate::button::use_button_state;
use crate::ButtonVariant;
use blinc_layout::InstanceKey;

use super::context_menu::{ContextMenuItem, SubmenuBuilder};

/// Position for dropdown menu relative to trigger
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DropdownPosition {
    /// Below the trigger (default)
    #[default]
    Bottom,
    /// Above the trigger
    Top,
    /// To the right of the trigger
    Right,
    /// To the left of the trigger
    Left,
}

/// Alignment for dropdown menu
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DropdownAlign {
    /// Align to start of trigger
    #[default]
    Start,
    /// Center with trigger
    Center,
    /// Align to end of trigger
    End,
}

/// Trigger builder function type
type TriggerBuilderFn = Arc<dyn Fn(bool) -> Div + Send + Sync>;

/// Builder for dropdown menu component
pub struct DropdownMenuBuilder {
    /// Trigger label (simple mode)
    trigger_label: Option<String>,
    /// Custom trigger builder (advanced mode)
    trigger_builder: Option<TriggerBuilderFn>,
    /// Menu items
    items: Vec<ContextMenuItem>,
    /// Minimum width for the dropdown
    min_width: f32,
    /// Position relative to trigger
    position: DropdownPosition,
    /// Alignment
    align: DropdownAlign,
    /// Offset from trigger (pixels)
    offset: f32,
    /// Unique instance key (UUID-based for loop/closure safety)
    key: InstanceKey,
    /// Built component cache
    built: OnceCell<DropdownMenu>,
}

impl std::fmt::Debug for DropdownMenuBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DropdownMenuBuilder")
            .field("trigger_label", &self.trigger_label)
            .field("items", &self.items.len())
            .field("min_width", &self.min_width)
            .field("position", &self.position)
            .field("align", &self.align)
            .finish()
    }
}

impl DropdownMenuBuilder {
    /// Create with a simple string label trigger
    #[track_caller]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            trigger_label: Some(label.into()),
            trigger_builder: None,
            items: Vec::new(),
            min_width: 180.0,
            position: DropdownPosition::Bottom,
            align: DropdownAlign::Start,
            offset: 4.0,
            key: InstanceKey::new("dropdown"),
            built: OnceCell::new(),
        }
    }

    /// Create with a custom trigger builder
    ///
    /// The builder receives a boolean indicating if the menu is open.
    #[track_caller]
    pub fn with_trigger<F>(trigger: F) -> Self
    where
        F: Fn(bool) -> Div + Send + Sync + 'static,
    {
        Self {
            trigger_label: None,
            trigger_builder: Some(Arc::new(trigger)),
            items: Vec::new(),
            min_width: 180.0,
            position: DropdownPosition::Bottom,
            align: DropdownAlign::Start,
            offset: 4.0,
            key: InstanceKey::new("dropdown"),
            built: OnceCell::new(),
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

    /// Set minimum width for the dropdown
    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    /// Set dropdown position relative to trigger
    pub fn position(mut self, position: DropdownPosition) -> Self {
        self.position = position;
        self
    }

    /// Set dropdown alignment
    pub fn align(mut self, align: DropdownAlign) -> Self {
        self.align = align;
        self
    }

    /// Set offset from trigger (in pixels)
    pub fn offset(mut self, offset: f32) -> Self {
        self.offset = offset;
        self
    }

    /// Get or build the component
    fn get_or_build(&self) -> &DropdownMenu {
        self.built.get_or_init(|| self.build_component())
    }

    /// Build the dropdown menu component
    fn build_component(&self) -> DropdownMenu {
        let theme = ThemeState::get();

        // Create open state using InstanceKey for unique identification
        let open_state: State<bool> =
            BlincContextState::get().use_state_keyed(self.key.get(), || false);

        // Store overlay handle ID
        let overlay_handle_state: State<Option<u64>> =
            BlincContextState::get().use_state_keyed(&self.key.derive("handle"), || None);

        // Clone values for closures
        let items = self.items.clone();
        let min_width = self.min_width;
        let position = self.position;
        let align = self.align;
        let offset = self.offset;
        let trigger_label = self.trigger_label.clone();
        let trigger_builder = self.trigger_builder.clone();

        let btn_variant = ButtonVariant::Outline;
        let button_key = self.key.derive("button");
        // Get the key string for use in closures (InstanceKey is not Sync)
        let menu_key = self.key.get().to_string();

        // Build trigger element
        let open_state_for_trigger = open_state.clone();
        let open_state_for_trigger_1 = open_state.clone();
        let overlay_handle_for_trigger = overlay_handle_state.clone();
        let items_for_show = items.clone();

        let trigger = stateful_with_key::<ButtonState>(&button_key)
            .deps([open_state.signal_id()])
            .on_state(move |ctx| {
                let state = ctx.state();
                let is_open = open_state_for_trigger.get();
                let bg = btn_variant.background(theme, state);

                // Build trigger content
                let trigger_content: Div = if let Some(ref builder) = trigger_builder {
                    builder(is_open)
                } else if let Some(ref label) = trigger_label {
                    // Default button trigger with chevron
                    // Use a simple div-based button to avoid state persistence issues
                    let theme = ThemeState::get();
                    let chevron_svg = if is_open {
                        CHEVRON_UP_SVG
                    } else {
                        CHEVRON_DOWN_SVG
                    };

                    div()
                        .gap(8.0)
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .px(4.0)
                        .py(2.0)
                        .rounded(theme.radius(RadiusToken::Md))
                        .border(1.0, theme.color(ColorToken::Border))
                        .bg(bg)
                        .child(
                            text(label)
                                .size(14.0)
                                .color(theme.color(ColorToken::TextPrimary))
                                .no_cursor()
                                .pointer_events_none(),
                        )
                        .child(
                            svg(chevron_svg)
                                .size(16.0, 16.0)
                                .color(theme.color(ColorToken::TextSecondary)),
                        )
                } else {
                    div() // Fallback empty div
                };

                div()
                    .w_fit()
                    .bg(btn_variant.background(theme, ButtonState::Idle))
                    .cursor_pointer()
                    .child(trigger_content)
            })
            .on_click(move |ctx| {
                // Use bounds directly from EventContext - more reliable than querying
                let bounds = ElementBounds {
                    x: ctx.bounds_x,
                    y: ctx.bounds_y,
                    width: ctx.bounds_width,
                    height: ctx.bounds_height,
                };

                let is_open = open_state_for_trigger_1.get();
                if is_open {
                    // Close the menu - state updates are handled by on_close callback
                    // after the exit animation completes (deferred in overlay manager)
                    if let Some(handle_id) = overlay_handle_for_trigger.get() {
                        let mgr = get_overlay_manager();
                        let handle = OverlayHandle::from_raw(handle_id);

                        // Check if overlay is already closing or pending close
                        if mgr.is_closing(handle) || mgr.is_pending_close(handle) {
                            // Close already in progress, don't trigger again
                            return;
                        }

                        mgr.close(handle);
                    }
                    // Don't update state here - let on_close callback handle it after animation
                } else {
                    // Calculate position based on trigger bounds from event context
                    let (x, y) =
                        calculate_dropdown_position(&bounds, position, align, offset, min_width);

                    // Show the dropdown
                    let overlay_handle = show_dropdown_menu(
                        x,
                        y,
                        &items_for_show,
                        min_width,
                        overlay_handle_for_trigger.clone(),
                        open_state_for_trigger_1.clone(),
                        menu_key.clone(),
                    );

                    overlay_handle_for_trigger.set(Some(overlay_handle.id()));
                    open_state_for_trigger_1.set(true);
                }
            });

        DropdownMenu { inner: trigger }
    }
}

/// Calculate dropdown position based on trigger bounds
fn calculate_dropdown_position(
    bounds: &ElementBounds,
    position: DropdownPosition,
    align: DropdownAlign,
    offset: f32,
    min_width: f32,
) -> (f32, f32) {
    let (x, y) = match position {
        DropdownPosition::Bottom => {
            let y = bounds.y + bounds.height + offset;
            let x = match align {
                DropdownAlign::Start => bounds.x,
                DropdownAlign::Center => bounds.x + (bounds.width - min_width) / 2.0,
                DropdownAlign::End => bounds.x + bounds.width - min_width,
            };
            (x, y)
        }
        DropdownPosition::Top => {
            // Will need menu height, estimate for now
            let menu_height = 200.0;
            let y = bounds.y - menu_height - offset;
            let x = match align {
                DropdownAlign::Start => bounds.x,
                DropdownAlign::Center => bounds.x + (bounds.width - min_width) / 2.0,
                DropdownAlign::End => bounds.x + bounds.width - min_width,
            };
            (x, y)
        }
        DropdownPosition::Right => {
            let x = bounds.x + bounds.width + offset;
            let y = match align {
                DropdownAlign::Start => bounds.y,
                DropdownAlign::Center => bounds.y,
                DropdownAlign::End => bounds.y,
            };
            (x, y)
        }
        DropdownPosition::Left => {
            let x = bounds.x - min_width - offset;
            let y = match align {
                DropdownAlign::Start => bounds.y,
                DropdownAlign::Center => bounds.y,
                DropdownAlign::End => bounds.y,
            };
            (x, y)
        }
    };

    (x.max(0.0), y.max(0.0))
}

/// Show the dropdown menu overlay
fn show_dropdown_menu(
    x: f32,
    y: f32,
    items: &[ContextMenuItem],
    min_width: f32,
    handle_state: State<Option<u64>>,
    open_state: State<bool>,
    key: String,
) -> OverlayHandle {
    let theme = ThemeState::get();
    let bg = theme.color(ColorToken::Surface);
    let border = theme.color(ColorToken::Border);
    let text_color = theme.color(ColorToken::TextPrimary);
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let text_tertiary = theme.color(ColorToken::TextTertiary);
    let surface_elevated = theme.color(ColorToken::SurfaceElevated);
    let radius = theme.radius(RadiusToken::Md);
    let font_size = 14.0;
    let padding = 12.0;

    let items = items.to_vec();

    let handle_state_for_content = handle_state.clone();
    let open_state_for_content = open_state.clone();
    let handle_state_for_close = handle_state.clone();
    let open_state_for_dismiss = open_state.clone();

    let mgr = get_overlay_manager();

    // Create a unique motion key for this dropdown instance
    // The motion is on the child of the wrapper div, so we need ":child:0" suffix
    let motion_key_str = format!("dropdown_{}", key);
    let motion_key_with_child = format!("{}:child:0", motion_key_str);

    let handle = mgr
        .dropdown()
        .at(x, y)
        .dismiss_on_escape(true)
        .motion_key(&motion_key_with_child)
        .on_close(move || {
            open_state_for_dismiss.set(false);
            handle_state_for_close.set(None);
        })
        .content(move || {
            build_dropdown_content(
                &items,
                min_width,
                &handle_state_for_content,
                &open_state_for_content,
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

    handle
}

/// Show a submenu overlay positioned to the right of the parent item
fn show_submenu(
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
    let surface_elevated = theme.color(ColorToken::SurfaceElevated);
    let radius = theme.radius(RadiusToken::Md);
    let font_size = 14.0;
    let padding = 12.0;

    let items = items.to_vec();

    let submenu_handle_for_content = submenu_handle_state.clone();
    let parent_handle_for_content = parent_handle_state.clone();
    let submenu_handle_for_close = submenu_handle_state.clone();

    let mgr = get_overlay_manager();

    let motion_key_str = format!("submenu_{}", key);
    let motion_key_with_child = format!("{}:child:0", motion_key_str);

    let handle = mgr
        .dropdown()
        .at(x, y)
        .dismiss_on_escape(true)
        .motion_key(&motion_key_with_child)
        .on_close(move || {
            submenu_handle_for_close.set(None);
        })
        .content(move || {
            build_submenu_content(
                &items,
                min_width,
                &parent_handle_for_content,
                &submenu_handle_for_content,
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

    handle
}

/// Build submenu content (recursive for nested submenus)
#[allow(clippy::too_many_arguments)]
fn build_submenu_content(
    items: &[ContextMenuItem],
    width: f32,
    parent_handle_state: &State<Option<u64>>,
    submenu_handle_state: &State<Option<u64>>,
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
    let menu_id = key;

    // State for tracking nested submenus
    let nested_submenu_handle: State<Option<u64>> =
        BlincContextState::get().use_state_keyed(&format!("{}_nested", key), || None);

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

            let parent_handle_for_click = parent_handle_state.clone();
            let submenu_handle_for_click = submenu_handle_state.clone();
            let nested_submenu_for_hover = nested_submenu_handle.clone();
            let nested_submenu_for_leave = nested_submenu_handle.clone();

            let item_key = format!("{}_item-{}", key, idx);
            let submenu_key = format!("{}_sub-{}", key, idx);

            let item_text_color = if item_disabled {
                text_tertiary
            } else {
                text_color
            };

            let shortcut_color = text_secondary;

            let mut row = stateful_with_key::<ButtonState>(&item_key)
                .on_state(move |ctx| {
                    let state = ctx.state();
                    let theme = ThemeState::get();
                    let item_bg = if (state == ButtonState::Hovered || state == ButtonState::Pressed) && !item_disabled {
                        theme.color(ColorToken::SecondaryHover).with_alpha(0.65)
                    } else {
                        bg
                    };

                    let text_col = if (state == ButtonState::Hovered || state == ButtonState::Pressed) && !item_disabled {
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
                        .py(padding / 4.0)
                        .px(padding / 2.0)
                        .bg(item_bg)
                        .cursor(if item_disabled {
                            CursorStyle::NotAllowed
                        } else {
                            CursorStyle::Pointer
                        })
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(left_side);

                    if let Some(right) = right_side {
                        row_content = row_content.child(right);
                    }

                    row_content
                })
                .on_click(move |_| {
                    if !item_disabled && !has_submenu {
                        if let Some(ref cb) = item_on_click {
                            cb();
                        }

                        // Close the submenu
                        if let Some(handle_id) = submenu_handle_for_click.get() {
                            let mgr = get_overlay_manager();
                            mgr.close(OverlayHandle::from_raw(handle_id));
                        }

                        // Close the parent menu
                        if let Some(handle_id) = parent_handle_for_click.get() {
                            let mgr = get_overlay_manager();
                            mgr.close(OverlayHandle::from_raw(handle_id));
                        }
                    }
                });

            // Add hover handlers for submenu items
            if has_submenu && !item_disabled {
                let submenu_items_for_hover = submenu_items.clone();
                let parent_handle_for_submenu = parent_handle_state.clone();
                let submenu_handle_for_submenu = submenu_handle_state.clone();
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

                        let handle = show_submenu(
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

    div().child(
        motion_derived(key)
            .enter_animation(AnimationPreset::dropdown_in(150))
            .exit_animation(AnimationPreset::dropdown_out(100))
            .child(menu),
    )
}

/// Build the dropdown menu content
#[allow(clippy::too_many_arguments)]
fn build_dropdown_content(
    items: &[ContextMenuItem],
    width: f32,
    overlay_handle_state: &State<Option<u64>>,
    open_state: &State<bool>,
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

            let item_text_color = if item_disabled {
                text_tertiary
            } else {
                text_color
            };

            let shortcut_color = text_secondary;

            // Build the stateful row element
            let mut row = stateful_with_key::<ButtonState>(&item_key)
                .on_state(move |ctx| {
                    let state = ctx.state();
                    let theme = ThemeState::get();
                    // Apply hover background based on button state
                    let item_bg = if (state == ButtonState::Hovered || state == ButtonState::Pressed) && !item_disabled {
                        theme.color(ColorToken::SecondaryHover).with_alpha(0.65)
                    } else {
                        bg
                    };

                    let text_col = if (state == ButtonState::Hovered || state == ButtonState::Pressed) && !item_disabled {
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
                            .color(text_col)
                            .no_cursor().pointer_events_none(),
                    ).pointer_events_none();

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
                        Some(div().child(svg(chevron_right).size(12.0, 12.0).color(text_tertiary)).pointer_events_none())
                    } else {
                        None
                    };

                    let mut row_content = div()
                        .w_full()
                        .h_fit()
                        .py(padding / 4.0)
                        .px(padding / 2.0)
                        .bg(item_bg)
                        .cursor(if item_disabled {
                            CursorStyle::NotAllowed
                        } else {
                            CursorStyle::Pointer
                        })
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(left_side);

                    if let Some(right) = right_side {
                        row_content = row_content.child(right);
                    }

                    row_content
                })
                .on_click(move |_| {
                    if !item_disabled && !has_submenu {
                        if let Some(ref cb) = item_on_click {
                            cb();
                        }

                        // Close the overlay - state updates are handled by on_close callback
                        // after the exit animation completes (deferred in overlay manager)
                        if let Some(handle_id) = handle_state_for_click.get() {
                            let mgr = get_overlay_manager();
                            mgr.close(OverlayHandle::from_raw(handle_id));
                        }
                        // Don't update state here - let on_close callback handle it after animation
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

                        let handle = show_submenu(
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

    // Wrap in motion for animation
    // Use motion_derived with the key so the overlay can trigger exit animation
    div().child(
        motion_derived(key)
            .enter_animation(AnimationPreset::dropdown_in(150))
            .exit_animation(AnimationPreset::dropdown_out(100))
            .child(menu),
    )
}

/// The built dropdown menu component
pub struct DropdownMenu {
    inner: Stateful<ButtonState>,
}

impl std::fmt::Debug for DropdownMenu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DropdownMenu").finish()
    }
}

impl ElementBuilder for DropdownMenuBuilder {
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
        self.get_or_build().inner.event_handlers()
    }
}

/// Create a dropdown menu with a text label trigger
///
/// # Example
///
/// ```ignore
/// cn::dropdown_menu("Options")
///     .item("Edit", || {})
///     .item("Delete", || {})
/// ```
#[track_caller]
pub fn dropdown_menu(label: impl Into<String>) -> DropdownMenuBuilder {
    DropdownMenuBuilder::new(label)
}

/// Create a dropdown menu with a custom trigger
///
/// The trigger builder receives a boolean indicating if the menu is open.
///
/// # Example
///
/// ```ignore
/// cn::dropdown_menu_custom(|open| {
///     cn::button(if open { "Close" } else { "Menu" })
///         .variant(ButtonVariant::Ghost)
/// })
/// .item("Option 1", || {})
/// .item("Option 2", || {})
/// ```
#[track_caller]
pub fn dropdown_menu_custom<F>(trigger: F) -> DropdownMenuBuilder
where
    F: Fn(bool) -> Div + Send + Sync + 'static,
{
    DropdownMenuBuilder::with_trigger(trigger)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dropdown_position_bottom() {
        let bounds = ElementBounds {
            x: 100.0,
            y: 50.0,
            width: 80.0,
            height: 32.0,
        };
        let (x, y) = calculate_dropdown_position(
            &bounds,
            DropdownPosition::Bottom,
            DropdownAlign::Start,
            4.0,
            180.0,
        );
        assert_eq!(x, 100.0);
        assert_eq!(y, 86.0); // 50 + 32 + 4
    }

    #[test]
    fn test_dropdown_position_end_align() {
        let bounds = ElementBounds {
            x: 100.0,
            y: 50.0,
            width: 80.0,
            height: 32.0,
        };
        let (x, _y) = calculate_dropdown_position(
            &bounds,
            DropdownPosition::Bottom,
            DropdownAlign::End,
            4.0,
            180.0,
        );
        assert_eq!(x, 0.0); // 100 + 80 - 180 = 0 (clamped)
    }
}
