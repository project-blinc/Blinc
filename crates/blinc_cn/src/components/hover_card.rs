//! Hover Card component - content revealed on hover with delay
//!
//! A styled overlay card that appears when hovering over a trigger element.
//! Similar to a tooltip but designed for richer content and interaction.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     // Basic hover card with user info
//!     cn::hover_card(|| text("@username"))
//!         .content(|| {
//!             div().flex_col().gap(8.0).children([
//!                 text("John Doe").size(16.0).bold(),
//!                 text("Software Engineer").size(14.0).color(Color::gray()),
//!                 text("Joined January 2024").size(12.0),
//!             ])
//!         })
//!
//!     // With custom delays
//!     cn::hover_card(|| cn::button("Hover me"))
//!         .open_delay_ms(300)
//!         .close_delay_ms(200)
//!         .content(|| text("Additional information"))
//!
//!     // Positioned to the right
//!     cn::hover_card(|| text("Hover"))
//!         .side(HoverCardSide::Right)
//!         .content(|| text("Content on the right"))
//! }
//! ```

use std::cell::OnceCell;
use std::sync::Arc;

use blinc_core::State;
use blinc_core::context_state::BlincContextState;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::overlay_state::overlay_stack;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::overlay::AnchorDirection;
use blinc_layout::widgets::overlay_stack::{OverlayBuilder, OverlayHandle};
use blinc_theme::{ColorToken, RadiusToken, SpacingToken, ThemeState};

use blinc_layout::InstanceKey;

/// Side where the hover card appears relative to the trigger
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HoverCardSide {
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

/// Alignment of the hover card relative to the trigger
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HoverCardAlign {
    /// Align to start of trigger
    #[default]
    Start,
    /// Center with trigger
    Center,
    /// Align to end of trigger
    End,
}

/// Content builder function type for hover card content
type ContentBuilderFn = Arc<dyn Fn() -> Div + Send + Sync>;

/// Trigger builder function type for hover card trigger
type TriggerBuilderFn = Arc<dyn Fn() -> Div + Send + Sync>;

/// Builder for hover card component
pub struct HoverCardBuilder {
    /// Trigger content (the element that triggers the hover card)
    trigger: TriggerBuilderFn,
    /// Content to show in the hover card
    content: Option<ContentBuilderFn>,
    /// Side where the card appears
    side: HoverCardSide,
    /// Alignment relative to trigger
    align: HoverCardAlign,
    /// Delay before opening (ms)
    open_delay_ms: u32,
    /// Delay before closing (ms)
    close_delay_ms: u32,
    /// Offset from trigger (pixels)
    offset: f32,
    /// Unique instance key
    key: InstanceKey,
    /// User-added CSS classes
    classes: Vec<std::sync::Arc<str>>,
    /// User-set element ID
    user_id: Option<String>,
    /// Built component cache
    built: OnceCell<HoverCard>,
}

impl std::fmt::Debug for HoverCardBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HoverCardBuilder")
            .field("side", &self.side)
            .field("align", &self.align)
            .field("open_delay_ms", &self.open_delay_ms)
            .field("close_delay_ms", &self.close_delay_ms)
            .field("offset", &self.offset)
            .finish()
    }
}

