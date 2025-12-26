//! RenderState - Dynamic render properties separate from tree structure
//!
//! This module provides a clean separation between:
//! - **RenderTree**: Stable tree structure (rebuilt only when elements are added/removed)
//! - **RenderState**: Dynamic render properties (updated every frame without tree rebuild)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  UI Thread                                                       │
//! │  Event → State Change → Tree Rebuild (only structural changes)  │
//! └─────────────────────────────────────────────────────────────────┘
//!                               │
//!                               ▼
//!                     RenderTree (stable)
//!                               │
//!                               ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  Render Loop (60fps)                                             │
//! │  1. Tick animations                                              │
//! │  2. Update RenderState from animations                           │
//! │  3. Render tree + state to GPU                                   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # What Goes Where
//!
//! | Property | RenderTree | RenderState |
//! |----------|------------|-------------|
//! | Element hierarchy | ✓ | |
//! | Layout constraints | ✓ | |
//! | Text content | ✓ | |
//! | Background color | | ✓ (animated) |
//! | Opacity | | ✓ (animated) |
//! | Transform | | ✓ (animated) |
//! | Cursor visibility | | ✓ (animated) |
//! | Scroll offset | | ✓ (animated) |
//! | Hover state | | ✓ (FSM) |
//! | Focus state | | ✓ (FSM) |

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use blinc_animation::{AnimationScheduler, SchedulerHandle, Spring, SpringConfig, SpringId};
use blinc_core::{Color, Transform};

use crate::tree::LayoutNodeId;

/// Dynamic render state for a single node
///
/// Contains all properties that can change without requiring a tree rebuild.
/// These properties are updated by animations or state machines.
#[derive(Clone, Debug)]
pub struct NodeRenderState {
    // =========================================================================
    // Animated visual properties
    // =========================================================================
    /// Current opacity (0.0 - 1.0)
    pub opacity: f32,

    /// Current background color (animated)
    pub background_color: Option<Color>,

    /// Current border color (animated)
    pub border_color: Option<Color>,

    /// Current transform (animated)
    pub transform: Option<Transform>,

    /// Current scale (animated, applied to transform)
    pub scale: f32,

    // =========================================================================
    // Animation handles (for tracking which properties are animating)
    // =========================================================================
    /// Spring ID for opacity animation
    pub opacity_spring: Option<SpringId>,

    /// Spring IDs for color animation (r, g, b, a)
    pub bg_color_springs: Option<[SpringId; 4]>,

    /// Spring IDs for transform (translate_x, translate_y, scale, rotate)
    pub transform_springs: Option<[SpringId; 4]>,

    // =========================================================================
    // Interaction state
    // =========================================================================
    /// Whether this node is currently hovered
    pub hovered: bool,

    /// Whether this node is currently focused
    pub focused: bool,

    /// Whether this node is currently pressed
    pub pressed: bool,
}

impl Default for NodeRenderState {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            background_color: None,
            border_color: None,
            transform: None,
            scale: 1.0,
            opacity_spring: None,
            bg_color_springs: None,
            transform_springs: None,
            hovered: false,
            focused: false,
            pressed: false,
        }
    }
}

impl NodeRenderState {
    /// Create a new node render state with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if any properties are currently animating
    pub fn is_animating(&self) -> bool {
        self.opacity_spring.is_some()
            || self.bg_color_springs.is_some()
            || self.transform_springs.is_some()
    }
}

/// Overlay type for rendering on top of the tree
#[derive(Clone, Debug)]
pub enum Overlay {
    /// Text cursor overlay
    Cursor {
        /// Position (x, y)
        position: (f32, f32),
        /// Size (width, height)
        size: (f32, f32),
        /// Color
        color: Color,
        /// Current opacity (for blinking)
        opacity: f32,
    },
    /// Text selection overlay
    Selection {
        /// Selection rectangles (multiple for multi-line)
        rects: Vec<(f32, f32, f32, f32)>,
        /// Selection color
        color: Color,
    },
    /// Focus ring overlay
    FocusRing {
        /// Position (x, y)
        position: (f32, f32),
        /// Size (width, height)
        size: (f32, f32),
        /// Corner radius
        radius: f32,
        /// Ring color
        color: Color,
        /// Ring thickness
        thickness: f32,
    },
}

