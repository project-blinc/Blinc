//! Global context state singleton
//!
//! BlincContextState provides a global singleton for accessing reactive state management
//! and other context-level resources without requiring explicit context parameters.
//!
//! This enables components to create internal state without leaking implementation details:
//!
//! ```ignore
//! // Before: user must manage internal component state
//! let fruit_open = ctx.use_state_keyed("fruit_open", || false);
//! cn::select(&fruit, &fruit_open)
//!
//! // After: component manages internal state via singleton
//! cn::select(&fruit)  // open_state is internal to the component
//! ```
//!
//! # Initialization
//!
//! The singleton must be initialized by the app layer before use:
//!
//! ```ignore
//! // In WindowedApp::run()
//! BlincContextState::init(reactive, hooks, dirty_flag);
//! ```
//!
//! # Usage
//!
//! Components can access state management via free functions:
//!
//! ```ignore
//! use blinc_core::context_state::{use_state_keyed, use_signal_keyed};
//!
//! // In a component:
//! let open_state = use_state_keyed("my_component_open", || false);
//! ```

use crate::reactive::{ReactiveGraph, Signal, SignalId, State};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

/// Global context state instance
static CONTEXT_STATE: OnceLock<BlincContextState> = OnceLock::new();

/// Shared reactive graph for thread-safe access
pub type SharedReactiveGraph = Arc<Mutex<ReactiveGraph>>;

/// Shared dirty flag for triggering UI rebuilds
pub type DirtyFlag = Arc<AtomicBool>;

/// Key for identifying a signal in the keyed state system
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StateKey {
    /// Hash of the user-provided key
    key_hash: u64,
    /// Type ID of the signal value
    type_id: TypeId,
}

impl StateKey {
    /// Create a new StateKey from a hashable key and type
    pub fn new<T: 'static, K: Hash>(key: &K) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        Self {
            key_hash: hasher.finish(),
            type_id: TypeId::of::<T>(),
        }
    }

    /// Create a StateKey from a string key and type
    pub fn from_string<T: 'static>(key: &str) -> Self {
        Self::new::<T, _>(&key)
    }
}

/// Stores keyed state across rebuilds
///
/// This enables component-level state management where each signal
/// is identified by a unique string key rather than call order.
pub struct HookState {
    /// Keyed signals: key -> raw signal ID
    signals: HashMap<StateKey, u64>,
}

impl Default for HookState {
    fn default() -> Self {
        Self::new()
    }
}

impl HookState {
    /// Create a new HookState
    pub fn new() -> Self {
        Self {
            signals: HashMap::new(),
        }
    }

    /// Get an existing signal by key
    pub fn get(&self, key: &StateKey) -> Option<u64> {
        self.signals.get(key).copied()
    }

    /// Store a signal with the given key
    pub fn insert(&mut self, key: StateKey, signal_id: u64) {
        self.signals.insert(key, signal_id);
    }
}

/// Shared hook state for the application
pub type SharedHookState = Arc<Mutex<HookState>>;

/// Callback for notifying stateful elements of signal changes
pub type StatefulCallback = Arc<dyn Fn(&[SignalId]) + Send + Sync>;

/// Callback for querying elements by ID
/// Returns the raw node ID (u64) if found, None otherwise
pub type QueryCallback = Arc<dyn Fn(&str) -> Option<u64> + Send + Sync>;

/// Simple bounds representation for element queries
/// Used by BlincContextState to avoid circular dependencies with blinc_layout
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    /// X position (absolute, after layout)
    pub x: f32,
    /// Y position (absolute, after layout)
    pub y: f32,
    /// Computed width
    pub width: f32,
    /// Computed height
    pub height: f32,
}

impl Bounds {
    /// Create new bounds
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Check if a point is inside the bounds
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }

    /// Check if bounds intersect with another bounds
    pub fn intersects(&self, other: &Bounds) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
}

/// Callback for getting element bounds by string ID
pub type BoundsCallback = Arc<dyn Fn(&str) -> Option<Bounds> + Send + Sync>;

/// Callback for focus management
/// Called with Some(id) to focus an element, None to clear focus
pub type FocusCallback = Arc<dyn Fn(Option<&str>) + Send + Sync>;

