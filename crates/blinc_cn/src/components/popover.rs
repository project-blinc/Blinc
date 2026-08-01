//! Popover component - click-triggered floating content
//!
//! A styled overlay that appears when clicking a trigger element.
//! Unlike dropdown menus, popovers can contain any content.
//! Unlike hover cards, popovers are click-triggered and dismissed by clicking outside.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     // Basic popover with button trigger
//!     cn::popover(|| cn::button("Open"))
//!         .content(|| {
//!             div().flex_col().gap(8.0).children([
//!                 text("Popover Content").size(16.0),
//!                 cn::button("Action"),
//!             ])
//!         })
//!
//!     // Positioned to the right
//!     cn::popover(|| text("Click me"))
//!         .side(PopoverSide::Right)
//!         .content(|| text("Content on the right"))
//!
//!     // With custom alignment
//!     cn::popover(|| cn::button("Settings"))
//!         .side(PopoverSide::Bottom)
//!         .align(PopoverAlign::End)
//!         .content(|| settings_panel())
//! }
//! ```

use std::cell::OnceCell;
use std::sync::Arc;

use blinc_core::State;
use blinc_core::context_state::BlincContextState;
use blinc_layout::InstanceKey;
use blinc_layout::click_outside;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{ElementBounds, RenderProps};
use blinc_layout::overlay_state::overlay_stack;
use blinc_layout::prelude::*;
use blinc_layout::stateful::{ButtonState, stateful_with_key};
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::overlay::AnchorDirection;
use blinc_layout::widgets::overlay_stack::{OverlayBuilder, OverlayHandle};
use blinc_theme::{ColorToken, RadiusToken, SpacingToken, ThemeState};

/// Side where the popover appears relative to the trigger
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopoverSide {
    /// Above the trigger
    Top,
    /// Below the trigger (default)
    #[default]
    Bottom,
    /// To the right of the trigger
    Right,
    /// To the left of the trigger
    Left,
}

/// Alignment of the popover relative to the trigger
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopoverAlign {
    /// Align to start of trigger
    #[default]
    Start,
    /// Center with trigger
    Center,
    /// Align to end of trigger
    End,
}

/// Content builder function type for popover content
type ContentBuilderFn = Arc<dyn Fn() -> Div + Send + Sync>;

/// Trigger builder function type for popover trigger
type TriggerBuilderFn = Arc<dyn Fn(bool) -> Div + Send + Sync>;

/// Builder for popover component
pub struct PopoverBuilder {
    /// Trigger builder (receives open state)
    trigger: TriggerBuilderFn,
    /// Content to show in the popover
    content: Option<ContentBuilderFn>,
    /// Side where the popover appears
    side: PopoverSide,
    /// Alignment relative to trigger
    align: PopoverAlign,
    /// Offset from trigger (pixels)
    offset: f32,
    /// Unique instance key
    key: InstanceKey,
    /// User-added CSS classes
    classes: Vec<std::sync::Arc<str>>,
    /// User-set element ID
    user_id: Option<String>,
    /// Built component cache
    built: OnceCell<Popover>,
}

impl std::fmt::Debug for PopoverBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PopoverBuilder")
            .field("side", &self.side)
            .field("align", &self.align)
            .field("offset", &self.offset)
            .finish()
    }
}

impl PopoverBuilder {
    /// Create a new popover builder with a trigger builder function
    ///
    /// The trigger builder receives a boolean indicating if the popover is open.
    #[track_caller]
    pub fn new<F>(trigger_fn: F) -> Self
    where
        F: Fn(bool) -> Div + Send + Sync + 'static,
    {
        Self {
            trigger: Arc::new(trigger_fn),
            content: None,
            side: PopoverSide::Bottom,
            align: PopoverAlign::Start,
            offset: 4.0,
            key: InstanceKey::new("popover"),
            classes: Vec::new(),
            user_id: None,
            built: OnceCell::new(),
        }
    }

    /// Create with a pre-created key
    pub fn with_key<F>(trigger_fn: F, key: InstanceKey) -> Self
    where
        F: Fn(bool) -> Div + Send + Sync + 'static,
    {
        Self {
            trigger: Arc::new(trigger_fn),
            content: None,
            side: PopoverSide::Bottom,
            align: PopoverAlign::Start,
            offset: 4.0,
            key,
            classes: Vec::new(),
            user_id: None,
            built: OnceCell::new(),
        }
    }

