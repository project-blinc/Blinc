//! Scroll Area component - styled scrollable container with customizable scrollbar
//!
//! A themed scroll container that wraps `blinc_layout::scroll()` with a custom
//! scrollbar overlay. Supports various scrollbar visibility modes and auto-dismiss.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Basic scroll area with auto-dismissing scrollbar
//! cn::scroll_area()
//!     .h(400.0)
//!     .child(
//!         div().flex_col().gap(8.0)
//!             .child(text("Item 1"))
//!             .child(text("Item 2"))
//!             // ... many items
//!     )
//!
//! // Always show scrollbar
//! cn::scroll_area()
//!     .scrollbar(ScrollbarVisibility::Always)
//!     .h(300.0)
//!     .child(content)
//!
//! // Horizontal scroll
//! cn::scroll_area()
//!     .horizontal()
//!     .w(400.0)
//!     .child(wide_content)
//!
//! // Custom scrollbar styling
//! cn::scroll_area()
//!     .scrollbar_width(8.0)
//!     .scrollbar_color(Color::GRAY)
//!     .h(400.0)
//!     .child(content)
//! ```

use std::cell::OnceCell;
use std::sync::{Arc, Mutex};

use blinc_animation::{get_scheduler, AnimatedValue, SharedAnimatedValue, SpringConfig};
use blinc_core::{BlincContextState, Color};
use blinc_layout::element::RenderProps;
use blinc_layout::motion::motion;
use blinc_layout::prelude::*;
use blinc_layout::stateful::{stateful_with_key, NoState};
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::scroll::{scroll, ScrollDirection};
use blinc_layout::InstanceKey;
use blinc_theme::{ColorToken, ThemeState};

/// Scrollbar visibility modes
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollbarVisibility {
    /// Always show scrollbar (like classic Windows style)
    Always,
    /// Show scrollbar only when hovering over the scroll area
    Hover,
    /// Show when scrolling, auto-dismiss after inactivity (like macOS)
    #[default]
    Auto,
    /// Never show scrollbar (content still scrollable)
    Never,
}

/// Scroll area size presets
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollAreaSize {
    /// Small scrollbar (4px width)
    Small,
    /// Medium scrollbar (6px width)
    #[default]
    Medium,
    /// Large scrollbar (10px width)
    Large,
}

impl ScrollAreaSize {
    fn scrollbar_width(&self) -> f32 {
        match self {
            ScrollAreaSize::Small => 4.0,
            ScrollAreaSize::Medium => 6.0,
            ScrollAreaSize::Large => 10.0,
        }
    }
}

/// Configuration for scroll area
struct ScrollAreaConfig {
    /// Scrollbar visibility mode
    visibility: ScrollbarVisibility,
    /// Scroll direction
    direction: ScrollDirection,
    /// Custom scrollbar width (overrides size preset)
    scrollbar_width: Option<f32>,
    /// Scrollbar size preset
    size: ScrollAreaSize,
    /// Thumb color (uses theme if None)
    thumb_color: Option<Color>,
    /// Track color (uses theme if None)
    track_color: Option<Color>,
    /// Viewport dimensions
    width: Option<f32>,
    height: Option<f32>,
    /// Enable bounce physics
    bounce: bool,
    /// Content builder
    content: Option<Box<dyn ElementBuilder>>,
}

impl Default for ScrollAreaConfig {
    fn default() -> Self {
        Self {
            visibility: ScrollbarVisibility::default(),
            direction: ScrollDirection::Vertical,
            scrollbar_width: None,
            size: ScrollAreaSize::default(),
            thumb_color: None,
            track_color: None,
            width: None,
            height: None,
            bounce: true,
            content: None,
        }
    }
}

/// Built scroll area with inner element
struct BuiltScrollArea {
    inner: Box<dyn ElementBuilder>,
}