/// Callback for scrolling an element into view
pub type ScrollCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Motion animation state for query API
///
/// Represents the current state of a motion animation.
/// Used by MotionHandle to query animation progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MotionAnimationState {
    /// Animation is suspended (waiting for explicit start)
    /// The motion is mounted with opacity 0, waiting for `MotionHandle.start()` to trigger
    Suspended,
    /// Animation hasn't started yet (waiting for delay)
    Waiting,
    /// Element is entering (fade-in, scale-in, etc.)
    Entering {
        /// Animation progress from 0.0 to 1.0
        progress: f32,
    },
    /// Element is fully visible (animation complete)
    Visible,
    /// Element is exiting (fade-out, scale-out, etc.)
    Exiting {
        /// Animation progress from 0.0 to 1.0
        progress: f32,
    },
    /// Element has been removed (exit animation complete)
    Removed,
    /// No motion animation found for this key
    NotFound,
}

impl MotionAnimationState {
    /// Check if the animation is still playing (not settled)
    ///
    /// Returns true for `Waiting`, `Entering`, `Exiting`, or `Suspended` states.
    /// Suspended is considered "animating" because the motion is waiting to start.
    pub fn is_animating(&self) -> bool {
        matches!(
            self,
            MotionAnimationState::Suspended
                | MotionAnimationState::Waiting
                | MotionAnimationState::Entering { .. }
                | MotionAnimationState::Exiting { .. }
        )
    }

    /// Check if the animation has settled (fully visible)
    pub fn is_settled(&self) -> bool {
        matches!(self, MotionAnimationState::Visible)
    }

    /// Check if the motion is suspended (waiting for explicit start)
    pub fn is_suspended(&self) -> bool {
        matches!(self, MotionAnimationState::Suspended)
    }

    /// Check if the element is entering
    pub fn is_entering(&self) -> bool {
        matches!(self, MotionAnimationState::Entering { .. })
    }

    /// Check if the element is exiting
    pub fn is_exiting(&self) -> bool {
        matches!(self, MotionAnimationState::Exiting { .. })
    }

    /// Get the animation progress (0.0 to 1.0)
    ///
    /// Returns 0.0 for Suspended/Waiting, 1.0 for Visible/Removed, and the actual
    /// progress for Entering/Exiting states.
    pub fn progress(&self) -> f32 {
        match self {
            MotionAnimationState::Suspended => 0.0,
            MotionAnimationState::Waiting => 0.0,
            MotionAnimationState::Entering { progress } => *progress,
            MotionAnimationState::Visible => 1.0,
            MotionAnimationState::Exiting { progress } => *progress,
            MotionAnimationState::Removed => 1.0,
            MotionAnimationState::NotFound => 1.0, // Treat as settled if not found
        }
    }
}

/// Callback for querying motion animation state by stable key
pub type MotionStateCallback = Arc<dyn Fn(&str) -> MotionAnimationState + Send + Sync>;

/// Callback for canceling a motion's exit animation
pub type MotionCancelExitCallback = Arc<dyn Fn(&str) + Send + Sync>;

// =========================================================================
// Recorder Callbacks (for blinc_recorder integration)
// =========================================================================

/// Type-erased recorded event for recorder callbacks
/// This avoids circular dependencies by using a boxed Any type
pub type RecordedEventAny = Box<dyn Any + Send>;

/// Callback for recording events (mouse, keyboard, scroll, etc.)
/// Events are passed as type-erased Any to avoid circular dependencies
pub type RecorderEventCallback = Arc<dyn Fn(RecordedEventAny) + Send + Sync>;

/// Type-erased tree snapshot for recorder callbacks
pub type TreeSnapshotAny = Box<dyn Any + Send>;

/// Callback for capturing tree snapshots after each frame
pub type RecorderSnapshotCallback = Arc<dyn Fn(TreeSnapshotAny) + Send + Sync>;

/// Type-erased element registry storage
/// This allows blinc_core to store the registry without depending on blinc_layout
pub type AnyElementRegistry = Arc<dyn Any + Send + Sync>;

