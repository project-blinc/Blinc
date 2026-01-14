//! Stateful elements with user-defined state types
//!
//! Provides `Stateful<S>` - a generic stateful element where users define
//! their own state enum/type and use pattern matching in callbacks:
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//! use blinc_core::Color;
//!
//! // Define your own state type
//! #[derive(Clone, Copy, PartialEq, Eq, Hash)]
//! enum ButtonState {
//!     Idle,
//!     Hovered,
//!     Pressed,
//!     Disabled,
//! }
//!
//! // Map events to state transitions
//! impl StateTransitions for ButtonState {
//!     fn on_event(&self, event: u32) -> Option<Self> {
//!         use blinc_core::events::event_types::*;
//!         match (self, event) {
//!             (ButtonState::Idle, POINTER_ENTER) => Some(ButtonState::Hovered),
//!             (ButtonState::Hovered, POINTER_LEAVE) => Some(ButtonState::Idle),
//!             (ButtonState::Hovered, POINTER_DOWN) => Some(ButtonState::Pressed),
//!             (ButtonState::Pressed, POINTER_UP) => Some(ButtonState::Hovered),
//!             (ButtonState::Pressed, POINTER_LEAVE) => Some(ButtonState::Idle),
//!             _ => None,
//!         }
//!     }
//! }
//!
//! let button = Stateful::new(ButtonState::Idle)
//!     .w(100.0)
//!     .h(40.0)
//!     .on_state(|state, div| {
//!         match state {
//!             ButtonState::Idle => {
//!                 *div = div.swap().bg(Color::BLUE).rounded(4.0);
//!             }
//!             ButtonState::Hovered => {
//!                 *div = div.swap().bg(Color::CYAN).rounded(8.0);
//!             }
//!             ButtonState::Pressed => {
//!                 *div = div.swap().bg(Color::BLUE).scale(0.97);
//!             }
//!             ButtonState::Disabled => {
//!                 *div = div.swap().bg(Color::GRAY).opacity(0.5);
//!             }
//!         }
//!     })
//!     .child(text("Click me"));
//! ```
//!
//! State callbacks receive the current state for pattern matching and a
//! mutable reference to the inner `Div` for full mutation capability.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, RwLock};

use crate::div::{Div, ElementBuilder, ElementRef, ElementTypeId};
use crate::element::RenderProps;
use crate::tree::{LayoutNodeId, LayoutTree};
use blinc_animation::{
    AnimatedKeyframe, AnimatedTimeline, AnimatedValue, Easing, SchedulerHandle, SpringConfig,
};
use blinc_core::reactive::SignalId;

/// Re-export SharedAnimatedValue from motion module
pub use crate::motion::SharedAnimatedValue;

/// Shared animated timeline that can be cloned and accessed from multiple places
pub type SharedAnimatedTimeline = Arc<Mutex<AnimatedTimeline>>;

/// Shared keyframe animation that can be cloned and accessed from multiple places
pub type SharedKeyframeTrack = Arc<Mutex<AnimatedKeyframe>>;

/// Handle for interacting with an animated timeline
///
/// Provides convenient methods without requiring manual mutex locking.
#[derive(Clone)]
pub struct TimelineHandle {
    inner: SharedAnimatedTimeline,
}

impl TimelineHandle {
    /// Get the current value for a timeline entry
    pub fn get(&self, entry_id: blinc_animation::TimelineEntryId) -> Option<f32> {
        self.inner.lock().unwrap().get(entry_id)
    }

    /// Restart the timeline from the beginning
    pub fn restart(&self) {
        self.inner.lock().unwrap().restart();
    }

    /// Start the timeline (if not already playing)
    pub fn start(&self) {
        self.inner.lock().unwrap().start();
    }

    /// Stop the timeline
    pub fn stop(&self) {
        self.inner.lock().unwrap().stop();
    }

    /// Check if the timeline is currently playing
    pub fn is_playing(&self) -> bool {
        self.inner.lock().unwrap().is_playing()
    }
}

/// Handle for interacting with an animated keyframe track
///
/// Provides convenient methods without requiring manual mutex locking.
#[derive(Clone)]
pub struct KeyframeHandle {
    inner: SharedKeyframeTrack,
}

/// Builder for keyframe animations with a user-friendly API
///
/// Collects keyframe points and settings, then builds into an AnimatedKeyframe.
pub struct KeyframeBuilder {
    points: Vec<(u32, f32)>, // (time_ms, value)
    default_easing: Easing,
    ping_pong: bool,
    iterations: i32,
    delay_ms: u32,
    auto_start: bool,
}

impl KeyframeBuilder {
    /// Create a new keyframe builder
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            default_easing: Easing::Linear,
            ping_pong: false,
            iterations: 1,
            delay_ms: 0,
            auto_start: false,
        }
    }

    /// Add a keyframe at the given time (in milliseconds)
    pub fn at(mut self, time_ms: u32, value: f32) -> Self {
        self.points.push((time_ms, value));
        self
    }

    /// Set the default easing function for keyframes
    pub fn ease(mut self, easing: Easing) -> Self {
        self.default_easing = easing;
        self
    }

    /// Enable ping-pong mode (reverse direction on each iteration)
    pub fn ping_pong(mut self) -> Self {
        self.ping_pong = true;
        self
    }

    /// Set number of iterations (-1 for infinite)
    pub fn loop_count(mut self, count: i32) -> Self {
        self.iterations = count;
        self
    }

    /// Enable infinite looping
    pub fn loop_infinite(mut self) -> Self {
        self.iterations = -1;
        self
    }

    /// Set delay before animation starts (in milliseconds)
    pub fn delay(mut self, delay_ms: u32) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    /// Auto-start the animation when created
    pub fn start(mut self) -> Self {
        self.auto_start = true;
        self
    }

    /// Build into an AnimatedKeyframe using the given scheduler handle
    pub(crate) fn build_with_handle(self, handle: SchedulerHandle) -> AnimatedKeyframe {
        // Calculate duration from keyframe points
        let duration_ms = self.points.iter().map(|(t, _)| *t).max().unwrap_or(0);

        // Create AnimatedKeyframe
        let mut anim = AnimatedKeyframe::new(handle, duration_ms);

        // Add keyframes (convert time_ms to normalized 0.0-1.0)
        for (time_ms, value) in &self.points {
            let time = if duration_ms > 0 {
                *time_ms as f32 / duration_ms as f32
            } else {
                0.0
            };
            anim = anim.keyframe(time, *value, self.default_easing);
        }

        // Apply settings
        anim = anim
            .iterations(self.iterations)
            .ping_pong(self.ping_pong)
            .delay(self.delay_ms);

        if self.auto_start {
            anim = anim.auto_start(true);
        }

        // Build and register with scheduler
        anim.build()
    }
}

impl Default for KeyframeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyframeHandle {
    /// Get the current animated value
    pub fn get(&self) -> f32 {
        self.inner.lock().unwrap().get()
    }

    /// Get the current progress (0.0 to 1.0)
    pub fn progress(&self) -> f32 {
        self.inner.lock().unwrap().progress()
    }

    /// Start the animation
    pub fn start(&self) {
        self.inner.lock().unwrap().start();
    }

    /// Stop the animation
    pub fn stop(&self) {
        self.inner.lock().unwrap().stop();
    }

    /// Restart the animation from the beginning
    pub fn restart(&self) {
        self.inner.lock().unwrap().restart();
    }

    /// Check if the animation is currently playing
    pub fn is_playing(&self) -> bool {
        self.inner.lock().unwrap().is_playing()
    }
}

/// Global storage for persisted animated values keyed by stateful context
static PERSISTED_ANIMATED_VALUES: LazyLock<RwLock<HashMap<String, SharedAnimatedValue>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Global storage for persisted animated timelines keyed by stateful context
static PERSISTED_ANIMATED_TIMELINES: LazyLock<RwLock<HashMap<String, SharedAnimatedTimeline>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Global storage for persisted keyframe tracks keyed by stateful context
static PERSISTED_KEYFRAME_TRACKS: LazyLock<RwLock<HashMap<String, SharedKeyframeTrack>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

// =========================================================================
// Global Redraw Flag
// =========================================================================

/// Global flag for requesting a redraw without tree rebuild
static NEEDS_REDRAW: AtomicBool = AtomicBool::new(false);

/// Request a redraw without rebuilding the tree
///
/// This is used by stateful elements when state changes cause visual updates
/// but don't require a tree structure change.
pub fn request_redraw() {
    NEEDS_REDRAW.store(true, Ordering::SeqCst);
}

/// Check and clear the redraw flag
/// Returns true if a redraw was requested since last check
pub fn take_needs_redraw() -> bool {
    NEEDS_REDRAW.swap(false, Ordering::SeqCst)
}

/// Peek at the redraw flag without clearing it
/// Used by iOS needs_render() to check if stateful updates are pending
pub fn peek_needs_redraw() -> bool {
    NEEDS_REDRAW.load(Ordering::SeqCst)
}

// =========================================================================
// Pending Prop Updates Queue
// =========================================================================

/// Queue of pending render prop updates (node_id, new_props)
///
/// When a stateful element's state changes, it computes new RenderProps
/// and queues the update here. The windowed app applies these updates
/// directly to the RenderTree, avoiding a full tree rebuild.
static PENDING_PROP_UPDATES: LazyLock<Mutex<Vec<(LayoutNodeId, RenderProps)>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Queue of pending subtree rebuilds
///
/// Each entry contains the parent node ID and the children to rebuild.
/// Children are stored as boxed Div elements (the result of the callback).
static PENDING_SUBTREE_REBUILDS: LazyLock<Mutex<Vec<PendingSubtreeRebuild>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// A pending subtree rebuild operation
pub struct PendingSubtreeRebuild {
    /// The parent node whose children should be rebuilt
    pub parent_id: LayoutNodeId,
    /// The new child element (a Div that was produced by the callback)
    pub new_child: crate::div::Div,
    /// Whether this rebuild requires layout recomputation
    /// False for visual-only updates (hover/press state changes)
    pub needs_layout: bool,
}

// Safety: PendingSubtreeRebuild is only accessed from the main thread
unsafe impl Send for PendingSubtreeRebuild {}

/// Queue a subtree rebuild for a node (with layout recomputation)
pub fn queue_subtree_rebuild(parent_id: LayoutNodeId, new_child: crate::div::Div) {
    PENDING_SUBTREE_REBUILDS
        .lock()
        .unwrap()
        .push(PendingSubtreeRebuild {
            parent_id,
            new_child,
            needs_layout: true,
        });
}

/// Queue a visual-only subtree rebuild (no layout recomputation)
///
/// Used for hover/press state changes where children's visual props change
/// but the tree structure remains the same.
pub fn queue_visual_subtree_rebuild(parent_id: LayoutNodeId, new_child: crate::div::Div) {
    PENDING_SUBTREE_REBUILDS
        .lock()
        .unwrap()
        .push(PendingSubtreeRebuild {
            parent_id,
            new_child,
            needs_layout: false,
        });
}

/// Take all pending subtree rebuilds
///
/// Called by the windowed app to apply incremental child updates to the RenderTree.
pub fn take_pending_subtree_rebuilds() -> Vec<PendingSubtreeRebuild> {
    std::mem::take(&mut *PENDING_SUBTREE_REBUILDS.lock().unwrap())
}

/// Put subtree rebuilds back in the queue (for other trees to process)
pub fn requeue_subtree_rebuilds(rebuilds: Vec<PendingSubtreeRebuild>) {
    PENDING_SUBTREE_REBUILDS.lock().unwrap().extend(rebuilds);
}

/// Check if there are pending subtree rebuilds without consuming them
///
/// Used to determine if layout recomputation is needed before processing.
pub fn has_pending_subtree_rebuilds() -> bool {
    !PENDING_SUBTREE_REBUILDS.lock().unwrap().is_empty()
}

/// Registry of stateful elements with signal dependencies
///
/// Maps stateful_key -> (deps, refresh_fn) where refresh_fn triggers a rebuild.
/// The windowed app checks these when signals change.
/// Using a HashMap with unique keys ensures that re-registration replaces the old
/// entry instead of accumulating duplicates on each rebuild.
/// Uses Arc instead of Box to allow cloning callbacks before releasing the lock.
static STATEFUL_DEPS: LazyLock<
    Mutex<std::collections::HashMap<u64, (Vec<SignalId>, Arc<dyn Fn() + Send + Sync>)>>,
> = LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// Register a stateful element's dependencies
///
/// Called internally when `.deps()` is used on a stateful element.
/// The `stateful_key` should be a unique identifier for the stateful instance
/// (e.g., pointer address of the SharedState). Re-registering with the same key
/// replaces the previous entry, preventing accumulation of stale callbacks.
pub(crate) fn register_stateful_deps(
    stateful_key: u64,
    deps: Vec<SignalId>,
    refresh_fn: Arc<dyn Fn() + Send + Sync>,
) {
    STATEFUL_DEPS
        .lock()
        .unwrap()
        .insert(stateful_key, (deps, refresh_fn));
}

/// Check all registered stateful deps against changed signals and trigger rebuilds
///
/// Called by windowed app after signal updates.
/// Returns true if any deps matched and subtree rebuilds were queued.
pub fn check_stateful_deps(changed_signals: &[SignalId]) -> bool {
    // Collect matching refresh callbacks first, then release lock before calling them.
    // This prevents deadlock when refresh callbacks call use_signal() -> use_effect()
    // -> register_stateful_deps() which would try to acquire the same lock.
    let callbacks_to_call: Vec<Arc<dyn Fn() + Send + Sync>> = {
        let registry = STATEFUL_DEPS.lock().unwrap();
        if !changed_signals.is_empty() {
            tracing::debug!(
                "check_stateful_deps: checking {} changed signals against {} registered statefuls",
                changed_signals.len(),
                registry.len()
            );
        }
        registry
            .iter()
            .filter_map(|(key, (deps, refresh_fn))| {
                if deps.iter().any(|d| changed_signals.contains(d)) {
                    tracing::debug!(
                        "check_stateful_deps: will trigger refresh for stateful_key={}",
                        key
                    );
                    Some(Arc::clone(refresh_fn))
                } else {
                    None
                }
            })
            .collect()
    };
    // Lock is now released - safe to call callbacks that may re-acquire the lock
    let triggered = !callbacks_to_call.is_empty();
    for callback in callbacks_to_call {
        callback();
    }
    triggered
}

// =========================================================================
// Animation-Driven Refresh Registry
// =========================================================================

/// Registry of stateful elements with active animations
///
/// Maps stateful_key -> (animation_keys, refresh_fn) where animation_keys are
/// the persisted animated value keys and refresh_fn triggers a callback re-run.
/// The windowed app checks these on animation frames to update animating statefuls.
static STATEFUL_ANIMATIONS: LazyLock<
    Mutex<std::collections::HashMap<u64, (Vec<String>, Arc<dyn Fn() + Send + Sync>)>>,
> = LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// Register a stateful element for animation-driven refresh
///
/// Called internally when `use_spring()` or `use_animated_value()` is used
/// and the animation is active (not settled).
pub(crate) fn register_stateful_animation(
    stateful_key: u64,
    animation_keys: Vec<String>,
    refresh_fn: Arc<dyn Fn() + Send + Sync>,
) {
    STATEFUL_ANIMATIONS
        .lock()
        .unwrap()
        .insert(stateful_key, (animation_keys, refresh_fn));
}

/// Unregister a stateful element from animation refresh
///
/// Called when all animations for a stateful have settled.
#[allow(dead_code)]
pub(crate) fn unregister_stateful_animation(stateful_key: u64) {
    STATEFUL_ANIMATIONS.lock().unwrap().remove(&stateful_key);
}

