//! Tooltip component - lightweight informational text on hover
//!
//! A styled tooltip that appears when hovering over a trigger element.
//! Designed for simple text labels, not rich content (use HoverCard for that).
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     // Basic tooltip
//!     cn::tooltip(|| cn::button("Hover me"))
//!         .text("This is a tooltip")
//!
//!     // Positioned to the right
//!     cn::tooltip(|| cn::button("Settings"))
//!         .text("Open settings panel")
//!         .side(TooltipSide::Right)
//!
//!     // With custom delays
//!     cn::tooltip(|| text("Help"))
//!         .text("Click for more info")
//!         .open_delay_ms(200)
//!         .close_delay_ms(0)
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
use blinc_layout::widgets::overlay_stack::{CloseReason, OverlayBuilder, OverlayHandle};
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

use blinc_layout::InstanceKey;

/// Side where the tooltip appears relative to the trigger
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TooltipSide {
    /// Above the trigger (default)
    #[default]
    Top,
    /// Below the trigger
    Bottom,
    /// To the right of the trigger
    Right,
    /// To the left of the trigger
    Left,
}

/// Alignment of the tooltip relative to the trigger
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TooltipAlign {
    /// Align to start of trigger
    Start,
    /// Center with trigger (default)
    #[default]
    Center,
    /// Align to end of trigger
    End,
}

/// Trigger builder function type for tooltip trigger
type TriggerBuilderFn = Arc<dyn Fn() -> Div + Send + Sync>;

/// Builder for tooltip component
pub struct TooltipBuilder {
    /// Trigger content (the element that triggers the tooltip)
    trigger: TriggerBuilderFn,
    /// Text to show in the tooltip
    text: Option<String>,
    /// Side where the tooltip appears
    side: TooltipSide,
    /// Alignment relative to trigger
    align: TooltipAlign,
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
    built: OnceCell<Tooltip>,
}

impl std::fmt::Debug for TooltipBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TooltipBuilder")
            .field("text", &self.text)
            .field("side", &self.side)
            .field("align", &self.align)
            .field("open_delay_ms", &self.open_delay_ms)
            .field("close_delay_ms", &self.close_delay_ms)
            .field("offset", &self.offset)
            .finish()
    }
}