/// Global render state - updated every frame independently of tree rebuilds
///
/// This holds all dynamic render properties that change frequently:
/// - Animated colors, transforms, opacity
/// - Cursor blink state
/// - Scroll positions (from physics)
/// - Hover/focus visual state
pub struct RenderState {
    /// Per-node animated properties
    node_states: HashMap<LayoutNodeId, NodeRenderState>,

    /// Global overlays (cursors, selections, focus rings)
    overlays: Vec<Overlay>,

    /// Animation scheduler (shared with app)
    animations: Arc<Mutex<AnimationScheduler>>,

    /// Cursor blink state (global for all text inputs)
    cursor_visible: bool,

    /// Last cursor blink toggle time
    cursor_blink_time: u64,

    /// Cursor blink interval in ms
    cursor_blink_interval: u64,
}

impl RenderState {
    /// Create a new render state with the given animation scheduler
    pub fn new(animations: Arc<Mutex<AnimationScheduler>>) -> Self {
        Self {
            node_states: HashMap::new(),
            overlays: Vec::new(),
            animations,
            cursor_visible: true,
            cursor_blink_time: 0,
            cursor_blink_interval: 400,
        }
    }

    /// Get animation scheduler handle for creating animations
    pub fn animation_handle(&self) -> SchedulerHandle {
        self.animations.lock().unwrap().handle()
    }

    /// Tick all animations and update render state
    ///
    /// Returns true if any animations are active (need another frame)
    pub fn tick(&mut self, current_time_ms: u64) -> bool {
        // Tick the animation scheduler
        let animations_active = self.animations.lock().unwrap().tick();

        // Update cursor blink
        if current_time_ms >= self.cursor_blink_time + self.cursor_blink_interval {
            self.cursor_visible = !self.cursor_visible;
            self.cursor_blink_time = current_time_ms;
        }

        // Update node states from their animation springs
        let scheduler = self.animations.lock().unwrap();
        for (_node_id, state) in &mut self.node_states {
            // Update opacity from spring
            if let Some(spring_id) = state.opacity_spring {
                if let Some(value) = scheduler.get_spring_value(spring_id) {
                    state.opacity = value.clamp(0.0, 1.0);
                }
            }

            // Update background color from springs
            if let Some(springs) = state.bg_color_springs {
                let r = scheduler.get_spring_value(springs[0]).unwrap_or(0.0);
                let g = scheduler.get_spring_value(springs[1]).unwrap_or(0.0);
                let b = scheduler.get_spring_value(springs[2]).unwrap_or(0.0);
                let a = scheduler.get_spring_value(springs[3]).unwrap_or(1.0);
                state.background_color = Some(Color::rgba(r, g, b, a));
            }

            // Update transform from springs
            // Note: For now, we only support translation. Scale/rotation would need
            // matrix composition which Transform doesn't expose directly.
            if let Some(springs) = state.transform_springs {
                let tx = scheduler.get_spring_value(springs[0]).unwrap_or(0.0);
                let ty = scheduler.get_spring_value(springs[1]).unwrap_or(0.0);
                let scale = scheduler.get_spring_value(springs[2]).unwrap_or(1.0);
                let _rotate = scheduler.get_spring_value(springs[3]).unwrap_or(0.0);
                // TODO: Support scale/rotation when Transform supports composition
                state.transform = Some(Transform::translate(tx, ty));
                state.scale = scale;
            }
        }

        // Update cursor overlays with blink state
        for overlay in &mut self.overlays {
            if let Overlay::Cursor { opacity, .. } = overlay {
                *opacity = if self.cursor_visible { 1.0 } else { 0.0 };
            }
        }

        animations_active || self.has_overlays()
    }

    /// Reset cursor blink (call when focus changes or user types)
    pub fn reset_cursor_blink(&mut self, current_time_ms: u64) {
        self.cursor_visible = true;
        self.cursor_blink_time = current_time_ms;
    }

    /// Set cursor blink interval
    pub fn set_cursor_blink_interval(&mut self, interval_ms: u64) {
        self.cursor_blink_interval = interval_ms;
    }