/// Check all registered statefuls with animations and trigger refresh for active ones
///
/// Called by windowed app on animation frames (when scheduler.take_needs_redraw() is true).
/// Returns true if any statefuls were refreshed.
pub fn check_stateful_animations() -> bool {
    // Collect callbacks to call and animation keys to check
    let entries: Vec<(u64, Vec<String>, Arc<dyn Fn() + Send + Sync>)> = {
        let registry = STATEFUL_ANIMATIONS.lock().unwrap();
        registry
            .iter()
            .map(|(key, (anim_keys, refresh_fn))| (*key, anim_keys.clone(), Arc::clone(refresh_fn)))
            .collect()
    };

    if entries.is_empty() {
        return false;
    }

    // Check which statefuls have active (not settled) animations
    let persisted_values = PERSISTED_ANIMATED_VALUES.read().unwrap();
    let persisted_timelines = PERSISTED_ANIMATED_TIMELINES.read().unwrap();
    let persisted_keyframes = PERSISTED_KEYFRAME_TRACKS.read().unwrap();
    let mut callbacks_to_call = Vec::new();
    let mut settled_statefuls = Vec::new();

    for (stateful_key, anim_keys, refresh_fn) in entries {
        let mut has_active = false;
        for anim_key in &anim_keys {
            // Check spring/animated values
            if let Some(animated) = persisted_values.get(anim_key) {
                if let Ok(guard) = animated.lock() {
                    if guard.is_animating() {
                        has_active = true;
                        break;
                    }
                }
            }
            // Check timelines
            if let Some(timeline) = persisted_timelines.get(anim_key) {
                if let Ok(guard) = timeline.lock() {
                    if guard.is_playing() {
                        has_active = true;
                        break;
                    }
                }
            }
            // Check keyframe tracks
            if let Some(track) = persisted_keyframes.get(anim_key) {
                if let Ok(mut guard) = track.lock() {
                    if guard.is_playing() {
                        has_active = true;
                        break;
                    }
                }
            }
        }

        if has_active {
            callbacks_to_call.push(refresh_fn);
        } else {
            // All animations settled - mark for unregistration
            settled_statefuls.push(stateful_key);
        }
    }
    drop(persisted_values);
    drop(persisted_timelines);
    drop(persisted_keyframes);

    // Unregister settled statefuls
    if !settled_statefuls.is_empty() {
        let mut registry = STATEFUL_ANIMATIONS.lock().unwrap();
        for key in settled_statefuls {
            registry.remove(&key);
        }
    }

    // Call refresh callbacks (lock is released)
    let triggered = !callbacks_to_call.is_empty();
    for callback in callbacks_to_call {
        callback();
    }

    triggered
}

/// Check if there are stateful elements registered for animation refresh
pub fn has_animating_statefuls() -> bool {
    !STATEFUL_ANIMATIONS.lock().unwrap().is_empty()
}

/// Take all pending prop updates
///
/// Called by the windowed app to apply incremental updates to the RenderTree.
/// Returns the queued updates and clears the queue.
pub fn take_pending_prop_updates() -> Vec<(LayoutNodeId, RenderProps)> {
    std::mem::take(&mut *PENDING_PROP_UPDATES.lock().unwrap())
}

/// Queue a render props update for a node
///
/// Called by stateful elements when their state changes, or by
/// `ElementHandle::mark_visual_dirty()` for explicit visual updates.
///
/// This queues a visual-only update that skips layout recomputation.
/// Use this for changes to background, opacity, shadows, etc.
pub fn queue_prop_update(node_id: LayoutNodeId, props: RenderProps) {
    PENDING_PROP_UPDATES.lock().unwrap().push((node_id, props));
    request_redraw();
}

// =========================================================================
// State Traits
// =========================================================================

/// Trait for user-defined state types that can handle event transitions
///
/// Implement this trait on your state enum to define how events cause
/// state transitions.
///
/// # Example
///
/// ```ignore
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
/// enum MyButtonState {
///     #[default]
///     Idle,
///     Hovered,
///     Pressed,
/// }
///
/// impl StateTransitions for MyButtonState {
///     fn on_event(&self, event: u32) -> Option<Self> {
///         use blinc_core::events::event_types::*;
///         match (self, event) {
///             (MyButtonState::Idle, POINTER_ENTER) => Some(MyButtonState::Hovered),
///             (MyButtonState::Hovered, POINTER_LEAVE) => Some(MyButtonState::Idle),
///             (MyButtonState::Hovered, POINTER_DOWN) => Some(MyButtonState::Pressed),
///             (MyButtonState::Pressed, POINTER_UP) => Some(MyButtonState::Hovered),
///             _ => None,
///         }
///     }
/// }
/// ```
pub trait StateTransitions:
    Clone + Copy + PartialEq + Eq + Hash + Send + Sync + std::fmt::Debug + 'static
{
    /// Handle an event and return the new state, or None if no transition
    fn on_event(&self, event: u32) -> Option<Self>;
}

/// A no-op state type for dependency-based refreshing without state transitions
///
/// Use this when you need `stateful()` for reactive dependency tracking
/// but don't have actual state transitions.
///
/// # Example
///
/// ```ignore
/// // Rebuild when `direction` signal changes, no state machine needed
/// stateful::<NoState>()
///     .deps(&[direction.signal_id()])
///     .on_state(|_ctx| {
///         div().child(build_content(direction.get()))
///     })
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NoState;

impl StateTransitions for NoState {
    fn on_event(&self, _event: u32) -> Option<Self> {
        None // Never transitions
    }
}

/// Trait for converting user state to/from internal u32 representation
///
/// This is auto-implemented for types that implement `Into<u32>` and `TryFrom<u32>`.
pub trait StateId: Clone + Copy + PartialEq + Eq + Hash + Send + Sync + 'static {
    /// Convert to internal u32 state ID
    fn to_id(&self) -> u32;

    /// Convert from internal u32 state ID
    fn from_id(id: u32) -> Option<Self>;
}

// =========================================================================
// State Callback Types
// =========================================================================

/// Callback type for state changes with user state type
/// Wrapped in Arc so it can be cloned for incremental updates
pub type StateCallback<S> = Arc<dyn Fn(&S, &mut Div) + Send + Sync>;

// =========================================================================
// Built-in State Types
// =========================================================================

/// Common button interaction states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ButtonState {
    #[default]
    Idle,
    Hovered,
    Pressed,
    Disabled,
}

impl StateTransitions for ButtonState {
    fn on_event(&self, event: u32) -> Option<Self> {
        use blinc_core::events::event_types::*;
        match (self, event) {
            // Desktop mouse: enter → hover
            (ButtonState::Idle, POINTER_ENTER) => Some(ButtonState::Hovered),
            (ButtonState::Hovered, POINTER_LEAVE) => Some(ButtonState::Idle),

            // Mouse/touch: press down
            (ButtonState::Hovered, POINTER_DOWN) => Some(ButtonState::Pressed),
            (ButtonState::Idle, POINTER_DOWN) => Some(ButtonState::Pressed), // Touch: no hover first

            // Mouse/touch: release
            (ButtonState::Pressed, POINTER_UP) => Some(ButtonState::Hovered),

            // Touch: finger lifted outside (or mouse left while pressed)
            (ButtonState::Pressed, POINTER_LEAVE) => Some(ButtonState::Idle),

            (ButtonState::Disabled, _) => None, // Disabled ignores all events
            _ => None,
        }
    }
}

/// Toggle states (on/off)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ToggleState {
    #[default]
    Off,
    On,
}

impl StateTransitions for ToggleState {
    fn on_event(&self, event: u32) -> Option<Self> {
        use blinc_core::events::event_types::*;
        match (self, event) {
            (ToggleState::Off, POINTER_UP) => Some(ToggleState::On),
            (ToggleState::On, POINTER_UP) => Some(ToggleState::Off),
            _ => None,
        }
    }
}

/// Checkbox states combining checked status and hover
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CheckboxState {
    #[default]
    UncheckedIdle,
    UncheckedHovered,
    CheckedIdle,
    CheckedHovered,
}

impl CheckboxState {
    /// Returns true if the checkbox is checked
    pub fn is_checked(&self) -> bool {
        matches!(
            self,
            CheckboxState::CheckedIdle | CheckboxState::CheckedHovered
        )
    }

    /// Returns true if the checkbox is hovered
    pub fn is_hovered(&self) -> bool {
        matches!(
            self,
            CheckboxState::UncheckedHovered | CheckboxState::CheckedHovered
        )
    }
}

impl StateTransitions for CheckboxState {
    fn on_event(&self, event: u32) -> Option<Self> {
        use blinc_core::events::event_types::*;
        match (self, event) {
            // Unchecked transitions
            (CheckboxState::UncheckedIdle, POINTER_ENTER) => Some(CheckboxState::UncheckedHovered),
            (CheckboxState::UncheckedHovered, POINTER_LEAVE) => Some(CheckboxState::UncheckedIdle),
            (CheckboxState::UncheckedHovered, POINTER_UP) => Some(CheckboxState::CheckedHovered),
            // Checked transitions
            (CheckboxState::CheckedIdle, POINTER_ENTER) => Some(CheckboxState::CheckedHovered),
            (CheckboxState::CheckedHovered, POINTER_LEAVE) => Some(CheckboxState::CheckedIdle),
            (CheckboxState::CheckedHovered, POINTER_UP) => Some(CheckboxState::UncheckedHovered),
            _ => None,
        }
    }
}

/// Text field focus states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextFieldState {
    #[default]
    Idle,
    Hovered,
    Focused,
    FocusedHovered,
    Disabled,
}

/// Scroll container states for webkit-style bounce scroll
///
/// State machine for handling scroll behavior with inertia and spring bounce:
///
/// ```text
///                    SCROLL
///     Idle ─────────────────────► Scrolling
///       ▲                            │
///       │                            │ SCROLL_END (velocity > 0)
///       │ settled                    ▼
///       └───────────── Decelerating ─┘
///       │                   │
///       │ settled           │ hit edge
///       │                   ▼
///       └───────────── Bouncing
/// ```
///
/// # Events
///
/// - `SCROLL` (30): Active scroll input (wheel/trackpad)
/// - `SCROLL_END` (31): User stopped scrolling, begin deceleration
/// - `ANIMATION_TICK` (internal): Spring/deceleration update
///
/// # Bounce Physics
///
/// When content scrolls past edges, enters `Bouncing` state with spring
/// animation that pulls content back to bounds. Uses `blinc_animation::Spring`
/// with webkit-style wobbly configuration for natural feel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ScrollState {
    /// No scrolling, content at rest
    #[default]
    Idle,
    /// Active user scrolling (receiving scroll events)
    Scrolling,
    /// Momentum scrolling after user release (inertia)
    Decelerating,
    /// Overscroll spring animation (bouncing back to bounds)
    Bouncing,
}

impl ScrollState {
    /// Returns true if the scroll is actively moving (not idle)
    pub fn is_active(&self) -> bool {
        !matches!(self, ScrollState::Idle)
    }

    /// Returns true if spring bounce animation is active
    pub fn is_bouncing(&self) -> bool {
        matches!(self, ScrollState::Bouncing)
    }

    /// Returns true if decelerating with momentum
    pub fn is_decelerating(&self) -> bool {
        matches!(self, ScrollState::Decelerating)
    }
}

/// Internal events for scroll animation (not exposed to users)
pub mod scroll_events {
    /// Animation tick (spring/deceleration update)
    pub const ANIMATION_TICK: u32 = 10000;
    /// Velocity has settled to zero
    pub const SETTLED: u32 = 10001;
    /// Scroll hit content edge (overscroll)
    pub const HIT_EDGE: u32 = 10002;
}

impl StateTransitions for ScrollState {
    fn on_event(&self, event: u32) -> Option<Self> {
        use blinc_core::events::event_types::*;
        use scroll_events::*;

        match (self, event) {
            // Idle -> Scrolling: User starts scrolling
            (ScrollState::Idle, SCROLL) => Some(ScrollState::Scrolling),

            // Scrolling -> Scrolling: Continue receiving scroll events (no change)
            (ScrollState::Scrolling, SCROLL) => None,

            // Scrolling -> Decelerating: User released, start momentum
            (ScrollState::Scrolling, SCROLL_END) => Some(ScrollState::Decelerating),

            // Scrolling -> Bouncing: Hit edge while scrolling
            (ScrollState::Scrolling, HIT_EDGE) => Some(ScrollState::Bouncing),

            // Decelerating -> Idle: Velocity settled
            (ScrollState::Decelerating, SETTLED) => Some(ScrollState::Idle),

            // Decelerating -> Bouncing: Hit edge during momentum
            (ScrollState::Decelerating, HIT_EDGE) => Some(ScrollState::Bouncing),

            // Decelerating -> Scrolling: User scrolls during deceleration
            (ScrollState::Decelerating, SCROLL) => Some(ScrollState::Scrolling),

            // Bouncing -> Idle: Spring settled
            (ScrollState::Bouncing, SETTLED) => Some(ScrollState::Idle),

            // Bouncing -> Scrolling: User scrolls during bounce
            (ScrollState::Bouncing, SCROLL) => Some(ScrollState::Scrolling),

            _ => None,
        }
    }
}

impl TextFieldState {
    /// Returns true if the text field is focused
    pub fn is_focused(&self) -> bool {
        matches!(
            self,
            TextFieldState::Focused | TextFieldState::FocusedHovered
        )
    }

    /// Returns true if the text field is hovered
    pub fn is_hovered(&self) -> bool {
        matches!(
            self,
            TextFieldState::Hovered | TextFieldState::FocusedHovered
        )
    }
}

impl StateTransitions for TextFieldState {
    fn on_event(&self, event: u32) -> Option<Self> {
        use blinc_core::events::event_types::*;
        match (self, event) {
            // Idle transitions
            (TextFieldState::Idle, POINTER_ENTER) => Some(TextFieldState::Hovered),
            (TextFieldState::Idle, FOCUS) => Some(TextFieldState::Focused),
            // Hovered transitions
            (TextFieldState::Hovered, POINTER_LEAVE) => Some(TextFieldState::Idle),
            (TextFieldState::Hovered, POINTER_DOWN) => Some(TextFieldState::Focused),
            (TextFieldState::Hovered, FOCUS) => Some(TextFieldState::FocusedHovered),
            // Focused transitions
            (TextFieldState::Focused, BLUR) => Some(TextFieldState::Idle),
            (TextFieldState::Focused, POINTER_ENTER) => Some(TextFieldState::FocusedHovered),
            // FocusedHovered transitions
            (TextFieldState::FocusedHovered, POINTER_LEAVE) => Some(TextFieldState::Focused),
            (TextFieldState::FocusedHovered, BLUR) => Some(TextFieldState::Hovered),
            // Disabled ignores all events
            (TextFieldState::Disabled, _) => None,
            _ => None,
        }
    }
}

/// Unit type implements StateTransitions as a no-op state
///
/// Use `()` as a dummy handle for stateful elements when you need
/// reactive rebuilding via `.deps()` but don't need state transitions.
///
/// # Example
///
/// ```ignore
/// let counter = ctx.use_state_keyed("counter", || 0);
///
/// // Use () when no explicit state type is needed
/// stateful(ctx.use_state(()))
///     .deps(&[counter.signal_id()])
///     .on_state(move |_, div| {
///         div.merge(text(&format!("Count: {}", counter.get())));
///     })
/// ```
impl StateTransitions for () {
    fn on_event(&self, _event: u32) -> Option<Self> {
        None // No state transitions - always stays as ()
    }
}

// =========================================================================
// Stateful<S> - Generic Stateful Element
// =========================================================================

/// A stateful element with user-defined state type
///
/// The state type `S` must implement `StateTransitions` to define how
/// events cause state changes. Use the `on_state` callback to apply
/// visual changes based on state using pattern matching.
///
/// # Example
///
/// ```ignore
/// use blinc_layout::prelude::*;
///
/// let button = Stateful::new(ButtonState::Idle)
///     .w(100.0).h(40.0)
///     .on_state(|state, div| match state {
///         ButtonState::Idle => { *div = div.swap().bg(Color::BLUE); }
///         ButtonState::Hovered => { *div = div.swap().bg(Color::CYAN); }
///         ButtonState::Pressed => { *div = div.swap().bg(Color::BLUE).scale(0.97); }
///         ButtonState::Disabled => { *div = div.swap().bg(Color::GRAY); }
///     });
/// ```
pub struct Stateful<S: StateTransitions> {
    /// Inner div with all layout/visual properties
    /// Uses RefCell for interior mutability during build()
    inner: RefCell<Div>,

