//! Spinner component for loading indicators
//!
//! A circular loading indicator that spins continuously using canvas animation.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//! use blinc_animation::AnimationContextExt;
//!
//! // Create a spinning loader
//! fn loading_view(ctx: &impl AnimationContext) -> impl ElementBuilder {
//!     let timeline = ctx.use_animated_timeline();
//!     cn::spinner(timeline)
//! }
//!
//! // Custom size and colors
//! fn custom_spinner(ctx: &impl AnimationContext) -> impl ElementBuilder {
//!     let timeline = ctx.use_animated_timeline();
//!     cn::spinner(timeline)
//!         .size(SpinnerSize::Large)
//!         .color(Color::BLUE)
//!         .track_color(Color::rgba(0.0, 0.0, 1.0, 0.2))
//! }
//!
//! // Custom rotation duration (slower spin)
//! fn slow_spinner(ctx: &impl AnimationContext) -> impl ElementBuilder {
//!     let timeline = ctx.use_animated_timeline();
//!     cn::spinner(timeline)
//!         .duration_ms(2000) // 2 seconds per rotation
//! }
//! ```

use blinc_animation::SharedAnimatedTimeline;
use blinc_core::{Brush, Color, CornerRadius, DrawContext, Rect};
use blinc_layout::canvas::{CanvasBounds, CanvasRenderFn};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::{ColorToken, ThemeState};
use std::f32::consts::PI;
use std::rc::Rc;
use std::sync::Arc;
use taffy::Style;

/// Spinner size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpinnerSize {
    /// Small spinner (16px)
    Small,
    /// Medium spinner (24px)
    #[default]
    Medium,
    /// Large spinner (32px)
    Large,
}

impl SpinnerSize {
    fn diameter(&self) -> f32 {
        match self {
            SpinnerSize::Small => 16.0,
            SpinnerSize::Medium => 24.0,
            SpinnerSize::Large => 32.0,
        }
    }

    fn border_width(&self) -> f32 {
        match self {
            SpinnerSize::Small => 2.0,
            SpinnerSize::Medium => 2.5,
            SpinnerSize::Large => 3.0,
        }
    }
}

/// Animated spinner component for loading indicators
///
/// Uses a canvas element with `AnimatedTimeline` for continuous rotation.
/// The timeline is configured automatically with infinite looping.
/// Canvas redraws every frame, making animation smooth.
pub struct Spinner {
    timeline: SharedAnimatedTimeline,
    size: SpinnerSize,
    color: Option<Color>,
    track_color: Option<Color>,
    duration_ms: u32,
}

impl Spinner {
    /// Create a new spinning spinner
    ///
    /// The timeline will be configured for infinite rotation on first render.
    pub fn new(timeline: SharedAnimatedTimeline) -> Self {
        Self {
            timeline,
            size: SpinnerSize::default(),
            color: None,
            track_color: None,
            duration_ms: 1000, // 1 second per rotation
        }
    }

    /// Set the spinner size
    pub fn size(mut self, size: SpinnerSize) -> Self {
        self.size = size;
        self
    }

    /// Set the spinner color (the spinning arc)
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Set the track color (the background circle)
    pub fn track_color(mut self, color: impl Into<Color>) -> Self {
        self.track_color = Some(color.into());
        self
    }

    /// Set the rotation duration in milliseconds (default: 1000ms)
    ///
    /// Lower values = faster spin, higher values = slower spin.
    pub fn duration_ms(mut self, duration: u32) -> Self {
        self.duration_ms = duration;
        self
    }