    /// Check if cursor is currently visible
    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    // =========================================================================
    // Node State Management
    // =========================================================================

    /// Get or create render state for a node
    pub fn get_or_create(&mut self, node_id: LayoutNodeId) -> &mut NodeRenderState {
        self.node_states
            .entry(node_id)
            .or_insert_with(NodeRenderState::new)
    }

    /// Get render state for a node (if exists)
    pub fn get(&self, node_id: LayoutNodeId) -> Option<&NodeRenderState> {
        self.node_states.get(&node_id)
    }

    /// Get mutable render state for a node (if exists)
    pub fn get_mut(&mut self, node_id: LayoutNodeId) -> Option<&mut NodeRenderState> {
        self.node_states.get_mut(&node_id)
    }

    /// Remove render state for a node
    pub fn remove(&mut self, node_id: LayoutNodeId) {
        self.node_states.remove(&node_id);
    }

    /// Clear all node states (call when tree is completely rebuilt)
    pub fn clear_nodes(&mut self) {
        self.node_states.clear();
    }

    // =========================================================================
    // Animation Control
    // =========================================================================

    /// Animate opacity for a node
    pub fn animate_opacity(&mut self, node_id: LayoutNodeId, target: f32, config: SpringConfig) {
        // Get current values first
        let (current, old_spring) = {
            let state = self
                .node_states
                .entry(node_id)
                .or_insert_with(NodeRenderState::new);
            (state.opacity, state.opacity_spring.take())
        };

        // Remove existing spring if any
        if let Some(old_id) = old_spring {
            self.animations.lock().unwrap().remove_spring(old_id);
        }

        // Create new spring
        let mut spring = Spring::new(config, current);
        spring.set_target(target);
        let spring_id = self.animations.lock().unwrap().add_spring(spring);

        // Store the new spring id
        if let Some(state) = self.node_states.get_mut(&node_id) {
            state.opacity_spring = Some(spring_id);
        }
    }

    /// Animate background color for a node
    pub fn animate_background(
        &mut self,
        node_id: LayoutNodeId,
        target: Color,
        config: SpringConfig,
    ) {
        // Get current values first
        let (current, old_springs) = {
            let state = self
                .node_states
                .entry(node_id)
                .or_insert_with(NodeRenderState::new);
            let current = state.background_color.unwrap_or(Color::TRANSPARENT);
            (current, state.bg_color_springs.take())
        };

        // Remove existing springs if any
        if let Some(old_ids) = old_springs {
            let mut scheduler = self.animations.lock().unwrap();
            for id in old_ids {
                scheduler.remove_spring(id);
            }
        }

        // Create springs for r, g, b, a
        let springs = {
            let mut scheduler = self.animations.lock().unwrap();
            [
                {
                    let mut s = Spring::new(config, current.r);
                    s.set_target(target.r);
                    scheduler.add_spring(s)
                },
                {
                    let mut s = Spring::new(config, current.g);
                    s.set_target(target.g);
                    scheduler.add_spring(s)
                },
                {
                    let mut s = Spring::new(config, current.b);
                    s.set_target(target.b);
                    scheduler.add_spring(s)
                },
                {
                    let mut s = Spring::new(config, current.a);
                    s.set_target(target.a);
                    scheduler.add_spring(s)
                },
            ]
        };

        // Store the new spring ids
        if let Some(state) = self.node_states.get_mut(&node_id) {
            state.bg_color_springs = Some(springs);
        }
    }

    /// Set background color immediately (no animation)
    pub fn set_background(&mut self, node_id: LayoutNodeId, color: Color) {
        // Get old springs first
        let old_springs = {
            let state = self
                .node_states
                .entry(node_id)
                .or_insert_with(NodeRenderState::new);
            state.bg_color_springs.take()
        };

        // Remove any active animation
        if let Some(old_ids) = old_springs {
            let mut scheduler = self.animations.lock().unwrap();
            for id in old_ids {
                scheduler.remove_spring(id);
            }
        }

        // Set the color
        if let Some(state) = self.node_states.get_mut(&node_id) {
            state.background_color = Some(color);
        }
    }