    /// Shared state that event handlers can mutate
    shared_state: Arc<Mutex<StatefulInner<S>>>,

    /// Children cache - populated after on_state callback is applied during build()
    /// This allows children_builders() to return a reference even with RefCell inner
    children_cache: RefCell<Vec<Box<dyn ElementBuilder>>>,

    /// Event handlers cache - populated during register_state_handlers() so that
    /// event_handlers() can return a stable reference for the renderer to capture
    event_handlers_cache: RefCell<crate::event_handler::EventHandlers>,

    /// Layout bounds storage - updated synchronously after each layout
    layout_bounds: crate::renderer::LayoutBoundsStorage,

    /// Layout bounds change callback - invoked synchronously when bounds change
    layout_bounds_cb: Option<crate::renderer::LayoutBoundsCallback>,
}

/// Internal state for `Stateful<S>`, wrapped in `Arc<Mutex<...>>` for event handler access
///
/// This is exposed publicly so that `SharedState<S>` can be created externally
/// for state persistence across rebuilds.
pub struct StatefulInner<S: StateTransitions> {
    /// Current state
    pub state: S,

    /// State change callback (receives state for pattern matching)
    /// Note: This is boxed and stored here, but the actual Div is updated
    /// when the Stateful is rebuilt or when render_props() is called.
    pub(crate) state_callback: Option<StateCallback<S>>,

    /// Flag indicating visual state changed and callback should be re-applied
    pub(crate) needs_visual_update: bool,

    /// Base render props (before state callback is applied)
    /// This captures the element's properties like rounded corners, shadows, etc.
    /// When state changes, we start from base and apply callback changes on top.
    pub(crate) base_render_props: Option<RenderProps>,

    /// Base taffy Style (before state callback is applied)
    /// This captures layout properties like width, height, overflow, padding, etc.
    /// When rebuilding subtree, we start from base style to preserve container properties.
    pub(crate) base_style: Option<taffy::Style>,

    /// The layout node ID for this element (set on first event)
    /// Used to apply incremental prop updates without tree rebuild
    pub(crate) node_id: Option<LayoutNodeId>,

    /// Signal dependencies - when any of these change, refresh props
    pub(crate) deps: Vec<SignalId>,

    /// Ancestor motion key (if inside a motion container)
    ///
    /// Set during tree building when the stateful element is inside a Motion.
    /// Used to check if the ancestor motion is animating before applying
    /// hover/press state transitions. This allows children to blend with
    /// the motion animation.
    pub(crate) ancestor_motion_key: Option<String>,

    /// Current event being processed (set during event handler invocation)
    /// This allows `StateContext::event()` to access the triggering event.
    pub(crate) current_event: Option<crate::event_handler::EventContext>,

    /// Refresh callback for re-registering deps dynamically
    /// Set during StatefulBuilder::on_state() and used by use_effect()
    pub(crate) refresh_callback: Option<Arc<dyn Fn() + Send + Sync>>,

    /// Animation keys used by this stateful (for animation-driven refresh)
    /// Updated after each callback invocation with keys of active animations.
    pub(crate) animation_keys: Vec<String>,
}

impl<S: StateTransitions> StatefulInner<S> {
    /// Create a new StatefulInner with the given initial state
    pub fn new(state: S) -> Self {
        Self {
            state,
            state_callback: None,
            needs_visual_update: false,
            base_render_props: None,
            base_style: None,
            node_id: None,
            deps: Vec::new(),
            ancestor_motion_key: None,
            current_event: None,
            refresh_callback: None,
            animation_keys: Vec::new(),
        }
    }
}

impl<S: StateTransitions + Default> Default for Stateful<S> {
    fn default() -> Self {
        Self::new(S::default())
    }
}

// Note: We can't implement Deref/DerefMut because the Div is behind a Mutex.
// Instead, we provide explicit builder methods that lock and update the div.

/// Shared state handle for `Stateful<S>` elements
///
/// This can be created externally and passed to multiple `Stateful` elements,
/// or stored for persistence across rebuilds (e.g., via `ctx.use_state()`).
pub type SharedState<S> = Arc<Mutex<StatefulInner<S>>>;

/// Get or create a persistent `SharedState<S>` for the given key.
///
/// This bridges `BlincContextState` (which stores arbitrary values via signals)
/// with `SharedState<S>` (which `Stateful` needs for FSM state management).
///
/// The state persists across UI rebuilds, making it safe to use in loops and closures
/// when combined with unique keys (e.g., from `InstanceKey`).
///
/// # Type Parameters
///
/// - `S`: The state type, must implement `StateTransitions + Default + Clone + Send + Sync`
///
/// # Example
///
/// ```ignore
/// use blinc_layout::prelude::*;
///
/// // Get or create a button state for a unique key
/// let button_state = use_shared_state::<ButtonState>("my-button");
///
/// // Use with Stateful
/// Stateful::with_shared_state(button_state)
///     .on_state(|state, div| { /* ... */ })
///
/// // Works with any state type
/// let checkbox_state = use_shared_state::<CheckboxState>("my-checkbox");
/// ```
pub fn use_shared_state<S>(key: &str) -> SharedState<S>
where
    S: StateTransitions + Default + Clone + Send + Sync + 'static,
{
    use blinc_core::context_state::BlincContextState;

    let ctx = BlincContextState::get();

    // We store the SharedState wrapped in an Option inside the signal
    // This way it persists across rebuilds
    let state: blinc_core::State<Option<SharedState<S>>> = ctx.use_state_keyed(key, || None);

    let existing = state.get();
    if let Some(shared) = existing {
        shared
    } else {
        // First time - create the SharedState and store it
        let shared: SharedState<S> = Arc::new(Mutex::new(StatefulInner::new(S::default())));
        state.set(Some(shared.clone()));
        shared
    }
}

/// Get or create a persistent `SharedState<S>` with a custom initial state.
///
/// Like `use_shared_state`, but allows specifying a non-default initial state.
///
/// # Example
///
/// ```ignore
/// // Start in a specific state
/// let state = use_shared_state_with::<ButtonState>("my-button", ButtonState::Disabled);
/// ```
pub fn use_shared_state_with<S>(key: &str, initial: S) -> SharedState<S>
where
    S: StateTransitions + Clone + Send + Sync + 'static,
{
    use blinc_core::context_state::BlincContextState;

    let ctx = BlincContextState::get();

    let state: blinc_core::State<Option<SharedState<S>>> = ctx.use_state_keyed(key, || None);

    let existing = state.get();
    if let Some(shared) = existing {
        shared
    } else {
        let shared: SharedState<S> = Arc::new(Mutex::new(StatefulInner::new(initial)));
        state.set(Some(shared.clone()));
        shared
    }
}

// =========================================================================
// StateContext API (New Stateful Container Design)
// =========================================================================

/// Counter for generating stable child keys within a stateful context
///
/// This tracks how many elements of each type have been created,
/// enabling deterministic key generation for motion/animation stability.
#[derive(Default)]
pub struct ChildKeyCounter {
    /// Counts by element type: "div" -> 0, 1, 2...
    counters: std::collections::HashMap<&'static str, usize>,
    /// Current hierarchy path for nested contexts
    path: Vec<String>,
}

impl ChildKeyCounter {
    /// Create a new counter
    pub fn new() -> Self {
        Self {
            counters: std::collections::HashMap::new(),
            path: Vec::new(),
        }
    }

    /// Get the next index for an element type and increment the counter
    pub fn next(&mut self, element_type: &'static str) -> usize {
        let index = self.counters.entry(element_type).or_insert(0);
        let current = *index;
        *index += 1;
        current
    }

    /// Reset all counters (called before each callback invocation)
    pub fn reset(&mut self) {
        self.counters.clear();
        self.path.clear();
    }

    /// Push a hierarchy level
    pub fn push(&mut self, segment: String) {
        self.path.push(segment);
    }

    /// Pop a hierarchy level
    pub fn pop(&mut self) {
        self.path.pop();
    }

    /// Get the current path as a string
    pub fn path_string(&self) -> String {
        self.path.join("->")
    }
}

/// Context for stateful elements providing scoped state management
///
/// `StateContext` is the core innovation of the new stateful API. It provides:
/// - Current state value for pattern matching
/// - Scoped signal/store factories that persist across rebuilds
/// - Automatic child key derivation for stable motion animations
/// - Dispatch method for triggering state transitions
///
/// # Example
///
/// ```ignore
/// stateful::<AccordionState>()
///     .on_state(|ctx| {
///         // Get current state for pattern matching
///         match ctx.state() {
///             AccordionState::Collapsed => div().h(0.0),
///             AccordionState::Expanded => div().h_auto(),
///         }
///     })
/// ```
#[derive(Clone)]
pub struct StateContext<S: StateTransitions> {
    /// Current state value
    state: S,

    /// Stable key for this stateful container
    key: Arc<String>,

    /// Counter for child key derivation (shared across clones)
    child_counter: Arc<RefCell<ChildKeyCounter>>,

    /// Access to reactive graph for signals/effects
    reactive: blinc_core::context_state::SharedReactiveGraph,

    /// Shared state handle for mutations
    shared_state: SharedState<S>,

    /// Parent context key for hierarchical nesting
    parent_key: Option<Arc<String>>,

    /// Signal dependencies registered via .deps()
    deps: Vec<blinc_core::SignalId>,

    /// The event that triggered this callback (if any)
    /// None when triggered by dependency changes, Some when triggered by user events.
    event: Option<crate::event_handler::EventContext>,

    /// Animation keys used during this callback (for animation-driven refresh)
    animation_keys: Arc<RefCell<Vec<String>>>,

    /// Timeline references tracked for animation refresh
    /// Stores (key, timeline) pairs - checked at end of callback to see if playing
    timeline_refs: Arc<RefCell<Vec<(String, SharedAnimatedTimeline)>>>,

    /// Keyframe track references tracked for animation refresh
    /// Stores (key, track) pairs - checked at end of callback to see if playing
    keyframe_refs: Arc<RefCell<Vec<(String, SharedKeyframeTrack)>>>,
}

impl<S: StateTransitions> StateContext<S> {
    /// Create a new StateContext
    pub(crate) fn new(
        state: S,
        key: String,
        reactive: blinc_core::context_state::SharedReactiveGraph,
        shared_state: SharedState<S>,
        parent_key: Option<Arc<String>>,
        deps: Vec<blinc_core::SignalId>,
        event: Option<crate::event_handler::EventContext>,
    ) -> Self {
        Self {
            state,
            key: Arc::new(key.clone()),
            child_counter: Arc::new(RefCell::new(ChildKeyCounter::new())),
            reactive,
            shared_state,
            parent_key,
            deps,
            event,
            animation_keys: Arc::new(RefCell::new(Vec::new())),
            timeline_refs: Arc::new(RefCell::new(Vec::new())),
            keyframe_refs: Arc::new(RefCell::new(Vec::new())),
        }
    }

    /// Get the animation keys used during this callback
    ///
    /// This includes spring animation keys, timeline keys for any timelines
    /// that are currently playing, and keyframe track keys for any that are playing.
    pub(crate) fn take_animation_keys(&self) -> Vec<String> {
        let mut keys = std::mem::take(&mut *self.animation_keys.borrow_mut());

        // Check timeline refs and include keys for any that are playing
        let timeline_refs = self.timeline_refs.borrow();
        for (key, timeline) in timeline_refs.iter() {
            if timeline.lock().unwrap().is_playing() {
                keys.push(key.clone());
            }
        }

        // Check keyframe refs and include keys for any that are playing
        let keyframe_refs = self.keyframe_refs.borrow();
        for (key, track) in keyframe_refs.iter() {
            if track.lock().unwrap().is_playing() {
                keys.push(key.clone());
            }
        }

        keys
    }

    /// Add an animation key to track (deduplicates)
    fn track_animation_key(&self, key: String) {
        let mut keys = self.animation_keys.borrow_mut();
        if !keys.contains(&key) {
            keys.push(key);
        }
    }

    /// Get the current state for pattern matching
    pub fn state(&self) -> S {
        self.state
    }

    /// Get the event that triggered this callback (if any)
    ///
    /// Returns `Some(EventContext)` when the callback was triggered by a user event
    /// (click, hover, key press, etc.), or `None` when triggered by dependency changes.
    ///
    /// # Example
    ///
    /// ```ignore
    /// stateful::<ButtonState>()
    ///     .on_state(|ctx| {
    ///         if let Some(event) = ctx.event() {
    ///             match event.event_type {
    ///                 POINTER_UP => println!("Clicked at ({}, {})", event.local_x, event.local_y),
    ///                 _ => {}
    ///             }
    ///         }
    ///         div()
    ///     })
    /// ```
    pub fn event(&self) -> Option<&crate::event_handler::EventContext> {
        self.event.as_ref()
    }

    /// Get the stable key for this stateful container
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Get the full key including parent hierarchy
    pub fn full_key(&self) -> String {
        match &self.parent_key {
            Some(parent) => format!("{}:{}", parent, self.key),
            None => self.key.to_string(),
        }
    }

    /// Dispatch an event to trigger a state transition
    ///
    /// This updates the shared state and triggers a visual update.
    pub fn dispatch(&self, event: u32) {
        let mut inner = self.shared_state.lock().unwrap();
        if let Some(new_state) = inner.state.on_event(event) {
            inner.state = new_state;
            inner.needs_visual_update = true;
            drop(inner);
            request_redraw();
        }
    }

    /// Create/retrieve a persistent signal scoped to this stateful
    ///
    /// The signal is keyed with format: `{stateful_key}:signal:{name}`
    /// This ensures the signal persists across rebuilds.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let scroll_pos = ctx.use_signal("scroll", || 0.0);
    /// scroll_pos.set(100.0);
    /// ```
    pub fn use_signal<T, F>(&self, name: &str, init: F) -> blinc_core::State<T>
    where
        T: Clone + Send + 'static,
        F: FnOnce() -> T,
    {
        let signal_key = format!("{}:signal:{}", self.full_key(), name);
        let state = blinc_core::context_state::use_state_keyed(&signal_key, init);

        // Automatically register as dependency so on_state re-runs when signal changes
        self.use_effect(&state);

        state
    }

    /// Derive a stable key for a child element
    ///
    /// Format: `{stateful_key}:{element_type}:{index}`
    /// or with path: `{stateful_key}:{path}->{element_type}:{index}`
    ///
    /// This is called automatically by motion containers when inside
    /// a stateful context with auto-keying enabled.
    pub fn derive_child_key(&self, element_type: &'static str) -> String {
        let mut counter = self.child_counter.borrow_mut();
        let index = counter.next(element_type);
        let path = counter.path_string();

        if path.is_empty() {
            format!("{}:{}:{}", self.full_key(), element_type, index)
        } else {
            format!("{}:{}->{}:{}", self.full_key(), path, element_type, index)
        }
    }

    /// Push a hierarchy level for nested child key derivation
    pub fn push_hierarchy(&self, segment: &str) {
        self.child_counter.borrow_mut().push(segment.to_string());
    }

    /// Pop a hierarchy level
    pub fn pop_hierarchy(&self) {
        self.child_counter.borrow_mut().pop();
    }

    /// Reset the child counter (called before each callback invocation)
    pub(crate) fn reset_counter(&self) {
        self.child_counter.borrow_mut().reset();
    }

    /// Get the shared state handle (for internal use)
    pub(crate) fn shared_state(&self) -> &SharedState<S> {
        &self.shared_state
    }

    /// Get the reactive graph (for internal use)
    pub(crate) fn reactive(&self) -> &blinc_core::context_state::SharedReactiveGraph {
        &self.reactive
    }

    /// Create/retrieve a persistent animated value scoped to this stateful
    ///
    /// The animated value is keyed with format: `{stateful_key}:anim:{name}`
    /// This ensures the animation persists across rebuilds with the same key.
    ///
    /// Uses the global animation scheduler (must be initialized via
    /// `blinc_animation::set_global_scheduler()` at app startup).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let opacity = ctx.use_animated_value("opacity", 1.0);
    /// opacity.lock().unwrap().set_target(0.5);
    /// let current = opacity.lock().unwrap().get();
    /// ```
    pub fn use_animated_value(&self, name: &str, initial: f32) -> SharedAnimatedValue {
        self.use_animated_value_with_config(name, initial, SpringConfig::stiff())
    }