/// Global context state singleton
///
/// Provides access to reactive state management and other context-level
/// resources without requiring explicit context parameters.
///
/// This follows the same OnceLock pattern as ThemeState.
pub struct BlincContextState {
    /// Reactive graph for signal-based state management
    reactive: SharedReactiveGraph,
    /// Hook state for keyed signal persistence
    hooks: SharedHookState,
    /// Dirty flag for triggering UI rebuilds
    dirty_flag: DirtyFlag,
    /// Optional callback for notifying stateful elements of signal changes
    stateful_callback: Option<StatefulCallback>,
    /// Optional callback for querying elements by ID
    query_callback: RwLock<Option<QueryCallback>>,

    // =========================================================================
    // ElementHandle Callbacks (set by WindowedApp)
    // =========================================================================
    /// Callback for getting element bounds by string ID
    bounds_callback: RwLock<Option<BoundsCallback>>,
    /// Callback for focus management
    focus_callback: RwLock<Option<FocusCallback>>,
    /// Callback for scrolling elements into view
    scroll_callback: RwLock<Option<ScrollCallback>>,
    /// Current viewport size (width, height)
    viewport_size: RwLock<(f32, f32)>,
    /// Currently focused element ID
    focused_element: RwLock<Option<String>>,
    /// Type-erased element registry for query API
    /// Stored as `AnyElementRegistry` to avoid circular dependency with blinc_layout
    element_registry: RwLock<Option<AnyElementRegistry>>,
    /// Callback for querying motion animation state by stable key
    motion_state_callback: RwLock<Option<MotionStateCallback>>,
    /// Callback for canceling a motion's exit animation
    motion_cancel_exit_callback: RwLock<Option<MotionCancelExitCallback>>,

    // =========================================================================
    // Recorder Callbacks (for blinc_recorder integration)
    // =========================================================================
    /// Callback for recording events
    recorder_event_callback: RwLock<Option<RecorderEventCallback>>,
    /// Callback for capturing tree snapshots
    recorder_snapshot_callback: RwLock<Option<RecorderSnapshotCallback>>,
}

impl BlincContextState {
    /// Initialize the global context state (call once at app startup)
    ///
    /// # Panics
    ///
    /// Panics if called more than once.
    pub fn init(reactive: SharedReactiveGraph, hooks: SharedHookState, dirty_flag: DirtyFlag) {
        let state = BlincContextState {
            reactive,
            hooks,
            dirty_flag,
            stateful_callback: None,
            query_callback: RwLock::new(None),
            bounds_callback: RwLock::new(None),
            focus_callback: RwLock::new(None),
            scroll_callback: RwLock::new(None),
            viewport_size: RwLock::new((0.0, 0.0)),
            focused_element: RwLock::new(None),
            element_registry: RwLock::new(None),
            motion_state_callback: RwLock::new(None),
            motion_cancel_exit_callback: RwLock::new(None),
            recorder_event_callback: RwLock::new(None),
            recorder_snapshot_callback: RwLock::new(None),
        };

        if CONTEXT_STATE.set(state).is_err() {
            panic!("BlincContextState::init() called more than once");
        }
    }

    /// Initialize with a stateful callback for notifying elements of signal changes
    pub fn init_with_callback(
        reactive: SharedReactiveGraph,
        hooks: SharedHookState,
        dirty_flag: DirtyFlag,
        callback: StatefulCallback,
    ) {
        let state = BlincContextState {
            reactive,
            hooks,
            dirty_flag,
            stateful_callback: Some(callback),
            query_callback: RwLock::new(None),
            bounds_callback: RwLock::new(None),
            focus_callback: RwLock::new(None),
            scroll_callback: RwLock::new(None),
            viewport_size: RwLock::new((0.0, 0.0)),
            focused_element: RwLock::new(None),
            element_registry: RwLock::new(None),
            motion_state_callback: RwLock::new(None),
            motion_cancel_exit_callback: RwLock::new(None),
            recorder_event_callback: RwLock::new(None),
            recorder_snapshot_callback: RwLock::new(None),
        };

        if CONTEXT_STATE.set(state).is_err() {
            panic!("BlincContextState::init() called more than once");
        }
    }

