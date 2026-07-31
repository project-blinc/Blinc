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
// Global animation scheduler handle
// =============================================================================

/// Global storage for the animation scheduler handle
///
/// This allows components to access animations without needing explicit context.
/// Set by RenderState on initialization, used by blinc_cn components.
#[allow(clippy::incompatible_msrv)]
static GLOBAL_SCHEDULER: LazyLock<RwLock<Option<SchedulerHandle>>> =
    LazyLock::new(|| RwLock::new(None));

/// Set the global animation scheduler handle
///
/// Called by RenderState during initialization to make the scheduler
/// available to all components.
pub fn set_global_scheduler(handle: SchedulerHandle) {
    let mut storage = GLOBAL_SCHEDULER.write().unwrap();
    *storage = Some(handle);
}

/// Get the global animation scheduler handle
///
/// Returns `None` if the scheduler hasn't been set yet (before app initialization).
///
/// # Example
///
/// ```ignore
/// let scheduler = get_global_scheduler()
///     .expect("Animation scheduler not initialized");
///
/// let scale = AnimatedValue::new(scheduler, 1.0, SpringConfig::snappy());
/// ```
pub fn get_global_scheduler() -> Option<SchedulerHandle> {
    GLOBAL_SCHEDULER.read().unwrap().clone()
}

/// Check if the global scheduler is available
pub fn has_global_scheduler() -> bool {
    GLOBAL_SCHEDULER.read().unwrap().is_some()
}

// =============================================================================
// Global pending motion replay queue
// =============================================================================

/// Global queue for motion keys that should replay their animation
///
/// This allows motion elements to request replay during tree building,
/// without needing direct access to RenderState.
#[allow(clippy::incompatible_msrv)]
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

// =============================================================================
// Global pending motion exit cancel queue
// =============================================================================

/// Global queue for motion keys that should cancel their exit animation
///
/// This allows components to request exit cancellation without needing direct
/// access to RenderState.
#[allow(clippy::incompatible_msrv)]
static PENDING_MOTION_EXIT_CANCELS: LazyLock<Mutex<Vec<String>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Queue a stable motion key for exit cancellation (global version)
///
/// Call this when an overlay's close is cancelled (e.g., mouse re-enters hover card).
/// The cancellation will be processed when `RenderState::process_global_motion_exit_cancels()`
/// is called during the next frame.
pub fn queue_global_motion_exit_cancel(key: String) {
    let mut queue = PENDING_MOTION_EXIT_CANCELS.lock().unwrap();
    if !queue.contains(&key) {
        queue.push(key);
    }
}

/// Take all pending global motion exit cancels
///
/// Returns the queued keys and clears the queue.
pub fn take_global_motion_exit_cancels() -> Vec<String> {
    std::mem::take(&mut *PENDING_MOTION_EXIT_CANCELS.lock().unwrap())
}

// =============================================================================
// Global pending motion exit start queue
// =============================================================================

/// Global queue for motion keys that should start their exit animation
///
/// This allows components to explicitly trigger exit animations without needing
/// direct access to RenderState.
#[allow(clippy::incompatible_msrv)]
static PENDING_MOTION_EXIT_STARTS: LazyLock<Mutex<Vec<String>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Queue a stable motion key for exit start (global version)
///
/// Call this to explicitly start the exit animation for a motion (e.g., when
/// a hover card close countdown completes).
pub fn queue_global_motion_exit_start(key: String) {
    let mut queue = PENDING_MOTION_EXIT_STARTS.lock().unwrap();
    if !queue.contains(&key) {
        queue.push(key);
    }
}

/// Take all pending global motion exit starts
///
/// Returns the queued keys and clears the queue.
pub fn take_global_motion_exit_starts() -> Vec<String> {
    std::mem::take(&mut *PENDING_MOTION_EXIT_STARTS.lock().unwrap())
}

// =============================================================================
// Global pending motion start queue (for suspended motions)
// =============================================================================

/// Global queue for motion keys that should start their enter animation from suspended state
///
/// This allows components to explicitly start suspended animations without needing
/// direct access to RenderState.
#[allow(clippy::incompatible_msrv)]
static PENDING_MOTION_STARTS: LazyLock<Mutex<Vec<String>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Queue a stable motion key to start its enter animation from suspended state
///
/// Call this to explicitly start a suspended motion's enter animation.
/// The motion must be in `Suspended` state for this to have an effect.
pub fn queue_global_motion_start(key: String) {
    let mut queue = PENDING_MOTION_STARTS.lock().unwrap();
    if !queue.contains(&key) {
        queue.push(key);
    }
}