    /// Create/retrieve a persistent animated value with custom spring config
    ///
    /// Like `use_animated_value()` but allows specifying a custom spring configuration.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let scale = ctx.use_animated_value_with_config(
    ///     "scale",
    ///     1.0,
    ///     SpringConfig::bouncy(),
    /// );
    /// ```
    pub fn use_animated_value_with_config(
        &self,
        name: &str,
        initial: f32,
        config: SpringConfig,
    ) -> SharedAnimatedValue {
        let anim_key = format!("{}:anim:{}", self.full_key(), name);

        // Check if we already have this animated value
        {
            let values = PERSISTED_ANIMATED_VALUES.read().unwrap();
            if let Some(existing) = values.get(&anim_key) {
                return Arc::clone(existing);
            }
        }

        // Create a new animated value
        let handle = blinc_animation::get_scheduler();
        let animated = AnimatedValue::new(handle, initial, config);
        let shared = Arc::new(Mutex::new(animated));

        // Store it for future lookups
        {
            let mut values = PERSISTED_ANIMATED_VALUES.write().unwrap();
            values.insert(anim_key, Arc::clone(&shared));
        }

        shared
    }

    /// Declarative spring animation - set target and get current value in one call
    ///
    /// This is a convenience method that combines `use_animated_value_with_config()`,
    /// `set_target()`, and `get()` into a single declarative API.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Before (verbose):
    /// let scale = ctx.use_animated_value_with_config("scale", 1.0, SpringConfig::wobbly());
    /// scale.lock().unwrap().set_target(target_scale);
    /// let current_scale = scale.lock().unwrap().get();
    ///
    /// // After (convenient):
    /// let current_scale = ctx.use_spring("scale", target_scale, SpringConfig::wobbly());
    /// ```
    pub fn use_spring(&self, name: &str, target: f32, config: SpringConfig) -> f32 {
        let anim_key = format!("{}:anim:{}", self.full_key(), name);
        let animated = self.use_animated_value_with_config(name, target, config);
        let mut guard = animated.lock().unwrap();
        guard.set_target(target);
        let value = guard.get();

        // Track this animation key for animation-driven refresh
        // Only track if animation is active (not settled)
        if guard.is_animating() {
            self.track_animation_key(anim_key);
        }

        value
    }

    /// Declarative spring animation with default stiff config
    ///
    /// Shorthand for `use_spring(name, target, SpringConfig::stiff())`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let current_opacity = ctx.spring("opacity", if hovered { 1.0 } else { 0.5 });
    /// ```
    pub fn spring(&self, name: &str, target: f32) -> f32 {
        self.use_spring(name, target, SpringConfig::stiff())
    }

    /// Create/retrieve a persistent animated timeline scoped to this stateful
    ///
    /// The timeline is keyed with format: `{stateful_key}:timeline:{name}`
    /// This ensures the timeline persists across rebuilds with the same key.
    ///
    /// The configuration callback is only called on first use - subsequent calls
    /// return the existing entry IDs without re-running the callback.
    ///
    /// Uses the global animation scheduler (must be initialized via
    /// `blinc_animation::set_global_scheduler()` at app startup).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Configure on first use, returns (entry_ids, handle)
    /// let (entry_id, timeline) = ctx.use_timeline("fade_sequence", |t| {
    ///     let id = t.add(0, 500, 0.0, 1.0);
    ///     t.set_loop(-1);  // Loop forever
    ///     t.start();
    ///     id
    /// });
    ///
    /// // Read current value
    /// let opacity = timeline.get(entry_id).unwrap_or(0.0);
    ///
    /// // Restart on click
    /// if let Some(event) = ctx.event() {
    ///     if event.event_type == event_types::POINTER_UP {
    ///         timeline.restart();
    ///     }
    /// }
    /// ```
    pub fn use_timeline<T, F>(&self, name: &str, configure: F) -> (T, TimelineHandle)
    where
        F: FnOnce(&mut AnimatedTimeline) -> T,
        T: blinc_animation::ConfigureResult,
    {
        let timeline_key = format!("{}:timeline:{}", self.full_key(), name);

        // Get or create the timeline
        let shared = {
            let timelines = PERSISTED_ANIMATED_TIMELINES.read().unwrap();
            if let Some(existing) = timelines.get(&timeline_key) {
                Arc::clone(existing)
            } else {
                drop(timelines);

                // Create a new timeline
                let handle = blinc_animation::get_scheduler();
                let timeline = AnimatedTimeline::new(handle);
                let shared = Arc::new(Mutex::new(timeline));

                // Store it for future lookups
                let mut timelines = PERSISTED_ANIMATED_TIMELINES.write().unwrap();
                timelines.insert(timeline_key.clone(), Arc::clone(&shared));
                shared
            }
        };

        // Configure and get entry IDs
        let result = {
            let mut tl = shared.lock().unwrap();
            tl.configure(configure)
        };

        // Track this timeline for animation refresh (only if not already tracked)
        {
            let mut refs = self.timeline_refs.borrow_mut();
            if !refs.iter().any(|(k, _)| k == &timeline_key) {
                refs.push((timeline_key, Arc::clone(&shared)));
            }
        }

        (result, TimelineHandle { inner: shared })
    }

    /// Create or retrieve a persisted keyframe animation
    ///
    /// Keyframe animations provide a fluent API for defining animations with multiple
    /// keyframes, easing, and playback options like ping-pong and looping.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name for this keyframe track (scoped to this stateful element)
    /// * `configure` - Configuration closure that builds the keyframe track
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Pulsing animation with ping-pong and easing
    /// let scale = ctx.use_keyframes("scale", |k| {
    ///     k.at(0, 0.8)
    ///      .at(800, 1.2)
    ///      .ease(Easing::EaseInOut)
    ///      .ping_pong()
    ///      .loop_infinite()
    ///      .start()
    /// });
    ///
    /// let current_scale = scale.get();
    /// ```
    pub fn use_keyframes<F>(&self, name: &str, configure: F) -> KeyframeHandle
    where
        F: FnOnce(KeyframeBuilder) -> KeyframeBuilder,
    {
        let keyframe_key = format!("{}:keyframes:{}", self.full_key(), name);

        // Get or create the keyframe animation
        let shared = {
            let tracks = PERSISTED_KEYFRAME_TRACKS.read().unwrap();
            if let Some(existing) = tracks.get(&keyframe_key) {
                Arc::clone(existing)
            } else {
                drop(tracks);

                // Create new keyframe animation using the builder
                let builder = KeyframeBuilder::new();
                let configured = configure(builder);
                let handle = blinc_animation::get_scheduler();
                let anim = configured.build_with_handle(handle);
                let shared = Arc::new(Mutex::new(anim));

                // Store it for future lookups
                let mut tracks = PERSISTED_KEYFRAME_TRACKS.write().unwrap();
                tracks.insert(keyframe_key.clone(), Arc::clone(&shared));
                shared
            }
        };

        // Track this keyframe animation for animation refresh (only if not already tracked)
        {
            let mut refs = self.keyframe_refs.borrow_mut();
            if !refs.iter().any(|(k, _)| k == &keyframe_key) {
                refs.push((keyframe_key, Arc::clone(&shared)));
            }
        }

        KeyframeHandle { inner: shared }
    }

    /// Access a dependent signal's value by its index in the deps array
    ///
    /// This allows reading external signal values that were registered via `.deps()`.
    /// The index corresponds to the order in which signals were passed to `.deps()`.
    ///
    /// Returns `None` if the index is out of bounds or the signal value cannot be retrieved.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let direction_signal: State<String> = use_state(|| "down".to_string());
    ///
    /// stateful::<NoState>()
    ///     .deps([direction_signal.signal_id()])
    ///     .on_state(|ctx| {
    ///         // Access the dependency value by index
    ///         let direction: String = ctx.dep(0).unwrap_or_default();
    ///         div().child(text(&format!("Direction: {}", direction)))
    ///     })
    /// ```
    pub fn dep<T: Clone + Send + Default + 'static>(&self, index: usize) -> Option<T> {
        let signal_id = self.deps.get(index)?;
        // Reconstruct a Signal<T> from the SignalId and get its value
        let signal = blinc_core::Signal::from_id(*signal_id);
        self.reactive.lock().unwrap().get(signal)
    }

    /// Get a `State<T>` handle for a dependent signal by its index
    ///
    /// This returns a full `State<T>` handle that supports `.get()`, `.set()`, etc.
    /// Useful when you need to both read and modify the dependent signal.
    ///
    /// Returns `None` if the index is out of bounds.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let counter: State<i32> = use_state(|| 0);
    ///
    /// stateful::<ButtonState>()
    ///     .deps([counter.signal_id()])
    ///     .on_state(|ctx| {
    ///         let counter_state: State<i32> = ctx.dep_as_state(0).unwrap();
    ///         let value = counter_state.get();
    ///         div()
    ///             .child(text(&format!("Count: {}", value)))
    ///             .on_click(move |_| {
    ///                 counter_state.set(value + 1);
    ///             })
    ///     })
    /// ```
    pub fn dep_as_state<T: Clone + Send + 'static>(
        &self,
        index: usize,
    ) -> Option<blinc_core::State<T>> {
        let signal_id = self.deps.get(index)?;
        let signal = blinc_core::Signal::from_id(*signal_id);
        let dirty_flag = blinc_core::context_state::BlincContextState::get()
            .dirty_flag()
            .clone();
        Some(blinc_core::State::new(
            signal,
            self.reactive.clone(),
            dirty_flag,
        ))
    }

    /// Get the SignalId of a dependency by index
    ///
    /// Returns `None` if the index is out of bounds.
    ///
    /// # Example
    ///
    /// ```ignore
    /// stateful::<NoState>()
    ///     .deps([signal_a.signal_id(), signal_b.signal_id()])
    ///     .on_state(|ctx| {
    ///         if let Some(id) = ctx.dep_signal_id(0) {
    ///             // Use the signal ID for comparison or other purposes
    ///         }
    ///         div()
    ///     })
    /// ```
    pub fn dep_signal_id(&self, index: usize) -> Option<blinc_core::SignalId> {
        self.deps.get(index).copied()
    }

    /// Register a signal as a dependency for this stateful element
    ///
    /// When the signal changes, the `on_state` callback will be re-invoked.
    /// This is useful when you create signals with `ctx.use_signal()` and need
    /// the stateful to react to their changes.
    ///
    /// # Example
    ///
    /// ```ignore
    /// stateful::<ButtonState>()
    ///     .on_state(|ctx| {
    ///         // Create a counter signal
    ///         let count = ctx.use_signal("count", || 0);
    ///
    ///         // Register it as a dependency - on_state re-runs when count changes
    ///         ctx.use_effect(&count);
    ///
    ///         // Update count on click (via ctx.event())
    ///         if let Some(event) = ctx.event() {
    ///             if event.event_type == POINTER_UP {
    ///                 count.update(|n| n + 1);
    ///             }
    ///         }
    ///
    ///         div().child(text(&format!("Count: {}", count.get())))
    ///     })
    /// ```
    pub fn use_effect<T: Clone + Send + 'static>(&self, signal: &blinc_core::State<T>) {
        let signal_id = signal.signal_id();

        // Lock shared state and check if signal is already a dependency
        let mut inner = self.shared_state.lock().unwrap();

        if inner.deps.contains(&signal_id) {
            // Already registered, nothing to do
            return;
        }

        // Add to deps
        inner.deps.push(signal_id);
        tracing::info!(
            "use_effect: registered signal {:?}, total deps: {}",
            signal_id,
            inner.deps.len()
        );

        // Get the refresh callback and current deps
        let refresh_callback = inner.refresh_callback.clone();
        let deps = inner.deps.clone();

        // Drop lock before registering
        drop(inner);

        // Re-register with updated deps
        if let Some(callback) = refresh_callback {
            let stateful_key = Arc::as_ptr(&self.shared_state) as u64;
            tracing::debug!(
                "use_effect: re-registering deps for stateful_key={}",
                stateful_key
            );
            register_stateful_deps(stateful_key, deps, Arc::new(move || callback()));
        } else {
            tracing::warn!("use_effect: no refresh_callback available!");
        }
    }

    /// Query a motion container by name within this stateful's scope
    ///
    /// The motion key is automatically derived from the stateful's full key,
    /// so you don't need to manually track keys.
    ///
    /// # Key Format
    ///
    /// The full key is: `motion:{stateful_key}:motion:{name}:child:0`
    ///
    /// # Example
    ///
    /// ```ignore
    /// stateful::<ButtonState>()
    ///     .on_state(|ctx| {
    ///         // Create a motion with auto-derived key
    ///         let content = ctx.motion("content")
    ///             .fade_in(200)
    ///             .child(my_content());
    ///
    ///         // Query the same motion to check its state
    ///         let motion = ctx.query_motion("content");
    ///         if motion.is_settled() {
    ///             // Animation complete, enable interactions
    ///         }
    ///
    ///         div().child(content)
    ///     })
    /// ```
    pub fn query_motion(&self, name: &str) -> crate::selector::MotionHandle {
        let motion_key = format!("motion:{}:motion:{}:child:0", self.full_key(), name);
        crate::selector::query_motion(&motion_key)
    }

    /// Create a motion container with an auto-derived stable key
    ///
    /// This is the preferred way to create motion containers inside stateful callbacks.
    /// The key is automatically derived from the stateful's full key, ensuring stable
    /// animation state across rebuilds.
    ///
    /// # Key Format
    ///
    /// The motion key is: `{stateful_key}:motion:{name}`
    ///
    /// # Example
    ///
    /// ```ignore
    /// stateful::<ButtonState>()
    ///     .on_state(|ctx| {
    ///         // Motion with auto-derived key - stable across rebuilds
    ///         let content = ctx.motion("content")
    ///             .fade_in(200)
    ///             .scale_in(1.1, 300)
    ///             .child(expanded_content());
    ///
    ///         div().child(content)
    ///     })
    /// ```
    pub fn motion(&self, name: &str) -> crate::motion::Motion {
        let motion_key = format!("{}:motion:{}", self.full_key(), name);
        crate::motion::motion_derived(&motion_key)
    }
}

// =========================================================================
// StatefulBuilder - New API Entry Point
// =========================================================================

/// Type alias for the new-style stateful callback that receives StateContext
pub type StateContextCallback<S> =
    Arc<dyn Fn(&StateContext<S>) -> crate::div::Div + Send + Sync + 'static>;

/// Builder for creating stateful containers with the new StateContext API
///
/// This builder creates a `Stateful<S>` that uses `StateContext` internally,
/// providing automatic key derivation and scoped state management.
///
/// # Example
///
/// ```ignore
/// use blinc_layout::prelude::*;
///
/// stateful::<ButtonState>()
///     .on_state(|ctx| {
///         match ctx.state() {
///             ButtonState::Idle => div().bg(gray),
///             ButtonState::Hovered => div().bg(blue),
///         }
///     })
/// ```
pub struct StatefulBuilder<S: StateTransitions> {
    /// Instance key for this stateful container
    key: crate::InstanceKey,
    /// Signal dependencies for refresh
    deps: Vec<blinc_core::reactive::SignalId>,
    /// Initial state (if explicitly set)
    initial_state: Option<S>,
    /// Parent context key (for nested statefuls)
    parent_key: Option<Arc<String>>,
}

impl<S: StateTransitions + Default> StatefulBuilder<S> {
    /// Create a new StatefulBuilder with auto-generated key
    #[track_caller]
    pub fn new() -> Self {
        Self {
            key: crate::InstanceKey::new("stateful"),
            deps: Vec::new(),
            initial_state: None,
            parent_key: None,
        }
    }

    /// Set signal dependencies for refresh
    ///
    /// When any of these signals change, the stateful callback will be re-invoked.
    pub fn deps(mut self, deps: impl IntoIterator<Item = blinc_core::reactive::SignalId>) -> Self {
        self.deps = deps.into_iter().collect();
        self
    }