    /// Set the content to display in the popover
    pub fn content<F>(mut self, content_fn: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.content = Some(Arc::new(content_fn));
        self
    }

    /// Set the side where the popover appears
    pub fn side(mut self, side: PopoverSide) -> Self {
        self.side = side;
        self
    }

    /// Set the alignment relative to the trigger
    pub fn align(mut self, align: PopoverAlign) -> Self {
        self.align = align;
        self
    }

    /// Set the offset from the trigger (in pixels)
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
    fn get_or_build(&self) -> &Popover {
        ::blinc_layout::build_once::build_once(&self.built, || self.build_component())
    }

    /// Build the popover component
    fn build_component(&self) -> Popover {
        // Create open state
        let open_state: State<bool> =
            BlincContextState::get().use_state_keyed(self.key.get(), || false);

        // Store overlay handle ID
        let overlay_handle_state: State<Option<u64>> =
            BlincContextState::get().use_state_keyed(&self.key.derive("handle"), || None);

        // Clone values for closures
        let side = self.side;
        let align = self.align;
        let offset = self.offset;
        let content_builder = self.content.clone();
        let trigger_builder = self.trigger.clone();
        let button_key = self.key.derive("button");

        // Clone states for closures
        let open_state_for_trigger = open_state.clone();
        let open_state_for_click = open_state.clone();
        let overlay_handle_for_click = overlay_handle_state.clone();
        let overlay_handle_for_show = overlay_handle_state.clone();
        let open_state_for_close = open_state.clone();
        let overlay_handle_for_close = overlay_handle_state.clone();

        // Build trigger with click handler
        let trigger = stateful_with_key::<ButtonState>(&button_key)
            .deps([open_state.signal_id()])
            .on_state(move |_ctx| {
                let is_open = open_state_for_trigger.get();

                // Build trigger content
                let trigger_content = (trigger_builder)(is_open);

                div().w_fit().cursor_pointer().child(trigger_content)
            })
            .on_click(move |ctx| {
                let bounds = ElementBounds {
                    x: ctx.bounds_x,
                    y: ctx.bounds_y,
                    width: ctx.bounds_width,
                    height: ctx.bounds_height,
                };

                let is_open = open_state_for_click.get();
                if is_open {
                    // Toggle off — close the active overlay if any.
                    if let Some(raw) = overlay_handle_for_click.get() {
                        let handle = OverlayHandle::from_raw(raw);
                        if handle.is_live() {
                            handle.close();
                        }
                    }
                    return;
                }

                // Toggle on — push a fresh popover overlay.
                let Some(ref content_fn) = content_builder else {
                    return;
                };
                let (x, y) = calculate_popover_position(&bounds, side, align, offset);

                let stored_close_handle = overlay_handle_for_close.clone();
                let stored_close_state = open_state_for_close.clone();
                let content_fn = Arc::clone(content_fn);

                let handle = build_popover_overlay(
                    x,
                    y,
                    side,
                    content_fn,
                    stored_close_state,
                    stored_close_handle,
                );

                overlay_handle_for_show.set(Some(handle.raw()));
                open_state_for_click.set(true);
            });

        let mut inner = trigger;
        for c in &self.classes {
            inner = inner.class(c);
        }
        if let Some(ref id) = self.user_id {
            inner = inner.id(id);
        }
        Popover { inner }
    }
}

