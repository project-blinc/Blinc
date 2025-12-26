//! Canvas-based cursor with smooth animation
//!
//! Uses the canvas element to draw a cursor that animates smoothly without
//! causing tree rebuilds. The cursor opacity is computed at render time using
//! either a smooth sine wave or spring-based animation.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use blinc_core::{Brush, Color, CornerRadius, DrawContext, Rect};

use crate::canvas::{canvas, Canvas, CanvasBounds};

/// Cursor animation style
#[derive(Clone, Copy, Debug, Default)]
pub enum CursorAnimation {
    /// Smooth sine wave fade (default, subtle)
    #[default]
    SmoothFade,
    /// Classic on/off blink (sharp transitions)
    Blink,
    /// Always visible (no animation)
    Solid,
}

/// Cursor state for smooth animation
///
/// Shared between the canvas render callback and the text input widget.
/// Uses `Arc<Mutex>` for thread-safe sharing.
#[derive(Clone, Debug)]
pub struct CursorState {
    /// Whether the cursor should be visible (focused state)
    pub visible: bool,
    /// Cursor color
    pub color: Color,
    /// Cursor width in pixels
    pub width: f32,
    /// X position of the cursor (relative to canvas)
    pub x: f32,
    /// Animation style
    pub animation: CursorAnimation,
    /// Blink period in milliseconds
    pub blink_period_ms: u64,
    /// Time when cursor was last reset (e.g., on keystroke)
    /// This keeps the cursor visible immediately after typing
    pub reset_time: Instant,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            visible: false,
            color: Color::rgba(0.4, 0.6, 1.0, 1.0),
            width: 2.0,
            x: 0.0,
            animation: CursorAnimation::default(),
            blink_period_ms: 530,
            reset_time: Instant::now(),
        }
    }
}

impl CursorState {
    /// Create a new cursor state
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with a specific color
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set cursor width
    pub fn with_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Set animation style
    pub fn with_animation(mut self, animation: CursorAnimation) -> Self {
        self.animation = animation;
        self
    }

    /// Set blink period
    pub fn with_blink_period(mut self, period_ms: u64) -> Self {
        self.blink_period_ms = period_ms;
        self
    }

    /// Reset cursor blink (call on keystroke to keep cursor visible)
    pub fn reset_blink(&mut self) {
        self.reset_time = Instant::now();
    }

    /// Set cursor visibility (focused state)
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        if visible {
            self.reset_blink();
        }
    }

    /// Set cursor x position
    pub fn set_x(&mut self, x: f32) {
        self.x = x;
    }

    /// Calculate current opacity based on animation style and time
    pub fn current_opacity(&self) -> f32 {
        if !self.visible {
            return 0.0;
        }

        let elapsed = self.reset_time.elapsed().as_millis() as f64;
        let period = self.blink_period_ms as f64;

        match self.animation {
            CursorAnimation::Solid => 1.0,
            CursorAnimation::Blink => {
                // Classic on/off blink
                let phase = (elapsed / period) as u64 % 2;
                if phase == 0 {
                    1.0
                } else {
                    0.0
                }
            }
            CursorAnimation::SmoothFade => {
                // Smooth sine wave between 0.3 and 1.0
                // This creates a gentle pulsing effect instead of harsh on/off
                let t = (elapsed / period) * std::f64::consts::PI;
                let sine = (t.sin() + 1.0) / 2.0; // 0.0 to 1.0
                                                  // Map to 0.3-1.0 range for subtle effect (never fully invisible)
                0.3 + (sine as f32 * 0.7)
            }
        }
    }
}

/// Shared cursor state handle
pub type SharedCursorState = Arc<Mutex<CursorState>>;

/// Create a shared cursor state
pub fn cursor_state() -> SharedCursorState {
    Arc::new(Mutex::new(CursorState::new()))
}