    /// Set initial state (defaults to `S::default()`)
    pub fn initial(mut self, state: S) -> Self {
        self.initial_state = Some(state);
        self
    }

    /// Set parent context key for hierarchical nesting
    pub fn parent_key(mut self, key: Arc<String>) -> Self {
        self.parent_key = Some(key);
        self
    }

    /// Build the stateful element with a StateContext callback
    ///
    /// The callback receives a `&StateContext<S>` and returns a `Div`.
    /// The returned Div is merged onto the base container.
    pub fn on_state<F>(self, callback: F) -> Stateful<S>
    where
        F: Fn(&StateContext<S>) -> crate::div::Div + Send + Sync + 'static,
    {
        let initial = self.initial_state.unwrap_or_default();
        let key_str = self.key.get().to_string();
        let parent_key = self.parent_key;
        let deps = self.deps;

        // Get or create persistent SharedState using the key
        let shared_state = use_shared_state_with::<S>(&key_str, initial);

        // Get the reactive graph from context
        let reactive = blinc_core::context_state::BlincContextState::get()
            .reactive()
            .clone();

        // Wrap the callback to use StateContext
        let callback = Arc::new(callback);
        let callback_clone = callback.clone();
        let key_str_clone = key_str.clone();
        let parent_key_clone = parent_key.clone();
        let reactive_clone = reactive.clone();
        let shared_state_clone = shared_state.clone();

        // Clone deps for use inside callback
        let deps_clone = deps.clone();

        // Create the legacy-style callback wrapper
        let legacy_callback: StateCallback<S> =
            Arc::new(move |state: &S, div: &mut crate::div::Div| {
                // Get current event from shared state (if any)
                let current_event = shared_state_clone.lock().unwrap().current_event.clone();

                // Create StateContext for this invocation
                let ctx = StateContext::new(
                    *state, // S is Copy, so dereference works
                    key_str_clone.clone(),
                    reactive_clone.clone(),
                    shared_state_clone.clone(),
                    parent_key_clone.clone(),
                    deps_clone.clone(),
                    current_event,
                );

                // Reset counter for deterministic key generation
                ctx.reset_counter();

                // Call user callback and get the returned Div
                let user_div = callback_clone(&ctx);

                // Set the stateful context key on the returned Div for auto-keying
                let user_div = user_div.with_stateful_context(ctx.full_key());

                // Merge the returned Div onto the base container
                div.merge(user_div);

                // Store animation keys from this callback invocation
                // This enables animation-driven refresh when springs are active
                let anim_keys = ctx.take_animation_keys();
                if !anim_keys.is_empty() {
                    shared_state_clone.lock().unwrap().animation_keys = anim_keys;
                }
            });

        // Create Stateful using the existing infrastructure
        let mut stateful = Stateful::with_shared_state(shared_state);

        // Set the callback
        stateful.shared_state.lock().unwrap().state_callback = Some(legacy_callback);
        stateful.shared_state.lock().unwrap().needs_visual_update = true;

        // Set dependencies
        if !deps.is_empty() {
            stateful.shared_state.lock().unwrap().deps = deps.clone();
        }

        // Register state handlers to enable event-driven state transitions
        // This sets up on_mouse_down, on_mouse_up, etc. to trigger StateTransitions::on_event()
        let stateful = stateful.register_state_handlers();

        // Create and store refresh callback BEFORE apply_state_callback
        // This is needed because use_signal() calls use_effect() which needs refresh_callback
        let shared = Arc::clone(&stateful.shared_state);
        let shared_for_refresh = Arc::clone(&shared);
        let refresh_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            refresh_stateful(&shared_for_refresh);
        });

        // Store refresh callback in StatefulInner for use_effect
        stateful.shared_state.lock().unwrap().refresh_callback =
            Some(Arc::clone(&refresh_callback));

        // Apply initial state callback to set up visual state
        // This may call use_signal() which needs the refresh_callback above
        stateful.apply_state_callback();

        // Register deps with the signal framework so changes trigger callback refresh
        // Note: deps may have been added by use_signal() during apply_state_callback()
        let current_deps = stateful.shared_state.lock().unwrap().deps.clone();
        let stateful_key = Arc::as_ptr(&shared) as u64;
        if !current_deps.is_empty() {
            register_stateful_deps(
                stateful_key,
                current_deps,
                Arc::new({
                    let refresh_callback = Arc::clone(&refresh_callback);
                    move || refresh_callback()
                }),
            );
        }

        // Register for animation-driven refresh if there are active animations
        // This ensures the callback re-runs while springs are animating
        let anim_keys = stateful.shared_state.lock().unwrap().animation_keys.clone();
        if !anim_keys.is_empty() {
            register_stateful_animation(stateful_key, anim_keys, Arc::clone(&refresh_callback));
        }

        stateful
    }
}

impl<S: StateTransitions + Default> Default for StatefulBuilder<S> {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a new stateful container with automatic key generation
///
/// This is the recommended way to create stateful elements. The returned
/// `StatefulBuilder` provides a fluent API for configuration.
///
/// # Example
///
/// ```ignore
/// use blinc_layout::prelude::*;
///
/// // Simple usage with pattern matching
/// stateful::<ButtonState>()
///     .on_state(|ctx| {
///         match ctx.state() {
///             ButtonState::Idle => div().bg(Color::GRAY),
///             ButtonState::Hovered => div().bg(Color::BLUE),
///         }
///     })
///
/// // With initial state and dependencies
/// stateful::<TabsState>()
///     .initial(TabsState::Tab1)
///     .deps([some_signal.id()])
///     .on_state(|ctx| {
///         let counter = ctx.use_signal("counter", || 0);
///         div().child(text(&format!("Count: {}", counter.get())))
///     })
/// ```
#[track_caller]
pub fn stateful<S: StateTransitions + Default>() -> StatefulBuilder<S> {
    StatefulBuilder::new()
}

/// Create a stateful container with an explicit key
///
/// Use this when you need deterministic key generation, such as in loops
/// or dynamic contexts where the auto-generated key might not be stable.
///
/// # Example
///
/// ```ignore
/// for (i, item) in items.iter().enumerate() {
///     stateful_with_key::<ItemState>(&format!("item_{}", i))
///         .on_state(|ctx| { /* ... */ })
/// }
/// ```
pub fn stateful_with_key<S: StateTransitions + Default>(
    key: impl Into<String>,
) -> StatefulBuilder<S> {
    StatefulBuilder {
        key: crate::InstanceKey::explicit(key),
        deps: Vec::new(),
        initial_state: None,
        parent_key: None,
    }
}

/// Trigger a refresh of the stateful element's props (internal use)
///
/// This re-runs the `on_state` callback and queues a prop update.
/// Called internally by the reactive system when dependencies change.
pub(crate) fn refresh_stateful<S: StateTransitions>(shared: &SharedState<S>) {
    Stateful::<S>::refresh_props_internal(shared);
}

impl<S: StateTransitions> Stateful<S> {
    /// Ensure the state callback is invoked if pending.
    ///
    /// This is crucial for the incremental diff system which may call
    /// `children_builders()` or `render_props()` BEFORE `build()` is called
    /// on new element instances. Without this, the diff sees stale content
    /// and incorrectly determines that children/props have changed.
    fn ensure_callback_invoked(&self) {
        let shared = self.shared_state.lock().unwrap();
        let has_callback = shared.state_callback.is_some();
        let needs_update = shared.needs_visual_update;
        tracing::trace!(
            "ensure_callback_invoked: has_callback={}, needs_update={}",
            has_callback,
            needs_update
        );
        if needs_update && has_callback {
            let callback = Arc::clone(shared.state_callback.as_ref().unwrap());
            let state_copy = shared.state;
            drop(shared); // Release lock before calling callback

            tracing::trace!("Invoking state callback for Stateful");
            // Apply callback to populate children and props
            callback(&state_copy, &mut *self.inner.borrow_mut());

            // Mark as updated
            self.shared_state.lock().unwrap().needs_visual_update = false;

            // Log children count after callback
            let children_count = self.inner.borrow().children.len();
            tracing::trace!("After callback: {} children in inner Div", children_count);
        }
    }

    /// Create a new stateful element with initial state
    pub fn new(initial_state: S) -> Self {
        Self {
            inner: RefCell::new(Div::new()),
            shared_state: Arc::new(Mutex::new(StatefulInner {
                state: initial_state,
                state_callback: None,
                needs_visual_update: false,
                base_render_props: None,
                base_style: None,
                node_id: None,
                deps: Vec::new(),
                ancestor_motion_key: None,
                current_event: None,
                refresh_callback: None,
                animation_keys: Vec::new(),
            })),
            children_cache: RefCell::new(Vec::new()),
            event_handlers_cache: RefCell::new(crate::event_handler::EventHandlers::new()),
            layout_bounds: Arc::new(std::sync::Mutex::new(None)),
            layout_bounds_cb: None,
        }
    }

    /// Create a stateful element with externally-provided shared state
    ///
    /// Use this when you need state to persist across rebuilds.
    /// The shared state can come from `WindowedContext::use_stateful_state()`
    /// or be created manually.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // State persists across rebuilds
    /// let state = ctx.use_stateful_state("my_button", ButtonState::Idle);
    /// button()
    ///     .with_state(state)
    ///     .on_state(|state, div| { ... })
    /// ```
    pub fn with_shared_state(shared_state: SharedState<S>) -> Self {
        Self {
            inner: RefCell::new(Div::new()),
            shared_state,
            children_cache: RefCell::new(Vec::new()),
            event_handlers_cache: RefCell::new(crate::event_handler::EventHandlers::new()),
            layout_bounds: Arc::new(std::sync::Mutex::new(None)),
            layout_bounds_cb: None,
        }
    }

    /// Get a clone of the shared state handle
    ///
    /// This can be stored externally for state persistence across rebuilds.
    pub fn shared_state(&self) -> SharedState<S> {
        Arc::clone(&self.shared_state)
    }

    /// Set the initial/default state
    ///
    /// This is useful when using the generic `stateful()` constructor
    /// with a custom state type.
    ///
    /// # Example
    ///
    /// ```ignore
    /// stateful()
    ///     .default_state(MyState::Ready)
    ///     .on_state(|state, div| { ... })
    /// ```
    pub fn default_state(self, state: S) -> Self {
        self.shared_state.lock().unwrap().state = state;
        self
    }

    /// Get the current state
    pub fn state(&self) -> S {
        self.shared_state.lock().unwrap().state
    }

    /// Get the render props of the inner Div
    ///
    /// This allows accessing the accumulated layout properties.
    pub fn inner_render_props(&self) -> RenderProps {
        self.inner.borrow().render_props()
    }

    /// Apply the state callback to update the inner div
    ///
    /// This is useful when you need to manually trigger a callback application,
    /// for example in a custom ElementBuilder::build() implementation.
    pub fn apply_callback(&self) {
        self.apply_state_callback();
    }

    /// Set the current state directly
    pub fn set_state(&self, state: S) {
        let mut inner = self.shared_state.lock().unwrap();
        inner.state = state;
    }

    // =========================================================================
    // State Callback
    // =========================================================================

    /// Set the state change callback
    ///
    /// The callback receives the current state for pattern matching and
    /// a mutable reference to a Div for applying visual changes.
    /// The callback is immediately applied to set the initial visual state,
    /// and event handlers are automatically registered to trigger state transitions.
    ///
    /// # Example
    ///
    /// ```ignore
    /// .on_state(|state, div| match state {
    ///     ButtonState::Idle => { *div = div.swap().bg(Color::BLUE); }
    ///     ButtonState::Hovered => { *div = div.swap().bg(Color::CYAN); }
    ///     // ...
    /// })
    /// ```
    pub fn on_state<F>(self, callback: F) -> Self
    where
        F: Fn(&S, &mut Div) + Send + Sync + 'static,
    {
        // Capture base render props and style BEFORE applying state callback
        // This preserves properties like rounded corners, shadows, overflow, etc.
        let inner_div = self.inner.borrow();
        let base_props = inner_div.render_props();
        let base_style = inner_div.layout_style().cloned();
        drop(inner_div);

        // Store the callback, base props, and base style
        {
            let mut inner = self.shared_state.lock().unwrap();
            inner.state_callback = Some(Arc::new(callback));
            inner.base_render_props = Some(base_props);
            inner.base_style = base_style;
        }

        // Register event handlers BEFORE applying state callback
        // This is important because register_state_handlers uses swap() which
        // would clear any children added by the state callback
        let s = self.register_state_handlers();

        // Apply initial state to get the initial div styling (including children)
        // Since apply_state_callback now takes &self (interior mutability), this works
        s.apply_state_callback();

        // Register deps if any were set
        let shared = Arc::clone(&s.shared_state);
        let deps = shared.lock().unwrap().deps.clone();
        if !deps.is_empty() {
            // Use Arc pointer address as unique key - prevents duplicate registrations
            // when the same Stateful is rebuilt multiple times
            let stateful_key = Arc::as_ptr(&shared) as u64;
            let shared_for_refresh = Arc::clone(&shared);
            register_stateful_deps(
                stateful_key,
                deps,
                Arc::new(move || {
                    refresh_stateful(&shared_for_refresh);
                }),
            );
        }

        s
    }

    /// Set signal dependencies for this stateful element
    ///
    /// When any of the specified signals change, the `on_state` callback
    /// will be re-run to update the element's visual props.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let direction = ctx.use_state_keyed("direction", || Direction::Vertical);
    ///
    /// stateful(button_state)
    ///     .deps(&[direction.signal_id()])
    ///     .on_state(move |state, div| {
    ///         // Read direction here - will be current on each refresh
    ///         let label = match direction.get() { ... };
    ///         div.set_child(span(label));
    ///     })
    /// ```
    pub fn deps(self, signals: &[SignalId]) -> Self {
        {
            let mut inner = self.shared_state.lock().unwrap();
            inner.deps = signals.to_vec();
        }
        self
    }

    /// Register event handlers for automatic state transitions
    ///
    /// These handlers are registered on the event_handlers_cache (not the inner Div)
    /// so that event_handlers() can return a stable reference for the renderer.
    fn register_state_handlers(self) -> Self {
        self.ensure_state_handlers_registered();
        self
    }

    /// Ensure state transition handlers are registered (idempotent)
    ///
    /// This is public so that wrappers like Button can call it to ensure
    /// the hover/press state handlers are registered even when not using on_state().
    pub fn ensure_state_handlers_registered(&self) {
        use blinc_core::events::event_types;

        let shared = Arc::clone(&self.shared_state);

        // Get mutable access to the cache
        let mut cache = self.event_handlers_cache.borrow_mut();

        // Check if already registered by looking for POINTER_ENTER handler
        // (all handlers are registered together, so checking one is sufficient)
        if cache.has_handler(event_types::POINTER_ENTER) {
            return; // Already registered
        }

        // POINTER_ENTER -> state transition
        {
            let shared_clone = Arc::clone(&shared);
            cache.on_hover_enter(move |ctx| {
                Self::handle_event_internal(
                    &shared_clone,
                    event_types::POINTER_ENTER,
                    Some(ctx.clone()),
                );
            });
        }

        // POINTER_LEAVE -> state transition
        {
            let shared_clone = Arc::clone(&shared);
            cache.on_hover_leave(move |ctx| {
                Self::handle_event_internal(
                    &shared_clone,
                    event_types::POINTER_LEAVE,
                    Some(ctx.clone()),
                );
            });
        }

        // POINTER_DOWN -> state transition
        {
            let shared_clone = Arc::clone(&shared);
            cache.on_mouse_down(move |ctx| {
                Self::handle_event_internal(
                    &shared_clone,
                    event_types::POINTER_DOWN,
                    Some(ctx.clone()),
                );
            });
        }

        // POINTER_UP -> state transition
        {
            let shared_clone = Arc::clone(&shared);
            cache.on_mouse_up(move |ctx| {
                Self::handle_event_internal(
                    &shared_clone,
                    event_types::POINTER_UP,
                    Some(ctx.clone()),
                );
            });
        }

        // DRAG -> state transition (for drag-aware states like SliderThumbState)
        {
            let shared_clone = Arc::clone(&shared);
            cache.on_drag(move |ctx| {
                Self::handle_event_internal(&shared_clone, event_types::DRAG, Some(ctx.clone()));
            });
        }

        // DRAG_END -> state transition
        {
            let shared_clone = Arc::clone(&shared);
            cache.on_drag_end(move |ctx| {
                Self::handle_event_internal(
                    &shared_clone,
                    event_types::DRAG_END,
                    Some(ctx.clone()),
                );
            });
        }
    }

