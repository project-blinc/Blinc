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

use std::hash::Hash;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use blinc_core::reactive::SignalId;
use crate::div::{Div, ElementBuilder, ElementRef, ElementTypeId};
use crate::element::RenderProps;
use crate::tree::{LayoutNodeId, LayoutTree};

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
}

// Safety: PendingSubtreeRebuild is only accessed from the main thread
unsafe impl Send for PendingSubtreeRebuild {}

/// Queue a subtree rebuild for a node
pub fn queue_subtree_rebuild(parent_id: LayoutNodeId, new_child: crate::div::Div) {
    PENDING_SUBTREE_REBUILDS.lock().unwrap().push(PendingSubtreeRebuild {
        parent_id,
        new_child,
    });
}

/// Take all pending subtree rebuilds
///
/// Called by the windowed app to apply incremental child updates to the RenderTree.
pub fn take_pending_subtree_rebuilds() -> Vec<PendingSubtreeRebuild> {
    std::mem::take(&mut *PENDING_SUBTREE_REBUILDS.lock().unwrap())
}

/// Registry of stateful elements with signal dependencies
///
/// Each entry is (deps, refresh_fn) where refresh_fn triggers a rebuild.
/// The windowed app checks these when signals change.
static STATEFUL_DEPS: LazyLock<Mutex<Vec<(Vec<SignalId>, Box<dyn Fn() + Send + Sync>)>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Register a stateful element's dependencies
///
/// Called internally when `.deps()` is used on a stateful element.
pub(crate) fn register_stateful_deps(deps: Vec<SignalId>, refresh_fn: Box<dyn Fn() + Send + Sync>) {
    STATEFUL_DEPS.lock().unwrap().push((deps, refresh_fn));
}