/// Take all pending global motion starts
///
/// Returns the queued keys and clears the queue.
pub fn take_global_motion_starts() -> Vec<String> {
    std::mem::take(&mut *PENDING_MOTION_STARTS.lock().unwrap())
}

/// Buffer zone around viewport for prefetching content
/// This prevents pop-in when scrolling slowly
const VIEWPORT_BUFFER: f32 = 100.0;

/// State of a motion animation
#[derive(Clone, Debug, Default)]
pub enum MotionState {
    /// Animation is suspended (waiting for explicit start)
    /// Content is rendered with opacity 0, waiting for `MotionHandle.start()` to trigger
    Suspended,
    /// Animation hasn't started yet (waiting for delay)
    Waiting { remaining_delay_ms: f32 },
    /// Animation is playing (enter animation)
    Entering { progress: f32, duration_ms: f32 },
    /// Element is fully visible (enter complete)
    #[default]
    Visible,
    /// Animation is playing (exit animation)
    Exiting { progress: f32, duration_ms: f32 },
    /// Element should be removed (exit complete)
    Removed,
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

/// Active CSS keyframe animation state
///
/// Tracks a running CSS animation for an element, including the full
/// `MultiKeyframeAnimation` with all keyframes preserved.
#[derive(Clone, Debug)]
pub struct ActiveCssAnimation {
    /// The running animation with all keyframes
    pub animation: blinc_animation::MultiKeyframeAnimation,
    /// Whether the animation is currently playing
    pub is_playing: bool,
    /// Current interpolated properties (cached each tick)
    pub current_properties: blinc_animation::KeyframeProperties,
}

impl ActiveCssAnimation {
    /// Create a new active CSS animation
    pub fn new(mut animation: blinc_animation::MultiKeyframeAnimation) -> Self {
        animation.start();
        let current = animation.current_properties();
        Self {
            animation,
            is_playing: true,
            current_properties: current,
        }
    }

    /// Tick the animation and update current properties
    ///
    /// Returns true if the animation is still playing
    pub fn tick(&mut self, dt_ms: f32) -> bool {
        if self.is_playing {
            self.animation.tick(dt_ms);
            self.current_properties = self.animation.current_properties();
            if !self.animation.is_playing() {
                self.is_playing = false;
            }
        }
        self.is_playing
    }
}

/// Shared CSS animation/transition store
///
/// Wraps CSS keyframe animations and CSS transitions in a single struct
/// that can be shared between the AnimationScheduler's background thread
/// (for ticking) and the main render thread (for reading/writing).
///
/// The background thread ticks all animations at 120fps via a tick callback.
/// The main thread inserts/removes animations and reads `current_properties`
/// to apply animated values to render props.
///
/// Keyed by `StableNodeId` (Phase 5 of the layout-id stability
/// refactor — see `project_stable_node_id_design`). Animations
/// survive rebuilds: a `Stateful` re-running its `on_state`, a
/// route swap, or any other rebuild trigger overwrites the entry
/// at the same stable id with fresh state. Previously
/// `LayoutNodeId`-keyed, which forced animations to restart on
/// every rebuild.
#[derive(Default)]
pub struct CssAnimationStore {
    /// Active CSS keyframe animations (from stylesheet `animation:` property)
    pub animations: HashMap<crate::tree::StableNodeId, ActiveCssAnimation>,
    /// Active CSS transitions (from stylesheet `transition:` property)
    pub transitions: HashMap<crate::tree::StableNodeId, ActiveCssAnimation>,
}

impl CssAnimationStore {
    /// Create a new empty store
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether any tracked animation or transition belongs to a node
    /// in the supplied `painted` set.
    ///
    /// The unfiltered "any active animation?" check used to keep the
    /// redraw chain alive for every `infinite` keyframe in the
    /// stylesheet, even when the animated element was scrolled
    /// off-screen — `styling_demo`, with ~25 infinite animations,
    /// pinned ~73 % CPU at idle. Filtering by visibility lets the
    /// chain die when the user isn't looking at the moving parts;
    /// when they scroll back, `tick(dt_ms)` catches up by elapsed
    /// time so the visible state is still correct.
    pub fn has_visible_active(
        &self,
        painted: &std::collections::HashSet<crate::tree::StableNodeId>,
    ) -> bool {
        // Match `has_active_animations` / `has_active_transitions`:
        // gate on the `is_playing` flag so completed entries kept
        // in the map for same-target restart suppression don't
        // pin the windowed redraw chain forever.
        self.animations
            .iter()
            .any(|(n, a)| a.is_playing && painted.contains(n))
            || self
                .transitions
                .iter()
                .any(|(n, t)| t.is_playing && painted.contains(n))
    }