    /// Internal handler for state transitions from event handlers
    ///
    /// This updates the state, computes new render props via the callback,
    /// and queues an incremental prop update. No tree rebuild is needed.
    ///
    /// If this stateful element is inside an animating motion container,
    /// the state transition is suppressed to allow children to blend with
    /// the motion animation. State transitions only apply after the motion settles.
    fn handle_event_internal(
        shared: &Arc<Mutex<StatefulInner<S>>>,
        event: u32,
        event_context: Option<crate::event_handler::EventContext>,
    ) {
        let mut guard = shared.lock().unwrap();

        // Store node_id for future use (from event context if available)
        if guard.node_id.is_none() {
            if let Some(ref ctx) = event_context {
                guard.node_id = Some(ctx.node_id);
            }
        }

        // Check if inside an animating motion container
        // If so, suppress state transitions until motion settles
        if let Some(ref motion_key) = guard.ancestor_motion_key {
            let motion_state = blinc_core::query_motion(motion_key);
            if motion_state.is_animating() {
                // Ancestor motion is still animating - suppress this state change
                // The element will respond to events once animation completes
                tracing::trace!(
                    "Suppressing state transition: ancestor motion '{}' is animating",
                    motion_key
                );
                return;
            }
        }

        // Check if state transition needed
        let new_state = match guard.state.on_event(event) {
            Some(s) if s != guard.state => s,
            _ => return,
        };

        // Update state and store current event for callback access
        guard.state = new_state;
        guard.needs_visual_update = true;
        guard.current_event = event_context;

        // Compute new props via callback and queue the update
        if let Some(ref callback) = guard.state_callback {
            let callback = Arc::clone(callback);
            let state_copy = guard.state;
            let cached_node_id = guard.node_id;
            let base_props = guard.base_render_props.clone();
            let base_style = guard.base_style.clone();
            drop(guard); // Release lock before calling callback

            // Create temp div with base style to preserve container properties (overflow, etc.)
            // Then apply callback to get state-specific changes
            let mut temp_div = if let Some(style) = base_style {
                Div::with_style(style)
            } else {
                Div::new()
            };
            callback(&state_copy, &mut temp_div);
            let callback_props = temp_div.render_props();

            // Start from base props and merge callback changes on top
            let mut final_props = base_props.unwrap_or_default();
            final_props.merge_from(&callback_props);

            // Queue the prop update for this node
            if let Some(nid) = cached_node_id {
                queue_prop_update(nid, final_props);

                // Queue visual-only subtree update for children's props (bg, border, etc)
                // This uses a non-destructive update that walks existing children
                // and updates their render props without removing/rebuilding them
                if !temp_div.children_builders().is_empty() {
                    queue_visual_subtree_rebuild(nid, temp_div);
                }
            }

            // Clear current event after callback completes
            let mut inner = shared.lock().unwrap();
            inner.current_event = None;

            // Register for animation-driven refresh if there are active animations
            // This ensures the callback re-runs while springs are animating
            let anim_keys = inner.animation_keys.clone();
            let refresh_cb = inner.refresh_callback.clone();
            drop(inner);

            if !anim_keys.is_empty() {
                if let Some(refresh_cb) = refresh_cb {
                    let stateful_key = Arc::as_ptr(&shared) as u64;
                    register_stateful_animation(stateful_key, anim_keys, refresh_cb);
                }
            }
        }

        // Just request redraw, not rebuild
        request_redraw();
    }

    /// Force re-run of the state callback and queue prop/subtree updates
    ///
    /// Call this when external state (like a label) changes and you need
    /// to update the element's props and/or children without a ButtonState change.
    fn refresh_props_internal(shared: &Arc<Mutex<StatefulInner<S>>>) {
        let mut guard = shared.lock().unwrap();

        // Clear current_event - this refresh is due to signal changes, not events.
        // Without this, the callback would see a stale event and potentially
        // re-trigger signal updates, causing infinite loops.
        guard.current_event = None;

        // Need node_id and callback to refresh
        let (callback, state_copy, cached_node_id, base_props, base_style, refresh_callback) =
            match (guard.state_callback.as_ref(), guard.node_id) {
                (Some(cb), Some(nid)) => {
                    let callback = Arc::clone(cb);
                    let state = guard.state;
                    let base = guard.base_render_props.clone();
                    let style = guard.base_style.clone();
                    let refresh_cb = guard.refresh_callback.clone();
                    drop(guard);
                    (callback, state, nid, base, style, refresh_cb)
                }
                _ => return,
            };

        // Create temp div with base style to preserve container properties (overflow, etc.)
        // Then apply callback to get state-specific changes
        let base_style_clone = base_style.clone();
        let mut temp_div = if let Some(style) = base_style {
            Div::with_style(style)
        } else {
            Div::new()
        };
        callback(&state_copy, &mut temp_div);
        let callback_props = temp_div.render_props();

        // Start from base props and merge callback changes on top
        let mut final_props = base_props.unwrap_or_default();
        final_props.merge_from(&callback_props);

        // Queue the prop update for this node
        queue_prop_update(cached_node_id, final_props);

        // Check if children were set OR if layout style changed - if so, queue a subtree rebuild
        // This is necessary because the callback may have modified height, overflow, etc.
        let children = temp_div.children_builders();
        let style_changed = temp_div.layout_style() != base_style_clone.as_ref();

        tracing::trace!(
            "refresh_props_internal: children={}, style_changed={}",
            !children.is_empty(),
            style_changed
        );

        if !children.is_empty() || style_changed {
            queue_subtree_rebuild(cached_node_id, temp_div);
        }

        // Register for animation-driven refresh if there are active animations
        // This ensures the callback re-runs while springs are animating
        let anim_keys = shared.lock().unwrap().animation_keys.clone();
        if !anim_keys.is_empty() {
            if let Some(refresh_cb) = refresh_callback {
                let stateful_key = Arc::as_ptr(shared) as u64;
                register_stateful_animation(stateful_key, anim_keys, refresh_cb);
            }
        }

        // Request redraw
        request_redraw();
    }

    /// Dispatch a new state
    ///
    /// Updates the current state and applies the callback if the state changed.
    /// Uses incremental prop/subtree updates instead of full tree rebuild.
    /// Returns true if the state changed.
    pub fn dispatch_state(&self, new_state: S) -> bool {
        let mut shared = self.shared_state.lock().unwrap();
        if shared.state != new_state {
            shared.state = new_state;
            drop(shared);
            // Use incremental update path (same as signal deps)
            Self::refresh_props_internal(&self.shared_state);
            true
        } else {
            false
        }
    }

    /// Handle an event and potentially transition state
    ///
    /// Returns true if the state changed.
    pub fn handle_event(&self, event: u32) -> bool {
        let new_state = {
            let inner = self.shared_state.lock().unwrap();
            inner.state.on_event(event)
        };
        if let Some(new_state) = new_state {
            self.dispatch_state(new_state)
        } else {
            false
        }
    }

    /// Apply the callback for the current state (if any)
    fn apply_state_callback(&self) {
        let mut shared = self.shared_state.lock().unwrap();
        // Clone callback to avoid borrow conflicts (Arc makes this cheap)
        if let Some(ref callback) = shared.state_callback {
            let callback = Arc::clone(callback);
            let state_copy = shared.state;
            shared.needs_visual_update = false;
            drop(shared); // Release lock before calling callback
            callback(&state_copy, &mut *self.inner.borrow_mut());
        }
    }

    pub fn id(self, id: &str) -> Self {
        self.merge_into_inner(Div::new().id(id));
        self
    }

    // =========================================================================
    // Builder pattern methods that return Self (not Div)
    // =========================================================================

    /// Helper to update inner Div with RefCell using merge
    fn merge_into_inner(&self, props: Div) {
        self.inner.borrow_mut().merge(props);
    }

    /// Set width (builder pattern)
    pub fn w(self, px: f32) -> Self {
        self.merge_into_inner(Div::new().w(px));
        self
    }

    /// Set height (builder pattern)
    pub fn h(self, px: f32) -> Self {
        self.merge_into_inner(Div::new().h(px));
        self
    }

    /// Set width to 100% (builder pattern)
    pub fn w_full(self) -> Self {
        self.merge_into_inner(Div::new().w_full());
        self
    }

    /// Set minimum width (builder pattern)
    pub fn min_w(self, px: f32) -> Self {
        self.merge_into_inner(Div::new().min_w(px));
        self
    }

    /// Set height to 100% (builder pattern)
    pub fn h_full(self) -> Self {
        self.merge_into_inner(Div::new().h_full());
        self
    }

    /// Set both width and height (builder pattern)
    pub fn size(self, w: f32, h: f32) -> Self {
        self.merge_into_inner(Div::new().size(w, h));
        self
    }

    /// Set square size (builder pattern)
    pub fn square(self, size: f32) -> Self {
        self.merge_into_inner(Div::new().square(size));
        self
    }

    /// Set flex direction to row (builder pattern)
    pub fn flex_row(self) -> Self {
        self.merge_into_inner(Div::new().flex_row());
        self
    }

    /// Set flex direction to column (builder pattern)
    pub fn flex_col(self) -> Self {
        self.merge_into_inner(Div::new().flex_col());
        self
    }

    /// Set flex grow (builder pattern)
    pub fn flex_grow(self) -> Self {
        self.merge_into_inner(Div::new().flex_grow());
        self
    }

    /// Set width to fit content (builder pattern)
    pub fn w_fit(self) -> Self {
        self.merge_into_inner(Div::new().w_fit());
        self
    }

    /// Set height to fit content (builder pattern)
    pub fn h_fit(self) -> Self {
        self.merge_into_inner(Div::new().h_fit());
        self
    }

    /// Set padding all sides (builder pattern)
    pub fn p(self, units: f32) -> Self {
        self.merge_into_inner(Div::new().p(units));
        self
    }

    /// Set horizontal padding (builder pattern)
    pub fn px(self, units: f32) -> Self {
        self.merge_into_inner(Div::new().px(units));
        self
    }

    /// Set vertical padding (builder pattern)
    pub fn py(self, units: f32) -> Self {
        self.merge_into_inner(Div::new().py(units));
        self
    }

    /// Set padding using a semantic Length value (builder pattern)
    pub fn padding(self, len: crate::units::Length) -> Self {
        self.merge_into_inner(Div::new().padding(len));
        self
    }

    /// Set horizontal padding using a semantic Length value (builder pattern)
    pub fn padding_x(self, len: crate::units::Length) -> Self {
        self.merge_into_inner(Div::new().padding_x(len));
        self
    }

    /// Set vertical padding using a semantic Length value (builder pattern)
    pub fn padding_y(self, len: crate::units::Length) -> Self {
        self.merge_into_inner(Div::new().padding_y(len));
        self
    }

    /// Set padding top (builder pattern)
    pub fn pt(self, units: f32) -> Self {
        self.merge_into_inner(Div::new().pt(units));
        self
    }

    /// Set padding bottom (builder pattern)
    pub fn pb(self, units: f32) -> Self {
        self.merge_into_inner(Div::new().pb(units));
        self
    }

    /// Set padding left (builder pattern)
    pub fn pl(self, units: f32) -> Self {
        self.merge_into_inner(Div::new().pl(units));
        self
    }

    /// Set padding right (builder pattern)
    pub fn pr(self, units: f32) -> Self {
        self.merge_into_inner(Div::new().pr(units));
        self
    }

    /// Set margin top (builder pattern)
    pub fn mt(self, units: f32) -> Self {
        self.merge_into_inner(Div::new().mt(units));
        self
    }

    /// Set margin bottom (builder pattern)
    pub fn mb(self, units: f32) -> Self {
        self.merge_into_inner(Div::new().mb(units));
        self
    }

    /// Set margin left (builder pattern)
    pub fn ml(self, units: f32) -> Self {
        self.merge_into_inner(Div::new().ml(units));
        self
    }

    /// Set margin right (builder pattern)
    pub fn mr(self, units: f32) -> Self {
        self.merge_into_inner(Div::new().mr(units));
        self
    }

    /// Set horizontal margin (builder pattern)
    pub fn mx(self, units: f32) -> Self {
        self.merge_into_inner(Div::new().mx(units));
        self
    }

    /// Set vertical margin (builder pattern)
    pub fn my(self, units: f32) -> Self {
        self.merge_into_inner(Div::new().my(units));
        self
    }

    /// Set margin all sides (builder pattern)
    pub fn m(self, units: f32) -> Self {
        self.merge_into_inner(Div::new().m(units));
        self
    }

    /// Set gap (builder pattern)
    pub fn gap(self, units: f32) -> Self {
        self.merge_into_inner(Div::new().gap(units));
        self
    }

    /// Align items to start (builder pattern)
    pub fn items_start(self) -> Self {
        self.merge_into_inner(Div::new().items_start());
        self
    }

    /// Center items (builder pattern)
    pub fn items_center(self) -> Self {
        self.merge_into_inner(Div::new().items_center());
        self
    }

    /// Align items to end (builder pattern)
    pub fn items_end(self) -> Self {
        self.merge_into_inner(Div::new().items_end());
        self
    }

    /// Justify content start (builder pattern)
    pub fn justify_start(self) -> Self {
        self.merge_into_inner(Div::new().justify_start());
        self
    }

    /// Center justify (builder pattern)
    pub fn justify_center(self) -> Self {
        self.merge_into_inner(Div::new().justify_center());
        self
    }

    /// Justify content end (builder pattern)
    pub fn justify_end(self) -> Self {
        self.merge_into_inner(Div::new().justify_end());
        self
    }

    /// Space between (builder pattern)
    pub fn justify_between(self) -> Self {
        self.merge_into_inner(Div::new().justify_between());
        self
    }

    /// Set background (builder pattern)
    pub fn bg(self, color: impl Into<blinc_core::Brush>) -> Self {
        self.merge_into_inner(Div::new().background(color));
        self
    }

    /// Set corner radius (builder pattern)
    pub fn rounded(self, radius: f32) -> Self {
        self.merge_into_inner(Div::new().rounded(radius));
        self
    }

    /// Set border with color and width (builder pattern)
    pub fn border(self, width: f32, color: blinc_core::Color) -> Self {
        self.merge_into_inner(Div::new().border(width, color));
        self
    }

    /// Set border color only (builder pattern)
    pub fn border_color(self, color: blinc_core::Color) -> Self {
        self.merge_into_inner(Div::new().border_color(color));
        self
    }

    /// Set border width only (builder pattern)
    pub fn border_width(self, width: f32) -> Self {
        self.merge_into_inner(Div::new().border_width(width));
        self
    }

    /// Set shadow (builder pattern)
    pub fn shadow(self, shadow: blinc_core::Shadow) -> Self {
        self.merge_into_inner(Div::new().shadow(shadow));
        self
    }

    /// Set small shadow (builder pattern)
    pub fn shadow_sm(self) -> Self {
        self.merge_into_inner(Div::new().shadow_sm());
        self
    }

    /// Set medium shadow (builder pattern)
    pub fn shadow_md(self) -> Self {
        self.merge_into_inner(Div::new().shadow_md());
        self
    }

    /// Set large shadow (builder pattern)
    pub fn shadow_lg(self) -> Self {
        self.merge_into_inner(Div::new().shadow_lg());
        self
    }

    /// Set extra-large shadow (builder pattern)
    pub fn shadow_xl(self) -> Self {
        self.merge_into_inner(Div::new().shadow_xl());
        self
    }

    /// Set opacity (builder pattern)
    pub fn opacity(self, opacity: f32) -> Self {
        self.merge_into_inner(Div::new().opacity(opacity));
        self
    }