    /// Set opacity immediately (no animation)
    pub fn set_opacity(&mut self, node_id: LayoutNodeId, opacity: f32) {
        // Get old spring first
        let old_spring = {
            let state = self
                .node_states
                .entry(node_id)
                .or_insert_with(NodeRenderState::new);
            state.opacity_spring.take()
        };

        // Remove any active animation
        if let Some(old_id) = old_spring {
            self.animations.lock().unwrap().remove_spring(old_id);
        }

        // Set the opacity
        if let Some(state) = self.node_states.get_mut(&node_id) {
            state.opacity = opacity;
        }
    }

    // =========================================================================
    // Overlay Management
    // =========================================================================

    /// Add a cursor overlay
    pub fn add_cursor(&mut self, x: f32, y: f32, width: f32, height: f32, color: Color) {
        self.overlays.push(Overlay::Cursor {
            position: (x, y),
            size: (width, height),
            color,
            opacity: if self.cursor_visible { 1.0 } else { 0.0 },
        });
    }

    /// Add a selection overlay
    pub fn add_selection(&mut self, rects: Vec<(f32, f32, f32, f32)>, color: Color) {
        self.overlays.push(Overlay::Selection { rects, color });
    }

    /// Add a focus ring overlay
    pub fn add_focus_ring(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        color: Color,
        thickness: f32,
    ) {
        self.overlays.push(Overlay::FocusRing {
            position: (x, y),
            size: (width, height),
            radius,
            color,
            thickness,
        });
    }

    /// Clear all overlays (call before each frame's overlay collection)
    pub fn clear_overlays(&mut self) {
        self.overlays.clear();
    }

    /// Get all overlays for rendering
    pub fn overlays(&self) -> &[Overlay] {
        &self.overlays
    }

    /// Check if there are any overlays
    pub fn has_overlays(&self) -> bool {
        !self.overlays.is_empty()
    }

    // =========================================================================
    // Interaction State
    // =========================================================================

    /// Set hover state for a node
    pub fn set_hovered(&mut self, node_id: LayoutNodeId, hovered: bool) {
        self.get_or_create(node_id).hovered = hovered;
    }

    /// Set focus state for a node
    pub fn set_focused(&mut self, node_id: LayoutNodeId, focused: bool) {
        self.get_or_create(node_id).focused = focused;
    }

    /// Set pressed state for a node
    pub fn set_pressed(&mut self, node_id: LayoutNodeId, pressed: bool) {
        self.get_or_create(node_id).pressed = pressed;
    }

    /// Check if a node is hovered
    pub fn is_hovered(&self, node_id: LayoutNodeId) -> bool {
        self.get(node_id).map(|s| s.hovered).unwrap_or(false)
    }

    /// Check if a node is focused
    pub fn is_focused(&self, node_id: LayoutNodeId) -> bool {
        self.get(node_id).map(|s| s.focused).unwrap_or(false)
    }

    /// Check if a node is pressed
    pub fn is_pressed(&self, node_id: LayoutNodeId) -> bool {
        self.get(node_id).map(|s| s.pressed).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_state_creation() {
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
        let state = RenderState::new(scheduler);

        assert!(state.cursor_visible());
        assert!(!state.has_overlays());
    }

    #[test]
    fn test_node_render_state() {
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
        let mut state = RenderState::new(scheduler);

        let node_id = LayoutNodeId::default();

        // Should auto-create on access
        state.set_hovered(node_id, true);
        assert!(state.is_hovered(node_id));

        state.set_opacity(node_id, 0.5);
        assert_eq!(state.get(node_id).unwrap().opacity, 0.5);
    }

    #[test]
    fn test_overlays() {
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
        let mut state = RenderState::new(scheduler);

        state.add_cursor(10.0, 20.0, 2.0, 16.0, Color::WHITE);
        assert!(state.has_overlays());
        assert_eq!(state.overlays().len(), 1);

        state.clear_overlays();
        assert!(!state.has_overlays());
    }

    #[test]
    fn test_cursor_blink() {
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
        let mut state = RenderState::new(scheduler);
        state.set_cursor_blink_interval(100);

        assert!(state.cursor_visible());

        // Tick past the blink interval
        state.tick(150);
        assert!(!state.cursor_visible());

        // Tick again
        state.tick(300);
        assert!(state.cursor_visible());
    }
}