impl TooltipBuilder {
    /// Create a new tooltip builder with a trigger builder function and a pre-created key
    pub fn with_key<F>(trigger_fn: F, key: InstanceKey) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        Self {
            trigger: Arc::new(trigger_fn),
            text: None,
            side: TooltipSide::Top,
            align: TooltipAlign::Center,
            open_delay_ms: 400, // Default 400ms delay before showing
            close_delay_ms: 0,  // Default 0ms delay - hide immediately
            offset: 6.0,
            key,
            classes: Vec::new(),
            user_id: None,
            built: OnceCell::new(),
        }
    }

    /// Set the text to display in the tooltip
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Set the side where the tooltip appears
    pub fn side(mut self, side: TooltipSide) -> Self {
        self.side = side;
        self
    }

    /// Set the alignment relative to the trigger
    pub fn align(mut self, align: TooltipAlign) -> Self {
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
    fn get_or_build(&self) -> &Tooltip {
        ::blinc_layout::build_once::build_once(&self.built, || self.build_component())
    }

    /// Build the tooltip component
    fn build_component(&self) -> Tooltip {
        // Per-instance handle state — survives rebuilds because it's keyed by
        // the InstanceKey. Stores the raw u64 of an active OverlayHandle.
        let overlay_handle_state: State<Option<u64>> =
            BlincContextState::get().use_state_keyed(&self.key.derive("handle"), || None);

        let side = self.side;
        let align = self.align;
        let offset = self.offset;
        let close_delay_ms = self.close_delay_ms;
        let tooltip_text = self.text.clone();
        let trigger_builder = self.trigger.clone();

        let stored_for_enter = overlay_handle_state.clone();
        let stored_for_leave = overlay_handle_state.clone();
        let stored_for_open = overlay_handle_state.clone();
        let stored_for_close = overlay_handle_state.clone();

        let trigger_content = (trigger_builder)();

        let trigger = div()
            // `w_fit()` already keeps this from stretching. Naming
            // `align_self_start()` on top made the value look authored,
            // so a row asking for `align-items: center` was ignored and
            // the trigger hung from the top against taller siblings.
            .w_fit()
            .child(trigger_content)
            .on_hover_enter(move |ctx| {
                // Existing tooltip alive? Cancel any pending mouse-leave close
                // and bail — no need to push a fresh entry.
                if let Some(raw) = stored_for_enter.get() {
                    let handle = OverlayHandle::from_raw(raw);
                    if handle.is_live() {
                        if let Ok(mut stack) = overlay_stack().lock() {
                            stack.handle_mouse_enter(handle);
                        }
                        return;
                    }
                    // Stale / exiting — drop the reference and open fresh.
                    stored_for_enter.set(None);
                }

                // Compute anchor in viewport coords from the trigger's bounds.
                let (x, y) = calculate_tooltip_position(
                    ctx.bounds_x,
                    ctx.bounds_y,
                    ctx.bounds_width,
                    ctx.bounds_height,
                    side,
                    align,
                    offset,
                );

                let Some(ref text_str) = tooltip_text else {
                    return;
                };
                let text_owned = text_str.clone();
                let theme = ThemeState::get();
                let bg = theme.color(ColorToken::TooltipBackground);
                let fg = theme.color(ColorToken::TooltipText);
                let radius = theme.radius(RadiusToken::Sm);
                let font_size = theme.typography().text_xs;
                let anchor_dir = match side {
                    TooltipSide::Top => AnchorDirection::Top,
                    TooltipSide::Bottom => AnchorDirection::Bottom,
                    TooltipSide::Left => AnchorDirection::Left,
                    TooltipSide::Right => AnchorDirection::Right,
                };

                // No global `close_all_of_kind(Tooltip)` here — see the
                // matching comment in `build_hover_card_overlay`. Each
                // on_close it fired would State::set(None), which the
                // reactive system treats as a global rebuild trigger.
                // Rapid hover across many tooltipped elements then
                // cascaded into many full UI rebuilds. We rely on
                // per-widget handle tracking instead; the trade-off is
                // that two tooltips can be visible briefly when the user
                // moves between them faster than the close-delay.

                let stored_close = stored_for_close.clone();
                let stored_open = stored_for_open.clone();
                // Tooltips are single-line `no_wrap`; the cap of
                // ~40 chars at text_xs covers the documented use
                // case (lightweight informational labels). Hosts
                // that pass longer strings will simply paint
                // wider than the hint — the size only feeds the
                // viewport clamp.
                let handle = OverlayBuilder::tooltip()
                    .at(x, y)
                    .size(280.0, 32.0)
                    .anchor_direction(anchor_dir)
                    .dismissable_by_mouse_leave(true, close_delay_ms)
                    // No motion_enter/_exit on the builder. Enter is driven by
                    // the CSS `@keyframes cn-tooltip-enter` rule on
                    // `.cn-tooltip` (defined in cn_styles.rs). Exit is instant
                    // for now — tooltips snap-close. See OVERLAY_STACK_DESIGN.md
                    // "explicit non-feature: no overlay-owned animation system"
                    // for the rationale.
                    .on_close(move |_reason| {
                        stored_close.set(None);
                    })
                    .content(move || {
                        div()
                            .class("cn-tooltip")
                            .flex_row()
                            .items_center()
                            .bg(bg)
                            .rounded(radius)
                            .lock_corner_shape()
                            .shadow_sm()
                            .child(text(&text_owned).size(font_size).color(fg).no_wrap())
                    })
                    .show();
                stored_open.set(Some(handle.raw()));
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

        Tooltip { inner }
    }
}

/// Calculate position for tooltip based on trigger bounds
fn calculate_tooltip_position(
    trigger_x: f32,
    trigger_y: f32,
    trigger_w: f32,
    trigger_h: f32,
    side: TooltipSide,
    align: TooltipAlign,
    offset: f32,
) -> (f32, f32) {
    // Estimate tooltip width for alignment calculations
    // Tooltips are typically small, so use a smaller estimate than hover cards
    let tooltip_width_estimate = 100.0;

    match side {
        TooltipSide::Top => {
            // Position above trigger - use trigger_y - offset as the bottom anchor point
            // The overlay content will be positioned to align its bottom edge here
            let y = trigger_y - (offset * 6.0);
            let x = match align {
                TooltipAlign::Start => trigger_x,
                TooltipAlign::Center => trigger_x + (trigger_w - tooltip_width_estimate) / 2.0,
                TooltipAlign::End => trigger_x + trigger_w - tooltip_width_estimate,
            };
            (x.max(0.0), y.max(0.0))
        }
        TooltipSide::Bottom => {
            // Position below trigger
            let y = trigger_y + trigger_h + offset;
            let x = match align {
                TooltipAlign::Start => trigger_x,
                TooltipAlign::Center => trigger_x + (trigger_w - tooltip_width_estimate) / 2.0,
                TooltipAlign::End => trigger_x + trigger_w - tooltip_width_estimate,
            };
            (x.max(0.0), y)
        }
        TooltipSide::Right => {
            // Position to the right of trigger
            let x = trigger_x + trigger_w + offset;
            let y = match align {
                TooltipAlign::Start => trigger_y,
                TooltipAlign::Center => trigger_y,
                TooltipAlign::End => trigger_y,
            };
            (x, y)
        }
        TooltipSide::Left => {
            // Position to the left of trigger
            let x = trigger_x - tooltip_width_estimate - offset;
            let y = match align {
                TooltipAlign::Start => trigger_y,
                TooltipAlign::Center => trigger_y,
                TooltipAlign::End => trigger_y,
            };
            (x.max(0.0), y)
        }
    }
}

/// Built tooltip component
pub struct Tooltip {
    inner: Div,
}

impl std::ops::Deref for Tooltip {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for Tooltip {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for TooltipBuilder {
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

impl ElementBuilder for Tooltip {
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

/// Create a tooltip component with a trigger
///
/// The tooltip appears when the user hovers over the trigger element.
///
/// # Example
///
/// ```ignore
/// cn::tooltip(|| cn::button("Hover me"))
///     .text("This is a helpful tooltip")
/// ```
#[track_caller]
pub fn tooltip<F>(trigger_fn: F) -> TooltipBuilder
where
    F: Fn() -> Div + Send + Sync + 'static,
{
    // Create the key here so it captures the caller's location, not TooltipBuilder's
    let key = InstanceKey::new("tooltip");
    TooltipBuilder::with_key(trigger_fn, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tooltip_position_top() {
        let (x, y) = calculate_tooltip_position(
            100.0,
            50.0,
            80.0,
            30.0,
            TooltipSide::Top,
            TooltipAlign::Center,
            6.0,
        );
        // y uses offset * 6.0 multiplier for top positioning
        assert_eq!(y, 50.0 - (6.0 * 6.0)); // 14.0
        // x should be centered (tooltip_width_estimate = 100.0)
        assert_eq!(x, 100.0 + (80.0 - 100.0) / 2.0); // 90.0
    }

    #[test]
    fn test_tooltip_position_bottom() {
        let (x, y) = calculate_tooltip_position(
            100.0,
            50.0,
            80.0,
            30.0,
            TooltipSide::Bottom,
            TooltipAlign::Start,
            6.0,
        );
        assert_eq!(x, 100.0);
        assert_eq!(y, 50.0 + 30.0 + 6.0); // trigger_y + trigger_h + offset
    }

    #[test]
    fn test_tooltip_position_right() {
        let (x, y) = calculate_tooltip_position(
            100.0,
            50.0,
            80.0,
            30.0,
            TooltipSide::Right,
            TooltipAlign::Center,
            6.0,
        );
        assert_eq!(x, 100.0 + 80.0 + 6.0); // trigger_x + trigger_w + offset
        assert_eq!(y, 50.0); // y aligns with trigger_y for right side
    }
}