    /// Get the global context state instance
    ///
    /// # Panics
    ///
    /// Panics if `init()` has not been called.
    pub fn get() -> &'static BlincContextState {
        CONTEXT_STATE.get().expect(
            "BlincContextState not initialized. Call BlincContextState::init() at app startup.",
        )
    }

    /// Try to get the global context state (returns None if not initialized)
    pub fn try_get() -> Option<&'static BlincContextState> {
        CONTEXT_STATE.get()
    }

    /// Check if the context state has been initialized
    pub fn is_initialized() -> bool {
        CONTEXT_STATE.get().is_some()
    }

    // =========================================================================
    // Reactive State Management
    // =========================================================================

    /// Create a persistent state value that survives across UI rebuilds (keyed)
    ///
    /// This creates component-level state identified by a unique string key.
    /// Returns a `State<T>` with direct `.get()` and `.set()` methods.
    pub fn use_state_keyed<T, F>(&self, key: &str, init: F) -> State<T>
    where
        T: Clone + Send + 'static,
        F: FnOnce() -> T,
    {
        let state_key = StateKey::from_string::<T>(key);
        let mut hooks = self.hooks.lock().unwrap();

        // Check if we have an existing signal with this key
        let signal = if let Some(raw_id) = hooks.get(&state_key) {
            // Reconstruct the signal from stored ID
            let signal_id = SignalId::from_raw(raw_id);
            Signal::from_id(signal_id)
        } else {
            // First time - create a new signal and store it
            let signal = self.reactive.lock().unwrap().create_signal(init());
            let raw_id = signal.id().to_raw();
            hooks.insert(state_key, raw_id);
            signal
        };

        // Create State with or without stateful callback
        if let Some(ref callback) = self.stateful_callback {
            State::with_stateful_callback(
                signal,
                Arc::clone(&self.reactive),
                Arc::clone(&self.dirty_flag),
                Arc::clone(callback),
            )
        } else {
            State::new(
                signal,
                Arc::clone(&self.reactive),
                Arc::clone(&self.dirty_flag),
            )
        }
    }

    /// Create a persistent signal that survives across UI rebuilds (keyed)
    ///
    /// Unlike `use_signal()` which creates a new signal each call, this method
    /// persists the signal using a unique string key.
    pub fn use_signal_keyed<T, F>(&self, key: &str, init: F) -> Signal<T>
    where
        T: Clone + Send + 'static,
        F: FnOnce() -> T,
    {
        let state_key = StateKey::from_string::<T>(key);
        let mut hooks = self.hooks.lock().unwrap();

        if let Some(raw_id) = hooks.get(&state_key) {
            let signal_id = SignalId::from_raw(raw_id);
            Signal::from_id(signal_id)
        } else {
            let signal = self.reactive.lock().unwrap().create_signal(init());
            let raw_id = signal.id().to_raw();
            hooks.insert(state_key, raw_id);
            signal
        }
    }

    /// Create a new reactive signal with an initial value (low-level API)
    ///
    /// **Note**: Prefer `use_state_keyed` in most cases, as it automatically
    /// persists signals across rebuilds.
    pub fn use_signal<T: Send + 'static>(&self, initial: T) -> Signal<T> {
        self.reactive.lock().unwrap().create_signal(initial)
    }

    /// Get the current value of a signal
    pub fn get_signal<T: Clone + 'static>(&self, signal: Signal<T>) -> Option<T> {
        self.reactive.lock().unwrap().get(signal)
    }

    /// Set the value of a signal, triggering reactive updates
    pub fn set_signal<T: Send + 'static>(&self, signal: Signal<T>, value: T) {
        self.reactive.lock().unwrap().set(signal, value);
    }

    /// Update a signal using a function
    pub fn update<T: Clone + Send + 'static, F: FnOnce(T) -> T>(&self, signal: Signal<T>, f: F) {
        let mut graph = self.reactive.lock().unwrap();
        if let Some(current) = graph.get(signal) {
            graph.set(signal, f(current));
        }
    }

    // =========================================================================
    // Access to Internal Resources
    // =========================================================================

    /// Get the shared reactive graph
    pub fn reactive(&self) -> &SharedReactiveGraph {
        &self.reactive
    }

    /// Get the shared hook state
    pub fn hooks(&self) -> &SharedHookState {
        &self.hooks
    }

    /// Get the dirty flag
    pub fn dirty_flag(&self) -> &DirtyFlag {
        &self.dirty_flag
    }

    /// Request a UI rebuild by setting the dirty flag
    pub fn request_rebuild(&self) {
        self.dirty_flag.store(true, Ordering::SeqCst);
    }

    /// Notify stateful elements of signal changes
    ///
    /// This triggers only the stateful elements that depend on the given signals,
    /// causing targeted subtree rebuilds rather than a full UI rebuild.
    pub fn notify_stateful_deps(&self, signal_ids: &[SignalId]) {
        if let Some(ref callback) = self.stateful_callback {
            callback(signal_ids);
        }
    }

    // =========================================================================
    // Element Query System
    // =========================================================================

    /// Set the query callback for element lookup
    ///
    /// This is called by `WindowedApp` to enable element querying by ID.
    /// The callback receives an element ID and returns the raw node ID if found.
    pub fn set_query_callback(&self, callback: QueryCallback) {
        *self.query_callback.write().unwrap() = Some(callback);
    }

    /// Query an element by ID
    ///
    /// Returns the raw node ID (u64) if an element with the given ID exists.
    /// This enables components to look up elements without needing a context reference.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_core::context_state::query;
    ///
    /// if let Some(node_id) = query("my-element") {
    ///     // Element exists
    /// }
    /// ```
    pub fn query(&self, id: &str) -> Option<u64> {
        self.query_callback
            .read()
            .unwrap()
            .as_ref()
            .and_then(|cb| cb(id))
    }

    // =========================================================================
    // Element Bounds & Visibility
    // =========================================================================

    /// Set the bounds callback for element bounds lookup
    ///
    /// Called by `WindowedApp` to enable bounds queries by element ID.
    pub fn set_bounds_callback(&self, callback: BoundsCallback) {
        *self.bounds_callback.write().unwrap() = Some(callback);
    }

    /// Get element bounds by string ID
    ///
    /// Returns the computed bounds after layout, or None if the element
    /// doesn't exist or hasn't been laid out yet.
    pub fn get_bounds(&self, id: &str) -> Option<Bounds> {
        self.bounds_callback
            .read()
            .unwrap()
            .as_ref()
            .and_then(|cb| cb(id))
    }

    /// Set the current viewport size
    ///
    /// Called by `WindowedApp` when the window is resized.
    pub fn set_viewport_size(&self, width: f32, height: f32) {
        *self.viewport_size.write().unwrap() = (width, height);
    }

    /// Get the current viewport size (width, height)
    pub fn viewport_size(&self) -> (f32, f32) {
        *self.viewport_size.read().unwrap()
    }

    // =========================================================================
    // Focus Management
    // =========================================================================

    /// Set the focus callback
    ///
    /// Called by `WindowedApp` to wire focus changes to the EventRouter.
    pub fn set_focus_callback(&self, callback: FocusCallback) {
        *self.focus_callback.write().unwrap() = Some(callback);
    }

    /// Set focus to an element by string ID
    ///
    /// Pass `None` to clear focus.
    pub fn set_focus(&self, id: Option<&str>) {
        // Update internal state
        *self.focused_element.write().unwrap() = id.map(|s| s.to_string());

        // Call the callback to update EventRouter
        if let Some(cb) = self.focus_callback.read().unwrap().as_ref() {
            cb(id);
        }
    }

    /// Get the currently focused element ID
    pub fn focused_element(&self) -> Option<String> {
        self.focused_element.read().unwrap().clone()
    }

    /// Check if an element is currently focused
    pub fn is_focused(&self, id: &str) -> bool {
        self.focused_element.read().unwrap().as_deref() == Some(id)
    }

    // =========================================================================
    // Scroll Into View
    // =========================================================================

    /// Set the scroll callback
    ///
    /// Called by `WindowedApp` to wire scroll requests to the RenderTree.
    pub fn set_scroll_callback(&self, callback: ScrollCallback) {
        *self.scroll_callback.write().unwrap() = Some(callback);
    }

    /// Scroll an element into view
    pub fn scroll_element_into_view(&self, id: &str) {
        if let Some(cb) = self.scroll_callback.read().unwrap().as_ref() {
            cb(id);
        }
    }

    // =========================================================================
    // Element Registry (for query API)
    // =========================================================================

    /// Set the element registry
    ///
    /// Called by `WindowedApp` to store the registry for the query API.
    /// The registry is stored as type-erased `AnyElementRegistry` to avoid
    /// circular dependencies with blinc_layout.
    pub fn set_element_registry(&self, registry: AnyElementRegistry) {
        *self.element_registry.write().unwrap() = Some(registry);
    }

    /// Get the element registry as type-erased Any
    ///
    /// Returns the raw `Arc` which can be downcast to the concrete
    /// `ElementRegistry` type in blinc_layout.
    pub fn element_registry_any(&self) -> Option<AnyElementRegistry> {
        self.element_registry.read().unwrap().clone()
    }

    /// Get the element registry, downcasting to the expected type
    ///
    /// This is a convenience method for use by blinc_layout's query function.
    pub fn element_registry<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.element_registry
            .read()
            .unwrap()
            .as_ref()
            .and_then(|r| r.clone().downcast::<T>().ok())
    }

    // =========================================================================
    // Motion Animation State Query
    // =========================================================================

    /// Set the motion state callback
    ///
    /// Called by `WindowedApp` to enable motion animation state queries.
    /// The callback receives a stable motion key and returns its animation state.
    pub fn set_motion_state_callback(&self, callback: MotionStateCallback) {
        *self.motion_state_callback.write().unwrap() = Some(callback);
    }

    /// Query motion animation state by stable key
    ///
    /// Returns the current state of a motion animation identified by its stable key.
    /// This enables components to check if a parent motion is still animating
    /// before rendering their own content.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_core::context_state::query_motion;
    ///
    /// let state = query_motion("dialog-content");
    /// if state.is_settled() {
    ///     // Safe to render hover effects, etc.
    /// }
    /// ```
    pub fn query_motion(&self, key: &str) -> MotionAnimationState {
        self.motion_state_callback
            .read()
            .unwrap()
            .as_ref()
            .map(|cb| cb(key))
            .unwrap_or(MotionAnimationState::NotFound)
    }

    /// Set the motion cancel exit callback
    ///
    /// Called by `WindowedApp` to enable motion exit cancellation.
    /// The callback receives a stable motion key and cancels its exit animation.
    pub fn set_motion_cancel_exit_callback(&self, callback: MotionCancelExitCallback) {
        *self.motion_cancel_exit_callback.write().unwrap() = Some(callback);
    }

    /// Cancel a motion's exit animation by stable key
    ///
    /// Used when an overlay's close is cancelled (e.g., mouse re-enters hover card).
    /// This interrupts the exit animation and immediately sets the motion to fully visible.
    ///
    /// No-op if the motion is not in Exiting state or callback is not set.
    pub fn cancel_motion_exit(&self, key: &str) {
        if let Some(ref cb) = *self.motion_cancel_exit_callback.read().unwrap() {
            cb(key);
        }
    }

    // =========================================================================
    // Recorder Integration (for blinc_recorder)
    // =========================================================================

    /// Set the recorder event callback
    ///
    /// Called by `blinc_recorder` to capture user interaction events.
    /// Events are passed as type-erased `RecordedEventAny` to avoid circular dependencies.
    pub fn set_recorder_event_callback(&self, callback: RecorderEventCallback) {
        *self.recorder_event_callback.write().unwrap() = Some(callback);
    }

    /// Clear the recorder event callback
    pub fn clear_recorder_event_callback(&self) {
        *self.recorder_event_callback.write().unwrap() = None;
    }

    /// Record an event if a recorder callback is set
    ///
    /// This is called by EventRouter and other event sources to record user interactions.
    pub fn record_event(&self, event: RecordedEventAny) {
        if let Some(ref cb) = *self.recorder_event_callback.read().unwrap() {
            cb(event);
        }
    }

    /// Check if event recording is enabled
    pub fn is_recording_events(&self) -> bool {
        self.recorder_event_callback.read().unwrap().is_some()
    }

    /// Set the recorder snapshot callback
    ///
    /// Called by `blinc_recorder` to capture tree snapshots after each frame.
    /// Snapshots are passed as type-erased `TreeSnapshotAny` to avoid circular dependencies.
    pub fn set_recorder_snapshot_callback(&self, callback: RecorderSnapshotCallback) {
        *self.recorder_snapshot_callback.write().unwrap() = Some(callback);
    }

    /// Clear the recorder snapshot callback
    pub fn clear_recorder_snapshot_callback(&self) {
        *self.recorder_snapshot_callback.write().unwrap() = None;
    }

    /// Record a tree snapshot if a recorder callback is set
    ///
    /// This is called by RenderTree after each frame to capture the element tree state.
    pub fn record_snapshot(&self, snapshot: TreeSnapshotAny) {
        if let Some(ref cb) = *self.recorder_snapshot_callback.read().unwrap() {
            cb(snapshot);
        }
    }

    /// Check if snapshot recording is enabled
    pub fn is_recording_snapshots(&self) -> bool {
        self.recorder_snapshot_callback.read().unwrap().is_some()
    }
}

