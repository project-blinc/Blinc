//! Continuous pointer query system for CSS-driven pointer interaction.
//!
//! Exposes per-element pointer data (position, velocity, distance, angle)
//! as CSS environment variables usable in `calc()` and `@flow` inputs.
//!
//! # CSS Properties
//!
//! ```css
//! #card {
//!     pointer-space: self;        /* coordinate space: self | parent | viewport */
//!     pointer-origin: center;     /* origin point: center | top-left | bottom-left */
//!     pointer-range: -1.0 1.0;   /* normalize coordinates to this range */
//!     pointer-smoothing: 0.1;    /* exponential smoothing (seconds) */
//! }
//! ```
//!
//! # Environment Variables
//!
//! Once `pointer-space` is set on an element, these `env()` variables resolve:
//!
//! - `env(pointer-x)` — normalized X position
//! - `env(pointer-y)` — normalized Y position
//! - `env(pointer-vx)` — X velocity (units/sec)
//! - `env(pointer-vy)` — Y velocity (units/sec)
//! - `env(pointer-speed)` — total speed
//! - `env(pointer-distance)` — distance from origin
//! - `env(pointer-angle)` — angle from origin (radians)
//! - `env(pointer-inside)` — 1.0 if pointer is inside, 0.0 otherwise
//! - `env(pointer-active)` — 1.0 if pointer is pressed, 0.0 otherwise
//! - `env(pointer-pressure)` — normalized touch/click pressure (0.0-1.0)
//! - `env(pointer-touch-count)` — number of active touch points (0 for mouse)
//! - `env(pointer-hover-duration)` — seconds since pointer entered

use std::collections::HashMap;

// The configuration half of pointer space lives in `blinc_css`, since
// `env(pointer-*)` is resolved by the stylesheet. The live tracking state
// below consumes it.
pub use blinc_css::pointer::{PointerOrigin, PointerSpace, PointerSpaceConfig};

/// Per-element continuous pointer state
#[derive(Clone, Debug)]
pub struct ElementPointerState {
    /// Normalized X position in configured range
    pub x: f32,
    /// Normalized Y position in configured range
    pub y: f32,
    /// X velocity (normalized units/sec)
    pub vx: f32,
    /// Y velocity (normalized units/sec)
    pub vy: f32,
    /// Total speed (sqrt(vx² + vy²))
    pub speed: f32,
    /// Distance from origin (in normalized units)
    pub distance: f32,
    /// Angle from origin (radians, 0 = right, π/2 = up)
    pub angle: f32,
    /// Whether the pointer is inside the element's bounds (raw boolean)
    pub inside: bool,
    /// Smoothed inside value (0.0 → 1.0 with exponential smoothing)
    pub smooth_inside: f32,
    /// Whether the pointer button is pressed while over this element
    pub active: bool,
    /// Smoothed pointer pressure (0.0-1.0)
    /// Touch: actual hardware pressure. Mouse: 0.0 released, 1.0 pressed.
    pub pressure: f32,
    /// Number of active touch points (0 for mouse-only input)
    pub touch_count: u32,
    /// Seconds since pointer entered (0 if not inside)
    pub hover_duration: f32,

    // Internal smoothing state
    smooth_x: f32,
    smooth_y: f32,
    smooth_pressure: f32,
    prev_x: f32,
    prev_y: f32,
    enter_time: Option<f64>,
}

impl Default for ElementPointerState {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            vx: 0.0,
            vy: 0.0,
            speed: 0.0,
            distance: 0.0,
            angle: 0.0,
            inside: false,
            smooth_inside: 0.0,
            active: false,
            pressure: 0.0,
            touch_count: 0,
            hover_duration: 0.0,
            smooth_x: 0.0,
            smooth_y: 0.0,
            smooth_pressure: 0.0,
            prev_x: 0.0,
            prev_y: 0.0,
            enter_time: None,
        }
    }
}