/// Calculate popover position based on trigger bounds
///
/// For Top/Left positioning, we position at the trigger edge and let content
/// extend naturally. The offset provides spacing from the trigger.
fn calculate_popover_position(
    bounds: &ElementBounds,
    side: PopoverSide,
    align: PopoverAlign,
    offset: f32,
) -> (f32, f32) {
    match side {
        PopoverSide::Top => {
            // Position just above trigger - with AnchorDirection::Top, y is the BOTTOM edge
            // So y = trigger.y - offset puts the popover bottom just above the trigger top
            let y = bounds.y - bounds.height - offset * 4.0;
            let x = match align {
                PopoverAlign::Start => bounds.x,
                PopoverAlign::Center => bounds.x + bounds.width / 2.0,
                PopoverAlign::End => bounds.x + bounds.width,
            };
            (x.max(0.0), y.max(0.0))
        }
        PopoverSide::Bottom => {
            // Position below trigger - content extends downward
            let y = bounds.y + bounds.height + offset;
            let x = match align {
                PopoverAlign::Start => bounds.x,
                PopoverAlign::Center => bounds.x + bounds.width / 2.0,
                PopoverAlign::End => bounds.x + bounds.width,
            };
            (x.max(0.0), y)
        }
        PopoverSide::Right => {
            // Position to the right of trigger
            let x = bounds.x + bounds.width + offset;
            let y = match align {
                PopoverAlign::Start => bounds.y,
                PopoverAlign::Center => bounds.y + bounds.height / 2.0,
                PopoverAlign::End => bounds.y + bounds.height,
            };
            (x, y.max(0.0))
        }
        PopoverSide::Left => {
            // Position to the left of trigger - content extends leftward
            let x = bounds.x - offset;
            let y = match align {
                PopoverAlign::Start => bounds.y,
                PopoverAlign::Center => bounds.y + bounds.height / 2.0,
                PopoverAlign::End => bounds.y + bounds.height,
            };
            (x.max(0.0), y.max(0.0))
        }
    }
}

/// Push a popover overlay to the global `OverlayStack`. Wires:
/// - Content with a unique `cn-popover-{handle.raw()}` element id so the
///   click_outside registry can detect "click inside" via ancestor-id match.
/// - on_close callback that flips the widget's open_state + overlay_handle_state
///   back to defaults AND unregisters the click_outside handler.
/// - click_outside registration that calls `handle.close()` on any mouse-down
///   outside the popover's element subtree.
///
/// Returns the new OverlayHandle. Enter animation is delegated to the CSS
/// `@keyframes cn-popover-enter` rule (see cn_styles.rs).
fn build_popover_overlay(
    x: f32,
    y: f32,
    side: PopoverSide,
    content_fn: ContentBuilderFn,
    open_state: State<bool>,
    overlay_handle_state: State<Option<u64>>,
) -> OverlayHandle {
    let theme = ThemeState::get();
    let bg = theme.color(ColorToken::SurfaceElevated);
    let border = theme.color(ColorToken::Border);
    let radius = theme.radius(RadiusToken::Lg);
    let padding = theme.spacing_value(SpacingToken::Space4);

    let anchor_dir = match side {
        PopoverSide::Top => AnchorDirection::Top,
        PopoverSide::Bottom => AnchorDirection::Bottom,
        PopoverSide::Left => AnchorDirection::Left,
        PopoverSide::Right => AnchorDirection::Right,
    };

    // Reserve the handle id before push so the content closure can stamp
    // the same id onto the rendered div for click_outside ancestor matching.
    // OverlayStack issues sequential handle ids; peek at the next one by
    // briefly locking + reading the counter. (Locking inside the show()
    // call would be re-entrant.)
    let next_handle_id = overlay_stack()
        .lock()
        .ok()
        .map(|s| s.peek_next_handle_id())
        .unwrap_or(0);

    let popover_id = format!("cn-popover-{}", next_handle_id);
    let click_outside_key = format!("popover:{}", next_handle_id);

    let popover_id_for_content = popover_id.clone();
    let click_outside_key_for_close = click_outside_key.clone();

    // Generous-side estimate for the viewport-clamp in
    // `position_wrapper`. Content is user-supplied (opaque
    // `Arc<dyn Fn() -> Div>`) so we can't measure exactly; the
    // 320 × 240 cap covers typical popovers (settings forms,
    // button rows, multi-line text + action) plus the cn chrome
    // (2 × 16 px Space4 padding + 1 px borders).
    let handle = OverlayBuilder::popover()
        .at(x, y)
        .size(320.0, 240.0)
        .anchor_direction(anchor_dir)
        // Defaults from DismissRules::default_for(Dropdown): on_escape=true,
        // on_click_outside=true (handled here via click_outside registry),
        // blocks_below=false, no backdrop.
        .on_close(move |_reason| {
            open_state.set(false);
            overlay_handle_state.set(None);
            click_outside::unregister_click_outside(&click_outside_key_for_close);
        })
        .content(move || {
            let user_content = (content_fn)();

            // `lock_corner_shape` keeps panel corners circular even when a
            // theme advertises a squircle exponent — overlay chrome reads
            // cleanest with platform-default rounded-rect shape.
            div()
                .class("cn-popover-content")
                .id(&popover_id_for_content)
                .flex_col()
                .bg(bg)
                .border(1.0, border)
                .rounded(radius)
                .lock_corner_shape()
                .p_px(padding)
                .shadow_lg()
                .min_w(150.0)
                .overflow_clip()
                .child(user_content)
        })
        .show();

    debug_assert_eq!(
        handle.raw(),
        next_handle_id,
        "peek_next_handle_id was stale — concurrent push?"
    );

    // Register click_outside AFTER show() so the popover content has its id
    // mounted on the next frame's tree walk. The registry doesn't depend on
    // the tree being ready — it just stores the key→id mapping until the
    // next mouse-down fires fire_click_outside().
    click_outside::register_click_outside(&click_outside_key, &popover_id, move || {
        handle.close();
    });

    handle
}