    /// Create the canvas render function for this spinner
    fn create_render_fn(&self) -> CanvasRenderFn {
        let theme = ThemeState::get();

        let diameter = self.size.diameter();
        let border_width = self.size.border_width();
        let spinner_color = self
            .color
            .unwrap_or_else(|| theme.color(ColorToken::Primary));
        let track_color = self
            .track_color
            .unwrap_or_else(|| theme.color(ColorToken::Border));
        let timeline = Arc::clone(&self.timeline);
        let duration_ms = self.duration_ms;

        // Configure timeline on first access (closure only runs once)
        let entry_id = timeline.lock().unwrap().configure(|t| {
            let id = t.add(0, duration_ms, 0.0, 360.0);
            t.set_loop(-1); // Infinite loop
            t.start();
            id
        });

        let render_timeline = Arc::clone(&timeline);

        Rc::new(move |ctx: &mut dyn DrawContext, bounds: CanvasBounds| {
            // Get current rotation angle from timeline
            let angle_deg = render_timeline
                .lock()
                .unwrap()
                .get(entry_id)
                .unwrap_or(0.0);
            let angle_rad = angle_deg * PI / 180.0;

            let cx = bounds.width / 2.0;
            let cy = bounds.height / 2.0;
            let radius = (diameter - border_width) / 2.0;

            // Draw track circle (background)
            let track_segments = 32;
            for i in 0..track_segments {
                let t1 = i as f32 / track_segments as f32;
                let t2 = (i + 1) as f32 / track_segments as f32;

                let a1 = t1 * PI * 2.0;
                let a2 = t2 * PI * 2.0;

                let x1 = cx + radius * a1.cos();
                let y1 = cy + radius * a1.sin();
                let x2 = cx + radius * a2.cos();
                let y2 = cy + radius * a2.sin();

                let dx = x2 - x1;
                let dy = y2 - y1;
                let len = (dx * dx + dy * dy).sqrt();

                ctx.fill_rect(
                    Rect::new(
                        x1 - border_width / 2.0,
                        y1 - border_width / 2.0,
                        len + border_width,
                        border_width,
                    ),
                    CornerRadius::uniform(border_width / 2.0),
                    Brush::Solid(track_color),
                );
            }

            // Draw spinning arc (270 degrees with fade effect)
            let arc_length = PI * 1.5; // 270 degrees
            let segments = 24;

            for i in 0..segments {
                let t1 = i as f32 / segments as f32;
                let t2 = (i + 1) as f32 / segments as f32;

                let a1 = angle_rad + t1 * arc_length;
                let a2 = angle_rad + t2 * arc_length;

                let x1 = cx + radius * a1.cos();
                let y1 = cy + radius * a1.sin();
                let x2 = cx + radius * a2.cos();
                let y2 = cy + radius * a2.sin();

                let dx = x2 - x1;
                let dy = y2 - y1;
                let len = (dx * dx + dy * dy).sqrt();

                // Fade effect: trail fades out behind the leading edge
                let alpha = 0.3 + 0.7 * t1;
                let color_with_alpha =
                    Color::rgba(spinner_color.r, spinner_color.g, spinner_color.b, alpha);

                ctx.fill_rect(
                    Rect::new(
                        x1 - border_width / 2.0,
                        y1 - border_width / 2.0,
                        len + border_width,
                        border_width,
                    ),
                    CornerRadius::uniform(border_width / 2.0),
                    Brush::Solid(color_with_alpha),
                );
            }
        })
    }
}

impl ElementBuilder for Spinner {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        let diameter = self.size.diameter();
        let border_width = self.size.border_width();
        // Add padding on both sides to account for the border width so drawing doesn't clip
        let total_size = diameter + border_width * 2.0;
        let style = Style {
            size: taffy::Size {
                width: taffy::Dimension::Length(total_size),
                height: taffy::Dimension::Length(total_size),
            },
            ..Default::default()
        };
        tree.create_node(style)
    }

    fn render_props(&self) -> RenderProps {
        RenderProps::default()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[]
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Canvas
    }

    fn canvas_render_info(&self) -> Option<CanvasRenderFn> {
        Some(self.create_render_fn())
    }

    fn layout_style(&self) -> Option<&Style> {
        None
    }
}

/// Create an animated spinner loading indicator
///
/// Takes an `AnimatedTimeline` from the animation context. The timeline
/// is automatically configured for infinite rotation.
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
/// use blinc_animation::AnimationContextExt;
///
/// fn loading(ctx: &impl AnimationContext) -> impl ElementBuilder {
///     let timeline = ctx.use_animated_timeline();
///     cn::spinner(timeline)
/// }
/// ```
pub fn spinner(timeline: SharedAnimatedTimeline) -> Spinner {
    Spinner::new(timeline)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_size_values() {
        assert_eq!(SpinnerSize::Small.diameter(), 16.0);
        assert_eq!(SpinnerSize::Medium.diameter(), 24.0);
        assert_eq!(SpinnerSize::Large.diameter(), 32.0);
    }

    #[test]
    fn test_spinner_border_widths() {
        assert_eq!(SpinnerSize::Small.border_width(), 2.0);
        assert_eq!(SpinnerSize::Medium.border_width(), 2.5);
        assert_eq!(SpinnerSize::Large.border_width(), 3.0);
    }
}