    /// Whether any visible animation or transition is currently
    /// touching a property classified as
    /// [`needs_vsync_for_smoothness`](blinc_animation::KeyframeProperties::needs_vsync_for_smoothness)
    /// — transforms, 3D rotation, layout sizing, font-size, clip-path
    /// geometry.
    ///
    /// Used by the windowed app to decide whether the configured
    /// `animation_fps_cap` should bypass for the next frame: if a
    /// visible rotate-y / grow-shrink / clip-reveal keyframe is
    /// mid-cycle, capping to 30 fps would visibly stair-step;
    /// opacity and color cycles elsewhere on the same screen
    /// tolerate the cap fine.
    pub fn has_visible_vsync_class(
        &self,
        painted: &std::collections::HashSet<crate::tree::StableNodeId>,
    ) -> bool {
        self.animations
            .iter()
            .filter(|(n, _)| painted.contains(*n))
            .any(|(_, a)| a.current_properties.needs_vsync_for_smoothness())
            || self
                .transitions
                .iter()
                .filter(|(n, _)| painted.contains(*n))
                .any(|(_, a)| a.current_properties.needs_vsync_for_smoothness())
    }

    /// Tick all active CSS animations and transitions
    ///
    /// Called from the AnimationScheduler's background thread via tick callback.
    /// Returns `(animations_active, transitions_active)`.
    pub fn tick(&mut self, dt_ms: f32) -> (bool, bool) {
        // Tick CSS animations
        let mut anim_playing = false;
        for anim in self.animations.values_mut() {
            if anim.tick(dt_ms) {
                anim_playing = true;
            }
        }

        // Tick CSS transitions
        // NOTE: Completed transitions are NOT removed here — they stay in the store
        // so apply_all_css_transition_props can apply their final values. Call
        // remove_completed_transitions() after applying transition props.
        let mut trans_playing = false;
        for trans in self.transitions.values_mut() {
            if trans.tick(dt_ms) {
                trans_playing = true;
            }
        }

        (anim_playing, trans_playing)
    }

    /// Remove completed transitions from the store.
    ///
    /// Must be called AFTER `apply_all_css_transition_props()` so that the final
    /// transition values are applied to render props before removal. If transitions
    /// are removed during tick(), the final values never get applied, causing the
    /// before-snapshot to use stale intermediate values and restarting the transition.
    pub fn remove_completed_transitions(&mut self) {
        self.transitions.retain(|_, trans| trans.is_playing);
    }

    /// Check if there are any active CSS animations
    pub fn has_active_animations(&self) -> bool {
        self.animations.values().any(|a| a.is_playing)
    }

    /// Check if there are any *playing* CSS transitions.
    ///
    /// Completed transitions intentionally stay in `self.transitions`
    /// so the same-target guard in `detect_and_start_transitions`
    /// can match against them (avoids endless restart loops on
    /// repeat hover styles). Checking `!is_empty()` here would mean
    /// the redraw / cache-invalidation chain fires forever after
    /// the first transition completes — observed in
    /// `image_css_demo`: hovering a `mask-image` transition pinned
    /// CPU at ~100 % indefinitely after the cursor moved away.
    /// Match `has_active_animations` and gate on the `is_playing`
    /// flag instead.
    pub fn has_active_transitions(&self) -> bool {
        self.transitions.values().any(|t| t.is_playing)
    }
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

    // =========================================================================
    // CSS keyframe animation state
    // =========================================================================
    /// Active CSS keyframe animation for this node
    ///
    /// This is separate from `motion` because CSS animations can have
    /// multiple keyframes (not just enter/exit) and different lifecycle.
    pub css_animation: Option<ActiveCssAnimation>,
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
            css_animation: None,
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
            || self.has_active_css_animation()
    }

    /// Check if this node has an active motion animation
    pub fn has_active_motion(&self) -> bool {
        if let Some(ref motion) = self.motion {
            !matches!(motion.state, MotionState::Visible | MotionState::Removed)
        } else {
            false
        }
    }

    // =========================================================================
    // CSS Animation methods
    // =========================================================================

    /// Start a CSS keyframe animation for this node
    ///
    /// Replaces any existing CSS animation.
    pub fn start_css_animation(&mut self, animation: blinc_animation::MultiKeyframeAnimation) {
        self.css_animation = Some(ActiveCssAnimation::new(animation));
    }

    /// Check if this node has an active CSS animation
    pub fn has_active_css_animation(&self) -> bool {
        self.css_animation
            .as_ref()
            .map(|a| a.is_playing)
            .unwrap_or(false)
    }