impl ElementPointerState {
    /// Resolve an environment variable name to its current value
    pub fn resolve_env(&self, name: &str) -> Option<f32> {
        match name {
            "pointer-x" => Some(self.x),
            "pointer-y" => Some(self.y),
            "pointer-vx" => Some(self.vx),
            "pointer-vy" => Some(self.vy),
            "pointer-speed" => Some(self.speed),
            "pointer-distance" => Some(self.distance),
            "pointer-angle" => Some(self.angle),
            "pointer-inside" => Some(self.smooth_inside),
            "pointer-active" => Some(if self.active { 1.0 } else { 0.0 }),
            "pointer-pressure" => Some(self.pressure),
            "pointer-touch-count" => Some(self.touch_count as f32),
            "pointer-hover-duration" => Some(self.hover_duration),
            _ => None,
        }
    }
}

/// Global pointer query state — tracks continuous pointer data per element.
///
/// Keyed by element string ID (not LayoutNodeId) so state persists across
/// tree rebuilds. LayoutNodeIds are resolved from the registry at update time.
#[derive(Clone, Debug, Default)]
pub struct PointerQueryState {
    /// Per-element pointer state, keyed by element string ID
    elements: HashMap<String, ElementPointerState>,
    /// Configs from CSS, keyed by element string ID
    configs: HashMap<String, PointerSpaceConfig>,
    /// Raw pointer pressure from the current input frame (0.0-1.0).
    /// Set per-event via `set_pressure()`, consumed by `update()` for smoothing.
    raw_pressure: f32,
    /// Number of active touch points (0 for mouse-only input).
    touch_count: u32,
}

