//! Skeleton component for loading placeholders
//!
//! A placeholder element that shows a shimmer/pulse effect while content loads.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Simple skeleton line
//! cn::skeleton().h(20.0).w(200.0)
//!
//! // Avatar skeleton
//! cn::skeleton_circle(48.0)
//!
//! // Card skeleton
//! div().col().gap(8.0)
//!     .child(cn::skeleton().h(200.0).w_full())  // Image
//!     .child(cn::skeleton().h(24.0).w(150.0))   // Title
//!     .child(cn::skeleton().h(16.0).w_full())   // Description line 1
//!     .child(cn::skeleton().h(16.0).w(180.0))   // Description line 2
//!
//! // With shimmer animation
//! let timeline = ctx.use_animated_timeline();
//! cn::skeleton().h(20.0).w(200.0).shimmer(timeline)
//! ```

use std::ops::{Deref, DerefMut};

use blinc_animation::SharedAnimatedTimeline;
use blinc_layout::div::{Div, ElementBuilder, ElementTypeId};
use blinc_layout::prelude::*;
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

/// Skeleton component for loading placeholders
pub struct Skeleton {
    inner: Div,
}

impl Skeleton {
    /// Create a new skeleton placeholder
    pub fn new() -> Self {
        let theme = ThemeState::get();

        // Use a muted background color for the skeleton
        let bg = theme.color(ColorToken::SurfaceElevated);
        let radius = theme.radius(RadiusToken::Default);

        let inner = div().bg(bg).rounded(radius);

        Self { inner }
    }

    /// Create a circular skeleton (for avatars, icons)
    pub fn circle(size: f32) -> Self {
        let theme = ThemeState::get();
        let bg = theme.color(ColorToken::SurfaceElevated);

        let inner = div()
            .bg(bg)
            .w(size)
            .h(size)
            .rounded(theme.radius(RadiusToken::Full));

        Self { inner }
    }

    /// Set width
    pub fn w(mut self, width: f32) -> Self {
        self.inner = self.inner.w(width);
        self
    }

    /// Set height
    pub fn h(mut self, height: f32) -> Self {
        self.inner = self.inner.h(height);
        self
    }

    /// Set full width
    pub fn w_full(mut self) -> Self {
        self.inner = self.inner.w_full();
        self
    }

    /// Set border radius
    pub fn rounded(mut self, radius: f32) -> Self {
        self.inner = self.inner.rounded(radius);
        self
    }

    /// Add a shimmer/pulse animation to the skeleton
    ///
    /// The skeleton will fade between 50% and 100% opacity in a continuous loop.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let timeline = ctx.use_animated_timeline();
    /// cn::skeleton().h(20.0).w(200.0).shimmer(timeline)
    /// ```
    pub fn shimmer(self, timeline: SharedAnimatedTimeline) -> AnimatedSkeleton {
        AnimatedSkeleton::new(self, timeline)
    }
}

/// Skeleton with shimmer animation
///
/// Created by calling `.shimmer(timeline)` on a `Skeleton`.
pub struct AnimatedSkeleton {
    skeleton: Skeleton,
    timeline: SharedAnimatedTimeline,
    duration_ms: u32,
    min_opacity: f32,
    max_opacity: f32,
}

impl AnimatedSkeleton {
    /// Create a new animated skeleton
    fn new(skeleton: Skeleton, timeline: SharedAnimatedTimeline) -> Self {
        Self {
            skeleton,
            timeline,
            duration_ms: 1500,  // 1.5 seconds for full cycle
            min_opacity: 0.4,
            max_opacity: 1.0,
        }
    }

    /// Set the animation duration in milliseconds (default: 1500ms)
    pub fn duration_ms(mut self, duration: u32) -> Self {
        self.duration_ms = duration;
        self
    }