/// Check all registered stateful deps against changed signals and trigger rebuilds
///
/// Called by windowed app after signal updates.
/// Returns true if any deps matched and subtree rebuilds were queued.
pub fn check_stateful_deps(changed_signals: &[SignalId]) -> bool {
    let registry = STATEFUL_DEPS.lock().unwrap();
    let mut triggered = false;
    for (deps, refresh_fn) in registry.iter() {
        // If any dep is in changed_signals, trigger the refresh
        if deps.iter().any(|d| changed_signals.contains(d)) {
            refresh_fn();
            triggered = true;
        }
    }
    triggered
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
/// Called by stateful elements when their state changes.
fn queue_prop_update(node_id: LayoutNodeId, props: RenderProps) {
    PENDING_PROP_UPDATES.lock().unwrap().push((node_id, props));
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
            (ButtonState::Idle, POINTER_ENTER) => Some(ButtonState::Hovered),
            (ButtonState::Hovered, POINTER_LEAVE) => Some(ButtonState::Idle),
            (ButtonState::Hovered, POINTER_DOWN) => Some(ButtonState::Pressed),
            (ButtonState::Pressed, POINTER_UP) => Some(ButtonState::Hovered),
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
    inner: Div,

    /// Shared state that event handlers can mutate
    shared_state: Arc<Mutex<StatefulInner<S>>>,
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

    /// The layout node ID for this element (set on first event)
    /// Used to apply incremental prop updates without tree rebuild
    pub(crate) node_id: Option<LayoutNodeId>,

    /// Signal dependencies - when any of these change, refresh props
    pub(crate) deps: Vec<SignalId>,
}

impl<S: StateTransitions> StatefulInner<S> {
    /// Create a new StatefulInner with the given initial state
    pub fn new(state: S) -> Self {
        Self {
            state,
            state_callback: None,
            needs_visual_update: false,
            base_render_props: None,
            node_id: None,
            deps: Vec::new(),
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

/// Trigger a refresh of the stateful element's props (internal use)
///
/// This re-runs the `on_state` callback and queues a prop update.
/// Called internally by the reactive system when dependencies change.
pub(crate) fn refresh_stateful<S: StateTransitions>(shared: &SharedState<S>) {
    Stateful::<S>::refresh_props_internal(shared);
}

impl<S: StateTransitions> Stateful<S> {
    /// Create a new stateful element with initial state
    pub fn new(initial_state: S) -> Self {
        Self {
            inner: Div::new(),
            shared_state: Arc::new(Mutex::new(StatefulInner {
                state: initial_state,
                state_callback: None,
                needs_visual_update: false,
                base_render_props: None,
                node_id: None,
                deps: Vec::new(),
            })),
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
            inner: Div::new(),
            shared_state,
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
    pub fn on_state<F>(mut self, callback: F) -> Self
    where
        F: Fn(&S, &mut Div) + Send + Sync + 'static,
    {
        // Capture base render props BEFORE applying state callback
        // This preserves properties like rounded corners, shadows, etc.
        let base_props = self.inner.render_props();

        // Store the callback and base props
        {
            let mut inner = self.shared_state.lock().unwrap();
            inner.state_callback = Some(Arc::new(callback));
            inner.base_render_props = Some(base_props);
        }

        // Apply initial state to get the initial div styling
        self.apply_state_callback();

        // Register event handlers that will trigger state transitions
        self = self.register_state_handlers();

        // Register deps if any were set
        let shared = Arc::clone(&self.shared_state);
        let deps = shared.lock().unwrap().deps.clone();
        if !deps.is_empty() {
            let shared_for_refresh = Arc::clone(&shared);
            register_stateful_deps(deps, Box::new(move || {
                refresh_stateful(&shared_for_refresh);
            }));
        }

        self
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
    fn register_state_handlers(mut self) -> Self {
        use blinc_core::events::event_types;

        let shared = Arc::clone(&self.shared_state);

        // POINTER_ENTER -> state transition
        {
            let shared_clone = Arc::clone(&shared);
            self.inner = self.inner.swap().on_hover_enter(move |ctx| {
                Self::handle_event_internal(&shared_clone, event_types::POINTER_ENTER, ctx.node_id);
            });
        }

        // POINTER_LEAVE -> state transition
        {
            let shared_clone = Arc::clone(&shared);
            self.inner = self.inner.swap().on_hover_leave(move |ctx| {
                Self::handle_event_internal(&shared_clone, event_types::POINTER_LEAVE, ctx.node_id);
            });
        }

        // POINTER_DOWN -> state transition
        {
            let shared_clone = Arc::clone(&shared);
            self.inner = self.inner.swap().on_mouse_down(move |ctx| {
                Self::handle_event_internal(&shared_clone, event_types::POINTER_DOWN, ctx.node_id);
            });
        }

        // POINTER_UP -> state transition
        {
            let shared_clone = Arc::clone(&shared);
            self.inner = self.inner.swap().on_mouse_up(move |ctx| {
                Self::handle_event_internal(&shared_clone, event_types::POINTER_UP, ctx.node_id);
            });
        }

        self
    }

    /// Internal handler for state transitions from event handlers
    ///
    /// This updates the state, computes new render props via the callback,
    /// and queues an incremental prop update. No tree rebuild is needed.
    fn handle_event_internal(shared: &Arc<Mutex<StatefulInner<S>>>, event: u32, node_id: LayoutNodeId) {
        let mut guard = shared.lock().unwrap();

        // Store node_id for future use
        if guard.node_id.is_none() {
            guard.node_id = Some(node_id);
        }

        // Check if state transition needed
        let new_state = match guard.state.on_event(event) {
            Some(s) if s != guard.state => s,
            _ => return,
        };

        // Update state
        guard.state = new_state;
        guard.needs_visual_update = true;

        // Compute new props via callback and queue the update
        if let Some(ref callback) = guard.state_callback {
            let callback = Arc::clone(callback);
            let state_copy = guard.state;
            let cached_node_id = guard.node_id;
            let base_props = guard.base_render_props.clone();
            drop(guard); // Release lock before calling callback

            // Create temp div, apply callback to get state-specific changes
            let mut temp_div = Div::new();
            callback(&state_copy, &mut temp_div);
            let callback_props = temp_div.render_props();

            // Start from base props and merge callback changes on top
            let mut final_props = base_props.unwrap_or_default();
            final_props.merge_from(&callback_props);

            // Queue the prop update for this node
            if let Some(nid) = cached_node_id {
                queue_prop_update(nid, final_props);
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
        let guard = shared.lock().unwrap();

        // Need node_id and callback to refresh
        let (callback, state_copy, cached_node_id, base_props) = match (
            guard.state_callback.as_ref(),
            guard.node_id,
        ) {
            (Some(cb), Some(nid)) => {
                let callback = Arc::clone(cb);
                let state = guard.state;
                let base = guard.base_render_props.clone();
                drop(guard);
                (callback, state, nid, base)
            }
            _ => return,
        };

        // Create temp div, apply callback to get state-specific changes
        let mut temp_div = Div::new();
        callback(&state_copy, &mut temp_div);
        let callback_props = temp_div.render_props();

        // Start from base props and merge callback changes on top
        let mut final_props = base_props.unwrap_or_default();
        final_props.merge_from(&callback_props);

        // Queue the prop update for this node
        queue_prop_update(cached_node_id, final_props);

        // Check if children were set - if so, queue a subtree rebuild
        if !temp_div.children_builders().is_empty() {
            // Queue the child for rebuild - the windowed app will process this
            queue_subtree_rebuild(cached_node_id, temp_div);
        }

        // Request redraw
        request_redraw();
    }

    /// Dispatch a new state
    ///
    /// Updates the current state and applies the callback if the state changed.
    /// Returns true if the state changed.
    pub fn dispatch_state(&mut self, new_state: S) -> bool {
        let mut shared = self.shared_state.lock().unwrap();
        if shared.state != new_state {
            shared.state = new_state;
            // Apply callback - clone via Arc to avoid borrow conflicts
            if let Some(ref callback) = shared.state_callback {
                let callback = Arc::clone(callback);
                let state_copy = shared.state;
                drop(shared); // Release lock before calling callback
                callback(&state_copy, &mut self.inner);
            }
            crate::widgets::request_rebuild();
            true
        } else {
            false
        }
    }

    /// Handle an event and potentially transition state
    ///
    /// Returns true if the state changed.
    pub fn handle_event(&mut self, event: u32) -> bool {
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
    fn apply_state_callback(&mut self) {
        let mut shared = self.shared_state.lock().unwrap();
        // Clone callback to avoid borrow conflicts (Arc makes this cheap)
        if let Some(ref callback) = shared.state_callback {
            let callback = Arc::clone(callback);
            let state_copy = shared.state;
            shared.needs_visual_update = false;
            drop(shared); // Release lock before calling callback
            callback(&state_copy, &mut self.inner);
        }
    }

    // =========================================================================
    // Builder pattern methods that return Self (not Div)
    // =========================================================================

    /// Set width (builder pattern)
    pub fn w(mut self, px: f32) -> Self {
        self.inner = self.inner.swap().w(px);
        self
    }

    /// Set height (builder pattern)
    pub fn h(mut self, px: f32) -> Self {
        self.inner = self.inner.swap().h(px);
        self
    }

    /// Set width to 100% (builder pattern)
    pub fn w_full(mut self) -> Self {
        self.inner = self.inner.swap().w_full();
        self
    }

    /// Set height to 100% (builder pattern)
    pub fn h_full(mut self) -> Self {
        self.inner = self.inner.swap().h_full();
        self
    }

    /// Set both width and height (builder pattern)
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.inner = self.inner.swap().size(w, h);
        self
    }

    /// Set square size (builder pattern)
    pub fn square(mut self, size: f32) -> Self {
        self.inner = self.inner.swap().square(size);
        self
    }

    /// Set flex direction to row (builder pattern)
    pub fn flex_row(mut self) -> Self {
        self.inner = self.inner.swap().flex_row();
        self
    }

    /// Set flex direction to column (builder pattern)
    pub fn flex_col(mut self) -> Self {
        self.inner = self.inner.swap().flex_col();
        self
    }

    /// Set flex grow (builder pattern)
    pub fn flex_grow(mut self) -> Self {
        self.inner = self.inner.swap().flex_grow();
        self
    }

    /// Set width to fit content (builder pattern)
    pub fn w_fit(mut self) -> Self {
        self.inner = self.inner.swap().w_fit();
        self
    }

    /// Set height to fit content (builder pattern)
    pub fn h_fit(mut self) -> Self {
        self.inner = self.inner.swap().h_fit();
        self
    }

    /// Set padding all sides (builder pattern)
    pub fn p(mut self, units: f32) -> Self {
        self.inner = self.inner.swap().p(units);
        self
    }

    /// Set horizontal padding (builder pattern)
    pub fn px(mut self, units: f32) -> Self {
        self.inner = self.inner.swap().px(units);
        self
    }

    /// Set vertical padding (builder pattern)
    pub fn py(mut self, units: f32) -> Self {
        self.inner = self.inner.swap().py(units);
        self
    }

    /// Set gap (builder pattern)
    pub fn gap(mut self, units: f32) -> Self {
        self.inner = self.inner.swap().gap(units);
        self
    }

    /// Center items (builder pattern)
    pub fn items_center(mut self) -> Self {
        self.inner = self.inner.swap().items_center();
        self
    }

    /// Center justify (builder pattern)
    pub fn justify_center(mut self) -> Self {
        self.inner = self.inner.swap().justify_center();
        self
    }

    /// Space between (builder pattern)
    pub fn justify_between(mut self) -> Self {
        self.inner = self.inner.swap().justify_between();
        self
    }

    /// Set background (builder pattern)
    pub fn bg(mut self, color: impl Into<blinc_core::Brush>) -> Self {
        self.inner = self.inner.swap().background(color);
        self
    }

    /// Set corner radius (builder pattern)
    pub fn rounded(mut self, radius: f32) -> Self {
        self.inner = self.inner.swap().rounded(radius);
        self
    }

    /// Set shadow (builder pattern)
    pub fn shadow(mut self, shadow: blinc_core::Shadow) -> Self {
        self.inner = self.inner.swap().shadow(shadow);
        self
    }

    /// Set transform (builder pattern)
    pub fn transform(mut self, transform: blinc_core::Transform) -> Self {
        self.inner = self.inner.swap().transform(transform);
        self
    }

    /// Add child (builder pattern)
    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        self.inner = self.inner.swap().child(child);
        self
    }

    /// Add children (builder pattern)
    pub fn children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator,
        I::Item: ElementBuilder + 'static,
    {
        self.inner = self.inner.swap().children(children);
        self
    }

    // =========================================================================
    // Event Handlers (builder pattern)
    // =========================================================================

    /// Register a click handler (builder pattern)
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.swap().on_click(handler);
        self
    }

    /// Register a mouse down handler (builder pattern)
    pub fn on_mouse_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.swap().on_mouse_down(handler);
        self
    }

    /// Register a mouse up handler (builder pattern)
    pub fn on_mouse_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.swap().on_mouse_up(handler);
        self
    }

    /// Register a hover enter handler (builder pattern)
    pub fn on_hover_enter<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.swap().on_hover_enter(handler);
        self
    }

    /// Register a hover leave handler (builder pattern)
    pub fn on_hover_leave<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.swap().on_hover_leave(handler);
        self
    }

    /// Register a focus handler (builder pattern)
    pub fn on_focus<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.swap().on_focus(handler);
        self
    }

    /// Register a blur handler (builder pattern)
    pub fn on_blur<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.swap().on_blur(handler);
        self
    }

    /// Register a mount handler (builder pattern)
    pub fn on_mount<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.swap().on_mount(handler);
        self
    }

    /// Register an unmount handler (builder pattern)
    pub fn on_unmount<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.swap().on_unmount(handler);
        self
    }

    /// Register a key down handler (builder pattern)
    pub fn on_key_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.swap().on_key_down(handler);
        self
    }

    /// Register a key up handler (builder pattern)
    pub fn on_key_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.swap().on_key_up(handler);
        self
    }

    /// Register a scroll handler (builder pattern)
    pub fn on_scroll<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.swap().on_scroll(handler);
        self
    }

    /// Register a resize handler (builder pattern)
    pub fn on_resize<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.swap().on_resize(handler);
        self
    }

    /// Register a handler for a specific event type (builder pattern)
    pub fn on_event<F>(mut self, event_type: blinc_core::events::EventType, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.swap().on_event(event_type, handler);
        self
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

    /// Set shadow (builder pattern)
    pub fn shadow(self, shadow: blinc_core::Shadow) -> Self {
        self.transform_inner(|s| s.shadow(shadow))
    }

    /// Set transform (builder pattern)
    pub fn transform_style(self, xform: blinc_core::Transform) -> Self {
        self.transform_inner(|s| s.transform(xform))
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
}

impl<S: StateTransitions> ElementBuilder for Stateful<S> {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Div
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
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

/// Create a stateful element from a shared state handle
///
/// This is the primary way to create stateful elements with persistent state:
///
/// ```ignore
/// let handle = ctx.use_state(ButtonState::Idle);
/// stateful(handle)
///     .on_state(|state, div| { ... })
///     .child(text("Click me"))
/// ```
pub fn stateful<S: StateTransitions>(handle: SharedState<S>) -> Stateful<S> {
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
    use crate::text::text;
    use blinc_core::events::event_types;
    use blinc_core::{Brush, Color, CornerRadius, Shadow, Transform};
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
    fn test_state_transition_with_enum() {
        let mut elem = stateful_button()
            .w(100.0)
            .h(40.0)
            .on_state(|state, div| match state {
                ButtonState::Idle => {
                    *div = div.swap().bg(Color::BLUE);
                }
                ButtonState::Hovered => {
                    *div = div.swap().bg(Color::GREEN);
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
        let mut elem = stateful_button()
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
    fn test_toggle_states() {
        let mut t = toggle(false)
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
        let mut cb = stateful_checkbox(false)
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
        let mut field = text_field()
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
        let mut btn = Stateful::new(ButtonState::Disabled)
            .w(100.0)
            .on_state(|_state, _div| {});

        assert_eq!(btn.state(), ButtonState::Disabled);

        assert!(!btn.handle_event(event_types::POINTER_ENTER));
        assert!(!btn.handle_event(event_types::POINTER_DOWN));
        assert!(!btn.handle_event(event_types::POINTER_UP));

        assert_eq!(btn.state(), ButtonState::Disabled);
    }
}
