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

use blinc_core::context_state::BlincContextState;
use blinc_core::{Color, Signal, State};
use blinc_layout::click_outside;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, ElementBounds, RenderProps};
use blinc_layout::overlay_state::overlay_stack;
use blinc_layout::prelude::*;
use blinc_layout::stateful::ButtonState;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::hr::hr;
use blinc_layout::widgets::overlay_stack::{OverlayBuilder, OverlayHandle};
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

/// Icon for chevron down
const CHEVRON_DOWN_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;

/// Icon for chevron up
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
    /// User-added CSS classes
    classes: Vec<std::sync::Arc<str>>,
    /// User-set element ID
    user_id: Option<String>,
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
            classes: Vec::new(),
            user_id: None,
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
            classes: Vec::new(),
            user_id: None,
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
    fn get_or_build(&self) -> &DropdownMenu {
        ::blinc_layout::build_once::build_once(&self.built, || self.build_component())
    }

    /// Build the dropdown menu component
    fn build_component(&self) -> DropdownMenu {
        let theme = ThemeState::get();

        // Create open state using InstanceKey for unique identification
        let open_state: Signal<bool> =
            BlincContextState::get().use_signal_keyed(self.key.get(), || false);

        // Store overlay handle ID
        let overlay_handle_state: Signal<Option<u64>> =
            BlincContextState::get().use_signal_keyed(&self.key.derive("handle"), || None);

        // Clone values for closures
        let items = self.items.clone();
        let min_width = self.min_width;
        let position = self.position;
        let align = self.align;
        let offset = self.offset;
        let trigger_label = self.trigger_label.clone();
        let trigger_builder = self.trigger_builder.clone();

        // Get the key string for use in closures (InstanceKey is not Sync)
        let menu_key = self.key.get().to_string();

        // Build trigger element
        let open_state_for_trigger = open_state;
        let overlay_handle_for_trigger = overlay_handle_state;
        let items_for_show = items.clone();

        // The trigger is built once. Its fill is the `.cn-dropdown-trigger`
        // rules, its chevron turns while the menu is open, and a custom
        // trigger is swapped for the open or closed one it builds.
        let trigger_content: Div = if let Some(builder) = trigger_builder {
            let closed = builder.clone();
            div().show_or(open_state, move || builder(true), move || closed(false))
        } else if let Some(label) = trigger_label {
            div()
                .class("cn-dropdown-trigger")
                .attr_when("data-state", "open", open_state)
                .gap(8.0)
                .flex_row()
                .items_center()
                .justify_between()
                .px(4.0)
                .py(2.0)
                .border_width(1.0)
                .child(
                    text(&label)
                        .size(theme.typography().text_sm)
                        .no_wrap()
                        .no_cursor()
                        .pointer_events_none(),
                )
                .child(
                    div()
                        .class("cn-chevron")
                        .child(svg(CHEVRON_DOWN_SVG).size(16.0, 16.0)),
                )
        } else {
            div()
        };

        let trigger = div()
            .id(trigger_element_id(&menu_key))
            .attr("tabindex", "0")
            .attr("aria-haspopup", "menu")
            .attr("data-focus-delegate", "true")
            .w_fit()
            .cursor_pointer()
            .child(trigger_content)
            .on_click(move |ctx| {
                let bounds = ElementBounds {
                    x: ctx.bounds_x,
                    y: ctx.bounds_y,
                    width: ctx.bounds_width,
                    height: ctx.bounds_height,
                };

                let is_open = open_state_for_trigger.get();
                if is_open {
                    if let Some(handle_id) = overlay_handle_for_trigger.get() {
                        OverlayHandle::from_raw(handle_id).close();
                    }
                } else {
                    let (x, y) =
                        calculate_dropdown_position(&bounds, position, align, offset, min_width);

                    let overlay_handle = spawn_dropdown_menu(
                        x,
                        y,
                        &items_for_show,
                        min_width,
                        overlay_handle_for_trigger,
                        open_state_for_trigger,
                        menu_key.clone(),
                    );

                    overlay_handle_for_trigger.set(Some(overlay_handle.raw()));
                    open_state_for_trigger.set(true);
                }
            });

        let mut inner = trigger;
        for c in &self.classes {
            inner = inner.class(c);
        }
        if let Some(ref id) = self.user_id {
            inner = inner.id(id);
        }
        DropdownMenu { inner }
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

// =============================================================================
// Dropdown / submenu spawning
// =============================================================================

/// Push a root dropdown onto the stack. Mirrors the context_menu approach:
/// click_outside is wired via the existing registry so clicks inside any open
/// submenu (registered in `id_chain`) don't dismiss the chain.
fn spawn_dropdown_menu(
    x: f32,
    y: f32,
    items: &[ContextMenuItem],
    min_width: f32,
    handle_state: Signal<Option<u64>>,
    open_state: Signal<bool>,
    key: String,
) -> OverlayHandle {
    let theme = ThemeState::get();
    let radius = theme.radius(RadiusToken::Md);
    let font_size = 14.0;
    let padding = 12.0;

    let items = items.to_vec();
    let handle_state_for_close = handle_state;
    let open_state_for_close = open_state;

    let next_handle_id = overlay_stack()
        .lock()
        .ok()
        .map(|s| s.peek_next_handle_id())
        .unwrap_or(0);
    let menu_handle = OverlayHandle::from_raw(next_handle_id);
    let menu_id = format!("cn-dropdown-menu-{}", next_handle_id);
    let click_outside_key = format!("dropdown_menu:{}", next_handle_id);

    let id_chain: Signal<Vec<String>> = BlincContextState::get()
        .use_signal_keyed(&format!("dropdown_menu_chain_{}", next_handle_id), || {
            Vec::<String>::new()
        });
    // The trigger counts as inside, so pressing it is not a dismissal: the
    // click that follows closes the menu itself instead of reopening it.
    id_chain.set(vec![menu_id.clone(), trigger_element_id(&key)]);

    let click_outside_key_for_close = click_outside_key.clone();
    let click_outside_key_for_content = click_outside_key.clone();
    let id_chain_for_content = id_chain;
    let menu_id_for_content = menu_id.clone();

    // Exact bounding box for the viewport-clamp in
    // `position_wrapper`. Per-row height ≈ font_size + padding
    // (matches the context_menu adoption); 20 px extra width
    // covers the 1 px border each side + shortcut/chevron
    // overflow margin. The clamp picks the right placement
    // before the panel even mounts.
    let est_w = min_width + 20.0;
    let est_h = items.len() as f32 * (font_size + padding) + padding;

    let handle = OverlayBuilder::dropdown()
        .at(x, y)
        .size(est_w, est_h)
        // Defaults: on_escape=true, on_click_outside=true (via registry).
        .on_close(move |_reason| {
            click_outside::unregister_click_outside(&click_outside_key_for_close);
            open_state_for_close.set(false);
            handle_state_for_close.set(None);
        })
        .content(move || {
            build_dropdown_menu_div(
                &items,
                min_width,
                menu_handle,
                &menu_id_for_content,
                &click_outside_key_for_content,
                &id_chain_for_content,
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

    click_outside::register_click_outside_multi(&click_outside_key, &id_chain.get(), move || {
        menu_handle.close();
    });

    handle
}

/// The element id of a menu's trigger, which its click-outside treats as inside.
fn trigger_element_id(key: &str) -> String {
    format!("cn-dropdown-trigger-{key}")
}

/// Push a submenu to the right of a parent item. Item clicks on leaves close
/// `root_handle` (closing the whole chain via stack cascade).
#[allow(clippy::too_many_arguments)]
fn spawn_dropdown_submenu(
    x: f32,
    y: f32,
    items: &[ContextMenuItem],
    min_width: f32,
    root_handle: OverlayHandle,
    root_click_outside_key: String,
    id_chain: Signal<Vec<String>>,
    parent_submenu_state: Signal<Option<u64>>,
) -> OverlayHandle {
    let theme = ThemeState::get();
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
    let submenu_id = format!("cn-dropdown-menu-{}", next_handle_id);

    // Push our id onto the root's inside-set so clicks inside us don't fire
    // the root's click_outside callback.
    let mut chain = id_chain.get();
    if !chain.contains(&submenu_id) {
        chain.push(submenu_id.clone());
        id_chain.set(chain);
        click_outside::update_click_outside_ids(&root_click_outside_key, &id_chain.get());
    }

    let id_chain_for_close = id_chain;
    let root_click_outside_key_for_close = root_click_outside_key.clone();
    let submenu_id_for_close = submenu_id.clone();
    let parent_submenu_state_for_close = parent_submenu_state;

    let id_chain_for_content = id_chain;
    let root_click_outside_key_for_content = root_click_outside_key.clone();
    let submenu_id_for_content = submenu_id.clone();

    // Same row-height formula as the root dropdown — submenus
    // share `build_dropdown_menu_div` so the painted chrome is
    // identical.
    let est_w = min_width + 20.0;
    let est_h = items.len() as f32 * (font_size + padding) + padding;

    let handle = OverlayBuilder::context_menu()
        .at(x, y)
        .size(est_w, est_h)
        .on_close(move |_reason| {
            let mut chain = id_chain_for_close.get();
            chain.retain(|id| id != &submenu_id_for_close);
            id_chain_for_close.set(chain);
            click_outside::update_click_outside_ids(
                &root_click_outside_key_for_close,
                &id_chain_for_close.get(),
            );
            if parent_submenu_state_for_close.get() == Some(submenu_handle.raw()) {
                parent_submenu_state_for_close.set(None);
            }
        })
        .content(move || {
            build_dropdown_menu_div(
                &items,
                min_width,
                root_handle,
                &submenu_id_for_content,
                &root_click_outside_key_for_content,
                &id_chain_for_content,
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

/// Build a dropdown menu's content (used for both root and submenus).
/// Enter animation lives on `.cn-dropdown-menu` via `@keyframes cn-dropdown-menu-enter`.
#[allow(clippy::too_many_arguments)]
fn build_dropdown_menu_div(
    items: &[ContextMenuItem],
    width: f32,
    root_handle: OverlayHandle,
    self_id: &str,
    root_click_outside_key: &str,
    id_chain: &Signal<Vec<String>>,
    radius: f32,
    font_size: f32,
    padding: f32,
) -> Div {
    let child_submenu_state: Signal<Option<u64>> = BlincContextState::get()
        .use_signal_keyed(&format!("dropdown_child_sub_{}", self_id), || None);

    let mut menu = super::menu::panel(radius)
        .class("cn-dropdown-menu")
        .id(self_id)
        .w(width);

    for (idx, item) in items.iter().enumerate() {
        if item.is_separator() {
            menu = menu.child(super::menu::separator());
        } else {
            let item_label = item.get_label().to_string();
            let item_shortcut = item.get_shortcut().map(|s| s.to_string());
            let item_icon = item.get_icon().map(|s| s.to_string());
            let item_disabled = item.is_disabled();
            let item_on_click = item.get_on_click();
            let has_submenu = item.has_submenu();
            let submenu_items = item.get_submenu().cloned();

            let child_submenu_for_hover_open = child_submenu_state;
            let child_submenu_for_hover_close = child_submenu_state;
            let id_chain_for_open = *id_chain;
            let root_click_outside_key_for_open = root_click_outside_key.to_string();
            let submenu_key = format!("{}_sub-{}", self_id, idx);

            // The row is a plain div with no Stateful wrapper: a rebuild
            // inside an overlay's content would freeze its hover look.
            let (left_side, right_side) = super::menu::item_parts(
                item_icon.as_deref(),
                &item_label,
                item_shortcut.as_deref(),
                has_submenu,
                font_size,
                padding / 4.0,
                true,
            );

            let mut row_content = div()
                .class("cn-menu-item")
                .class("cn-dropdown-item")
                .attr("role", "menuitem")
                .attr("tabindex", "-1")
                .w_full()
                .h_fit()
                .py(padding / 4.0)
                .px(padding / 2.0);
            if has_submenu {
                // CSS hook for the panel's `:has()` flicker
                // suppression rule. See cn_styles.rs's
                // `.cn-dropdown-menu:has(.cn-dropdown-item--has-submenu:hover)`.
                row_content = row_content.class("cn-dropdown-item--has-submenu");
            }
            row_content = row_content
                .attr_when("disabled", "", item_disabled)
                .flex_row()
                .items_center()
                .justify_between()
                .child(left_side);

            if let Some(right) = right_side {
                row_content = row_content.child(right);
            }

            let mut row = row_content.on_click(move |_| {
                if !item_disabled && !has_submenu {
                    // Close root FIRST so `cb()`-pushed overlays (e.g. a
                    // confirm dialog) aren't immediately cascade-closed by
                    // the menu's `UnwindFromBelow`. See cn::context_menu
                    // for the full rationale + verified trace.
                    root_handle.close();
                    if let Some(ref cb) = item_on_click {
                        cb();
                    }
                }
            });

            if has_submenu && !item_disabled {
                let submenu_items_for_hover = submenu_items.clone();
                let id_chain_for_hover = id_chain_for_open;
                let root_key_for_hover = root_click_outside_key_for_open.clone();

                let open_submenu =
                    std::rc::Rc::new(move |ctx: &blinc_layout::event_handler::EventContext| {
                        if let Some(handle_id) = child_submenu_for_hover_open.get() {
                            OverlayHandle::from_raw(handle_id).close();
                        }

                        if let Some(ref items) = submenu_items_for_hover {
                            let x = ctx.bounds_x + ctx.bounds_width + 4.0;
                            let y = ctx.bounds_y;

                            let handle = spawn_dropdown_submenu(
                                x,
                                y,
                                items,
                                160.0,
                                root_handle,
                                root_key_for_hover.clone(),
                                id_chain_for_hover,
                                child_submenu_for_hover_open,
                            );
                            child_submenu_for_hover_open.set(Some(handle.raw()));
                        }
                        let _ = &submenu_key;
                    });
                let click_submenu = open_submenu.clone();
                row = row
                    .attr("aria-haspopup", "menu")
                    .on_click(move |ctx| click_submenu(ctx))
                    .on_hover_enter(move |ctx| open_submenu(ctx));
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

/// The built dropdown menu component
pub struct DropdownMenu {
    inner: Div,
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
        ElementBuilder::event_handlers(&self.get_or_build().inner)
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().inner.element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().inner.element_id()
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
#[path = "tests/dropdown_menu.rs"]
mod tests;