impl HoverCardBuilder {
    /// Create a new hover card builder with a trigger builder function and a pre-created key
    pub fn with_key<F>(trigger_fn: F, key: InstanceKey) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        Self {
            trigger: Arc::new(trigger_fn),
            content: None,
            side: HoverCardSide::Bottom,
            align: HoverCardAlign::Start,
            open_delay_ms: 500,  // Default 500ms delay before showing
            close_delay_ms: 300, // Default 300ms delay before hiding
            offset: 8.0,
            key,
            classes: Vec::new(),
            user_id: None,
            built: OnceCell::new(),
        }
    }

    /// Set the content to display in the hover card
    pub fn content<F>(mut self, content_fn: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.content = Some(Arc::new(content_fn));
        self
    }

    /// Set the side where the card appears
    pub fn side(mut self, side: HoverCardSide) -> Self {
        self.side = side;
        self
    }

    /// Set the alignment relative to the trigger
    pub fn align(mut self, align: HoverCardAlign) -> Self {
        self.align = align;
        self
    }

    /// Set the delay before opening (in milliseconds)
    pub fn open_delay_ms(mut self, delay: u32) -> Self {
        self.open_delay_ms = delay;
        self
    }

    /// Set the delay before closing (in milliseconds)
    pub fn close_delay_ms(mut self, delay: u32) -> Self {
        self.close_delay_ms = delay;
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
    fn get_or_build(&self) -> &HoverCard {
        ::blinc_layout::build_once::build_once(&self.built, || self.build_component())
    }

    /// Build the hover card component
    fn build_component(&self) -> HoverCard {
        // Per-instance handle state, survives rebuilds.
        let overlay_handle_state: State<Option<u64>> =
            BlincContextState::get().use_state_keyed(&self.key.derive("handle"), || None);

        let side = self.side;
        let align = self.align;
        let offset = self.offset;
        let close_delay_ms = self.close_delay_ms;
        let content_builder = self.content.clone();
        let trigger_builder = self.trigger.clone();

        let stored_for_enter = overlay_handle_state.clone();
        let stored_for_leave = overlay_handle_state.clone();
        let stored_for_open = overlay_handle_state.clone();

        let trigger_content = (trigger_builder)();

        let trigger = div()
            // `w_fit()` already keeps this from stretching. Naming
            // `align_self_start()` on top made the value look authored,
            // so a row asking for `align-items: center` was ignored and
            // the trigger hung from the top against taller siblings.
            .w_fit()
            .child(trigger_content)
            .on_hover_enter(move |ctx| {
                // Live overlay → cancel pending close (or revive an exit
                // that already started during the close-delay window).
                if let Some(raw) = stored_for_enter.get() {
                    let handle = OverlayHandle::from_raw(raw);
                    if handle.is_live() {
                        if let Ok(mut stack) = overlay_stack().lock() {
                            stack.handle_mouse_enter(handle);
                        }
                        return;
                    }
                    if handle.is_exiting() {
                        // Exit animation already started — revive it.
                        if let Ok(mut stack) = overlay_stack().lock() {
                            stack.revive(handle);
                        }
                        return;
                    }
                    stored_for_enter.set(None);
                }

                let Some(ref content_fn) = content_builder else {
                    return;
                };

                let (x, y) = calculate_hover_card_position(
                    ctx.bounds_x,
                    ctx.bounds_y,
                    ctx.bounds_width,
                    ctx.bounds_height,
                    side,
                    align,
                    offset,
                );

                let content_fn = Arc::clone(content_fn);
                let stored_close = stored_for_open.clone();

                let handle = build_hover_card_overlay(
                    x,
                    y,
                    side,
                    content_fn,
                    close_delay_ms,
                    stored_close.clone(),
                );

                stored_for_open.set(Some(handle.raw()));
            })
            .on_hover_leave(move |_| {
                if let Some(raw) = stored_for_leave.get() {
                    let handle = OverlayHandle::from_raw(raw);
                    if handle.is_live() {
                        if let Ok(mut stack) = overlay_stack().lock() {
                            stack.handle_mouse_leave(handle);
                        }
                    }
                }
            });

        let mut inner = trigger;
        for c in &self.classes {
            inner = inner.class(c);
        }
        if let Some(ref id) = self.user_id {
            inner = inner.id(id);
        }
        HoverCard { inner }
    }
}