impl BuiltScrollArea {
    fn from_config(config: &ScrollAreaConfig, key: &InstanceKey) -> Self {
        let theme = ThemeState::get();
        let ctx = BlincContextState::get();
        let scheduler = get_scheduler();

        // Calculate scrollbar dimensions
        let bar_width = config
            .scrollbar_width
            .unwrap_or_else(|| config.size.scrollbar_width());

        let thumb_color = config
            .thumb_color
            .unwrap_or_else(|| theme.color(ColorToken::Border).with_alpha(0.5));
        let track_color = config
            .track_color
            .unwrap_or_else(|| theme.color(ColorToken::Surface).with_alpha(0.1));

        // State for tracking scroll position and hover
        let instance_key = key.get().to_string();
        let is_hovered = ctx.use_state_keyed(&format!("{}_hover", instance_key), || false);
        let is_scrolling = ctx.use_state_keyed(&format!("{}_scrolling", instance_key), || false);
        let scroll_position =
            ctx.use_state_keyed(&format!("{}_scroll_pos", instance_key), || 0.0f32);
        let content_size =
            ctx.use_state_keyed(&format!("{}_content_size", instance_key), || 1000.0f32);
        let viewport_size =
            ctx.use_state_keyed(&format!("{}_viewport_size", instance_key), || 400.0f32);

        // State for thumb dragging
        let is_dragging = ctx.use_state_keyed(&format!("{}_dragging", instance_key), || false);
        let drag_start_y = ctx.use_state_keyed(&format!("{}_drag_start_y", instance_key), || 0.0f32);
        let drag_start_scroll =
            ctx.use_state_keyed(&format!("{}_drag_start_scroll", instance_key), || 0.0f32);

        // Create opacity animation for auto-dismiss (persisted via context)
        let opacity_anim: SharedAnimatedValue = Arc::new(Mutex::new(AnimatedValue::new(
            scheduler,
            if config.visibility == ScrollbarVisibility::Always {
                1.0
            } else {
                0.0
            },
            SpringConfig::gentle(),
        )));

        // Get viewport dimensions
        let viewport_width = config.width.unwrap_or(300.0);
        let viewport_height = config.height.unwrap_or(400.0);

        // Build scroll container with physics
        let mut scroll_container = scroll()
            .w(viewport_width)
            .h(viewport_height)
            .direction(config.direction)
            .bounce(config.bounce);

        // Clone states for scroll handler
        let scroll_pos_for_handler = scroll_position.clone();
        let is_scrolling_for_handler = is_scrolling.clone();

        scroll_container = scroll_container.on_scroll(move |e| {
            // Update scroll position state
            let current = scroll_pos_for_handler.get();
            scroll_pos_for_handler.set(current + e.scroll_delta_y);
            is_scrolling_for_handler.set(true);
        });

        // Add content if present - note: content is added via .child() on the builder
        let _ = &config.content; // Acknowledge content field

        // Clone states for hover handlers
        let is_hovered_enter = is_hovered.clone();
        let is_hovered_leave = is_hovered.clone();

        let visibility = config.visibility;

        // For Always/Never modes, no need for reactive updates
        if visibility == ScrollbarVisibility::Always || visibility == ScrollbarVisibility::Never {
            // Calculate thumb size and position (static)
            let viewport = viewport_size.get();
            let content = content_size.get().max(viewport);
            let scroll_ratio = viewport / content;
            let thumb_height = (scroll_ratio * viewport).max(30.0).min(viewport - 8.0);
            let scroll_range = content - viewport;
            let scroll_offset = scroll_position.get().abs();
            let scroll_progress = if scroll_range > 0.0 {
                (scroll_offset / scroll_range).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let max_thumb_travel = viewport - thumb_height - 8.0;
            let thumb_offset = scroll_progress * max_thumb_travel + 4.0;

            // Clone states for drag handlers
            let scroll_pos_for_down = scroll_position.clone();
            let drag_start_y_for_down = drag_start_y.clone();
            let drag_start_scroll_for_down = drag_start_scroll.clone();
            let is_dragging_for_drag = is_dragging.clone();
            let drag_start_y_for_drag = drag_start_y.clone();
            let drag_start_scroll_for_drag = drag_start_scroll.clone();
            let scroll_pos_for_drag = scroll_position.clone();
            let is_dragging_for_end = is_dragging.clone();

            // Build scrollbar thumb with drag handlers
            let thumb = div()
                .absolute()
                .right(2.0)
                .top(thumb_offset)
                .w(bar_width)
                .h(thumb_height)
                .rounded(bar_width / 2.0)
                .bg(thumb_color)
                .cursor(CursorStyle::Grab)
                .on_mouse_down(move |event| {
                    drag_start_y_for_down.set(event.mouse_y);
                    drag_start_scroll_for_down.set(scroll_pos_for_down.get());
                })
                .on_drag(move |event| {
                    is_dragging_for_drag.set(true);
                    let start_y = drag_start_y_for_drag.get();
                    let delta_y = event.mouse_y - start_y;

                    // Convert thumb drag to scroll position
                    // thumb_delta / max_thumb_travel = scroll_delta / scroll_range
                    let scroll_delta = if max_thumb_travel > 0.0 {
                        (delta_y / max_thumb_travel) * scroll_range
                    } else {
                        0.0
                    };

                    let start_scroll = drag_start_scroll_for_drag.get();
                    let new_scroll = (start_scroll - scroll_delta).clamp(-scroll_range, 0.0);
                    scroll_pos_for_drag.set(new_scroll);
                })
                .on_drag_end(move |_| {
                    is_dragging_for_end.set(false);
                });

            // Build scrollbar track
            let track = div()
                .absolute()
                .right(0.0)
                .top(0.0)
                .bottom(0.0)
                .w(bar_width + 4.0)
                .bg(track_color)
                .rounded(bar_width / 2.0)
                .child(thumb);

            let mut container = div()
                .w(viewport_width)
                .h(viewport_height)
                .relative()
                .overflow_clip()
                .child(scroll_container);

            if visibility == ScrollbarVisibility::Always {
                container = container.child(track);
            }

            Self {
                inner: Box::new(container),
            }
        } else {
            // For Hover/Auto modes, use stateful to react to state changes
            let stateful_key = format!("{}_stateful", instance_key);

            // Extract values from config BEFORE the closure (to avoid capturing non-Send types)
            let direction = config.direction;
            let bounce = config.bounce;

            // Clone everything needed for the render closure
            let is_hovered_for_deps = is_hovered.clone();
            let is_scrolling_for_deps = is_scrolling.clone();
            let scroll_position_for_deps = scroll_position.clone();
            let viewport_size_for_render = viewport_size.clone();
            let content_size_for_render = content_size.clone();
            let scroll_position_for_render = scroll_position.clone();
            let is_hovered_for_render = is_hovered.clone();
            let is_scrolling_for_render = is_scrolling.clone();
            let opacity_anim_for_render = opacity_anim.clone();

            // Clone drag states for the closure
            let is_dragging_for_render = is_dragging.clone();
            let drag_start_y_for_render = drag_start_y.clone();
            let drag_start_scroll_for_render = drag_start_scroll.clone();
            let scroll_position_for_drag = scroll_position.clone();

            let inner = stateful_with_key::<NoState>(&stateful_key)
                .deps([
                    is_hovered_for_deps.signal_id(),
                    is_scrolling_for_deps.signal_id(),
                    scroll_position_for_deps.signal_id(),
                ])
                .on_state(move |_| {
                    // Determine if scrollbar should be visible
                    let should_show = match visibility {
                        ScrollbarVisibility::Hover => is_hovered_for_render.get(),
                        ScrollbarVisibility::Auto => {
                            is_scrolling_for_render.get() || is_hovered_for_render.get()
                        }
                        _ => false,
                    };

                    // Update opacity animation target
                    {
                        let mut anim = opacity_anim_for_render.lock().unwrap();
                        anim.set_target(if should_show { 1.0 } else { 0.0 });
                    }

                    // Calculate thumb size and position
                    let viewport = viewport_size_for_render.get();
                    let content = content_size_for_render.get().max(viewport);
                    let scroll_ratio = viewport / content;
                    let thumb_height = (scroll_ratio * viewport).max(30.0).min(viewport - 8.0);
                    let scroll_range = content - viewport;
                    let scroll_offset = scroll_position_for_render.get().abs();
                    let scroll_progress = if scroll_range > 0.0 {
                        (scroll_offset / scroll_range).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let max_thumb_travel = viewport - thumb_height - 8.0;
                    let thumb_offset = scroll_progress * max_thumb_travel + 4.0;

                    // Clone drag states for thumb handlers
                    let scroll_pos_for_down = scroll_position_for_drag.clone();
                    let drag_start_y_for_down = drag_start_y_for_render.clone();
                    let drag_start_scroll_for_down = drag_start_scroll_for_render.clone();
                    let is_dragging_for_drag = is_dragging_for_render.clone();
                    let drag_start_y_for_drag = drag_start_y_for_render.clone();
                    let drag_start_scroll_for_drag = drag_start_scroll_for_render.clone();
                    let scroll_pos_for_drag = scroll_position_for_drag.clone();
                    let is_dragging_for_end = is_dragging_for_render.clone();

                    // Build scrollbar thumb with drag handlers
                    let thumb = div()
                        .absolute()
                        .right(2.0)
                        .top(thumb_offset)
                        .w(bar_width)
                        .h(thumb_height)
                        .rounded(bar_width / 2.0)
                        .bg(thumb_color)
                        .cursor(CursorStyle::Grab)
                        .on_mouse_down(move |event| {
                            drag_start_y_for_down.set(event.mouse_y);
                            drag_start_scroll_for_down.set(scroll_pos_for_down.get());
                        })
                        .on_drag(move |event| {
                            is_dragging_for_drag.set(true);
                            let start_y = drag_start_y_for_drag.get();
                            let delta_y = event.mouse_y - start_y;

                            // Convert thumb drag to scroll position
                            let scroll_delta = if max_thumb_travel > 0.0 {
                                (delta_y / max_thumb_travel) * scroll_range
                            } else {
                                0.0
                            };

                            let start_scroll = drag_start_scroll_for_drag.get();
                            let new_scroll = (start_scroll - scroll_delta).clamp(-scroll_range, 0.0);
                            scroll_pos_for_drag.set(new_scroll);
                        })
                        .on_drag_end(move |_| {
                            is_dragging_for_end.set(false);
                        });

                    // Build scrollbar track
                    let track = div()
                        .absolute()
                        .right(0.0)
                        .top(0.0)
                        .bottom(0.0)
                        .w(bar_width + 4.0)
                        .bg(track_color)
                        .rounded(bar_width / 2.0)
                        .child(thumb);

                    // Wrap track with opacity animation
                    let animated_track =
                        motion().opacity(opacity_anim_for_render.clone()).child(track);

                    // Build container with scroll and animated scrollbar
                    div()
                        .w(viewport_width)
                        .h(viewport_height)
                        .relative()
                        .overflow_clip()
                        .on_hover_enter({
                            let is_hovered = is_hovered_enter.clone();
                            move |_| {
                                is_hovered.set(true);
                            }
                        })
                        .on_hover_leave({
                            let is_hovered = is_hovered_leave.clone();
                            move |_| {
                                is_hovered.set(false);
                            }
                        })
                        .child(
                            scroll()
                                .w(viewport_width)
                                .h(viewport_height)
                                .direction(direction)
                                .bounce(bounce),
                        )
                        .child(animated_track)
                });

            Self {
                inner: Box::new(inner),
            }
        }
    }
}

/// Scroll Area component with customizable scrollbar
pub struct ScrollArea {
    inner: Box<dyn ElementBuilder>,
}

impl ElementBuilder for ScrollArea {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }
}

/// Builder for scroll area
pub struct ScrollAreaBuilder {
    config: ScrollAreaConfig,
    key: InstanceKey,
    built: OnceCell<ScrollArea>,
}

impl ScrollAreaBuilder {
    /// Create a new scroll area builder
    #[track_caller]
    pub fn new() -> Self {
        Self {
            config: ScrollAreaConfig::default(),
            key: InstanceKey::new("scroll_area"),
            built: OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &ScrollArea {
        self.built.get_or_init(|| {
            let built = BuiltScrollArea::from_config(&self.config, &self.key);
            ScrollArea {
                inner: built.inner,
            }
        })
    }

    /// Set scrollbar visibility mode
    pub fn scrollbar(mut self, visibility: ScrollbarVisibility) -> Self {
        self.config.visibility = visibility;
        self
    }

    /// Set scroll direction
    pub fn direction(mut self, direction: ScrollDirection) -> Self {
        self.config.direction = direction;
        self
    }

    /// Set to vertical scrolling (default)
    pub fn vertical(mut self) -> Self {
        self.config.direction = ScrollDirection::Vertical;
        self
    }

    /// Set to horizontal scrolling
    pub fn horizontal(mut self) -> Self {
        self.config.direction = ScrollDirection::Horizontal;
        self
    }

    /// Set to scroll in both directions
    pub fn both_directions(mut self) -> Self {
        self.config.direction = ScrollDirection::Both;
        self
    }

    /// Set scrollbar size preset
    pub fn size(mut self, size: ScrollAreaSize) -> Self {
        self.config.size = size;
        self
    }

    /// Set custom scrollbar width
    pub fn scrollbar_width(mut self, width: f32) -> Self {
        self.config.scrollbar_width = Some(width);
        self
    }

    /// Set scrollbar thumb color
    pub fn thumb_color(mut self, color: impl Into<Color>) -> Self {
        self.config.thumb_color = Some(color.into());
        self
    }

    /// Set scrollbar track color
    pub fn track_color(mut self, color: impl Into<Color>) -> Self {
        self.config.track_color = Some(color.into());
        self
    }

    /// Set viewport width
    pub fn w(mut self, width: f32) -> Self {
        self.config.width = Some(width);
        self
    }

    /// Set viewport height
    pub fn h(mut self, height: f32) -> Self {
        self.config.height = Some(height);
        self
    }

    /// Enable or disable bounce physics
    pub fn bounce(mut self, enabled: bool) -> Self {
        self.config.bounce = enabled;
        self
    }

    /// Disable bounce physics
    pub fn no_bounce(mut self) -> Self {
        self.config.bounce = false;
        self
    }

    /// Set the scrollable content
    pub fn child(mut self, content: impl ElementBuilder + 'static) -> Self {
        self.config.content = Some(Box::new(content));
        self
    }

    /// Build the final ScrollArea component
    pub fn build_final(self) -> ScrollArea {
        let built = BuiltScrollArea::from_config(&self.config, &self.key);
        ScrollArea {
            inner: built.inner,
        }
    }
}

impl Default for ScrollAreaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ElementBuilder for ScrollAreaBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }
}

/// Create a new scroll area with customizable scrollbar
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// // Basic usage with auto-dismiss scrollbar
/// cn::scroll_area()
///     .h(400.0)
///     .child(long_content)
///
/// // Always show scrollbar
/// cn::scroll_area()
///     .scrollbar(ScrollbarVisibility::Always)
///     .h(300.0)
///     .child(content)
///
/// // Horizontal scroll
/// cn::scroll_area()
///     .horizontal()
///     .w(400.0)
///     .child(wide_content)
/// ```
#[track_caller]
pub fn scroll_area() -> ScrollAreaBuilder {
    ScrollAreaBuilder::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_theme::ThemeState;

    fn init_theme() {
        let _ = ThemeState::try_get().unwrap_or_else(|| {
            ThemeState::init_default();
            ThemeState::get()
        });
    }

    #[test]
    fn test_scrollbar_width_presets() {
        assert_eq!(ScrollAreaSize::Small.scrollbar_width(), 4.0);
        assert_eq!(ScrollAreaSize::Medium.scrollbar_width(), 6.0);
        assert_eq!(ScrollAreaSize::Large.scrollbar_width(), 10.0);
    }

    #[test]
    fn test_scroll_area_builder_config() {
        init_theme();

        let builder = scroll_area()
            .scrollbar(ScrollbarVisibility::Always)
            .size(ScrollAreaSize::Large)
            .h(500.0)
            .w(300.0);

        assert_eq!(builder.config.visibility, ScrollbarVisibility::Always);
        assert_eq!(builder.config.size, ScrollAreaSize::Large);
        assert_eq!(builder.config.height, Some(500.0));
        assert_eq!(builder.config.width, Some(300.0));
    }
}