impl PointerQueryState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an element for pointer tracking with the given config.
    /// Called when CSS `pointer-space` is set on an element.
    pub fn register(&mut self, id: impl Into<String>, config: PointerSpaceConfig) {
        let id = id.into();
        self.configs.insert(id.clone(), config);
        self.elements.entry(id).or_default();
    }

    /// Remove an element from pointer tracking
    pub fn unregister(&mut self, id: &str) {
        self.configs.remove(id);
        self.elements.remove(id);
    }

    /// Clear all registrations (call on full tree rebuild)
    pub fn clear(&mut self) {
        self.configs.clear();
        self.elements.clear();
    }

    /// Get the pointer state for an element by its string ID
    pub fn get(&self, id: &str) -> Option<&ElementPointerState> {
        self.elements.get(id)
    }

    /// Scan a stylesheet and register elements with `pointer-space` set.
    /// Preserves existing accumulated state for elements that remain.
    pub fn register_from_stylesheet(&mut self, stylesheet: &crate::css_parser::Stylesheet) {
        // Collect new configs from stylesheet
        let mut new_configs = HashMap::new();
        for id in stylesheet.ids() {
            // Skip state variants like "button:hover" and pseudo-elements like "id::placeholder"
            if id.contains(':') {
                continue;
            }
            if let Some(style) = stylesheet.get(id) {
                if let Some(ref config) = style.pointer_space {
                    new_configs.insert(id.to_string(), config.clone());
                }
            }
        }

        // Remove entries that are no longer in the stylesheet
        self.configs.retain(|id, _| new_configs.contains_key(id));
        self.elements.retain(|id, _| new_configs.contains_key(id));

        // Add/update configs (preserving existing element state)
        for (id, config) in new_configs {
            self.configs.insert(id.clone(), config);
            self.elements.entry(id).or_default();
        }
    }

    /// Set the current raw pointer pressure (0.0-1.0).
    /// Call this per-event before `update()`.
    /// - Touch: actual hardware pressure from platform
    /// - Mouse: 0.0 when released, 1.0 when pressed
    pub fn set_pressure(&mut self, pressure: f32) {
        self.raw_pressure = pressure.clamp(0.0, 1.0);
    }

    /// Set the current active touch count.
    /// 0 for mouse-only input, 1+ for touch input.
    pub fn set_touch_count(&mut self, count: u32) {
        self.touch_count = count;
    }

    /// Check if any elements are registered for pointer tracking
    pub fn is_empty(&self) -> bool {
        self.configs.is_empty()
    }

    /// Get number of tracked elements
    pub fn len(&self) -> usize {
        self.configs.len()
    }

    /// Iterate over all tracked element IDs and their pointer states.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ElementPointerState)> {
        self.elements.iter().map(|(id, state)| (id.as_str(), state))
    }

    /// Update all tracked elements. Called once per frame after hit testing.
    ///
    /// Uses the event router's hit test results for `inside` detection and bounds,
    /// ensuring coordinates are consistent with the rendering pipeline (handles
    /// scroll offsets, transforms, etc.).
    ///
    /// - `mouse_x`, `mouse_y`: logical cursor position in viewport space
    /// - `is_pressed`: whether any mouse button is currently pressed
    /// - `dt`: frame delta time in seconds
    /// - `time`: total elapsed time in seconds
    /// - `get_hover_bounds`: closure that, given a string ID, returns Some((x, y, w, h))
    ///   if the element is currently hovered (bounds from the event router's hit test),
    ///   or None if not hovered
    pub fn update(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
        is_pressed: bool,
        dt: f32,
        time: f64,
        get_hover_bounds: impl Fn(&str) -> Option<(f32, f32, f32, f32)>,
    ) {
        for (id, config) in &self.configs {
            let state = self.elements.entry(id.clone()).or_default();

            // Use event router's hit test to determine hover and get bounds.
            // The router already handles scroll offsets, transforms, and occlusion.
            let hover_bounds = get_hover_bounds(id);

            let was_inside = state.inside;
            state.inside = hover_bounds.is_some();

            // Update hover duration
            if state.inside {
                if !was_inside {
                    state.enter_time = Some(time);
                }
                if let Some(enter) = state.enter_time {
                    state.hover_duration = (time - enter) as f32;
                }
            } else {
                state.enter_time = None;
                state.hover_duration = 0.0;
            }

            // Active state (pressed while over element)
            state.active = state.inside && is_pressed;

            // Pressure: use raw pressure when active, decay to 0 otherwise
            let target_pressure = if state.inside && is_pressed {
                self.raw_pressure
            } else {
                0.0
            };
            if config.smoothing > 0.0 && dt > 0.0 {
                let alpha = 1.0 - (-dt / config.smoothing).exp();
                state.smooth_pressure += (target_pressure - state.smooth_pressure) * alpha;
                if (state.smooth_pressure - target_pressure).abs() < 0.001 {
                    state.smooth_pressure = target_pressure;
                }
            } else {
                state.smooth_pressure = target_pressure;
            }
            state.pressure = state.smooth_pressure;

            // Touch count (global, not per-element, no smoothing)
            state.touch_count = self.touch_count;

            // Compute raw normalized position based on config
            // Only compute meaningful position when we have bounds (hovered)
            if let Some((bx, by, bw, bh)) = hover_bounds {
                let (raw_x, raw_y) =
                    compute_normalized_position(mouse_x, mouse_y, bx, by, bw, bh, config);

                // Apply exponential smoothing
                if config.smoothing > 0.0 && dt > 0.0 {
                    let alpha = 1.0 - (-dt / config.smoothing).exp();
                    state.smooth_x += (raw_x - state.smooth_x) * alpha;
                    state.smooth_y += (raw_y - state.smooth_y) * alpha;
                } else {
                    state.smooth_x = raw_x;
                    state.smooth_y = raw_y;
                }
            } else {
                // Not hovered — smoothly decay towards origin (0,0 for center origin)
                if config.smoothing > 0.0 && dt > 0.0 {
                    let alpha = 1.0 - (-dt / config.smoothing).exp();
                    state.smooth_x += (0.0 - state.smooth_x) * alpha;
                    state.smooth_y += (0.0 - state.smooth_y) * alpha;
                } else {
                    state.smooth_x = 0.0;
                    state.smooth_y = 0.0;
                }
            }

            // Compute velocity (change per second)
            if dt > 0.0 {
                state.vx = (state.smooth_x - state.prev_x) / dt;
                state.vy = (state.smooth_y - state.prev_y) / dt;
            }

            state.prev_x = state.smooth_x;
            state.prev_y = state.smooth_y;

            // Smooth the inside flag (0→1 and 1→0 transitions)
            let target_inside = if state.inside { 1.0 } else { 0.0 };
            if config.smoothing > 0.0 && dt > 0.0 {
                let alpha = 1.0 - (-dt / config.smoothing).exp();
                state.smooth_inside += (target_inside - state.smooth_inside) * alpha;
                // Snap to exact 0/1 when very close to avoid perpetual micro-updates
                if (state.smooth_inside - target_inside).abs() < 0.001 {
                    state.smooth_inside = target_inside;
                }
            } else {
                state.smooth_inside = target_inside;
            }

            // Set final output values
            state.x = state.smooth_x;
            state.y = state.smooth_y;
            state.speed = (state.vx * state.vx + state.vy * state.vy).sqrt();
            state.distance = (state.x * state.x + state.y * state.y).sqrt();
            state.angle = state.y.atan2(state.x);
        }
    }
}