/// Calculate position for hover card based on trigger bounds
fn calculate_hover_card_position(
    trigger_x: f32,
    trigger_y: f32,
    trigger_w: f32,
    trigger_h: f32,
    side: HoverCardSide,
    align: HoverCardAlign,
    offset: f32,
) -> (f32, f32) {
    // Estimate card width for alignment calculations
    let card_width_estimate = 280.0;

    match side {
        HoverCardSide::Top => {
            // Position above trigger - use trigger_y - offset as the bottom anchor point
            // The overlay content will be positioned to align its bottom edge here
            let y = trigger_y - (offset * 8.0);
            let x = match align {
                HoverCardAlign::Start => trigger_x,
                HoverCardAlign::Center => trigger_x + (trigger_w - card_width_estimate) / 2.0,
                HoverCardAlign::End => trigger_x + trigger_w - card_width_estimate,
            };
            (x.max(0.0), y.max(0.0))
        }
        HoverCardSide::Bottom => {
            // Position below trigger
            let y = trigger_y + trigger_h + offset;
            let x = match align {
                HoverCardAlign::Start => trigger_x,
                HoverCardAlign::Center => trigger_x + (trigger_w - card_width_estimate) / 2.0,
                HoverCardAlign::End => trigger_x + trigger_w - card_width_estimate,
            };
            (x.max(0.0), y)
        }
        HoverCardSide::Right => {
            // Position to the right of trigger
            let x = trigger_x + trigger_w + offset;
            let y = match align {
                HoverCardAlign::Start => trigger_y,
                HoverCardAlign::Center => trigger_y,
                HoverCardAlign::End => trigger_y,
            };
            (x, y)
        }
        HoverCardSide::Left => {
            // Position to the left of trigger
            let x = trigger_x - card_width_estimate - offset;
            let y = match align {
                HoverCardAlign::Start => trigger_y,
                HoverCardAlign::Center => trigger_y,
                HoverCardAlign::End => trigger_y,
            };
            (x.max(0.0), y)
        }
    }
}

/// Push a hover card overlay to the global `OverlayStack`. Wires:
/// - Card content `on_hover_enter` cancels any pending mouse-leave close
///   (mouse moved from trigger → card without crossing dead space). Also
///   revives an in-flight exit if the close-delay countdown elapsed before
///   the user re-entered the card.
/// - Card `on_hover_leave` restarts the mouse-leave countdown.
/// - on_close clears the widget's stored handle.
///
/// Enter animation is delegated to CSS `@keyframes cn-hover-card-enter`
/// (see cn_styles.rs) — same approach as cn::tooltip/popover. Exit snaps
/// for now pending the motion-FSM fix.
fn build_hover_card_overlay(
    x: f32,
    y: f32,
    side: HoverCardSide,
    content_fn: ContentBuilderFn,
    close_delay_ms: u32,
    overlay_handle_state: State<Option<u64>>,
) -> OverlayHandle {
    let theme = ThemeState::get();
    let bg = theme.color(ColorToken::SurfaceElevated);
    let border = theme.color(ColorToken::Border);
    let radius = theme.radius(RadiusToken::Lg);
    let padding = theme.spacing_value(SpacingToken::Space4);

    // Single-instance is enforced PER-TRIGGER via the widget's stored handle
    // (see `build_component`). Avoid `close_all_of_kind(Tooltip)` here —
    // each on_close it would fire calls State::set(None), which the
    // reactive system treats as a global rebuild trigger. Rapid hover
    // between multiple hover_card triggers then cascades into many
    // full-UI rebuilds and the app locks up.

    let anchor_dir = match side {
        HoverCardSide::Top => AnchorDirection::Top,
        HoverCardSide::Bottom => AnchorDirection::Bottom,
        HoverCardSide::Left => AnchorDirection::Left,
        HoverCardSide::Right => AnchorDirection::Right,
    };

    // Pre-allocate handle id so the card content can capture it in its
    // hover_enter / hover_leave closures (to call back into the stack
    // with the right handle).
    let next_handle_id = overlay_stack()
        .lock()
        .ok()
        .map(|s| s.peek_next_handle_id())
        .unwrap_or(0);

    let stored_close = overlay_handle_state.clone();
    let next_id_for_card = next_handle_id;

    // max_w(320) + chrome (border, padding) and a generous-side
    // height that covers the documented profile-card layout
    // (avatar + multi-line bio + stats).
    let handle = OverlayBuilder::tooltip()
        .at(x, y)
        .size(360.0, 400.0)
        .anchor_direction(anchor_dir)
        // Tooltip kind already has on_mouse_leave=true. Use the configured
        // close_delay so user can move mouse from trigger to card without
        // the card popping closed.
        .dismissable_by_mouse_leave(true, close_delay_ms)
        .on_close(move |_reason| {
            stored_close.set(None);
        })
        .content(move || {
            let user_content = (content_fn)();
            let handle = OverlayHandle::from_raw(next_id_for_card);

            // Card-side hover handlers — keep card open when mouse moves
            // from trigger into card.
            div()
                .class("cn-hover-card-content")
                .flex_col()
                .bg(bg)
                .border(1.0, border)
                .rounded(radius)
                .lock_corner_shape()
                .p_px(padding)
                .shadow_lg()
                .min_w(200.0)
                .max_w(320.0)
                .child(user_content)
                .on_hover_enter(move |_| {
                    if let Ok(mut stack) = overlay_stack().lock() {
                        // CRITICAL: don't call `handle.is_exiting()` here —
                        // it would re-lock the same mutex we already hold,
                        // deadlocking on std::sync::Mutex. Inspect the
                        // stack's entries directly while we hold the lock.
                        let exiting = stack
                            .iter_bottom_up()
                            .any(|e| e.handle == handle && e.exiting);
                        if exiting {
                            stack.revive(handle);
                        } else {
                            stack.handle_mouse_enter(handle);
                        }
                    }
                })
                .on_hover_leave(move |_| {
                    if let Ok(mut stack) = overlay_stack().lock() {
                        stack.handle_mouse_leave(handle);
                    }
                })
        })
        .show();

    debug_assert_eq!(
        handle.raw(),
        next_handle_id,
        "peek_next_handle_id was stale — concurrent push?"
    );

    handle
}