    /// Tick the CSS animation and get current properties
    ///
    /// Returns Some with current properties if animation is active, None otherwise.
    pub fn tick_css_animation(
        &mut self,
        dt_ms: f32,
    ) -> Option<&blinc_animation::KeyframeProperties> {
        if let Some(ref mut active) = self.css_animation {
            active.tick(dt_ms);
            if active.is_playing {
                return Some(&active.current_properties);
            }
        }
        None
    }

    /// Stop and remove the CSS animation
    pub fn stop_css_animation(&mut self) {
        if let Some(ref mut active) = self.css_animation {
            active.is_playing = false;
        }
    }

    /// Get the current CSS animation properties (if any)
    pub fn css_animation_properties(&self) -> Option<&blinc_animation::KeyframeProperties> {
        self.css_animation
            .as_ref()
            .filter(|a| a.is_playing)
            .map(|a| &a.current_properties)
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

    /// Monotonic counter bumped whenever `stable_motions` is mutated
    /// (insert, remove, tick that changed state, exit start, etc.).
    /// `sync_shared_motion_states` compares against
    /// `last_synced_motion_generation` and skips the work when the
    /// counter hasn't advanced — at idle cn_demo this is the common
    /// case (motions all in `Visible`) and the bg thread was paying
    /// hundreds of String clones + HashMap inserts per second to
    /// re-sync identical state.
    pub(crate) motion_generation: u64,
    /// Last value of `motion_generation` written through to
    /// `shared_motion_states`. Skip sync when equal.
    pub(crate) last_synced_motion_generation: std::cell::Cell<u64>,
}

impl RenderState {
    /// Create a new render state with the given animation scheduler
    pub fn new(animations: Arc<Mutex<AnimationScheduler>>) -> Self {
        // Set global scheduler so components can access animations without context
        let handle = animations.lock().unwrap().handle();
        set_global_scheduler(handle);

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
            motion_generation: 0,
            last_synced_motion_generation: std::cell::Cell::new(0),
        }
    }