    /// Set flex shrink (builder pattern)
    pub fn flex_shrink(self) -> Self {
        self.merge_into_inner(Div::new().flex_shrink());
        self
    }

    /// Set flex shrink to 0 (no shrinking) (builder pattern)
    pub fn flex_shrink_0(self) -> Self {
        self.merge_into_inner(Div::new().flex_shrink_0());
        self
    }

    /// Set transform (builder pattern)
    pub fn transform(self, transform: blinc_core::Transform) -> Self {
        self.merge_into_inner(Div::new().transform(transform));
        self
    }

    /// Set overflow to clip (clips children to container bounds)
    pub fn overflow_clip(self) -> Self {
        self.merge_into_inner(Div::new().overflow_clip());
        self
    }

    /// Set cursor style (builder pattern)
    pub fn cursor(self, cursor: crate::element::CursorStyle) -> Self {
        self.merge_into_inner(Div::new().cursor(cursor));
        self
    }

    /// Set cursor to pointer (hand) - convenience for clickable elements
    pub fn cursor_pointer(self) -> Self {
        self.cursor(crate::element::CursorStyle::Pointer)
    }

    /// Set cursor to text (I-beam) - for text input areas
    pub fn cursor_text(self) -> Self {
        self.cursor(crate::element::CursorStyle::Text)
    }

    // =========================================================================
    // Position (builder pattern)
    // =========================================================================

    /// Set position to absolute (builder pattern)
    pub fn absolute(self) -> Self {
        self.merge_into_inner(Div::new().absolute());
        self
    }

    /// Set position to relative (builder pattern)
    pub fn relative(self) -> Self {
        self.merge_into_inner(Div::new().relative());
        self
    }

    /// Set top position (builder pattern)
    pub fn top(self, px: f32) -> Self {
        self.merge_into_inner(Div::new().top(px));
        self
    }

    /// Set bottom position (builder pattern)
    pub fn bottom(self, px: f32) -> Self {
        self.merge_into_inner(Div::new().bottom(px));
        self
    }

    /// Set left position (builder pattern)
    pub fn left(self, px: f32) -> Self {
        self.merge_into_inner(Div::new().left(px));
        self
    }

    /// Set right position (builder pattern)
    pub fn right(self, px: f32) -> Self {
        self.merge_into_inner(Div::new().right(px));
        self
    }

    /// Add child (builder pattern)
    pub fn child(self, child: impl ElementBuilder + 'static) -> Self {
        self.merge_into_inner(Div::new().child(child));
        self
    }

    /// Add children (builder pattern)
    pub fn children<I>(self, children: I) -> Self
    where
        I: IntoIterator,
        I::Item: ElementBuilder + 'static,
    {
        self.merge_into_inner(Div::new().children(children));
        self
    }

    // =========================================================================
    // Event Handlers (builder pattern)
    // =========================================================================
    //
    // Note: All event handlers are registered on event_handlers_cache, not on
    // the inner Div. This allows event_handlers() to return a stable reference
    // that the renderer can use to register handlers with the tree.

    /// Register a click handler (builder pattern)
    pub fn on_click<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        tracing::debug!("Stateful::on_click - registering click handler");
        self.event_handlers_cache.borrow_mut().on_click(handler);
        tracing::debug!(
            "Stateful::on_click - cache empty after: {}",
            self.event_handlers_cache.borrow().is_empty()
        );
        self
    }

    /// Register a mouse down handler (builder pattern)
    pub fn on_mouse_down<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.event_handlers_cache
            .borrow_mut()
            .on_mouse_down(handler);
        self
    }

    /// Register a mouse up handler (builder pattern)
    pub fn on_mouse_up<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.event_handlers_cache.borrow_mut().on_mouse_up(handler);
        self
    }

    /// Register a hover enter handler (builder pattern)
    pub fn on_hover_enter<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.event_handlers_cache
            .borrow_mut()
            .on_hover_enter(handler);
        self
    }

    /// Register a hover leave handler (builder pattern)
    pub fn on_hover_leave<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.event_handlers_cache
            .borrow_mut()
            .on_hover_leave(handler);
        self
    }

    /// Register a focus handler (builder pattern)
    pub fn on_focus<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.event_handlers_cache.borrow_mut().on_focus(handler);
        self
    }

    /// Register a blur handler (builder pattern)
    pub fn on_blur<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.event_handlers_cache.borrow_mut().on_blur(handler);
        self
    }

    /// Register a mount handler (builder pattern)
    pub fn on_mount<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.event_handlers_cache.borrow_mut().on_mount(handler);
        self
    }

    /// Register an unmount handler (builder pattern)
    pub fn on_unmount<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.event_handlers_cache.borrow_mut().on_unmount(handler);
        self
    }

    /// Register a key down handler (builder pattern)
    pub fn on_key_down<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.event_handlers_cache.borrow_mut().on_key_down(handler);
        self
    }

    /// Register a key up handler (builder pattern)
    pub fn on_key_up<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.event_handlers_cache.borrow_mut().on_key_up(handler);
        self
    }

    /// Register a scroll handler (builder pattern)
    pub fn on_scroll<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.event_handlers_cache.borrow_mut().on_scroll(handler);
        self
    }

    /// Register a mouse move handler (builder pattern)
    pub fn on_mouse_move<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.event_handlers_cache
            .borrow_mut()
            .on(blinc_core::events::event_types::POINTER_MOVE, handler);
        self
    }

    /// Register a drag handler (builder pattern)
    ///
    /// Drag events are emitted when the mouse moves while a button is pressed.
    /// Use `EventContext::local_x/y` to get the current position during drag.
    pub fn on_drag<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.event_handlers_cache
            .borrow_mut()
            .on(blinc_core::events::event_types::DRAG, handler);
        self
    }

    /// Register a drag end handler (builder pattern)
    ///
    /// Called when the mouse button is released after a drag operation.
    pub fn on_drag_end<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.event_handlers_cache
            .borrow_mut()
            .on(blinc_core::events::event_types::DRAG_END, handler);
        self
    }

    /// Register a resize handler (builder pattern)
    pub fn on_resize<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.event_handlers_cache.borrow_mut().on_resize(handler);
        self
    }

    /// Register a handler for a specific event type (builder pattern)
    pub fn on_event<F>(self, event_type: blinc_core::events::EventType, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.event_handlers_cache
            .borrow_mut()
            .on(event_type, handler);
        self
    }

    /// Set a layout callback that fires synchronously after each layout computation
    ///
    /// Unlike `on_ready` which fires once with a delay, `on_layout` fires immediately
    /// and synchronously every time the element's bounds are computed. This is useful
    /// for position-dependent operations like dropdown positioning.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let trigger_bounds: State<(f32, f32, f32, f32)> = ...;
    /// Stateful::new(())
    ///     .w(200.0)
    ///     .h(40.0)
    ///     .on_layout(move |bounds| {
    ///         trigger_bounds.set((bounds.x, bounds.y, bounds.width, bounds.height));
    ///     })
    /// ```
    pub fn on_layout<F>(mut self, callback: F) -> Self
    where
        F: Fn(crate::element::ElementBounds) + Send + Sync + 'static,
    {
        self.layout_bounds_cb = Some(std::sync::Arc::new(callback));
        self
    }

    /// Get the layout bounds storage for reading current bounds
    ///
    /// Returns a shared reference to the storage that is updated after each layout.
    /// Use this to read the current bounds in event handlers.
    pub fn bounds_storage(&self) -> crate::renderer::LayoutBoundsStorage {
        Arc::clone(&self.layout_bounds)
    }

    /// Bind this element to an ElementRef for external access
    ///
    /// Returns a `BoundStateful` that continues the fluent API chain while
    /// also making the element accessible via the ref.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let button_ref = ElementRef::<Button>::new();
    ///
    /// let ui = div()
    ///     .child(
    ///         button()
    ///             .bind(&button_ref)  // Binds and continues chain
    ///             .on_state(|state, div| { ... })
    ///     );
    ///
    /// // Later, access via the ref
    /// button_ref.with_mut(|btn| {
    ///     btn.dispatch_state(ButtonState::Pressed);
    /// });
    /// ```
    pub fn bind(self, element_ref: &ElementRef<Self>) -> BoundStateful<S> {
        // Store self in the ElementRef's shared storage
        element_ref.set(self);
        // Return a wrapper that shares the same storage
        BoundStateful {
            storage: element_ref.storage(),
        }
    }
}

// =========================================================================
// BoundStateful - Wrapper for bound stateful elements
// =========================================================================

/// A bound stateful element that maintains shared storage with an ElementRef
///
/// This wrapper is returned by `Stateful::bind()` and provides the same
/// fluent API as `Stateful`, but all modifications go through shared storage
/// accessible via the original `ElementRef`.
pub struct BoundStateful<S: StateTransitions> {
    storage: Arc<Mutex<Option<Stateful<S>>>>,
}

impl<S: StateTransitions> BoundStateful<S> {
    /// Apply a transformation to the stored element
    fn transform_inner<F>(self, f: F) -> Self
    where
        F: FnOnce(Stateful<S>) -> Stateful<S>,
    {
        let mut guard = self.storage.lock().unwrap();
        if let Some(elem) = guard.take() {
            *guard = Some(f(elem));
        }
        drop(guard);
        self
    }

    // =========================================================================
    // Delegated builder methods
    // =========================================================================

    /// Set the state callback (builder pattern)
    pub fn on_state<F>(self, callback: F) -> Self
    where
        F: Fn(&S, &mut Div) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_state(callback))
    }

    /// Set width (builder pattern)
    pub fn w(self, px: f32) -> Self {
        self.transform_inner(|s| s.w(px))
    }

    /// Set height (builder pattern)
    pub fn h(self, px: f32) -> Self {
        self.transform_inner(|s| s.h(px))
    }

    /// Set width to 100% (builder pattern)
    pub fn w_full(self) -> Self {
        self.transform_inner(|s| s.w_full())
    }

    /// Set height to 100% (builder pattern)
    pub fn h_full(self) -> Self {
        self.transform_inner(|s| s.h_full())
    }

    /// Set both width and height (builder pattern)
    pub fn size(self, w: f32, h: f32) -> Self {
        self.transform_inner(|s| s.size(w, h))
    }

    /// Set square size (builder pattern)
    pub fn square(self, size: f32) -> Self {
        self.transform_inner(|s| s.square(size))
    }

    /// Set flex direction to row (builder pattern)
    pub fn flex_row(self) -> Self {
        self.transform_inner(|s| s.flex_row())
    }

    /// Set flex direction to column (builder pattern)
    pub fn flex_col(self) -> Self {
        self.transform_inner(|s| s.flex_col())
    }

    /// Set flex grow (builder pattern)
    pub fn flex_grow(self) -> Self {
        self.transform_inner(|s| s.flex_grow())
    }

    /// Set width to fit content (builder pattern)
    pub fn w_fit(self) -> Self {
        self.transform_inner(|s| s.w_fit())
    }

    /// Set height to fit content (builder pattern)
    pub fn h_fit(self) -> Self {
        self.transform_inner(|s| s.h_fit())
    }

    /// Set padding all sides (builder pattern)
    pub fn p(self, units: f32) -> Self {
        self.transform_inner(|s| s.p(units))
    }

    /// Set horizontal padding (builder pattern)
    pub fn px(self, units: f32) -> Self {
        self.transform_inner(|s| s.px(units))
    }

    /// Set vertical padding (builder pattern)
    pub fn py(self, units: f32) -> Self {
        self.transform_inner(|s| s.py(units))
    }

    /// Set gap (builder pattern)
    pub fn gap(self, units: f32) -> Self {
        self.transform_inner(|s| s.gap(units))
    }

    /// Center items (builder pattern)
    pub fn items_center(self) -> Self {
        self.transform_inner(|s| s.items_center())
    }

    /// Center justify (builder pattern)
    pub fn justify_center(self) -> Self {
        self.transform_inner(|s| s.justify_center())
    }

    /// Space between (builder pattern)
    pub fn justify_between(self) -> Self {
        self.transform_inner(|s| s.justify_between())
    }

    /// Set background (builder pattern)
    pub fn bg(self, color: impl Into<blinc_core::Brush>) -> Self {
        let brush = color.into();
        self.transform_inner(|s| s.bg(brush))
    }

    /// Set corner radius (builder pattern)
    pub fn rounded(self, radius: f32) -> Self {
        self.transform_inner(|s| s.rounded(radius))
    }

    /// Set border with color and width (builder pattern)
    pub fn border(self, width: f32, color: blinc_core::Color) -> Self {
        self.transform_inner(|s| s.border(width, color))
    }

    /// Set border color only (builder pattern)
    pub fn border_color(self, color: blinc_core::Color) -> Self {
        self.transform_inner(|s| s.border_color(color))
    }

    /// Set border width only (builder pattern)
    pub fn border_width(self, width: f32) -> Self {
        self.transform_inner(|s| s.border_width(width))
    }

    /// Set shadow (builder pattern)
    pub fn shadow(self, shadow: blinc_core::Shadow) -> Self {
        self.transform_inner(|s| s.shadow(shadow))
    }

    /// Set small shadow (builder pattern)
    pub fn shadow_sm(self) -> Self {
        self.transform_inner(|s| s.shadow_sm())
    }

    /// Set medium shadow (builder pattern)
    pub fn shadow_md(self) -> Self {
        self.transform_inner(|s| s.shadow_md())
    }

    /// Set large shadow (builder pattern)
    pub fn shadow_lg(self) -> Self {
        self.transform_inner(|s| s.shadow_lg())
    }

    /// Set extra-large shadow (builder pattern)
    pub fn shadow_xl(self) -> Self {
        self.transform_inner(|s| s.shadow_xl())
    }

    /// Set opacity (builder pattern)
    pub fn opacity(self, opacity: f32) -> Self {
        self.transform_inner(|s| s.opacity(opacity))
    }

    /// Set flex shrink (builder pattern)
    pub fn flex_shrink(self) -> Self {
        self.transform_inner(|s| s.flex_shrink())
    }

    /// Set flex shrink to 0 (builder pattern)
    pub fn flex_shrink_0(self) -> Self {
        self.transform_inner(|s| s.flex_shrink_0())
    }

    /// Set transform (builder pattern)
    pub fn transform_style(self, xform: blinc_core::Transform) -> Self {
        self.transform_inner(|s| s.transform(xform))
    }

    /// Set overflow to clip (clips children to container bounds)
    pub fn overflow_clip(self) -> Self {
        self.transform_inner(|s| s.overflow_clip())
    }

    /// Set cursor style (builder pattern)
    pub fn cursor(self, cursor: crate::element::CursorStyle) -> Self {
        self.transform_inner(|s| s.cursor(cursor))
    }

    /// Set cursor to pointer (hand) - convenience for clickable elements
    pub fn cursor_pointer(self) -> Self {
        self.cursor(crate::element::CursorStyle::Pointer)
    }

    /// Set cursor to text (I-beam) - for text input areas
    pub fn cursor_text(self) -> Self {
        self.cursor(crate::element::CursorStyle::Text)
    }

    /// Add child (builder pattern)
    pub fn child(self, child: impl ElementBuilder + 'static) -> Self {
        self.transform_inner(|s| s.child(child))
    }

    // =========================================================================
    // Event Handlers (delegated builder pattern)
    // =========================================================================

    /// Register a click handler (builder pattern)
    pub fn on_click<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_click(handler))
    }

    /// Register a mouse down handler (builder pattern)
    pub fn on_mouse_down<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_mouse_down(handler))
    }

    /// Register a mouse up handler (builder pattern)
    pub fn on_mouse_up<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_mouse_up(handler))
    }

    /// Register a hover enter handler (builder pattern)
    pub fn on_hover_enter<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_hover_enter(handler))
    }

    /// Register a hover leave handler (builder pattern)
    pub fn on_hover_leave<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_hover_leave(handler))
    }

    /// Register a focus handler (builder pattern)
    pub fn on_focus<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_focus(handler))
    }

    /// Register a blur handler (builder pattern)
    pub fn on_blur<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_blur(handler))
    }

    /// Register a mount handler (builder pattern)
    pub fn on_mount<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_mount(handler))
    }

    /// Register an unmount handler (builder pattern)
    pub fn on_unmount<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_unmount(handler))
    }

    /// Register a key down handler (builder pattern)
    pub fn on_key_down<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_key_down(handler))
    }

    /// Register a key up handler (builder pattern)
    pub fn on_key_up<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_key_up(handler))
    }

    /// Register a scroll handler (builder pattern)
    pub fn on_scroll<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_scroll(handler))
    }

    /// Register a mouse move handler (builder pattern)
    pub fn on_mouse_move<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_mouse_move(handler))
    }

    /// Register a resize handler (builder pattern)
    pub fn on_resize<F>(self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_resize(handler))
    }

    /// Register a handler for a specific event type (builder pattern)
    pub fn on_event<F>(self, event_type: blinc_core::events::EventType, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_event(event_type, handler))
    }

    /// Set a layout callback that fires synchronously after each layout computation (builder pattern)
    pub fn on_layout<F>(self, callback: F) -> Self
    where
        F: Fn(crate::element::ElementBounds) + Send + Sync + 'static,
    {
        self.transform_inner(|s| s.on_layout(callback))
    }
}