/// Compute normalized pointer position within an element
fn compute_normalized_position(
    mouse_x: f32,
    mouse_y: f32,
    bounds_x: f32,
    bounds_y: f32,
    bounds_w: f32,
    bounds_h: f32,
    config: &PointerSpaceConfig,
) -> (f32, f32) {
    // Local position within element (0..1)
    let local_x = if bounds_w > 0.0 {
        (mouse_x - bounds_x) / bounds_w
    } else {
        0.5
    };
    let local_y = if bounds_h > 0.0 {
        (mouse_y - bounds_y) / bounds_h
    } else {
        0.5
    };

    // Apply origin transform
    let (norm_x, norm_y) = match config.origin {
        PointerOrigin::TopLeft => (local_x, local_y),
        PointerOrigin::Center => (local_x - 0.5, local_y - 0.5),
        PointerOrigin::BottomLeft => (local_x, 1.0 - local_y),
    };

    // Map to output range
    let (r_min, r_max) = config.range;
    let range_scale = r_max - r_min;

    match config.origin {
        PointerOrigin::TopLeft | PointerOrigin::BottomLeft => {
            // 0..1 → range
            (r_min + norm_x * range_scale, r_min + norm_y * range_scale)
        }
        PointerOrigin::Center => {
            // -0.5..0.5 → range (centered)
            let mid = (r_min + r_max) / 2.0;
            (mid + norm_x * range_scale, mid + norm_y * range_scale)
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PointerSpaceConfig::default();
        assert_eq!(config.space, PointerSpace::SelfSpace);
        assert_eq!(config.origin, PointerOrigin::Center);
        assert_eq!(config.range, (-1.0, 1.0));
        assert_eq!(config.smoothing, 0.0);
    }

    #[test]
    fn test_register_and_get() {
        let mut pq = PointerQueryState::new();
        pq.register("card", PointerSpaceConfig::default());
        assert!(!pq.is_empty());
        assert_eq!(pq.len(), 1);
        assert!(pq.get("card").is_some());
    }

    #[test]
    fn test_update_center_origin() {
        let mut pq = PointerQueryState::new();
        pq.register("card", PointerSpaceConfig::default());

        // Element at (100, 100) with size 200x200
        // Mouse at center (200, 200)
        pq.update(200.0, 200.0, false, 0.016, 1.0, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });

        let state = pq.get("card").unwrap();
        // Center origin: mouse at center → (0, 0)
        assert!((state.x - 0.0).abs() < 0.01, "x={}", state.x);
        assert!((state.y - 0.0).abs() < 0.01, "y={}", state.y);
        assert!(state.inside);
    }

    #[test]
    fn test_update_top_left_origin() {
        let mut pq = PointerQueryState::new();
        pq.register(
            "card",
            PointerSpaceConfig {
                origin: PointerOrigin::TopLeft,
                range: (0.0, 1.0),
                ..Default::default()
            },
        );

        // Mouse at top-left corner of element
        pq.update(100.0, 100.0, false, 0.016, 1.0, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });

        let state = pq.get("card").unwrap();
        assert!((state.x - 0.0).abs() < 0.01);
        assert!((state.y - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_update_bottom_right() {
        let mut pq = PointerQueryState::new();
        pq.register(
            "card",
            PointerSpaceConfig {
                origin: PointerOrigin::TopLeft,
                range: (0.0, 1.0),
                ..Default::default()
            },
        );

        // Mouse at bottom-right corner
        pq.update(300.0, 300.0, false, 0.016, 1.0, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });

        let state = pq.get("card").unwrap();
        assert!((state.x - 1.0).abs() < 0.01);
        assert!((state.y - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_pointer_outside() {
        let mut pq = PointerQueryState::new();
        pq.register("card", PointerSpaceConfig::default());

        // Mouse outside element — callback returns None (not hovered per event router)
        pq.update(50.0, 50.0, false, 0.016, 1.0, |_| None);

        let state = pq.get("card").unwrap();
        assert!(!state.inside);
    }

    #[test]
    fn test_active_state() {
        let mut pq = PointerQueryState::new();
        pq.register("card", PointerSpaceConfig::default());

        // Mouse inside, button pressed
        pq.update(200.0, 200.0, true, 0.016, 1.0, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });

        let state = pq.get("card").unwrap();
        assert!(state.inside);
        assert!(state.active);
    }

    #[test]
    fn test_velocity_computation() {
        let mut pq = PointerQueryState::new();
        pq.register(
            "card",
            PointerSpaceConfig {
                origin: PointerOrigin::TopLeft,
                range: (0.0, 1.0),
                ..Default::default()
            },
        );

        // Frame 1: mouse at center
        pq.update(200.0, 200.0, false, 0.016, 0.016, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });

        // Frame 2: mouse moved right
        pq.update(220.0, 200.0, false, 0.016, 0.032, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });

        let state = pq.get("card").unwrap();
        // Moved 20px in 200px width = 0.1 in 0.016s = 6.25/s
        assert!(state.vx > 0.0, "vx should be positive: {}", state.vx);
    }

    #[test]
    fn test_smoothing() {
        let mut pq = PointerQueryState::new();
        pq.register(
            "card",
            PointerSpaceConfig {
                origin: PointerOrigin::TopLeft,
                range: (0.0, 1.0),
                smoothing: 0.1, // 100ms smoothing
                ..Default::default()
            },
        );

        // Jump from 0 to 1 (full width)
        pq.update(100.0, 100.0, false, 0.016, 0.016, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });
        pq.update(300.0, 100.0, false, 0.016, 0.032, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });

        let state = pq.get("card").unwrap();
        // With smoothing, x should be less than 1.0 (hasn't caught up yet)
        assert!(state.x < 1.0, "smoothed x should lag: {}", state.x);
        assert!(state.x > 0.0, "smoothed x should be moving: {}", state.x);
    }

    #[test]
    fn test_hover_duration() {
        let mut pq = PointerQueryState::new();
        pq.register("card", PointerSpaceConfig::default());

        // Frame 1: mouse enters at t=1.0
        pq.update(200.0, 200.0, false, 0.016, 1.0, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });
        assert!(pq.get("card").unwrap().inside);

        // Frame 2: still inside at t=1.5
        pq.update(200.0, 200.0, false, 0.016, 1.5, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });
        let state = pq.get("card").unwrap();
        assert!((state.hover_duration - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_env_resolution() {
        let state = ElementPointerState {
            x: 0.5,
            y: -0.3,
            inside: true,
            smooth_inside: 1.0,
            active: false,
            speed: 2.0,
            ..Default::default()
        };

        assert_eq!(state.resolve_env("pointer-x"), Some(0.5));
        assert_eq!(state.resolve_env("pointer-y"), Some(-0.3));
        assert_eq!(state.resolve_env("pointer-inside"), Some(1.0));
        assert_eq!(state.resolve_env("pointer-active"), Some(0.0));
        assert_eq!(state.resolve_env("pointer-speed"), Some(2.0));
        assert_eq!(state.resolve_env("unknown"), None);
    }

    #[test]
    fn test_clear() {
        let mut pq = PointerQueryState::new();
        pq.register("a", PointerSpaceConfig::default());
        pq.register("b", PointerSpaceConfig::default());
        assert_eq!(pq.len(), 2);
        pq.clear();
        assert!(pq.is_empty());
    }

    #[test]
    fn test_pressure_env_resolution() {
        let state = ElementPointerState {
            pressure: 0.75,
            touch_count: 2,
            ..Default::default()
        };
        assert_eq!(state.resolve_env("pointer-pressure"), Some(0.75));
        assert_eq!(state.resolve_env("pointer-touch-count"), Some(2.0));
    }

    #[test]
    fn test_pressure_without_smoothing() {
        let mut pq = PointerQueryState::new();
        pq.register("card", PointerSpaceConfig::default());

        // Set pressure and update with mouse inside + pressed
        pq.set_pressure(0.6);
        pq.update(200.0, 200.0, true, 0.016, 1.0, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });

        let state = pq.get("card").unwrap();
        assert!(
            (state.pressure - 0.6).abs() < 0.01,
            "pressure={}, expected 0.6",
            state.pressure
        );
    }

    #[test]
    fn test_pressure_with_smoothing() {
        let mut pq = PointerQueryState::new();
        pq.register(
            "card",
            PointerSpaceConfig {
                smoothing: 0.1,
                ..Default::default()
            },
        );

        // Apply pressure
        pq.set_pressure(1.0);
        pq.update(200.0, 200.0, true, 0.016, 1.0, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });

        let state = pq.get("card").unwrap();
        // With smoothing, pressure should be moving toward 1.0 but not there yet
        assert!(state.pressure > 0.0, "pressure should be increasing");
        assert!(state.pressure < 1.0, "pressure should lag due to smoothing");
    }

    #[test]
    fn test_pressure_decays_when_released() {
        let mut pq = PointerQueryState::new();
        pq.register("card", PointerSpaceConfig::default());

        // Press with pressure
        pq.set_pressure(1.0);
        pq.update(200.0, 200.0, true, 0.016, 1.0, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });
        assert!((pq.get("card").unwrap().pressure - 1.0).abs() < 0.01);

        // Release
        pq.set_pressure(0.0);
        pq.update(200.0, 200.0, false, 0.016, 1.016, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });
        assert!(
            (pq.get("card").unwrap().pressure - 0.0).abs() < 0.01,
            "pressure should decay to 0 without smoothing"
        );
    }

    #[test]
    fn test_touch_count() {
        let mut pq = PointerQueryState::new();
        pq.register("card", PointerSpaceConfig::default());

        pq.set_touch_count(3);
        pq.update(200.0, 200.0, false, 0.016, 1.0, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });
        assert_eq!(pq.get("card").unwrap().touch_count, 3);

        pq.set_touch_count(0);
        pq.update(200.0, 200.0, false, 0.016, 1.016, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });
        assert_eq!(pq.get("card").unwrap().touch_count, 0);
    }

    #[test]
    fn test_state_persists_across_reregister() {
        let mut pq = PointerQueryState::new();
        pq.register("card", PointerSpaceConfig::default());

        // Update with mouse inside
        pq.update(200.0, 200.0, false, 0.016, 1.0, |_| {
            Some((100.0, 100.0, 200.0, 200.0))
        });
        assert!(pq.get("card").unwrap().inside);
        let x_before = pq.get("card").unwrap().x;

        // Simulate tree rebuild: register_from_stylesheet preserves state
        // (register with same ID doesn't reset existing entry)
        pq.register("card", PointerSpaceConfig::default());
        let state = pq.get("card").unwrap();
        assert_eq!(
            state.x, x_before,
            "state should persist across re-registration"
        );
    }
}