    /// Bump the motion generation counter. Public methods that
    /// mutate `stable_motions` (insert, state-change, remove, clear)
    /// call this so `sync_shared_motion_states` can skip the write
    /// when nothing has changed. Cheap (one wrapping_add).
    #[inline]
    pub fn bump_motion_generation(&mut self) {
        self.motion_generation = self.motion_generation.wrapping_add(1);
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
            // Skip when nothing has changed since the last sync.
            // `motion_generation` advances on any mutation of
            // `stable_motions` — insert / remove / state transition
            // inside a tick / explicit exit / replay. At idle every
            // motion sits in `Visible` and the generation doesn't
            // move, so the bg-thread path was burning ~50 String
            // clones + 50 HashMap inserts per frame against the
            // shared `RwLock` for no observable change. Per-frame
            // gate matches the same pattern we use elsewhere
            // (state_fingerprint for stylesheet apply).
            if self.last_synced_motion_generation.get() == self.motion_generation {
                return;
            }
            let mut states = shared.write().unwrap();
            states.clear();
            for (key, motion) in &self.stable_motions {
                let state = match &motion.state {
                    MotionState::Suspended => MotionAnimationState::Suspended,
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
            self.last_synced_motion_generation
                .set(self.motion_generation);
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
        // Calculate delta time. `elapsed_ms()` is ms-resolution and macOS
        // winit can deliver back-to-back Frame events at microsecond cadence
        // (memory note: "winit on macOS delivers `request_redraw`'d frames
        // back-to-back at microsecond cadence"). When the clock hasn't
        // advanced between consecutive ticks the raw delta is 0, motion
        // progress doesn't advance, and the FSM gets stuck mid-animation
        // until something pushes the wall clock forward — exactly the
        // "animation freezes near the end until I move my mouse" symptom.
        //
        // Floor `dt_ms` at 1 ms so a tick that observed *any* time advance
        // (real or coalesced into the same millisecond bucket) still moves
        // the FSM forward. The cap-paced wake_at scheduler already
        // guarantees no more than one frame per cap interval in steady
        // state; this floor is just for the sub-ms storm.
        let raw_dt_ms = if let Some(last_time) = self.last_tick_time {
            (current_time_ms.saturating_sub(last_time)) as f32
        } else {
            16.0 // Assume ~60fps for first frame
        };
        let dt_ms = raw_dt_ms.max(1.0);
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
            for state in self.node_states.values_mut() {
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

        // Mirror the stable-motion redraw-self-perpetuate guard for node-based
        // motions — same FSM-poll race could otherwise leave a node-bound
        // enter / exit animation stalled until the next external input.
        if motion_active {
            crate::stateful::request_redraw();
        }

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
            MotionState::Suspended => true, // Still animating (waiting for start)
            MotionState::Visible => false,  // Not animating
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
        self.node_states.entry(node_id).or_default()
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
            let state = self.node_states.entry(node_id).or_default();
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
            let state = self.node_states.entry(node_id).or_default();
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
            let state = self.node_states.entry(node_id).or_default();
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
            let state = self.node_states.entry(node_id).or_default();
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
    #[allow(clippy::too_many_arguments)]
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
    ///
    /// **No-op if the node already has an active motion in flight.**
    /// `initialize_motion_animations` runs after every layout pass (full
    /// rebuild + every subtree rebuild + every Stateful refresh), so a
    /// node carrying motion config gets this method invoked many times
    /// per its lifetime. Without the existing-state guard, every frame
    /// reset the FSM back to `Entering { progress: 0.0 }` and the
    /// animation could never advance — visible as a transient-motion
    /// page that stays at `scale=0, opacity=0` forever (GH #39
    /// follow-up). Mirrors the same guard `start_stable_motion` has on
    /// `Suspended | Waiting | Entering | Visible`.
    pub fn start_enter_motion(&mut self, node_id: LayoutNodeId, config: MotionAnimation) {
        if let Some(state) = self.node_states.get(&node_id) {
            if let Some(ref motion) = state.motion {
                match motion.state {
                    MotionState::Suspended
                    | MotionState::Waiting { .. }
                    | MotionState::Entering { .. }
                    | MotionState::Visible
                    | MotionState::Exiting { .. } => return,
                    MotionState::Removed => {
                        // Allow restart after a completed exit.
                    }
                }
            }
        }

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

    /// Get the current motion values for a node.
    ///
    /// Completed motions keep their final values so rendering can still read
    /// them; use [`Self::is_motion_active`] to decide whether a redraw loop is
    /// still needed.
    pub fn get_motion_values(&self, node_id: LayoutNodeId) -> Option<&MotionKeyframe> {
        self.get(node_id)
            .and_then(|s| s.motion.as_ref())
            .map(|m| &m.current)
    }

    /// Check if a node's motion is still animating.
    pub fn is_motion_active(&self, node_id: LayoutNodeId) -> bool {
        self.get(node_id)
            .is_some_and(NodeRenderState::has_active_motion)
    }

    /// Check if a node's motion animation is complete and should be removed
    pub fn is_motion_removed(&self, node_id: LayoutNodeId) -> bool {
        self.get(node_id)
            .and_then(|s| s.motion.as_ref())
            .map(|m| matches!(m.state, MotionState::Removed))
            .unwrap_or(false)
    }

    /// Diagnostic: how many per-node render states exist. Walked every
    /// frame by the redraw predicate, so unbounded growth here shows up
    /// directly as rising idle CPU.
    pub fn node_state_count(&self) -> usize {
        self.node_states.len()
    }

    /// Diagnostic: how many stable-keyed motions exist. Same reasoning.
    pub fn stable_motion_count(&self) -> usize {
        self.stable_motions.len()
    }

    /// Active motion animations on nodes that were actually PAINTED.
    ///
    /// The visibility-gated counterpart to [`Self::has_active_motions`].
    /// The redraw predicate gates its other terms on the painted set —
    /// visual animations, flip animations and CSS animations all have
    /// `_visible` variants — but motions did not, so an enter/exit
    /// animation on a node scrolled far out of view kept the redraw
    /// chain alive at full frame rate with nothing on screen moving.
    ///
    /// Stable-keyed motions (overlays) are NOT filtered: they are
    /// positioned outside the scrolled tree and their node ids are not
    /// in the painted set even when the overlay is plainly visible.
    pub fn has_active_motions_visible(
        &self,
        painted: &std::collections::HashSet<LayoutNodeId>,
    ) -> bool {
        self.node_states
            .iter()
            .any(|(id, s)| painted.contains(id) && s.has_active_motion())
            || self
                .stable_motions
                .values()
                .any(|m| !matches!(m.state, MotionState::Visible | MotionState::Removed))
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
    /// Motion exit is now triggered explicitly via `MotionHandle.exit()` /
    /// `start_stable_motion_exit()` instead of the old `is_exiting` flag.
    pub fn start_stable_motion(&mut self, key: &str, config: MotionAnimation, replay: bool) {
        // Mark this key as used this frame (for garbage collection)
        self.stable_motions_used.insert(key.to_string());

        // Check if motion already exists
        if let Some(existing) = self.stable_motions.get_mut(key) {
            // NOTE: replay flag is intentionally ignored here for existing motions.
            // The replay mechanism via this flag doesn't work correctly because
            // initialize_motion_animations is called for ALL motions in the tree,
            // not just the ones that changed. Use `replay_stable_motion(key)` instead,
            // called from an on_ready callback when the motion is first mounted.

            // If already animating, suspended, or visible, leave it alone
            match existing.state {
                MotionState::Suspended
                | MotionState::Waiting { .. }
                | MotionState::Entering { .. }
                | MotionState::Visible => {
                    // Don't restart - animation is either suspended, in progress, or completed
                    return;
                }
                MotionState::Exiting { .. } => {
                    // Motion is exiting - do NOT cancel it automatically.
                    // Exit animations should only be cancelled by an explicit cancel_exit() call.
                    // This ensures exit animations play fully even when the tree is rebuilt
                    // (e.g., during overlay close animation where content is still rendered).
                    tracing::debug!(
                        "Motion '{}': Exiting, continuing exit animation (use cancel_exit() to interrupt)",
                        key
                    );
                    return;
                }
                // Motion was removed (exit animation completed) - do NOT restart!
                // The motion should only restart when it's been fully cleaned up from stable_motions
                // (via end_stable_motion_frame) and then created fresh. This prevents the enter
                // animation from replaying immediately after exit completes while the overlay
                // content is still being rendered during the Closing state.
                MotionState::Removed => {
                    tracing::debug!(
                        "Motion '{}': Removed state, NOT restarting (wait for cleanup)",
                        key
                    );
                    return;
                }
            }
        }

        // Create new motion
        tracing::debug!(
            "Motion '{}': Creating new motion (enter_duration={}ms)",
            key,
            config.enter_duration_ms
        );
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

    /// Cancel a stable motion's exit animation and return to Visible state
    ///
    /// Used when an overlay's close is cancelled (e.g., mouse re-enters hover card).
    /// This interrupts the exit animation and immediately sets the motion to fully visible.
    pub fn cancel_stable_motion_exit(&mut self, key: &str) {
        if let Some(motion) = self.stable_motions.get_mut(key) {
            if matches!(motion.state, MotionState::Exiting { .. }) {
                // Return to fully visible state
                motion.state = MotionState::Visible;
                motion.current = MotionKeyframe::default(); // Reset to default (fully visible)
            }
        }
    }

    /// Start a suspended stable-keyed motion
    ///
    /// Unlike `start_stable_motion`, this creates the motion in `Suspended` state.
    /// The motion renders with opacity 0 and waits for an explicit `start()` call
    /// via `MotionHandle.start()` to begin the enter animation.
    ///
    /// This is useful for tab transitions and other cases where you want to:
    /// 1. Mount the content invisibly
    /// 2. Perform any setup/measurement
    /// 3. Then trigger the animation manually
    ///
    /// **Important**: Unlike regular motions, suspended motions will reset to
    /// `Suspended` state when this is called again, even if the motion is already
    /// `Visible`. This enables tab-like behavior where each time a tab becomes
    /// active, it resets to suspended and waits for explicit `start()`.
    ///
    /// # Parameters
    ///
    /// * `key` - Stable string key for this motion
    /// * `config` - Animation configuration (enter/exit animations)
    ///
    /// Returns `true` if a new motion was created or an existing one was reset
    /// (meaning the on_ready callback should be re-registered).
    pub fn start_stable_motion_suspended(&mut self, key: &str, config: MotionAnimation) -> bool {
        // Mark this key as used this frame (for garbage collection)
        self.stable_motions_used.insert(key.to_string());

        // Check if motion already exists
        if let Some(existing) = self.stable_motions.get_mut(key) {
            match existing.state {
                // Already suspended or animating - leave it alone
                MotionState::Suspended
                | MotionState::Waiting { .. }
                | MotionState::Entering { .. } => {
                    return false;
                }
                // Motion is visible - leave it visible, don't reset
                // The suspended animation is only for first appearance
                // Re-entry animations should use .replay() instead
                MotionState::Visible => {
                    return false;
                }
                // Exiting - let it finish, don't interrupt
                MotionState::Exiting { .. } => {
                    return false;
                }
                MotionState::Removed => {
                    tracing::debug!(
                        "Motion '{}': Removed state, NOT restarting suspended (wait for cleanup)",
                        key
                    );
                    return false;
                }
            }
        }

        // Create new suspended motion
        tracing::debug!(
            "Motion '{}': Creating new SUSPENDED motion (will wait for start())",
            key
        );

        // Initial keyframe has opacity 0 for suspended state
        let mut current = config.enter_from.clone().unwrap_or_default();
        current.opacity = Some(0.0);

        self.stable_motions.insert(
            key.to_string(),
            ActiveMotion {
                config,
                state: MotionState::Suspended,
                current,
            },
        );

        // New motion created, on_ready callback should be registered
        true
    }

    /// Start the enter animation for a suspended motion
    ///
    /// Transitions a motion from `Suspended` → `Waiting` or `Entering` state.
    /// No-op if the motion is not in `Suspended` state.
    pub fn start_suspended_motion(&mut self, key: &str) {
        if let Some(motion) = self.stable_motions.get_mut(key) {
            if matches!(motion.state, MotionState::Suspended) {
                let config = &motion.config;
                tracing::debug!(
                    "Motion '{}': Starting from suspended (enter_duration={}ms)",
                    key,
                    config.enter_duration_ms
                );

                // Transition to the appropriate next state
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

                // Reset current values to enter_from state
                motion.current = if matches!(motion.state, MotionState::Visible) {
                    MotionKeyframe::default()
                } else {
                    config.enter_from.clone().unwrap_or_default()
                };
            }
        }
    }

    /// Process all pending motion starts from the global queue
    ///
    /// Call this during the render loop to start any suspended motions
    /// that were queued via `queue_global_motion_start()`.
    pub fn process_global_motion_starts(&mut self) {
        let keys = take_global_motion_starts();
        for key in keys {
            self.start_suspended_motion(&key);
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

    /// Process all pending motion exit cancels from the global queue
    ///
    /// Call this during the render loop to cancel any exit animations
    /// that were queued via `queue_global_motion_exit_cancel()`.
    pub fn process_global_motion_exit_cancels(&mut self) {
        let keys = take_global_motion_exit_cancels();
        for key in keys {
            self.cancel_stable_motion_exit(&key);
        }
    }

    /// Process all pending motion exit starts from the global queue
    ///
    /// Call this during the render loop to start any exit animations
    /// that were queued via `queue_global_motion_exit_start()`.
    pub fn process_global_motion_exit_starts(&mut self) {
        let keys = take_global_motion_exit_starts();
        for key in keys {
            self.start_stable_motion_exit(&key);
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

    /// Get the current motion values for a stable-keyed animation.
    ///
    /// Completed motions keep their final values so rendering can still read
    /// them; use [`Self::is_stable_motion_active`] to decide whether a redraw
    /// loop is still needed.
    pub fn get_stable_motion_values(&self, key: &str) -> Option<&MotionKeyframe> {
        self.stable_motions.get(key).map(|m| &m.current)
    }

    /// Check if a stable-keyed motion is still animating.
    pub fn is_stable_motion_active(&self, key: &str) -> bool {
        self.stable_motions
            .get(key)
            .is_some_and(|m| !matches!(m.state, MotionState::Visible | MotionState::Removed))
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
                MotionState::Suspended => MotionAnimationState::Suspended,
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
        // Bump the motion generation when any motion is mid-flight —
        // its tick advances progress / can transition state, both of
        // which need to flow to `shared_motion_states`. Cheap check
        // before the iteration: if every motion is at a settled
        // terminal state (Visible, Removed, Suspended), tick is a
        // no-op and `sync_shared_motion_states` doesn't need to
        // rewrite the shared store. The Settled-fast-path is the
        // common case at idle.
        let any_mid_flight = self.stable_motions.values().any(|m| {
            !matches!(
                m.state,
                MotionState::Visible | MotionState::Removed | MotionState::Suspended
            )
        });
        if any_mid_flight {
            self.motion_generation = self.motion_generation.wrapping_add(1);
            // Force the runner to fire another frame.
            //
            // The post-frame redraw chain in the windowed runner OR's
            // `rs.has_active_motions()` into `any_redraw_signal`, which
            // is supposed to flip `frame_dirty` + arm the next wake. In
            // practice users saw stable-motion animations freeze
            // mid-flight until a mouse-move kicked the loop — the
            // chain's wake path raced the FSM-poll in a way that
            // sometimes settled before the post-frame check observed
            // it. Asserting `NEEDS_REDRAW` directly from the tick
            // guarantees the next Frame gate cannot skip while *any*
            // stable motion is still entering / exiting / waiting.
            crate::stateful::request_redraw();
        }
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
            MotionState::Suspended | MotionState::Visible | MotionState::Removed => {
                // Suspended: waiting for explicit start() call - no tick needed
                // Visible/Removed: animation complete - nothing to do
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
    /// For motions that weren't accessed this frame (content removed from tree):
    /// - If they have an exit animation, transition to Exiting state
    /// - If already Exiting and complete (Removed), actually remove them
    /// - This enables CSS-like behavior: mount = enter, unmount = exit
    pub fn end_stable_motion_frame(&mut self) {
        // Collect keys to remove (motions that completed exit)
        let mut to_remove = Vec::new();

        for (key, motion) in self.stable_motions.iter_mut() {
            if !self.stable_motions_used.contains(key) {
                // This motion's content was not in the tree this frame
                match &motion.state {
                    MotionState::Removed => {
                        // Exit complete, safe to remove
                        tracing::debug!("Motion '{}': Removed state, cleaning up", key);
                        to_remove.push(key.clone());
                    }
                    MotionState::Exiting { .. } => {
                        // Already exiting, let it continue
                    }
                    _ => {
                        // Not exiting yet - start exit animation
                        if motion.config.exit_to.is_some() && motion.config.exit_duration_ms > 0 {
                            tracing::debug!(
                                "Motion '{}': Starting exit animation ({}ms)",
                                key,
                                motion.config.exit_duration_ms
                            );
                            motion.state = MotionState::Exiting {
                                progress: 0.0,
                                duration_ms: motion.config.exit_duration_ms as f32,
                            };
                            motion.current = MotionKeyframe::default(); // Start from visible
                        } else {
                            // No exit animation configured, remove immediately
                            tracing::debug!(
                                "Motion '{}': No exit config, removing immediately",
                                key
                            );
                            to_remove.push(key.clone());
                        }
                    }
                }
            }
        }

        // Remove completed motions
        for key in to_remove {
            self.stable_motions.remove(&key);
        }
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

    #[test]
    fn test_completed_node_motion_is_not_active() {
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
        let mut state = RenderState::new(scheduler);
        let node_id = LayoutNodeId::default();
        let config = MotionAnimation {
            enter_from: Some(MotionKeyframe::new().opacity(0.0)),
            enter_duration_ms: 50,
            ..Default::default()
        };

        state.start_enter_motion(node_id, config);
        assert!(state.is_motion_active(node_id));

        state.tick(0);
        state.tick(100);

        assert!(!state.is_motion_active(node_id));
        assert!(state.get_motion_values(node_id).is_some());
    }

    #[test]
    fn test_completed_stable_motion_is_not_active() {
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
        let mut state = RenderState::new(scheduler);
        let config = MotionAnimation {
            enter_from: Some(MotionKeyframe::new().opacity(0.0)),
            enter_duration_ms: 50,
            ..Default::default()
        };

        state.start_stable_motion("motion:navmenu_test", config, false);
        assert!(state.is_stable_motion_active("motion:navmenu_test"));

        state.tick(0);
        state.tick(100);

        assert!(!state.is_stable_motion_active("motion:navmenu_test"));
        assert!(
            state
                .get_stable_motion_values("motion:navmenu_test")
                .is_some()
        );
    }
}

#[cfg(test)]
mod motion_visibility_tests {
    use super::*;
    use crate::element::MotionAnimation;
    use std::collections::HashSet;

    /// An enter/exit motion on a node that was not painted must not
    /// report as active.
    ///
    /// This is what keeps a scrolled-away animation from pinning the
    /// redraw chain: the windowed runner ORs this into the signal that
    /// decides whether to request another frame, so an ungated `true`
    /// here means vsync forever with nothing on screen moving.
    #[test]
    fn an_unpainted_motion_does_not_report_active() {
        let scheduler = std::sync::Arc::new(std::sync::Mutex::new(
            blinc_animation::AnimationScheduler::new(),
        ));
        let mut rs = RenderState::new(scheduler);
        let node = LayoutNodeId::default();
        // A delay is enough to put the motion in `Waiting`, which counts
        // as active. `default()` lands straight on `Visible`, which does
        // not — and would make every assertion below vacuous.
        let config = MotionAnimation {
            enter_delay_ms: 5_000,
            ..Default::default()
        };
        rs.start_enter_motion(node, config);

        assert!(
            rs.has_active_motions(),
            "the ungated view still sees it — that is what it is for"
        );

        let nothing_painted: HashSet<LayoutNodeId> = HashSet::new();
        assert!(
            !rs.has_active_motions_visible(&nothing_painted),
            "but a node that was never painted must not keep frames coming"
        );

        let painted: HashSet<LayoutNodeId> = [node].into_iter().collect();
        assert!(
            rs.has_active_motions_visible(&painted),
            "and a painted one still must, or entry animations would not run"
        );
    }
}
