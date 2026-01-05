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
use std::sync::{Arc, LazyLock, Mutex, RwLock};

use blinc_animation::{AnimationScheduler, SchedulerHandle, Spring, SpringConfig, SpringId};
use blinc_core::context_state::MotionAnimationState;
use blinc_core::{Color, Rect, Transform};

use crate::element::{MotionAnimation, MotionKeyframe};
use crate::tree::LayoutNodeId;

/// Shared motion state for query API access
///
/// This stores a snapshot of motion animation states that can be queried
/// from outside the render loop via the query_motion API.
pub type SharedMotionStates = Arc<RwLock<HashMap<String, MotionAnimationState>>>;

/// Create a new shared motion state store
pub fn create_shared_motion_states() -> SharedMotionStates {
    Arc::new(RwLock::new(HashMap::new()))
}

// =============================================================================
// Global pending motion replay queue
// =============================================================================

/// Global queue for motion keys that should replay their animation
///
/// This allows motion elements to request replay during tree building,
/// without needing direct access to RenderState.
static PENDING_MOTION_REPLAYS: LazyLock<Mutex<Vec<String>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Queue a stable motion key for replay (global version)
///
/// Call this from within motion element construction when `.replay()` is used.
/// The replay will be processed when `RenderState::process_global_motion_replays()`
/// is called after `initialize_motion_animations()`.
pub fn queue_global_motion_replay(key: String) {
    let mut queue = PENDING_MOTION_REPLAYS.lock().unwrap();
    if !queue.contains(&key) {
        queue.push(key);
    }
}

/// Take all pending global motion replays
///
/// Returns the queued keys and clears the queue.
pub fn take_global_motion_replays() -> Vec<String> {
    std::mem::take(&mut *PENDING_MOTION_REPLAYS.lock().unwrap())
}

/// Buffer zone around viewport for prefetching content
/// This prevents pop-in when scrolling slowly
const VIEWPORT_BUFFER: f32 = 100.0;

/// State of a motion animation
#[derive(Clone, Debug)]
pub enum MotionState {
    /// Animation hasn't started yet (waiting for delay)
    Waiting { remaining_delay_ms: f32 },
    /// Animation is playing (enter animation)
    Entering { progress: f32, duration_ms: f32 },
    /// Element is fully visible (enter complete)
    Visible,
    /// Animation is playing (exit animation)
    Exiting { progress: f32, duration_ms: f32 },
    /// Element should be removed (exit complete)
    Removed,
}

impl Default for MotionState {
    fn default() -> Self {
        MotionState::Visible
    }
}