// =========================================================================
// Convenience Free Functions
// =========================================================================

/// Create a persistent state value that survives across UI rebuilds (keyed)
///
/// This is a convenience wrapper around `BlincContextState::get().use_state_keyed()`.
///
/// # Panics
///
/// Panics if `BlincContextState::init()` has not been called.
///
/// # Example
///
/// ```ignore
/// use blinc_core::context_state::use_state_keyed;
///
/// // In a component:
/// let open_state = use_state_keyed("my_component_open", || false);
/// ```
pub fn use_state_keyed<T, F>(key: &str, init: F) -> State<T>
where
    T: Clone + Send + 'static,
    F: FnOnce() -> T,
{
    BlincContextState::get().use_state_keyed(key, init)
}

/// Create a persistent signal that survives across UI rebuilds (keyed)
///
/// This is a convenience wrapper around `BlincContextState::get().use_signal_keyed()`.
///
/// # Panics
///
/// Panics if `BlincContextState::init()` has not been called.
pub fn use_signal_keyed<T, F>(key: &str, init: F) -> Signal<T>
where
    T: Clone + Send + 'static,
    F: FnOnce() -> T,
{
    BlincContextState::get().use_signal_keyed(key, init)
}

/// Request a UI rebuild
///
/// This is a convenience wrapper around `BlincContextState::get().request_rebuild()`.
///
/// # Panics
///
/// Panics if `BlincContextState::init()` has not been called.
pub fn request_rebuild() {
    BlincContextState::get().request_rebuild();
}