    /// Set the minimum opacity (default: 0.4)
    pub fn min_opacity(mut self, opacity: f32) -> Self {
        self.min_opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Set the maximum opacity (default: 1.0)
    pub fn max_opacity(mut self, opacity: f32) -> Self {
        self.max_opacity = opacity.clamp(0.0, 1.0);
        self
    }
}

impl ElementBuilder for AnimatedSkeleton {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        // Configure timeline for shimmer animation
        // To simulate ping-pong, we add two entries: 0->1 and 1->0
        let half_duration = self.duration_ms / 2;
        let (entry1, entry2) = self.timeline.lock().unwrap().configure(|t| {
            let id1 = t.add(0, half_duration, 0.0, 1.0);           // Fade to max
            let id2 = t.add(half_duration as i32, half_duration, 1.0, 0.0); // Fade back to min
            t.set_loop(-1); // Infinite loop
            t.start();
            (id1, id2)
        });

        // Use a canvas element to read timeline and draw with animated opacity
        let timeline = self.timeline.clone();
        let min_opacity = self.min_opacity;
        let max_opacity = self.max_opacity;
        let theme = ThemeState::get();
        let bg_color = theme.color(ColorToken::SurfaceElevated);
        let radius = theme.radius(RadiusToken::Default);

        use blinc_layout::canvas::{canvas, CanvasBounds};
        use blinc_core::{Brush, DrawContext, Rect, CornerRadius};

        // Get dimensions from skeleton style
        let skeleton_style = self.skeleton.inner.layout_style().cloned().unwrap_or_default();
        let width = match skeleton_style.size.width {
            taffy::Dimension::Length(l) => Some(l),
            _ => None,
        };
        let height = match skeleton_style.size.height {
            taffy::Dimension::Length(l) => Some(l),
            _ => None,
        };
        let is_full_width = matches!(skeleton_style.size.width, taffy::Dimension::Percent(p) if p >= 0.99);

        // Build canvas with animated rendering
        let mut canvas_builder = canvas(move |ctx: &mut dyn DrawContext, bounds: CanvasBounds| {
            // Get current timeline values - either entry might be active
            let t1 = timeline.lock().unwrap().get(entry1).unwrap_or(0.0);
            let t2 = timeline.lock().unwrap().get(entry2).unwrap_or(0.0);
            // One entry will be at 0, the other will have the active value
            let t_value = if t1 > 0.0 { t1 } else { t2 };

            // Map 0.0-1.0 to min_opacity-max_opacity
            let opacity = min_opacity + t_value * (max_opacity - min_opacity);
            let color = bg_color.with_alpha(opacity);

            ctx.fill_rect(
                Rect::new(0.0, 0.0, bounds.width, bounds.height),
                CornerRadius::uniform(radius),
                Brush::Solid(color),
            );
        });

        // Apply dimensions from skeleton style
        if let Some(w) = width {
            canvas_builder = canvas_builder.w(w);
        }
        if let Some(h) = height {
            canvas_builder = canvas_builder.h(h);
        }
        if is_full_width {
            canvas_builder = canvas_builder.w_full();
        }

        canvas_builder.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        blinc_layout::element::RenderProps::default()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[]
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Div
    }
}

impl Default for Skeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Skeleton {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Skeleton {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for Skeleton {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(&self.inner)
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementBuilder::element_type_id(&self.inner)
    }
}

/// Create a skeleton placeholder
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// // Text line skeleton
/// cn::skeleton().h(16.0).w(200.0)
///
/// // Avatar skeleton
/// cn::skeleton().circle(40.0)
/// ```
pub fn skeleton() -> Skeleton {
    Skeleton::new()
}

/// Create a circular skeleton
///
/// # Example
///
/// ```ignore
/// cn::skeleton_circle(48.0)  // 48px avatar placeholder
/// ```
pub fn skeleton_circle(size: f32) -> Skeleton {
    Skeleton::circle(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_theme() {
        let _ = ThemeState::try_get().unwrap_or_else(|| {
            ThemeState::init_default();
            ThemeState::get()
        });
    }

    #[test]
    fn test_skeleton_default() {
        init_theme();
        let _ = skeleton();
    }

    #[test]
    fn test_skeleton_sized() {
        init_theme();
        let _ = skeleton().h(20.0).w(200.0);
    }

    #[test]
    fn test_skeleton_circle() {
        init_theme();
        let _ = skeleton_circle(48.0);
    }
}