/// Active motion animation for a node
#[derive(Clone, Debug)]
pub struct ActiveMotion {
    /// The animation configuration
    pub config: MotionAnimation,
    /// Current state of the animation
    pub state: MotionState,
    /// Current interpolated values
    pub current: MotionKeyframe,
}

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

    // =========================================================================
    // Motion animation state
    // =========================================================================
    /// Active motion animation (enter/exit) for this node
    pub motion: Option<ActiveMotion>,
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
            motion: None,
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
            || self.has_active_motion()
    }

    /// Check if this node has an active motion animation
    pub fn has_active_motion(&self) -> bool {
        if let Some(ref motion) = self.motion {
            !matches!(motion.state, MotionState::Visible | MotionState::Removed)
        } else {
            false
        }
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
/// - Viewport for visibility culling
pub struct RenderState {
    /// Per-node animated properties
    node_states: HashMap<LayoutNodeId, NodeRenderState>,

    /// Stable-keyed motion animations (for overlays that rebuild each frame)
    /// Key is a stable string ID (e.g., overlay handle ID), value is the motion state
    stable_motions: HashMap<String, ActiveMotion>,

    /// Set of stable motion keys that were accessed this frame
    /// Used for mark-and-sweep cleanup of unused motions
    stable_motions_used: std::collections::HashSet<String>,

    /// Queue of stable motion keys that should replay their animation
    /// These are processed after initialize_motion_animations completes
    pending_motion_replays: Vec<String>,

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

    /// Last tick time (for calculating delta time)
    last_tick_time: Option<u64>,

    /// Current viewport bounds for visibility culling
    /// Updated each frame based on window size and scroll position
    viewport: Rect,

    /// Whether viewport has been set (false = no culling)
    viewport_set: bool,

    /// Shared motion state for query API access
    /// Updated after each tick to expose motion states to components
    shared_motion_states: Option<SharedMotionStates>,
}

impl RenderState {
    /// Create a new render state with the given animation scheduler
    pub fn new(animations: Arc<Mutex<AnimationScheduler>>) -> Self {
        Self {
            node_states: HashMap::new(),
            stable_motions: HashMap::new(),
            stable_motions_used: std::collections::HashSet::new(),
            pending_motion_replays: Vec::new(),
            overlays: Vec::new(),
            animations,
            cursor_visible: true,
            cursor_blink_time: 0,
            cursor_blink_interval: 400,
            last_tick_time: None,
            viewport: Rect::new(0.0, 0.0, 0.0, 0.0),
            viewport_set: false,
            shared_motion_states: None,
        }
    }

    /// Set the shared motion states for query API access
    ///
    /// Call this after creating the RenderState to enable motion state queries.
    pub fn set_shared_motion_states(&mut self, shared: SharedMotionStates) {
        self.shared_motion_states = Some(shared);
    }

    /// Sync motion states to the shared store
    ///
    /// Call this after tick() to update the shared motion states for query API.
    pub fn sync_shared_motion_states(&self) {
        if let Some(ref shared) = self.shared_motion_states {
            let mut states = shared.write().unwrap();
            states.clear();
            for (key, motion) in &self.stable_motions {
                let state = match &motion.state {
                    MotionState::Waiting { .. } => MotionAnimationState::Waiting,
                    MotionState::Entering { progress, .. } => MotionAnimationState::Entering {
                        progress: *progress,
                    },
                    MotionState::Visible => MotionAnimationState::Visible,
                    MotionState::Exiting { progress, .. } => MotionAnimationState::Exiting {
                        progress: *progress,
                    },
                    MotionState::Removed => MotionAnimationState::Removed,
                };
                states.insert(key.clone(), state);
            }
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
        // Calculate delta time
        let dt_ms = if let Some(last_time) = self.last_tick_time {
            (current_time_ms.saturating_sub(last_time)) as f32
        } else {
            16.0 // Assume ~60fps for first frame
        };
        self.last_tick_time = Some(current_time_ms);

        // Tick the animation scheduler
        let animations_active = self.animations.lock().unwrap().tick();

        // Update cursor blink
        if current_time_ms >= self.cursor_blink_time + self.cursor_blink_interval {
            self.cursor_visible = !self.cursor_visible;
            self.cursor_blink_time = current_time_ms;
        }

        // Track if any motion animations are active
        let mut motion_active = false;

        // Update node states from their animation springs and motion animations
        {
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

                // Update motion animation
                if let Some(ref mut motion) = state.motion {
                    if Self::tick_motion(motion, dt_ms) {
                        motion_active = true;
                    }
                }
            }
        } // Drop scheduler lock

        // Tick stable-keyed motions (for overlays)
        self.tick_stable_motions(dt_ms);

        // Update cursor overlays with blink state
        for overlay in &mut self.overlays {
            if let Overlay::Cursor { opacity, .. } = overlay {
                *opacity = if self.cursor_visible { 1.0 } else { 0.0 };
            }
        }

        animations_active || motion_active || self.has_active_motions() || self.has_overlays()
    }

    /// Tick a motion animation, returns true if still active
    fn tick_motion(motion: &mut ActiveMotion, dt_ms: f32) -> bool {
        match &mut motion.state {
            MotionState::Waiting { remaining_delay_ms } => {
                *remaining_delay_ms -= dt_ms;
                if *remaining_delay_ms <= 0.0 {
                    // Start enter animation
                    if motion.config.enter_from.is_some() && motion.config.enter_duration_ms > 0 {
                        tracing::debug!(
                            "Motion: Starting enter animation, duration={}ms",
                            motion.config.enter_duration_ms
                        );
                        motion.state = MotionState::Entering {
                            progress: 0.0,
                            duration_ms: motion.config.enter_duration_ms as f32,
                        };
                        // Initialize current to the "from" state
                        motion.current = motion.config.enter_from.clone().unwrap_or_default();
                    } else {
                        motion.state = MotionState::Visible;
                        motion.current = MotionKeyframe::default(); // Fully visible
                    }
                }
                true // Still animating
            }
            MotionState::Entering {
                progress,
                duration_ms,
            } => {
                *progress += dt_ms / *duration_ms;
                if *progress >= 1.0 {
                    motion.state = MotionState::Visible;
                    motion.current = MotionKeyframe::default(); // Fully visible (opacity=1, scale=1, etc.)
                    false // Done animating
                } else {
                    // Interpolate from enter_from to visible (default)
                    let from = motion
                        .config
                        .enter_from
                        .as_ref()
                        .cloned()
                        .unwrap_or_default();
                    let to = MotionKeyframe::default();
                    // Apply ease-in-out for enter animation
                    // This provides a smooth start that doesn't feel "sudden"
                    // when items appear in sequence (stagger animations)
                    let eased = ease_in_out_cubic(*progress);
                    motion.current = from.lerp(&to, eased);
                    true // Still animating
                }
            }
            MotionState::Visible => false, // Not animating
            MotionState::Exiting {
                progress,
                duration_ms,
            } => {
                *progress += dt_ms / *duration_ms;
                if *progress >= 1.0 {
                    motion.state = MotionState::Removed;
                    motion.current = motion.config.exit_to.clone().unwrap_or_default();
                    false // Done animating
                } else {
                    // Interpolate from visible to exit_to
                    let from = MotionKeyframe::default();
                    let to = motion.config.exit_to.as_ref().cloned().unwrap_or_default();
                    // Apply ease-in for exit animation
                    let eased = ease_in_cubic(*progress);
                    motion.current = from.lerp(&to, eased);
                    true // Still animating
                }
            }
            MotionState::Removed => false, // Not animating
        }
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

    // =========================================================================
    // Motion Animation Control
    // =========================================================================

    /// Start an enter motion animation for a node
    ///
    /// This is called when a node with motion config first appears in the tree.
    pub fn start_enter_motion(&mut self, node_id: LayoutNodeId, config: MotionAnimation) {
        let state = self.get_or_create(node_id);

        // Determine initial state based on delay
        let initial_state = if config.enter_delay_ms > 0 {
            MotionState::Waiting {
                remaining_delay_ms: config.enter_delay_ms as f32,
            }
        } else if config.enter_from.is_some() && config.enter_duration_ms > 0 {
            MotionState::Entering {
                progress: 0.0,
                duration_ms: config.enter_duration_ms as f32,
            }
        } else {
            MotionState::Visible
        };

        // Initial values come from enter_from (the starting state)
        let current = if matches!(initial_state, MotionState::Visible) {
            MotionKeyframe::default() // Already fully visible
        } else {
            config.enter_from.clone().unwrap_or_default()
        };

        state.motion = Some(ActiveMotion {
            config,
            state: initial_state,
            current,
        });
    }

    /// Start an exit motion animation for a node
    ///
    /// This is called when a node with motion config is about to be removed.
    pub fn start_exit_motion(&mut self, node_id: LayoutNodeId) {
        if let Some(state) = self.node_states.get_mut(&node_id) {
            if let Some(ref mut motion) = state.motion {
                if motion.config.exit_to.is_some() && motion.config.exit_duration_ms > 0 {
                    motion.state = MotionState::Exiting {
                        progress: 0.0,
                        duration_ms: motion.config.exit_duration_ms as f32,
                    };
                    motion.current = MotionKeyframe::default(); // Start from visible
                } else {
                    motion.state = MotionState::Removed;
                }
            }
        }
    }

    /// Get the current motion values for a node
    ///
    /// Returns the interpolated keyframe values if the node has an active motion.
    pub fn get_motion_values(&self, node_id: LayoutNodeId) -> Option<&MotionKeyframe> {
        self.get(node_id)
            .and_then(|s| s.motion.as_ref())
            .map(|m| &m.current)
    }

    /// Check if a node's motion animation is complete and should be removed
    pub fn is_motion_removed(&self, node_id: LayoutNodeId) -> bool {
        self.get(node_id)
            .and_then(|s| s.motion.as_ref())
            .map(|m| matches!(m.state, MotionState::Removed))
            .unwrap_or(false)
    }

    /// Check if any nodes have active motion animations
    pub fn has_active_motions(&self) -> bool {
        self.node_states.values().any(|s| s.has_active_motion())
            || self
                .stable_motions
                .values()
                .any(|m| !matches!(m.state, MotionState::Visible | MotionState::Removed))
    }

    // =========================================================================
    // Stable-Keyed Motion Animations (for overlays)
    // =========================================================================

    /// Start or get a stable-keyed motion animation
    ///
    /// Unlike node-based motions, these persist across tree rebuilds using a
    /// stable string key (e.g., overlay handle ID).
    ///
    /// If the motion already exists and is still animating (Waiting, Entering),
    /// we leave it alone. If it's in Visible or Exiting state, we also leave it
    /// alone. Only when in Removed state do we restart (overlay was closed and
    /// reopened).
    ///
    /// If `replay` is true, the animation restarts from the beginning even if
    /// it already exists (useful for tab transitions where content changes).
    ///
    /// If the overlay is closing (checked via `is_overlay_closing()`), this will
    /// start the exit animation instead of leaving the motion alone.
    pub fn start_stable_motion(&mut self, key: &str, config: MotionAnimation, replay: bool) {
        use crate::overlay_state::is_overlay_closing;

        // Mark this key as used this frame (for garbage collection)
        self.stable_motions_used.insert(key.to_string());

        // Check if we're in overlay closing mode
        let is_closing = is_overlay_closing();

        // Check if motion already exists
        if let Some(existing) = self.stable_motions.get_mut(key) {
            // If closing and not already exiting, start exit animation
            if is_closing
                && !matches!(
                    existing.state,
                    MotionState::Exiting { .. } | MotionState::Removed
                )
            {
                // Start exit animation
                if config.exit_to.is_some() && config.exit_duration_ms > 0 {
                    tracing::debug!(
                        "start_stable_motion: Starting exit animation for key={}, duration={}ms",
                        key,
                        config.exit_duration_ms
                    );
                    existing.config = config;
                    existing.state = MotionState::Exiting {
                        progress: 0.0,
                        duration_ms: existing.config.exit_duration_ms as f32,
                    };
                    existing.current = MotionKeyframe::default(); // Start from visible
                } else {
                    tracing::debug!(
                        "start_stable_motion: Closing but no exit animation configured for key={}, exit_to={:?}, exit_duration={}",
                        key,
                        config.exit_to.is_some(),
                        config.exit_duration_ms
                    );
                }
                return;
            }

            // NOTE: replay flag is intentionally ignored here for existing motions.
            // The replay mechanism via this flag doesn't work correctly because
            // initialize_motion_animations is called for ALL motions in the tree,
            // not just the ones that changed. Use `replay_stable_motion(key)` instead,
            // called from an on_ready callback when the motion is first mounted.

            // If already animating or visible, leave it alone
            match existing.state {
                MotionState::Waiting { .. }
                | MotionState::Entering { .. }
                | MotionState::Visible
                | MotionState::Exiting { .. } => {
                    // Don't restart - animation is either in progress or completed
                    return;
                }
                // Only restart if the motion was previously removed (overlay closed then reopened)
                MotionState::Removed => {
                    // Reset to initial enter state
                    existing.config = config.clone();
                    existing.state = if config.enter_delay_ms > 0 {
                        MotionState::Waiting {
                            remaining_delay_ms: config.enter_delay_ms as f32,
                        }
                    } else if config.enter_from.is_some() && config.enter_duration_ms > 0 {
                        MotionState::Entering {
                            progress: 0.0,
                            duration_ms: config.enter_duration_ms as f32,
                        }
                    } else {
                        MotionState::Visible
                    };
                    existing.current = if matches!(existing.state, MotionState::Visible) {
                        MotionKeyframe::default()
                    } else {
                        config.enter_from.clone().unwrap_or_default()
                    };
                    return;
                }
            }
        }

        // Create new motion
        let initial_state = if config.enter_delay_ms > 0 {
            MotionState::Waiting {
                remaining_delay_ms: config.enter_delay_ms as f32,
            }
        } else if config.enter_from.is_some() && config.enter_duration_ms > 0 {
            MotionState::Entering {
                progress: 0.0,
                duration_ms: config.enter_duration_ms as f32,
            }
        } else {
            MotionState::Visible
        };

        // Initial values come from enter_from (the starting state)
        let current = if matches!(initial_state, MotionState::Visible) {
            MotionKeyframe::default() // Already fully visible
        } else {
            config.enter_from.clone().unwrap_or_default()
        };

        self.stable_motions.insert(
            key.to_string(),
            ActiveMotion {
                config,
                state: initial_state,
                current,
            },
        );
    }

    /// Start exit animation for a stable-keyed motion
    pub fn start_stable_motion_exit(&mut self, key: &str) {
        if let Some(motion) = self.stable_motions.get_mut(key) {
            if motion.config.exit_to.is_some() && motion.config.exit_duration_ms > 0 {
                motion.state = MotionState::Exiting {
                    progress: 0.0,
                    duration_ms: motion.config.exit_duration_ms as f32,
                };
                motion.current = MotionKeyframe::default(); // Start from visible
            } else {
                motion.state = MotionState::Removed;
            }
        }
    }

    /// Queue a stable motion key for replay
    ///
    /// The replay will be processed after `initialize_motion_animations` completes.
    /// This allows motion elements to request replay during tree building without
    /// affecting other motions.
    ///
    /// Call `process_pending_motion_replays()` after `initialize_motion_animations()`
    /// to actually perform the replays.
    pub fn queue_motion_replay(&mut self, key: String) {
        if !self.pending_motion_replays.contains(&key) {
            self.pending_motion_replays.push(key);
        }
    }

    /// Process all pending motion replays (from local queue)
    ///
    /// Call this after `initialize_motion_animations()` to replay any motions
    /// that requested it via `queue_motion_replay()`.
    pub fn process_pending_motion_replays(&mut self) {
        let keys = std::mem::take(&mut self.pending_motion_replays);
        for key in keys {
            self.replay_stable_motion(&key);
        }
    }

    /// Process all pending motion replays from the global queue
    ///
    /// Call this after `initialize_motion_animations()` to replay any motions
    /// that were queued via `queue_global_motion_replay()` during tree building.
    pub fn process_global_motion_replays(&mut self) {
        let keys = take_global_motion_replays();
        for key in keys {
            self.replay_stable_motion(&key);
        }
    }

    /// Replay a stable-keyed motion animation from the beginning
    ///
    /// This restarts the animation if it's in Visible state.
    /// Prefer using `queue_motion_replay()` during tree building, and
    /// `process_pending_motion_replays()` after initialization.
    pub fn replay_stable_motion(&mut self, key: &str) {
        if let Some(motion) = self.stable_motions.get_mut(key) {
            // Only replay if animation is complete (Visible state)
            if matches!(motion.state, MotionState::Visible) {
                let config = motion.config.clone();
                motion.state = if config.enter_delay_ms > 0 {
                    MotionState::Waiting {
                        remaining_delay_ms: config.enter_delay_ms as f32,
                    }
                } else if config.enter_from.is_some() && config.enter_duration_ms > 0 {
                    MotionState::Entering {
                        progress: 0.0,
                        duration_ms: config.enter_duration_ms as f32,
                    }
                } else {
                    MotionState::Visible
                };
                motion.current = if matches!(motion.state, MotionState::Visible) {
                    MotionKeyframe::default()
                } else {
                    config.enter_from.clone().unwrap_or_default()
                };
            }
        }
    }

    /// Get the current motion values for a stable-keyed animation
    pub fn get_stable_motion_values(&self, key: &str) -> Option<&MotionKeyframe> {
        self.stable_motions.get(key).map(|m| &m.current)
    }

    /// Get the animation state for a stable-keyed motion
    ///
    /// Returns the current state of the motion animation as `MotionAnimationState`.
    /// This is used by the query API to expose animation state to components.
    pub fn get_stable_motion_state(
        &self,
        key: &str,
    ) -> blinc_core::context_state::MotionAnimationState {
        use blinc_core::context_state::MotionAnimationState;

        match self.stable_motions.get(key) {
            Some(motion) => match &motion.state {
                MotionState::Waiting { .. } => MotionAnimationState::Waiting,
                MotionState::Entering { progress, .. } => MotionAnimationState::Entering {
                    progress: *progress,
                },
                MotionState::Visible => MotionAnimationState::Visible,
                MotionState::Exiting { progress, .. } => MotionAnimationState::Exiting {
                    progress: *progress,
                },
                MotionState::Removed => MotionAnimationState::Removed,
            },
            None => MotionAnimationState::NotFound,
        }
    }

    /// Check if a stable-keyed motion is complete and should be removed
    pub fn is_stable_motion_removed(&self, key: &str) -> bool {
        self.stable_motions
            .get(key)
            .map(|m| matches!(m.state, MotionState::Removed))
            .unwrap_or(false)
    }

    /// Reset all stable motions to replay on next frame
    ///
    /// Call this before a full UI rebuild to ensure all motion animations
    /// replay when the UI is reconstructed. This resets motions in `Visible`
    /// state back to their initial `Waiting` or `Entering` state.
    ///
    /// Motions that are currently animating (Entering/Exiting) or already
    /// Removed are left alone.
    pub fn reset_stable_motions_for_rebuild(&mut self) {
        for motion in self.stable_motions.values_mut() {
            if matches!(motion.state, MotionState::Visible) {
                let config = &motion.config;
                motion.state = if config.enter_delay_ms > 0 {
                    MotionState::Waiting {
                        remaining_delay_ms: config.enter_delay_ms as f32,
                    }
                } else if config.enter_from.is_some() && config.enter_duration_ms > 0 {
                    MotionState::Entering {
                        progress: 0.0,
                        duration_ms: config.enter_duration_ms as f32,
                    }
                } else {
                    MotionState::Visible
                };
                motion.current = if matches!(motion.state, MotionState::Visible) {
                    MotionKeyframe::default()
                } else {
                    motion.config.enter_from.clone().unwrap_or_default()
                };
            }
        }
    }

    /// Clear all stable motions
    ///
    /// Use this for a complete reset, e.g., when navigating to a completely
    /// different view. For normal full rebuilds, prefer `reset_stable_motions_for_rebuild()`
    /// which preserves motion configs but replays animations.
    pub fn clear_stable_motions(&mut self) {
        self.stable_motions.clear();
        self.stable_motions_used.clear();
    }

    /// Remove a stable-keyed motion (after exit animation completes)
    pub fn remove_stable_motion(&mut self, key: &str) {
        self.stable_motions.remove(key);
    }

    /// Tick stable-keyed motions (called from tick())
    fn tick_stable_motions(&mut self, dt_ms: f32) {
        for motion in self.stable_motions.values_mut() {
            Self::tick_single_motion(motion, dt_ms);
        }
    }

    /// Helper to tick a single motion animation
    fn tick_single_motion(motion: &mut ActiveMotion, dt_ms: f32) {
        match &mut motion.state {
            MotionState::Waiting { remaining_delay_ms } => {
                *remaining_delay_ms -= dt_ms;
                if *remaining_delay_ms <= 0.0 {
                    if motion.config.enter_from.is_some() && motion.config.enter_duration_ms > 0 {
                        motion.state = MotionState::Entering {
                            progress: 0.0,
                            duration_ms: motion.config.enter_duration_ms as f32,
                        };
                    } else {
                        motion.state = MotionState::Visible;
                    }
                }
            }
            MotionState::Entering {
                progress,
                duration_ms,
            } => {
                *progress += dt_ms / *duration_ms;
                if *progress >= 1.0 {
                    motion.state = MotionState::Visible;
                    motion.current = MotionKeyframe::default();
                } else {
                    // Interpolate from enter_from to default (fully visible)
                    if let Some(ref from) = motion.config.enter_from {
                        motion.current = from.lerp(&MotionKeyframe::default(), *progress);
                    }
                }
            }
            MotionState::Exiting {
                progress,
                duration_ms,
            } => {
                *progress += dt_ms / *duration_ms;
                if *progress >= 1.0 {
                    motion.state = MotionState::Removed;
                    if let Some(ref to) = motion.config.exit_to {
                        motion.current = to.clone();
                    }
                } else {
                    // Interpolate from default (fully visible) to exit_to
                    if let Some(ref to) = motion.config.exit_to {
                        motion.current = MotionKeyframe::default().lerp(to, *progress);
                    }
                }
            }
            MotionState::Visible | MotionState::Removed => {
                // Nothing to do
            }
        }
    }

    /// Begin a new frame for stable motion tracking
    ///
    /// Call this before rendering overlay trees to reset the "used" tracking.
    /// Stable motions that aren't accessed during the frame will be marked as
    /// removed when `end_stable_motion_frame()` is called.
    pub fn begin_stable_motion_frame(&mut self) {
        self.stable_motions_used.clear();
    }

    /// End the frame for stable motion tracking
    ///
    /// Removes any stable motions that weren't accessed this frame.
    /// Since each motion container gets a unique UUID key, unused motions
    /// from closed overlays should be cleaned up to prevent memory leaks.
    pub fn end_stable_motion_frame(&mut self) {
        // Remove motions that weren't used this frame
        // With UUID-based keys, each new motion container gets a unique key,
        // so we need to actually remove old entries rather than just marking them
        self.stable_motions
            .retain(|key, _| self.stable_motions_used.contains(key));
    }

    // =========================================================================
    // Viewport / Visibility Culling
    // =========================================================================

    /// Set the current viewport bounds
    ///
    /// Call this each frame with the visible area (window size).
    /// Used for visibility culling of emoji and lazy-loaded images.
    pub fn set_viewport(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.viewport = Rect::new(x, y, width, height);
        self.viewport_set = true;
    }

    /// Set the viewport from window dimensions (assumes origin at 0,0)
    pub fn set_viewport_size(&mut self, width: f32, height: f32) {
        self.set_viewport(0.0, 0.0, width, height);
    }

    /// Get the current viewport bounds
    pub fn viewport(&self) -> Rect {
        self.viewport
    }

    /// Get the viewport expanded by buffer zone for prefetching
    ///
    /// Content within this area should be loaded to prevent pop-in during scroll.
    pub fn viewport_with_buffer(&self) -> Rect {
        Rect::new(
            self.viewport.x() - VIEWPORT_BUFFER,
            self.viewport.y() - VIEWPORT_BUFFER,
            self.viewport.width() + 2.0 * VIEWPORT_BUFFER,
            self.viewport.height() + 2.0 * VIEWPORT_BUFFER,
        )
    }

    /// Check if a rect is visible in the current viewport
    ///
    /// Returns true if the rect intersects with the viewport.
    /// If viewport hasn't been set, always returns true (no culling).
    pub fn is_visible(&self, bounds: &Rect) -> bool {
        if !self.viewport_set {
            return true; // No culling if viewport not set
        }
        self.viewport.intersects(bounds)
    }

    /// Check if a rect is visible with buffer zone (for prefetching)
    ///
    /// Returns true if the rect intersects with the expanded viewport.
    /// Use this for deciding what to load ahead of time.
    pub fn is_visible_with_buffer(&self, bounds: &Rect) -> bool {
        if !self.viewport_set {
            return true; // No culling if viewport not set
        }
        self.viewport_with_buffer().intersects(bounds)
    }

    /// Check if a rect is fully clipped (completely outside viewport)
    ///
    /// Returns true if the rect does not intersect with the viewport at all.
    pub fn is_clipped(&self, bounds: &Rect) -> bool {
        if !self.viewport_set {
            return false; // Nothing clipped if viewport not set
        }
        !self.viewport.intersects(bounds)
    }

    /// Check if viewport has been set
    pub fn has_viewport(&self) -> bool {
        self.viewport_set
    }
}

// ============================================================================
// Easing helper functions
// ============================================================================

/// Cubic ease-in-out (slow start, slow end) - good for stagger enter animations
/// This prevents the "sudden" appearance when items animate in sequence
fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Cubic ease-in (slow start, fast end) - good for exit animations
fn ease_in_cubic(t: f32) -> f32 {
    t * t * t
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