/// Built popover component
pub struct Popover {
    inner: blinc_layout::stateful::Stateful<ButtonState>,
}

impl std::fmt::Debug for Popover {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Popover").finish()
    }
}

impl ElementBuilder for PopoverBuilder {
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

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().inner.element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().inner.element_id()
    }
}

impl ElementBuilder for Popover {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.inner.event_handlers()
    }
}

/// Create a popover component with a trigger
///
/// The trigger builder receives a boolean indicating if the popover is open,
/// allowing the trigger to visually respond to the open state.
///
/// # Example
///
/// ```ignore
/// cn::popover(|open| {
///     cn::button(if open { "Close" } else { "Open" })
/// })
/// .content(|| {
///     div().flex_col().gap(8.0).children([
///         text("Popover Title").size(16.0),
///         text("Some content here."),
///     ])
/// })
/// ```
#[track_caller]
pub fn popover<F>(trigger_fn: F) -> PopoverBuilder
where
    F: Fn(bool) -> Div + Send + Sync + 'static,
{
    PopoverBuilder::new(trigger_fn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_popover_position_bottom() {
        let bounds = ElementBounds {
            x: 100.0,
            y: 50.0,
            width: 80.0,
            height: 32.0,
        };
        let (x, y) =
            calculate_popover_position(&bounds, PopoverSide::Bottom, PopoverAlign::Start, 4.0);
        assert_eq!(x, 100.0);
        assert_eq!(y, 86.0); // 50 + 32 + 4
    }

    #[test]
    fn test_popover_position_right() {
        let bounds = ElementBounds {
            x: 100.0,
            y: 50.0,
            width: 80.0,
            height: 32.0,
        };
        let (x, y) =
            calculate_popover_position(&bounds, PopoverSide::Right, PopoverAlign::Start, 8.0);
        assert_eq!(x, 188.0); // 100 + 80 + 8
        assert_eq!(y, 50.0);
    }

    #[test]
    fn test_popover_position_top() {
        let bounds = ElementBounds {
            x: 100.0,
            y: 100.0,
            width: 80.0,
            height: 32.0,
        };
        let (x, y) =
            calculate_popover_position(&bounds, PopoverSide::Top, PopoverAlign::Start, 4.0);
        assert_eq!(x, 100.0);
        // y = trigger.y - trigger.height - offset * 4.0 = 100 - 32 - 16 = 52
        // This is where the popover's BOTTOM edge will be (via anchor_direction)
        assert_eq!(y, 52.0);
    }

    #[test]
    fn test_popover_position_center_align() {
        let bounds = ElementBounds {
            x: 100.0,
            y: 50.0,
            width: 80.0,
            height: 32.0,
        };
        let (x, _y) =
            calculate_popover_position(&bounds, PopoverSide::Bottom, PopoverAlign::Center, 4.0);
        // x = 100 + 80/2 = 140 (center of trigger)
        assert_eq!(x, 140.0);
    }
}