/// Create a cursor canvas element
///
/// The cursor is drawn using a canvas element, which means:
/// - No tree rebuilds for cursor animation
/// - Smooth opacity transitions
/// - Efficient GPU rendering
///
/// # Arguments
///
/// * `state` - Shared cursor state controlling visibility and position
/// * `height` - Height of the cursor in pixels
///
/// # Example
///
/// ```ignore
/// let cursor_state = cursor_state();
///
/// // In your text input:
/// cursor_state.borrow_mut().set_visible(is_focused);
/// cursor_state.borrow_mut().set_x(cursor_x_position);
///
/// // Add to your layout:
/// div()
///     .child(cursor_canvas(&cursor_state, font_size * 1.2))
/// ```
pub fn cursor_canvas(state: &SharedCursorState, height: f32) -> Canvas {
    let state = Arc::clone(state);

    canvas(move |ctx: &mut dyn DrawContext, bounds: CanvasBounds| {
        let s = state.lock().unwrap();

        // Skip drawing if not visible
        if !s.visible {
            return;
        }

        // Calculate current opacity
        let opacity = s.current_opacity();

        // Skip drawing if fully transparent
        if opacity < 0.01 {
            return;
        }

        // Create color with current opacity
        let color = Color::rgba(s.color.r, s.color.g, s.color.b, s.color.a * opacity);

        // Draw the cursor bar
        // The cursor is drawn relative to the canvas position
        // x is typically 0 since the canvas is positioned where the cursor should be
        ctx.fill_rect(
            Rect::new(0.0, 0.0, s.width, bounds.height),
            CornerRadius::default(),
            Brush::Solid(color),
        );
    })
    .w(2.0) // Default cursor width
    .h(height)
}

/// Create an absolutely positioned cursor canvas
///
/// This version positions the cursor absolutely within a relative container.
/// The cursor x position is read from the state on each render.
///
/// # Arguments
///
/// * `state` - Shared cursor state controlling visibility and position
/// * `height` - Height of the cursor in pixels
/// * `top` - Top offset for vertical centering
pub fn cursor_canvas_absolute(state: &SharedCursorState, height: f32, top: f32) -> Canvas {
    let state = Arc::clone(state);

    canvas(move |ctx: &mut dyn DrawContext, bounds: CanvasBounds| {
        let s = state.lock().unwrap();

        // Skip drawing if not visible
        if !s.visible {
            return;
        }

        // Calculate current opacity
        let opacity = s.current_opacity();

        // Skip drawing if fully transparent
        if opacity < 0.01 {
            return;
        }

        // Create color with current opacity
        let color = Color::rgba(s.color.r, s.color.g, s.color.b, s.color.a * opacity);

        // Draw the cursor bar at the x position from state
        // The canvas is sized to cover the full text area,
        // and we draw the cursor at the correct x position
        ctx.fill_rect(
            Rect::new(s.x, 0.0, s.width, bounds.height),
            CornerRadius::default(),
            Brush::Solid(color),
        );
    })
    .absolute()
    .left(0.0)
    .top(top)
    .w_full()
    .h(height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_state_defaults() {
        let state = CursorState::new();
        assert!(!state.visible);
        assert_eq!(state.width, 2.0);
        assert_eq!(state.blink_period_ms, 530);
    }

    #[test]
    fn test_cursor_opacity_when_not_visible() {
        let state = CursorState::new();
        assert_eq!(state.current_opacity(), 0.0);
    }

    #[test]
    fn test_cursor_opacity_solid() {
        let mut state = CursorState::new();
        state.visible = true;
        state.animation = CursorAnimation::Solid;
        assert_eq!(state.current_opacity(), 1.0);
    }

    #[test]
    fn test_cursor_opacity_smooth_fade_range() {
        let mut state = CursorState::new();
        state.visible = true;
        state.animation = CursorAnimation::SmoothFade;

        // Should be between 0.3 and 1.0
        let opacity = state.current_opacity();
        assert!(opacity >= 0.3);
        assert!(opacity <= 1.0);
    }
}