/// Built hover card component
pub struct HoverCard {
    inner: Div,
}

impl std::ops::Deref for HoverCard {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for HoverCard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for HoverCardBuilder {
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

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.get_or_build().inner)
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().inner.layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().inner.element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().inner.element_id()
    }
}

impl ElementBuilder for HoverCard {
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

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }
}

/// Create a hover card component with a trigger
///
/// The hover card appears when the user hovers over the trigger element.
///
/// # Example
///
/// ```ignore
/// cn::hover_card(|| text("@username"))
///     .content(|| {
///         div().flex_col().gap(8.0).children([
///             text("John Doe").size(16.0),
///             text("Software Engineer").size(14.0),
///         ])
///     })
/// ```
#[track_caller]
pub fn hover_card<F>(trigger_fn: F) -> HoverCardBuilder
where
    F: Fn() -> Div + Send + Sync + 'static,
{
    // Create the key here so it captures the caller's location, not HoverCardBuilder's
    let key = InstanceKey::new("hover_card");
    HoverCardBuilder::with_key(trigger_fn, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hover_card_position_bottom() {
        let (x, y) = calculate_hover_card_position(
            100.0,
            50.0,
            80.0,
            30.0,
            HoverCardSide::Bottom,
            HoverCardAlign::Start,
            8.0,
        );
        assert_eq!(x, 100.0);
        assert_eq!(y, 50.0 + 30.0 + 8.0); // trigger_y + trigger_h + offset
    }

    #[test]
    fn test_hover_card_position_right() {
        let (x, y) = calculate_hover_card_position(
            100.0,
            50.0,
            80.0,
            30.0,
            HoverCardSide::Right,
            HoverCardAlign::Start,
            8.0,
        );
        assert_eq!(x, 100.0 + 80.0 + 8.0); // trigger_x + trigger_w + offset
        assert_eq!(y, 50.0);
    }
}
