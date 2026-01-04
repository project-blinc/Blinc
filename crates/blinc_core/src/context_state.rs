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
use std::any::TypeId;
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