/// Query an element by ID
///
/// Returns the raw node ID (u64) if an element with the given ID exists.
/// This is a convenience wrapper around `BlincContextState::get().query()`.
///
/// # Panics
///
/// Panics if `BlincContextState::init()` has not been called.
///
/// # Example
///
/// ```ignore
/// use blinc_core::context_state::query;
///
/// if let Some(node_id) = query("my-element") {
///     // Element with ID "my-element" exists
/// }
/// ```
pub fn query(id: &str) -> Option<u64> {
    BlincContextState::get().query(id)
}

/// Query motion animation state by stable key
///
/// Returns the current state of a motion animation identified by its stable key.
/// This enables components to check if a parent motion is still animating
/// before rendering their own content.
///
/// # Panics
///
/// Panics if `BlincContextState::init()` has not been called.
///
/// # Example
///
/// ```ignore
/// use blinc_core::context_state::query_motion;
///
/// let state = query_motion("dialog-content");
/// if state.is_settled() {
///     // Safe to render hover effects, etc.
/// }
/// ```
pub fn query_motion(key: &str) -> MotionAnimationState {
    BlincContextState::get().query_motion(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_key() {
        let key1 = StateKey::from_string::<i32>("counter");
        let key2 = StateKey::from_string::<i32>("counter");
        let key3 = StateKey::from_string::<String>("counter");

        assert_eq!(key1, key2);
        assert_ne!(key1, key3); // Different types
    }

    #[test]
    fn test_hook_state() {
        let mut hooks = HookState::new();
        let key = StateKey::from_string::<i32>("test");

        assert!(hooks.get(&key).is_none());

        hooks.insert(key.clone(), 42);
        assert_eq!(hooks.get(&key), Some(42));
    }
}