impl<S: StateTransitions + Default> ElementBuilder for BoundStateful<S> {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.storage
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.build(tree))
            .expect("BoundStateful: element not bound")
    }

    fn render_props(&self) -> RenderProps {
        self.storage
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.render_props())
            .unwrap_or_default()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        // Can't return reference through mutex, children handled via build()
        &[]
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Div
    }

    fn layout_animation_config(&self) -> Option<crate::layout_animation::LayoutAnimationConfig> {
        self.storage
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|s| s.layout_animation_config())
    }

    fn visual_animation_config(&self) -> Option<crate::visual_animation::VisualAnimationConfig> {
        self.storage
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|s| s.visual_animation_config())
    }
}

impl<S: StateTransitions> ElementBuilder for Stateful<S> {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        // Capture ancestor motion key if inside a motion container
        // This enables deferring visual state updates until motion animation completes
        {
            let mut guard = self.shared_state.lock().unwrap();
            guard.ancestor_motion_key = crate::motion::current_motion_key();
        }

        // Ensure callback has been invoked to populate children
        self.ensure_callback_invoked();

        // Extract children from inner Div to the cache for children_builders() to return
        // This is done by swapping them out, since we can't hold a reference across RefCell
        {
            let mut inner = self.inner.borrow_mut();
            let children = std::mem::take(&mut inner.children);
            *self.children_cache.borrow_mut() = children;
        }

        // Put children back for build() to use
        {
            let _cache = self.children_cache.borrow();
            let _inner = self.inner.borrow_mut();
            // We need to clone the children references - but Box<dyn ElementBuilder> isn't Clone
            // So we'll leave them in the cache and manually build them
        }

        // Build the inner div's node (without children since we extracted them)
        let inner = self.inner.borrow_mut();
        let node = tree.create_node(inner.style.clone());

        // Build children from the cache and add to tree
        for child in self.children_cache.borrow().iter() {
            let child_node = child.build(tree);
            tree.add_child(node, child_node);
        }

        // Store node_id for incremental updates
        self.shared_state.lock().unwrap().node_id = Some(node);

        node
    }

    fn render_props(&self) -> RenderProps {
        // Ensure callback is invoked if needed - the diff system may call render_props()
        // before build() is called on the new element instance
        self.ensure_callback_invoked();
        self.inner.borrow().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        // Ensure callback is invoked if needed - this is crucial for the incremental
        // diff system which calls children_builders() BEFORE build() is called.
        // Without this, the diff sees empty children and incorrectly removes content.
        self.ensure_callback_invoked();

        // Now return children from inner Div
        // SAFETY: We use a raw pointer to get a reference that outlives the RefCell borrow.
        // This is safe as long as children_builders() is only called during the
        // render phase when the Div is no longer being mutated.
        unsafe {
            let cache = self.children_cache.as_ptr();
            if !(*cache).is_empty() {
                return (*cache).as_slice();
            }
            // Cache is empty - return from inner Div directly
            let inner = self.inner.as_ptr();
            (*inner).children.as_slice()
        }
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Div
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        // SAFETY: We use a raw pointer here because we need to return a reference
        // to the event handlers cache. The cache is stable during rendering.
        // This is safe as long as event_handlers() is only called during the
        // render phase when the cache is no longer being mutated.
        unsafe {
            let cache = self.event_handlers_cache.as_ptr();
            let handlers = &*cache;
            if handlers.is_empty() {
                None
            } else {
                Some(handlers)
            }
        }
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        // SAFETY: Similar to children_builders - we use a raw pointer because
        // RefCell can't return a reference to inner data directly.
        unsafe {
            let inner = self.inner.as_ptr();
            Some(&(*inner).style)
        }
    }

    fn layout_bounds_storage(&self) -> Option<crate::renderer::LayoutBoundsStorage> {
        Some(Arc::clone(&self.layout_bounds))
    }

    fn layout_bounds_callback(&self) -> Option<crate::renderer::LayoutBoundsCallback> {
        self.layout_bounds_cb.clone()
    }

    fn layout_animation_config(&self) -> Option<crate::layout_animation::LayoutAnimationConfig> {
        self.ensure_callback_invoked();
        self.inner.borrow().layout_animation_config()
    }

    fn visual_animation_config(&self) -> Option<crate::visual_animation::VisualAnimationConfig> {
        self.ensure_callback_invoked();
        self.inner.borrow().visual_animation_config()
    }
}

// =========================================================================
// Convenience Type Aliases
// =========================================================================

/// A button element with hover/press states
pub type Button = Stateful<ButtonState>;

/// A toggle element (on/off)
pub type Toggle = Stateful<ToggleState>;

/// A checkbox element with checked/unchecked states
pub type Checkbox = Stateful<CheckboxState>;

/// A text field element with focus states
pub type TextField = Stateful<TextFieldState>;

/// A scroll container element with momentum scrolling
pub type ScrollContainer = Stateful<ScrollState>;

// =========================================================================
// Convenience Constructors
// =========================================================================

/// Create a stateful element from a shared state handle (legacy API)
///
/// **Deprecated**: Use the new `stateful::<S>()` builder API instead, which
/// provides automatic key management and `StateContext`.
///
/// ```ignore
/// // Old API (deprecated):
/// let handle = use_shared_state::<ButtonState>("my-button");
/// stateful_from_handle(handle)
///     .on_state(|state, div| { ... })
///
/// // New API (recommended):
/// stateful::<ButtonState>()
///     .on_state(|ctx| {
///         match ctx.state() {
///             ButtonState::Idle => div().bg(gray),
///             ButtonState::Hovered => div().bg(blue),
///         }
///     })
/// ```
#[deprecated(
    since = "0.5.0",
    note = "Use stateful::<S>().on_state(|ctx| ...) instead"
)]
pub fn stateful_from_handle<S: StateTransitions>(handle: SharedState<S>) -> Stateful<S> {
    Stateful::with_shared_state(handle)
}

/// Create a stateful button element with custom styling
///
/// This is the low-level constructor for custom button styling.
/// For a ready-to-use button with built-in styling, use `widgets::button()`.
///
/// ```ignore
/// stateful_button()
///     .on_state(|state, div| match state {
///         ButtonState::Idle => { *div = div.swap().bg(Color::BLUE); }
///         ButtonState::Hovered => { *div = div.swap().bg(Color::CYAN); }
///         // ...
///     })
///     .child(text("Click me"))
/// ```
pub fn stateful_button() -> Button {
    Stateful::new(ButtonState::Idle)
}

/// Create a toggle element
pub fn toggle(initially_on: bool) -> Toggle {
    Stateful::new(if initially_on {
        ToggleState::On
    } else {
        ToggleState::Off
    })
}

/// Create a stateful checkbox element with custom styling
///
/// This is the low-level constructor for custom checkbox styling.
/// For a ready-to-use checkbox with built-in styling, use `widgets::checkbox()`.
pub fn stateful_checkbox(initially_checked: bool) -> Checkbox {
    Stateful::new(if initially_checked {
        CheckboxState::CheckedIdle
    } else {
        CheckboxState::UncheckedIdle
    })
}

/// Create a text field element
pub fn text_field() -> TextField {
    Stateful::new(TextFieldState::Idle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_core::events::event_types;
    use blinc_core::{Brush, Color, CornerRadius};
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]

    fn test_stateful_basic() {
        let elem: Stateful<ButtonState> = Stateful::new(ButtonState::Idle)
            .w(100.0)
            .h(40.0)
            .bg(Color::BLUE)
            .rounded(8.0);

        let mut tree = LayoutTree::new();
        let _node = elem.build(&mut tree);
    }

    #[test]
    fn test_state_callback_with_pattern_matching() {
        let elem = stateful_button()
            .w(100.0)
            .h(40.0)
            .on_state(|state, div| match state {
                ButtonState::Idle => {
                    *div = div.swap().bg(Color::BLUE).rounded(4.0);
                }
                ButtonState::Hovered => {
                    *div = div.swap().bg(Color::GREEN).rounded(8.0);
                }
                ButtonState::Pressed => {
                    *div = div.swap().bg(Color::RED);
                }
                ButtonState::Disabled => {
                    *div = div.swap().bg(Color::GRAY);
                }
            });

        let props = elem.render_props();
        assert!(matches!(props.background, Some(Brush::Solid(c)) if c == Color::BLUE));
        assert_eq!(props.border_radius, CornerRadius::uniform(4.0));
    }

    #[test]
    #[ignore = "Test needs to be updated for new API"]
    fn test_state_transition_with_enum() {
        let elem = stateful_button()
            .w(100.0)
            .h(40.0)
            .on_state(|state, container| match state {
                ButtonState::Idle => {
                    container.merge(crate::div().bg(Color::BLUE));
                }
                ButtonState::Hovered => {
                    container.merge(crate::div().bg(Color::GREEN));
                }
                _ => {}
            });

        let props = elem.render_props();
        assert!(matches!(props.background, Some(Brush::Solid(c)) if c == Color::BLUE));

        let changed = elem.dispatch_state(ButtonState::Hovered);
        assert!(changed);
        assert_eq!(elem.state(), ButtonState::Hovered);

        let props = elem.render_props();
        assert!(matches!(props.background, Some(Brush::Solid(c)) if c == Color::GREEN));

        let changed = elem.dispatch_state(ButtonState::Hovered);
        assert!(!changed);
    }

    #[test]
    fn test_handle_event() {
        let elem = stateful_button()
            .w(100.0)
            .on_state(|state, div| match state {
                ButtonState::Idle => {
                    *div = div.swap().bg(Color::BLUE);
                }
                ButtonState::Hovered => {
                    *div = div.swap().bg(Color::GREEN);
                }
                ButtonState::Pressed => {
                    *div = div.swap().bg(Color::RED);
                }
                _ => {}
            });

        assert_eq!(elem.state(), ButtonState::Idle);

        let changed = elem.handle_event(event_types::POINTER_ENTER);
        assert!(changed);
        assert_eq!(elem.state(), ButtonState::Hovered);

        let changed = elem.handle_event(event_types::POINTER_DOWN);
        assert!(changed);
        assert_eq!(elem.state(), ButtonState::Pressed);

        let changed = elem.handle_event(event_types::POINTER_UP);
        assert!(changed);
        assert_eq!(elem.state(), ButtonState::Hovered);
    }

    #[test]
    fn test_callback_is_called() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let _elem = stateful_button().w(100.0).on_state(move |_state, _div| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    #[ignore = "Test needs to be updated to match latest API"]
    fn test_toggle_states() {
        let t = toggle(false)
            .w(50.0)
            .h(30.0)
            .on_state(|state, div| match state {
                ToggleState::Off => {
                    *div = div.swap().bg(Color::GRAY);
                }
                ToggleState::On => {
                    *div = div.swap().bg(Color::GREEN);
                }
            });

        assert_eq!(t.state(), ToggleState::Off);
        let props = t.render_props();
        assert!(matches!(props.background, Some(Brush::Solid(c)) if c == Color::GRAY));

        t.handle_event(event_types::POINTER_UP);
        assert_eq!(t.state(), ToggleState::On);
        let props = t.render_props();
        assert!(matches!(props.background, Some(Brush::Solid(c)) if c == Color::GREEN));

        t.handle_event(event_types::POINTER_UP);
        assert_eq!(t.state(), ToggleState::Off);
    }

    #[test]
    fn test_checkbox_states() {
        let cb = stateful_checkbox(false)
            .square(24.0)
            .on_state(|state, div| match state {
                CheckboxState::UncheckedIdle => {
                    *div = div.swap().bg(Color::WHITE).rounded(4.0);
                }
                CheckboxState::UncheckedHovered => {
                    *div = div.swap().bg(Color::GRAY).rounded(4.0);
                }
                CheckboxState::CheckedIdle => {
                    *div = div.swap().bg(Color::BLUE).rounded(4.0);
                }
                CheckboxState::CheckedHovered => {
                    *div = div.swap().bg(Color::CYAN).rounded(4.0);
                }
            });

        assert!(!cb.state().is_checked());

        cb.handle_event(event_types::POINTER_ENTER);
        assert_eq!(cb.state(), CheckboxState::UncheckedHovered);
        assert!(cb.state().is_hovered());

        cb.handle_event(event_types::POINTER_UP);
        assert_eq!(cb.state(), CheckboxState::CheckedHovered);
        assert!(cb.state().is_checked());

        cb.handle_event(event_types::POINTER_LEAVE);
        assert_eq!(cb.state(), CheckboxState::CheckedIdle);
        assert!(cb.state().is_checked());
        assert!(!cb.state().is_hovered());
    }

    #[test]
    fn test_text_field_states() {
        let field = text_field()
            .w(200.0)
            .h(40.0)
            .on_state(|state, div| match state {
                TextFieldState::Idle => {
                    *div = div.swap().bg(Color::WHITE).rounded(4.0);
                }
                TextFieldState::Hovered => {
                    *div = div.swap().bg(Color::WHITE).rounded(4.0);
                }
                TextFieldState::Focused => {
                    *div = div.swap().bg(Color::WHITE).rounded(4.0);
                }
                TextFieldState::FocusedHovered => {
                    *div = div.swap().bg(Color::WHITE).rounded(4.0);
                }
                TextFieldState::Disabled => {
                    *div = div.swap().bg(Color::GRAY);
                }
            });

        assert_eq!(field.state(), TextFieldState::Idle);
        assert!(!field.state().is_focused());

        field.handle_event(event_types::POINTER_ENTER);
        field.handle_event(event_types::POINTER_DOWN);
        assert!(field.state().is_focused());

        field.handle_event(event_types::BLUR);
        assert!(!field.state().is_focused());
    }

    #[test]
    fn test_disabled_button_ignores_events() {
        let btn = Stateful::new(ButtonState::Disabled)
            .w(100.0)
            .on_state(|_state, _div| {});

        assert_eq!(btn.state(), ButtonState::Disabled);

        assert!(!btn.handle_event(event_types::POINTER_ENTER));
        assert!(!btn.handle_event(event_types::POINTER_DOWN));
        assert!(!btn.handle_event(event_types::POINTER_UP));

        assert_eq!(btn.state(), ButtonState::Disabled);
    }

    #[test]
    fn test_unit_state_ignores_all_events() {
        // Unit type () as a dummy state for stateful elements
        let elem: Stateful<()> =
            Stateful::new(())
                .w(100.0)
                .h(40.0)
                .bg(Color::BLUE)
                .on_state(|_state, div| {
                    div.set_bg(Color::RED);
                });

        // State should always be ()
        assert_eq!(elem.state(), ());

        // No events should cause state transitions
        assert!(!elem.handle_event(event_types::POINTER_ENTER));
        assert!(!elem.handle_event(event_types::POINTER_DOWN));
        assert!(!elem.handle_event(event_types::POINTER_UP));
        assert!(!elem.handle_event(event_types::POINTER_LEAVE));

        // State should still be ()
        assert_eq!(elem.state(), ());
    }
}
