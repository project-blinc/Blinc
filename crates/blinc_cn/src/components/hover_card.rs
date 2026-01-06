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

use blinc_animation::AnimationPreset;
use blinc_core::context_state::BlincContextState;
use blinc_core::State;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::motion::motion;
use blinc_layout::overlay_state::get_overlay_manager;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::overlay::{OverlayHandle, OverlayManagerExt};
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

    /// Get or build the component
    fn get_or_build(&self) -> &HoverCard {
        self.built.get_or_init(|| self.build_component())
    }

    /// Build the hover card component
    fn build_component(&self) -> HoverCard {
        let _theme = ThemeState::get();

        // Create state for tracking overlay handle
        let overlay_handle_state: State<Option<u64>> =
            BlincContextState::get().use_state_keyed(&self.key.derive("handle"), || None);

        // Clone values for closures
        let side = self.side;
        let align = self.align;
        let offset = self.offset;
        let content_builder = self.content.clone();
        let trigger_builder = self.trigger.clone();
        // Use the instance key to create a unique motion key for this hover card
        // This prevents collisions when multiple hover cards exist
        let motion_key_str = format!("hovercard_{}", self.key.get());

        // Build trigger with hover handlers
        let overlay_handle_for_show = overlay_handle_state.clone();
        let overlay_handle_for_trigger_leave = overlay_handle_state.clone();
        let overlay_handle_for_trigger_enter = overlay_handle_state.clone();
        let content_builder_for_show = content_builder.clone();
        let motion_key_for_trigger = motion_key_str.clone();

        // Build the trigger element with hover detection
        let trigger_content = (trigger_builder)();

        let trigger = div()
            .w_fit()
            .align_self_start() // Prevent stretching in flex containers
            .child(trigger_content)
            .on_hover_enter(move |ctx| {
                // Build the full motion key (motion_derived adds "motion:" prefix)
                // The actual animation is on the child, so we need ":child:0" suffix
                let full_motion_key = format!("motion:{}:child:0", motion_key_for_trigger);

                // First, check if we have an existing overlay that's pending close or closing
                if let Some(handle_id) = overlay_handle_for_trigger_enter.get() {
                    let mgr = get_overlay_manager();
                    let handle = OverlayHandle::from_raw(handle_id);

                    // If overlay is visible, cancel any pending close
                    if mgr.is_visible(handle) {
                        if mgr.is_pending_close(handle) {
                            // Cancel pending close - mouse re-entered trigger
                            mgr.hover_enter(handle);
                        }
                        // Also cancel any exit animation
                        let motion =
                            blinc_layout::selector::query_motion(&full_motion_key);
                        if motion.is_exiting() {
                            mgr.cancel_close(handle);
                            motion.cancel_exit();
                        }
                        return;
                    }
                    // Our overlay was closed externally, clear our state
                    overlay_handle_for_trigger_enter.set(None);
                }

                // Check if motion is already animating (entering) to prevent restart jitter
                let motion = blinc_layout::selector::query_motion(&full_motion_key);
                if motion.is_animating() && !motion.is_exiting() {
                    return;
                }

                // Get bounds for positioning
                let trigger_x = ctx.bounds_x;
                let trigger_y = ctx.bounds_y;
                let trigger_w = ctx.bounds_width;
                let trigger_h = ctx.bounds_height;

                // Calculate position based on side and alignment
                let (x, y) = calculate_hover_card_position(
                    trigger_x, trigger_y, trigger_w, trigger_h, side, align, offset,
                );

                // Show the hover card content
                if let Some(ref content_fn) = content_builder_for_show {
                    let content_fn_clone = Arc::clone(content_fn);
                    let overlay_handle_for_content = overlay_handle_for_show.clone();

                    let handle = show_hover_card_overlay(
                        x,
                        y,
                        side,
                        content_fn_clone,
                        overlay_handle_for_content,
                        motion_key_for_trigger.clone(),
                    );

                    overlay_handle_for_show.set(Some(handle.id()));
                }
            })
            .on_hover_leave(move |_| {
                // Start close delay countdown when mouse leaves trigger
                // The countdown can be canceled if mouse enters the card
                if let Some(handle_id) = overlay_handle_for_trigger_leave.get() {
                    let mgr = get_overlay_manager();
                    let handle = OverlayHandle::from_raw(handle_id);

                    // Only start close delay if overlay is visible and in Open state
                    if mgr.is_visible(handle) && !mgr.is_pending_close(handle) {
                        // Start close delay countdown (Open -> PendingClose)
                        mgr.hover_leave(handle);
                    }
                }
            });

        HoverCard { inner: trigger }
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
            let y = trigger_y - (offset  * 8.0);
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

/// Show the hover card overlay
fn show_hover_card_overlay(
    x: f32,
    y: f32,
    side: HoverCardSide,
    content_fn: ContentBuilderFn,
    overlay_handle_state: State<Option<u64>>,
    motion_key: String,
) -> OverlayHandle {
    let theme = ThemeState::get();
    let bg = theme.color(ColorToken::SurfaceElevated);
    let border = theme.color(ColorToken::Border);
    let radius = theme.radius(RadiusToken::Lg);
    let padding = theme.spacing_value(SpacingToken::Space4);

    let mgr = get_overlay_manager();

    // Close any existing tooltip/hover card overlays before opening a new one
    // This ensures only one hover card is visible at a time
    mgr.close_all_of(blinc_layout::widgets::overlay::OverlayKind::Tooltip);

    // Clone state and key for closures
    let overlay_handle_for_leave = overlay_handle_state.clone();
    let motion_key_for_content = motion_key.clone();
    let motion_key_for_hover = motion_key.clone();

    // Use hover_card() which is a TRANSIENT overlay - no backdrop, no scroll blocking
    // Multiple hover cards can coexist without interfering with each other
    // Set the motion_key so overlay can trigger exit animation when closing
    // The actual animation is on the child of motion_derived, so include ":child:0" suffix
    let motion_key_with_child = format!("{}:child:0", motion_key);
    mgr.hover_card()
        .at(x, y)
        .motion_key(&motion_key_with_child)
        .content(move || {
            let user_content = (content_fn)();

            // Clone for the hover enter closure (to cancel closing when mouse enters card)
            let overlay_handle_for_card_enter = overlay_handle_for_leave.clone();
            // Clone the motion key for use inside the on_hover_enter closure
            let motion_key_for_card_enter = motion_key_for_hover.clone();

            // Styled card container with hover enter detection
            // When mouse enters the card (after leaving trigger), cancel any pending close
            // This prevents the "blip" when moving mouse from trigger to card
            let card = div()
                .flex_col()
                .bg(bg)
                .border(1.0, border)
                .rounded(radius)
                .p_px(padding)
                .shadow_lg()
                .min_w(200.0)
                .max_w(320.0)
                .child(user_content)
                .on_hover_enter(move |_| {
                    // When mouse enters the card, cancel any pending close delay
                    // This handles the case where user moves mouse from trigger to card
                    if let Some(handle_id) = overlay_handle_for_card_enter.get() {
                        let mgr = get_overlay_manager();
                        let handle = OverlayHandle::from_raw(handle_id);

                        // Check if we're in PendingClose state (delay countdown running)
                        if mgr.is_pending_close(handle) {
                            // Cancel the delay - transitions PendingClose -> Open
                            mgr.hover_enter(handle);
                        }

                        // Also check if we're in Closing state (exit animation playing)
                        // Use the unique motion key for this hover card
                        // The actual animation is on the child, so we need ":child:0" suffix
                        let full_motion_key = format!("motion:{}:child:0", motion_key_for_card_enter);
                        let motion = blinc_layout::selector::query_motion(&full_motion_key);
                        if motion.is_exiting() {
                            // Cancel close - transitions Closing -> Open
                            mgr.cancel_close(handle);
                            // Cancel the motion's exit animation
                            motion.cancel_exit();
                        }
                    }
                });

            // Wrap in motion for enter/exit animations
            // Use motion_derived with unique key so animation state persists across rebuilds
            // and doesn't collide with other hover cards
            div().child(
                blinc_layout::motion::motion_derived(&motion_key_for_content)
                    .enter_animation(AnimationPreset::grow_in(150))
                    .exit_animation(AnimationPreset::grow_out(150))
                    .child(card),
            )
        })
        .on_close({
            let overlay_handle = overlay_handle_state.clone();
            move || {
                overlay_handle.set(None);
            }
        })
        .show()
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
