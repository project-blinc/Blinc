//! Overlay System - Modals, Dialogs, Context Menus, Toasts
//!
//! A flexible overlay infrastructure that renders in a separate pass after the main UI tree,
//! guaranteeing overlays always appear on top regardless of z-index complexity.
//!
//! # Architecture
//!
//! - **OverlayManager**: Global registry accessible via `ctx.overlay_manager()`
//! - **Separate Render Pass**: Overlays render after main tree for guaranteed z-ordering
//! - **FSM-driven State**: Each overlay has Opening/Open/Closing/Closed states
//! - **Motion Animations**: Enter/exit animations via the Motion system
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     let overlay_manager = ctx.overlay_manager();
//!
//!     div()
//!         .child(
//!             button("Open Modal").on_click({
//!                 let mgr = overlay_manager.clone();
//!                 move |_| {
//!                     mgr.modal()
//!                         .child(my_modal_content())
//!                         .show();
//!                 }
//!             })
//!         )
//! }
//! ```

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use blinc_animation::{AnimationPreset, MultiKeyframeAnimation};
use blinc_core::Color;
use indexmap::IndexMap;

use crate::div::{div, Div};
use crate::key::InstanceKey;
use crate::renderer::RenderTree;
use crate::stack::stack;
use crate::stateful::StateTransitions;
use crate::tree::LayoutNodeId;

// =============================================================================
// Overlay Event Types
// =============================================================================

/// Custom event types for overlay state machine
pub mod overlay_events {
    /// Open the overlay (Closed -> Opening)
    pub const OPEN: u32 = 20001;
    /// Close the overlay (Open -> Closing)
    pub const CLOSE: u32 = 20002;
    /// Animation completed (Opening -> Open, Closing -> Closed)
    pub const ANIMATION_COMPLETE: u32 = 20003;
    /// Backdrop was clicked
    pub const BACKDROP_CLICK: u32 = 20004;
    /// Escape key pressed
    pub const ESCAPE: u32 = 20005;
    /// Cancel a pending close (Closing -> Open) - used when mouse re-enters hover card
    pub const CANCEL_CLOSE: u32 = 20006;
    /// Mouse left trigger/content - start close delay countdown (Open -> PendingClose)
    pub const HOVER_LEAVE: u32 = 20007;
    /// Mouse re-entered trigger/content - cancel close delay (PendingClose -> Open)
    pub const HOVER_ENTER: u32 = 20008;
    /// Close delay expired - now actually close (PendingClose -> Closing)
    pub const DELAY_EXPIRED: u32 = 20009;
}

// =============================================================================
// OverlayKind
// =============================================================================

/// Categorizes overlay behavior and default configuration
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OverlayKind {
    /// Modal dialog - blocks interaction, centered, has backdrop
    Modal,
    /// Dialog - like modal but semantic (confirm/alert)
    Dialog,
    /// Context menu - positioned at cursor, click-away dismisses
    ContextMenu,
    /// Toast notification - positioned in corner, auto-dismiss, no block
    Toast,
    /// Tooltip - follows cursor, no block, short-lived
    Tooltip,
    /// Dropdown - positioned relative to anchor element
    Dropdown,
}

impl Default for OverlayKind {
    fn default() -> Self {
        Self::Modal
    }
}

// =============================================================================
// AnchorDirection - for positioned overlays like hover cards
// =============================================================================

/// Direction an overlay is anchored relative to a trigger element.
/// Used to calculate correct bounds for occlusion testing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AnchorDirection {
    /// Overlay appears above the trigger (y is the bottom edge of the overlay)
    Top,
    /// Overlay appears below the trigger (y is the top edge of the overlay)
    #[default]
    Bottom,
    /// Overlay appears to the left of the trigger (x is the right edge)
    Left,
    /// Overlay appears to the right of the trigger (x is the left edge)
    Right,
}

// =============================================================================
// OverlayState - FSM for overlay lifecycle
// =============================================================================

/// State machine for overlay lifecycle
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum OverlayState {
    /// Overlay is not visible
    #[default]
    Closed,
    /// Enter animation is playing
    Opening,
    /// Overlay is fully visible and interactive
    Open,
    /// Mouse left overlay/trigger, waiting for close delay to expire
    /// Used by hover cards to allow mouse movement between trigger and content
    PendingClose,
    /// Exit animation is playing
    Closing,
}

impl OverlayState {
    /// Check if overlay should be rendered
    pub fn is_visible(&self) -> bool {
        !matches!(self, OverlayState::Closed)
    }

    /// Check if overlay is fully open and interactive
    pub fn is_open(&self) -> bool {
        matches!(self, OverlayState::Open | OverlayState::PendingClose)
    }

    /// Check if overlay is animating
    pub fn is_animating(&self) -> bool {
        matches!(self, OverlayState::Opening | OverlayState::Closing)
    }

    /// Check if overlay is waiting for close delay to expire
    pub fn is_pending_close(&self) -> bool {
        matches!(self, OverlayState::PendingClose)
    }
}

impl StateTransitions for OverlayState {
    fn on_event(&self, event: u32) -> Option<Self> {
        use overlay_events::*;
        use OverlayState::*;

        match (self, event) {
            // Closed -> Opening: Start show animation
            (Closed, OPEN) => Some(Opening),

            // Opening -> Open: Animation finished
            (Opening, ANIMATION_COMPLETE) => Some(Open),

            // Open -> Closing: Start hide animation (immediate close)
            (Open, CLOSE) | (Open, ESCAPE) | (Open, BACKDROP_CLICK) => Some(Closing),

            // Open -> PendingClose: Mouse left, start close delay countdown
            (Open, HOVER_LEAVE) => Some(PendingClose),

            // PendingClose -> Open: Mouse re-entered, cancel close delay
            (PendingClose, HOVER_ENTER) => Some(Open),

            // PendingClose -> Closing: Close delay expired, now actually close
            (PendingClose, DELAY_EXPIRED) => Some(Closing),

            // PendingClose -> Closing: Immediate close events still work
            (PendingClose, CLOSE) | (PendingClose, ESCAPE) | (PendingClose, BACKDROP_CLICK) => {
                Some(Closing)
            }

            // Closing -> Closed: Animation finished, remove overlay
            (Closing, ANIMATION_COMPLETE) => Some(Closed),

            // Interrupt opening with close
            (Opening, CLOSE) | (Opening, ESCAPE) => Some(Closing),


            // Cancel close - interrupt exit animation and return to Open state
            // Used when mouse re-enters hover card during exit animation
            (Closing, CANCEL_CLOSE) => Some(Open),

            _ => None,
        }
    }
}

// =============================================================================
// OverlayPosition
// =============================================================================

/// How to position an overlay
#[derive(Clone, Debug)]
pub enum OverlayPosition {
    /// Center in viewport (modals, dialogs)
    Centered,
    /// Position at specific coordinates (context menus)
    AtPoint { x: f32, y: f32 },
    /// Position in a corner (toasts)
    Corner(Corner),
    /// Position relative to an anchor element (dropdowns)
    RelativeToAnchor {
        anchor: LayoutNodeId,
        offset_x: f32,
        offset_y: f32,
    },
}

impl Default for OverlayPosition {
    fn default() -> Self {
        Self::Centered
    }
}

/// Corner positions for toast notifications
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Corner {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
}

// =============================================================================
// BackdropConfig
// =============================================================================

/// Configuration for overlay backdrop
#[derive(Clone, Debug)]
pub struct BackdropConfig {
    /// Backdrop color (usually semi-transparent black)
    pub color: Color,
    /// Whether clicking backdrop closes the overlay
    pub dismiss_on_click: bool,
    /// Blur amount for frosted glass effect (0.0 = no blur)
    pub blur: f32,
}

impl Default for BackdropConfig {
    fn default() -> Self {
        Self {
            color: Color::rgba(0.0, 0.0, 0.0, 0.5),
            dismiss_on_click: true,
            blur: 0.0,
        }
    }
}

impl BackdropConfig {
    /// Create a dark semi-transparent backdrop
    pub fn dark() -> Self {
        Self::default()
    }

    /// Create a light semi-transparent backdrop
    pub fn light() -> Self {
        Self {
            color: Color::rgba(1.0, 1.0, 1.0, 0.3),
            ..Self::default()
        }
    }

    /// Create a backdrop that doesn't dismiss on click
    pub fn persistent() -> Self {
        Self {
            dismiss_on_click: false,
            ..Self::default()
        }
    }

    /// Set the backdrop color
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set whether clicking dismisses the overlay
    pub fn dismiss_on_click(mut self, dismiss: bool) -> Self {
        self.dismiss_on_click = dismiss;
        self
    }

    /// Set blur amount for frosted glass effect
    pub fn blur(mut self, blur: f32) -> Self {
        self.blur = blur;
        self
    }
}

// =============================================================================
// OverlayAnimation
// =============================================================================

/// Animation configuration for overlay enter/exit
#[derive(Clone)]
pub struct OverlayAnimation {
    /// Enter animation
    pub enter: MultiKeyframeAnimation,
    /// Exit animation
    pub exit: MultiKeyframeAnimation,
}

impl Default for OverlayAnimation {
    fn default() -> Self {
        Self::modal()
    }
}

impl std::fmt::Debug for OverlayAnimation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OverlayAnimation")
            .field("enter", &"MultiKeyframeAnimation")
            .field("exit", &"MultiKeyframeAnimation")
            .finish()
    }
}

impl OverlayAnimation {
    /// Default modal animation (scale + fade)
    ///
    /// Note: Exit duration must be >= motion exit animation duration (150ms default for dialogs)
    /// to ensure motion animations complete before overlay is removed.
    pub fn modal() -> Self {
        Self {
            enter: AnimationPreset::scale_in(200),
            exit: AnimationPreset::fade_out(170), // Slightly longer than motion exit (150ms)
        }
    }

    /// Context menu animation (pop in)
    ///
    /// Note: Exit duration must be >= motion exit animation duration (100ms default)
    /// to ensure motion animations complete before overlay is removed.
    pub fn context_menu() -> Self {
        Self {
            enter: AnimationPreset::pop_in(150),
            exit: AnimationPreset::fade_out(120), // Slightly longer than motion exit (100ms)
        }
    }

    /// Toast animation (slide from right)
    pub fn toast() -> Self {
        Self {
            enter: AnimationPreset::slide_in_right(200, 100.0),
            exit: AnimationPreset::slide_out_right(150, 100.0),
        }
    }

    /// Dropdown animation (fade in quickly)
    ///
    /// Note: Exit duration must be >= motion exit animation duration (100ms default)
    /// to ensure motion animations complete before overlay is removed.
    pub fn dropdown() -> Self {
        Self {
            enter: AnimationPreset::fade_in(100),
            exit: AnimationPreset::fade_out(120), // Slightly longer than motion exit (100ms)
        }
    }

    /// No animation (instant show/hide)
    pub fn none() -> Self {
        Self {
            enter: AnimationPreset::fade_in(0),
            exit: AnimationPreset::fade_out(0),
        }
    }

    /// Custom animation
    pub fn custom(enter: MultiKeyframeAnimation, exit: MultiKeyframeAnimation) -> Self {
        Self { enter, exit }
    }
}

// =============================================================================
// OverlayConfig
// =============================================================================

/// Configuration for an overlay instance
#[derive(Clone, Debug)]
pub struct OverlayConfig {
    /// Type of overlay (affects default behavior)
    pub kind: OverlayKind,
    /// How to position the overlay
    pub position: OverlayPosition,
    /// Backdrop configuration (None = no backdrop)
    pub backdrop: Option<BackdropConfig>,
    /// Animation configuration
    pub animation: OverlayAnimation,
    /// Close on Escape key press
    pub dismiss_on_escape: bool,
    /// Close when mouse leaves the overlay content (for hover cards)
    pub dismiss_on_hover_leave: bool,
    /// Auto-dismiss after duration (for toasts)
    pub auto_dismiss_ms: Option<u32>,
    /// Delay before closing after mouse leaves (for hover cards)
    /// When set, mouse leave triggers PendingClose state with this delay
    /// before actually closing. Mouse re-entering cancels the delay.
    pub close_delay_ms: Option<u32>,
    /// Trap focus within overlay (for modals)
    pub focus_trap: bool,
    /// Z-priority (higher = more on top)
    pub z_priority: i32,
    /// Explicit size (None = content-sized)
    pub size: Option<(f32, f32)>,
    /// Motion key for content animation
    ///
    /// When set, the overlay will trigger exit animation on this motion key
    /// when transitioning to Closing state. The full key will be `"motion:{motion_key}"`.
    /// Use this with `motion_derived(motion_key)` in your content builder.
    pub motion_key: Option<String>,
    /// Direction the overlay is anchored relative to its trigger.
    ///
    /// Used to calculate correct bounds for occlusion testing. For example,
    /// a hover card with `Top` direction has its y coordinate at the BOTTOM edge,
    /// not the top edge.
    pub anchor_direction: AnchorDirection,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self::modal()
    }
}

impl OverlayConfig {
    /// Create modal configuration
    pub fn modal() -> Self {
        Self {
            kind: OverlayKind::Modal,
            position: OverlayPosition::Centered,
            backdrop: Some(BackdropConfig::default()),
            animation: OverlayAnimation::modal(),
            dismiss_on_escape: true,
            dismiss_on_hover_leave: false,
            auto_dismiss_ms: None,
            close_delay_ms: None,
            focus_trap: true,
            z_priority: 100,
            size: None,
            motion_key: None,
            anchor_direction: AnchorDirection::Bottom,
        }
    }

    /// Create dialog configuration
    pub fn dialog() -> Self {
        Self {
            kind: OverlayKind::Dialog,
            ..Self::modal()
        }
    }

    /// Create context menu configuration
    pub fn context_menu() -> Self {
        Self {
            kind: OverlayKind::ContextMenu,
            position: OverlayPosition::AtPoint { x: 0.0, y: 0.0 },
            backdrop: None,
            animation: OverlayAnimation::context_menu(),
            dismiss_on_escape: true,
            dismiss_on_hover_leave: false,
            auto_dismiss_ms: None,
            close_delay_ms: None,
            focus_trap: false,
            z_priority: 200,
            size: None,
            motion_key: None,
            anchor_direction: AnchorDirection::Bottom,
        }
    }

    /// Create toast configuration
    pub fn toast() -> Self {
        Self {
            kind: OverlayKind::Toast,
            position: OverlayPosition::Corner(Corner::TopRight),
            backdrop: None,
            animation: OverlayAnimation::toast(),
            dismiss_on_escape: false,
            dismiss_on_hover_leave: false,
            auto_dismiss_ms: Some(3000),
            close_delay_ms: None,
            focus_trap: false,
            z_priority: 300,
            size: None,
            motion_key: None,
            anchor_direction: AnchorDirection::Bottom,
        }
    }

    /// Create dropdown configuration
    pub fn dropdown() -> Self {
        Self {
            kind: OverlayKind::Dropdown,
            position: OverlayPosition::Centered, // Will be overridden by anchor or at()
            // Transparent backdrop that dismisses on click outside
            backdrop: Some(BackdropConfig {
                color: blinc_core::Color::TRANSPARENT,
                dismiss_on_click: true,
                blur: 0.0,
            }),
            animation: OverlayAnimation::dropdown(),
            dismiss_on_escape: true,
            dismiss_on_hover_leave: false,
            auto_dismiss_ms: None,
            close_delay_ms: None,
            focus_trap: false,
            z_priority: 150,
            size: None,
            motion_key: None,
            anchor_direction: AnchorDirection::Bottom,
        }
    }

    /// Create hover card configuration (dropdown that dismisses on mouse leave)
    ///
    /// Hover cards are TRANSIENT overlays - they have NO backdrop and don't block
    /// interaction with the UI below. Multiple hover cards can coexist.
    /// Uses close_delay_ms to allow mouse movement between trigger and content.
    pub fn hover_card() -> Self {
        Self {
            kind: OverlayKind::Tooltip, // Use Tooltip kind for transient behavior
            position: OverlayPosition::Centered, // Will be overridden by at()
            // NO backdrop - transient overlays don't block interaction
            backdrop: None,
            animation: OverlayAnimation::dropdown(),
            dismiss_on_escape: true,
            dismiss_on_hover_leave: true,
            auto_dismiss_ms: Some(5000), // Auto-dismiss after 5 seconds as fallback
            close_delay_ms: Some(300),   // 300ms delay before closing on mouse leave
            focus_trap: false,
            z_priority: 150,
            size: None,
            motion_key: None,
            anchor_direction: AnchorDirection::Bottom, // Will be overridden by anchor_direction()
        }
    }
}

// =============================================================================
// OverlayHandle
// =============================================================================

/// Handle to a specific overlay instance for management
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OverlayHandle(u64);

impl OverlayHandle {
    /// Create a new handle with a unique ID
    fn new(id: u64) -> Self {
        Self(id)
    }

    /// Reconstruct a handle from a raw ID
    ///
    /// This is useful for storing handles in state and reconstructing them later.
    pub fn from_raw(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw ID
    pub fn id(&self) -> u64 {
        self.0
    }
}

// =============================================================================
// ActiveOverlay
// =============================================================================

/// Callback invoked when an overlay is closed
pub type OnCloseCallback = Arc<dyn Fn() + Send + Sync>;

/// An active overlay instance
pub struct ActiveOverlay {
    /// Handle for this overlay
    pub handle: OverlayHandle,
    /// Configuration
    pub config: OverlayConfig,
    /// Current state
    pub state: OverlayState,
    /// Content builder function
    content_builder: Box<dyn Fn() -> Div + Send + Sync>,
    /// Time when overlay was created (for enter animation timing)
    created_at_ms: Option<u64>,
    /// Time when overlay was opened (for auto-dismiss)
    opened_at_ms: Option<u64>,
    /// Time when close animation started (for exit animation timing)
    close_started_at_ms: Option<u64>,
    /// Time when pending close started (for close delay countdown)
    pending_close_at_ms: Option<u64>,
    /// Cached content size after layout (for positioning)
    pub cached_size: Option<(f32, f32)>,
    /// Callback invoked when the overlay is closed (backdrop click, escape, etc.)
    on_close: Option<OnCloseCallback>,
    /// Flag: start close delay when Opening -> Open transition completes
    /// Used when hover_leave is called during Opening state
    pending_close_on_open: bool,
}

impl ActiveOverlay {
    /// Check if overlay should be visible
    pub fn is_visible(&self) -> bool {
        self.state.is_visible()
    }

    /// Build the overlay content
    pub fn build_content(&self) -> Div {
        (self.content_builder)()
    }

    /// Transition to a new state
    ///
    /// When transitioning to Closing state, this will automatically trigger
    /// the exit animation on any motion container with the overlay's motion key
    /// (if configured via `OverlayConfig.motion_key`).
    pub fn transition(&mut self, event: u32) -> bool {
        if let Some(new_state) = self.state.on_event(event) {
            let old_state = self.state;
            self.state = new_state;

            // When transitioning TO Closing state, trigger motion exit if motion_key is configured
            if new_state == OverlayState::Closing && old_state != OverlayState::Closing {
                if let Some(ref key) = self.config.motion_key {
                    let full_motion_key = format!("motion:{}", key);
                    crate::selector::query_motion(&full_motion_key).exit();
                    tracing::debug!(
                        "Overlay {:?} transitioning to Closing - triggered motion exit for key={}",
                        self.handle,
                        full_motion_key
                    );
                }
            }

            true
        } else {
            false
        }
    }

    /// Get the current animation progress (0.0 to 1.0)
    ///
    /// Returns (progress, is_entering) where:
    /// - progress: 0.0 = start of animation, 1.0 = end
    /// - is_entering: true for enter animation, false for exit
    ///
    /// Returns None if not animating (fully visible or closed)
    pub fn animation_progress(&self, current_time_ms: u64) -> Option<(f32, bool)> {
        match self.state {
            OverlayState::Opening => {
                let duration = self.config.animation.enter.duration_ms() as f32;
                if duration <= 0.0 {
                    return None;
                }
                let created_at = self.created_at_ms.unwrap_or(current_time_ms);
                let elapsed = (current_time_ms.saturating_sub(created_at)) as f32;
                let progress = (elapsed / duration).clamp(0.0, 1.0);
                Some((progress, true))
            }
            OverlayState::Closing => {
                let duration = self.config.animation.exit.duration_ms() as f32;
                if duration <= 0.0 {
                    return None;
                }
                let close_started = self.close_started_at_ms.unwrap_or(current_time_ms);
                let elapsed = (current_time_ms.saturating_sub(close_started)) as f32;
                let progress = (elapsed / duration).clamp(0.0, 1.0);
                Some((progress, false))
            }
            _ => None,
        }
    }
}

// =============================================================================
// OverlayManagerInner
// =============================================================================

/// Inner state of the overlay manager
pub struct OverlayManagerInner {
    /// Active overlays indexed by handle
    overlays: IndexMap<OverlayHandle, ActiveOverlay>,
    /// Next overlay ID
    next_id: AtomicU64,
    /// Dirty flag - set when overlays change structurally (added/removed)
    /// This triggers a full content rebuild
    dirty: AtomicBool,
    /// Animation dirty flag - set when animation state changes but content is same
    /// This triggers a re-render but NOT a content rebuild
    animation_dirty: AtomicBool,
    /// Viewport dimensions for positioning (logical pixels)
    viewport: (f32, f32),
    /// DPI scale factor
    scale_factor: f32,
    /// Toast corner preference
    toast_corner: Corner,
    /// Maximum visible toasts
    max_toasts: usize,
    /// Gap between stacked toasts
    toast_gap: f32,
    /// Current time in milliseconds (set by update())
    current_time_ms: u64,
}

impl OverlayManagerInner {
    /// Create a new overlay manager
    pub fn new() -> Self {
        Self {
            overlays: IndexMap::new(),
            next_id: AtomicU64::new(1),
            dirty: AtomicBool::new(false),
            animation_dirty: AtomicBool::new(false),
            viewport: (0.0, 0.0),
            scale_factor: 1.0,
            toast_corner: Corner::TopRight,
            max_toasts: 5,
            toast_gap: 8.0,
            current_time_ms: 0,
        }
    }

    /// Update viewport dimensions (in logical pixels)
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport = (width, height);
    }

    /// Update viewport dimensions with scale factor
    pub fn set_viewport_with_scale(&mut self, width: f32, height: f32, scale_factor: f32) {
        self.viewport = (width, height);
        self.scale_factor = scale_factor;
    }

    /// Get the current scale factor
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Set toast corner preference
    pub fn set_toast_corner(&mut self, corner: Corner) {
        self.toast_corner = corner;
    }

    /// Check and clear dirty flag (content changed, needs full rebuild)
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::SeqCst)
    }

    /// Check dirty flag without clearing (for peeking before render)
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::SeqCst)
    }

    /// Check and clear animation dirty flag (just needs re-render, no content rebuild)
    pub fn take_animation_dirty(&self) -> bool {
        self.animation_dirty.swap(false, Ordering::SeqCst)
    }

    /// Check if needs any kind of redraw (content or animation)
    pub fn needs_redraw(&self) -> bool {
        self.dirty.load(Ordering::SeqCst) || self.animation_dirty.load(Ordering::SeqCst)
    }

    /// Mark as dirty (content changed)
    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// Mark animation dirty (animation state changed but content is same)
    fn mark_animation_dirty(&self) {
        self.animation_dirty.store(true, Ordering::SeqCst);
    }

    /// Add a new overlay
    pub fn add(
        &mut self,
        config: OverlayConfig,
        content: impl Fn() -> Div + Send + Sync + 'static,
    ) -> OverlayHandle {
        self.add_with_close_callback(config, content, None)
    }

    /// Add a new overlay with a close callback
    ///
    /// The callback is invoked when the overlay is dismissed (backdrop click, escape, etc.)
    pub fn add_with_close_callback(
        &mut self,
        config: OverlayConfig,
        content: impl Fn() -> Div + Send + Sync + 'static,
        on_close: Option<OnCloseCallback>,
    ) -> OverlayHandle {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let handle = OverlayHandle::new(id);

        tracing::debug!(
            "OverlayManager::add - adding {:?} overlay with handle {:?}",
            config.kind,
            handle
        );

        let overlay = ActiveOverlay {
            handle,
            config,
            state: OverlayState::Opening,
            content_builder: Box::new(content),
            created_at_ms: None, // Will be set on first update
            opened_at_ms: None,
            close_started_at_ms: None,
            pending_close_at_ms: None,
            cached_size: None,
            on_close,
            pending_close_on_open: false,
        };

        self.overlays.insert(handle, overlay);
        self.mark_dirty();

        tracing::debug!(
            "OverlayManager::add - now have {} overlays",
            self.overlays.len()
        );

        handle
    }

    /// Update overlay states - call this every frame
    ///
    /// This handles:
    /// - Transitioning Opening -> Open after enter animation completes
    /// - Transitioning Closing -> Closed after exit animation completes
    /// - Auto-dismissing toasts after their duration expires
    /// - Removing closed overlays
    pub fn update(&mut self, current_time_ms: u64) {
        // Store current time for use in build_overlay_layer
        self.current_time_ms = current_time_ms;

        let mut to_close = Vec::new();
        let mut content_dirty = false;
        let mut animation_dirty = false;

        for (handle, overlay) in self.overlays.iter_mut() {
            // Initialize created_at_ms on first update
            if overlay.created_at_ms.is_none() {
                overlay.created_at_ms = Some(current_time_ms);
                content_dirty = true;
            }

            match overlay.state {
                OverlayState::Opening => {
                    // Check if enter animation has completed
                    let enter_duration = overlay.config.animation.enter.duration_ms();
                    if let Some(created_at) = overlay.created_at_ms {
                        let elapsed = current_time_ms.saturating_sub(created_at);
                        if elapsed >= enter_duration as u64 {
                            // Animation complete, transition to Open
                            if overlay.transition(overlay_events::ANIMATION_COMPLETE) {
                                overlay.opened_at_ms = Some(current_time_ms);
                                // State transition doesn't change content, just animation
                                animation_dirty = true;

                                // Check if we queued a close during Opening (TOP hover card case)
                                // If mouse left before opening completed, close immediately
                                // without showing the card or playing exit animation
                                if overlay.pending_close_on_open {
                                    overlay.pending_close_on_open = false;
                                    tracing::debug!(
                                        "Overlay {:?} just opened but pending_close_on_open - removing immediately",
                                        handle
                                    );
                                    // Go directly to Closed (skip both Open state and exit animation)
                                    // This prevents the "blink" when mouse leaves trigger before card opens
                                    overlay.state = OverlayState::Closed;
                                    content_dirty = true;
                                }
                            }
                        } else {
                            // Animation still in progress, keep redrawing (no content change)
                            animation_dirty = true;
                        }
                    }
                }
                OverlayState::Open => {
                    // Check for auto-dismiss (toasts)
                    if let Some(duration_ms) = overlay.config.auto_dismiss_ms {
                        if let Some(opened_at) = overlay.opened_at_ms {
                            if current_time_ms >= opened_at + duration_ms as u64 {
                                to_close.push(*handle);
                            }
                        }
                    }
                }
                OverlayState::PendingClose => {
                    // Check if close delay has expired
                    if let Some(close_delay_ms) = overlay.config.close_delay_ms {
                        if let Some(pending_close_at) = overlay.pending_close_at_ms {
                            let elapsed = current_time_ms.saturating_sub(pending_close_at);
                            if elapsed >= close_delay_ms as u64 {
                                // Delay expired, now actually start closing
                                tracing::debug!(
                                    "Overlay {:?} close delay expired after {}ms, transitioning to Closing",
                                    handle,
                                    elapsed
                                );
                                if overlay.transition(overlay_events::DELAY_EXPIRED) {
                                    animation_dirty = true;
                                }
                            }
                            // While waiting, no dirty flag needed - just keep checking
                        }
                    } else {
                        // No close delay configured, immediately close
                        if overlay.transition(overlay_events::DELAY_EXPIRED) {
                            animation_dirty = true;
                        }
                    }
                }
                OverlayState::Closing => {
                    // Initialize close_started_at_ms if not set
                    if overlay.close_started_at_ms.is_none() {
                        overlay.close_started_at_ms = Some(current_time_ms);
                        tracing::debug!(
                            "Overlay {:?} started closing at {}ms, exit duration={}ms",
                            handle,
                            current_time_ms,
                            overlay.config.animation.exit.duration_ms()
                        );
                        // Starting close animation - animation change, not content
                        animation_dirty = true;
                    }

                    // Check if exit animation has completed
                    // Must wait for BOTH:
                    // 1. Overlay's own exit duration
                    // 2. Motion animation (if motion_key configured) to complete
                    let exit_duration = overlay.config.animation.exit.duration_ms();
                    let overlay_exit_complete = if let Some(close_started) = overlay.close_started_at_ms
                    {
                        let elapsed = current_time_ms.saturating_sub(close_started);
                        elapsed >= exit_duration as u64
                    } else {
                        false
                    };

                    // Check if motion animation has completed (if configured)
                    let motion_exit_complete = if let Some(ref key) = overlay.config.motion_key {
                        let full_motion_key = format!("motion:{}", key);
                        let motion = crate::selector::query_motion(&full_motion_key);
                        // Motion is complete if it's not animating (either Visible, Removed, or doesn't exist)
                        !motion.is_animating()
                    } else {
                        true // No motion configured, consider it complete
                    };

                    if overlay_exit_complete && motion_exit_complete {
                        // Both animations complete, transition to Closed
                        tracing::debug!(
                            "Overlay {:?} exit complete (overlay_exit={}, motion_exit={})",
                            handle,
                            overlay_exit_complete,
                            motion_exit_complete
                        );
                        if overlay.transition(overlay_events::ANIMATION_COMPLETE) {
                            // Overlay will be removed - this is a content change
                            content_dirty = true;
                        }
                    } else {
                        // Animation still in progress, keep redrawing (no content change)
                        animation_dirty = true;
                    }
                }
                OverlayState::Closed => {
                    // Will be removed below
                }
            }
        }

        // Close expired toasts
        for handle in to_close {
            if let Some(overlay) = self.overlays.get_mut(&handle) {
                overlay.transition(overlay_events::CLOSE);
                // Starting close - animation change (content rebuild when actually removed)
                animation_dirty = true;
            }
        }

        // Remove closed overlays
        let count_before = self.overlays.len();
        self.overlays.retain(|_, o| o.state != OverlayState::Closed);
        if self.overlays.len() != count_before {
            // Overlays were removed - content change
            content_dirty = true;
        }

        if content_dirty {
            self.mark_dirty();
        } else if animation_dirty {
            self.mark_animation_dirty();
        }
    }

    /// Close an overlay by handle
    pub fn close(&mut self, handle: OverlayHandle) {
        if let Some(overlay) = self.overlays.get_mut(&handle) {
            if overlay.transition(overlay_events::CLOSE) {
                // Starting close animation - animation dirty, not content
                self.mark_animation_dirty();
            }
        }
    }

    /// Cancel a pending close and return overlay to Open state
    ///
    /// Used when mouse re-enters a hover card during exit animation.
    /// This interrupts the exit animation and keeps the overlay visible.
    pub fn cancel_close(&mut self, handle: OverlayHandle) {
        if let Some(overlay) = self.overlays.get_mut(&handle) {
            if overlay.transition(overlay_events::CANCEL_CLOSE) {
                // Canceled close - need to reset motion animation state
                self.mark_animation_dirty();
            }
        }
    }

    /// Trigger hover leave event - starts close delay countdown
    ///
    /// For overlays with `close_delay_ms` configured (like hover cards),
    /// this transitions to PendingClose state. The overlay will close
    /// after the delay unless `hover_enter` is called.
    pub fn hover_leave(&mut self, handle: OverlayHandle) {
        if let Some(overlay) = self.overlays.get_mut(&handle) {
            let old_state = overlay.state;

            // If overlay is still Opening, queue the close for when it opens
            // This handles the TOP hover card case where mouse leaves trigger
            // before the card finishes opening (mouse moving away from card)
            // if old_state == OverlayState::Opening {
            //     tracing::debug!("hover_leave: overlay is Opening, queuing close for when open");
            //     overlay.pending_close_on_open = true;
            //     return;
            // }

            if overlay.transition(overlay_events::HOVER_LEAVE) {
                let new_state = overlay.state;
                tracing::debug!(
                    "Overlay {:?} hover leave: {:?} -> {:?} at {}ms",
                    handle,
                    old_state,
                    new_state,
                    self.current_time_ms
                );
                // If transitioned to PendingClose, record when it started
                if new_state == OverlayState::PendingClose {
                    overlay.pending_close_at_ms = Some(self.current_time_ms);
                }
                // No dirty flag needed - update loop will handle the delay
            }
        }
    }

    /// Trigger hover enter event - cancels close delay countdown
    ///
    /// If overlay is in PendingClose state, this cancels the delay
    /// and returns to Open state.
    pub fn hover_enter(&mut self, handle: OverlayHandle) {
        if let Some(overlay) = self.overlays.get_mut(&handle) {
            // Cancel any queued close-on-open (mouse entered card during Opening)
            if overlay.pending_close_on_open {
                tracing::debug!("hover_enter: canceling pending_close_on_open");
                overlay.pending_close_on_open = false;
            }

            if overlay.transition(overlay_events::HOVER_ENTER) {
                // Clear pending close timestamp
                overlay.pending_close_at_ms = None;
                tracing::debug!(
                    "Overlay {:?} hover enter -> Open (canceled pending close)",
                    handle
                );
                // No dirty flag needed - just staying open
            }
        }
    }

    /// Check if an overlay is in PendingClose state or has queued close
    ///
    /// Returns true if:
    /// - Overlay is in PendingClose state (waiting for close delay), OR
    /// - Overlay is in Opening state with pending_close_on_open flag set
    ///   (close will happen as soon as opening animation completes)
    pub fn is_pending_close(&self, handle: OverlayHandle) -> bool {
        self.overlays
            .get(&handle)
            .map(|o| o.state.is_pending_close() || o.pending_close_on_open)
            .unwrap_or(false)
    }

    /// Set the cached content size for an overlay (for hit testing)
    ///
    /// This is typically called from an `on_ready` callback after the overlay
    /// content has been laid out, providing accurate size for backdrop click detection.
    pub fn set_content_size(&mut self, handle: OverlayHandle, width: f32, height: f32) {
        if let Some(overlay) = self.overlays.get_mut(&handle) {
            overlay.cached_size = Some((width, height));
        }
    }

    /// Get bounds of all visible overlays for occlusion testing
    ///
    /// Returns a list of (x, y, width, height) rectangles for all visible overlay content.
    /// This can be used to determine if a hit test point is within an overlay's bounds,
    /// which helps block hover events on UI elements underneath overlays.
    ///
    /// Note: Uses default size (300x200) for overlays without cached size.
    pub fn get_visible_overlay_bounds(&self) -> Vec<(f32, f32, f32, f32)> {
        let (vp_width, vp_height) = self.viewport;

        self.overlays
            .values()
            .filter(|o| o.is_visible())
            .filter_map(|overlay| {
                // Use cached size if available, otherwise use a reasonable default
                let (w, h) = overlay.cached_size.unwrap_or((300.0, 200.0));

                // Calculate position based on OverlayPosition
                let (mut x, mut y) = match &overlay.config.position {
                    OverlayPosition::AtPoint { x, y } => (*x, *y),
                    OverlayPosition::Centered => {
                        // Centered position - use viewport center minus half size
                        ((vp_width - w) / 2.0, (vp_height - h) / 2.0)
                    }
                    OverlayPosition::Corner(corner) => {
                        let margin = 16.0;
                        match corner {
                            Corner::TopLeft => (margin, margin),
                            Corner::TopRight => (vp_width - w - margin, margin),
                            Corner::BottomLeft => (margin, vp_height - h - margin),
                            Corner::BottomRight => {
                                (vp_width - w - margin, vp_height - h - margin)
                            }
                        }
                    }
                    OverlayPosition::RelativeToAnchor { .. } => {
                        // We don't have anchor bounds here - return None
                        return None;
                    }
                };

                // Adjust position based on anchor direction
                // For AtPoint positions, the (x, y) may be a different edge depending on direction
                if matches!(overlay.config.position, OverlayPosition::AtPoint { .. }) {
                    match overlay.config.anchor_direction {
                        AnchorDirection::Top => {
                            // y is the bottom edge of the overlay, so top edge is y - h
                            y -= h;
                        }
                        AnchorDirection::Left => {
                            // x is the right edge of the overlay, so left edge is x - w
                            x -= w;
                        }
                        AnchorDirection::Bottom | AnchorDirection::Right => {
                            // x/y already represents the top-left corner, no adjustment needed
                        }
                    }
                }

                tracing::debug!(
                    "get_visible_overlay_bounds: kind={:?} pos={:?} dir={:?} bounds=({}, {}, {}, {})",
                    overlay.config.kind,
                    overlay.config.position,
                    overlay.config.anchor_direction,
                    x, y, w, h
                );

                Some((x, y, w, h))
            })
            .collect()
    }

    /// Close the topmost overlay
    pub fn close_top(&mut self) {
        // Find highest z-priority open overlay
        if let Some(handle) = self
            .overlays
            .values()
            .filter(|o| o.state.is_open())
            .max_by_key(|o| o.config.z_priority)
            .map(|o| o.handle)
        {
            self.close(handle);
        }
    }

    /// Close all overlays of a specific kind
    pub fn close_all_of(&mut self, kind: OverlayKind) {
        let handles: Vec<_> = self
            .overlays
            .values()
            .filter(|o| o.config.kind == kind && o.is_visible())
            .map(|o| o.handle)
            .collect();

        for handle in handles {
            self.close(handle);
        }
    }

    /// Close all overlays
    pub fn close_all(&mut self) {
        let handles: Vec<_> = self
            .overlays
            .values()
            .filter(|o| o.is_visible())
            .map(|o| o.handle)
            .collect();

        for handle in handles {
            self.close(handle);
        }
    }

    /// Remove closed overlays
    pub fn cleanup(&mut self) {
        self.overlays.retain(|_, o| o.state != OverlayState::Closed);
    }

    /// Handle escape key - close topmost dismissable overlay
    pub fn handle_escape(&mut self) -> bool {
        if let Some(handle) = self
            .overlays
            .values()
            .filter(|o| o.state.is_open() && o.config.dismiss_on_escape)
            .max_by_key(|o| o.config.z_priority)
            .map(|o| o.handle)
        {
            if let Some(overlay) = self.overlays.get_mut(&handle) {
                // Get callback before mutating
                let on_close = overlay.on_close.clone();
                if overlay.transition(overlay_events::ESCAPE) {
                    self.mark_dirty();
                    // Invoke on_close callback
                    if let Some(cb) = on_close {
                        cb();
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Check if any modal is blocking interaction
    pub fn has_blocking_overlay(&self) -> bool {
        self.overlays.values().any(|o| {
            o.is_visible()
                && matches!(o.config.kind, OverlayKind::Modal | OverlayKind::Dialog)
                && o.config.backdrop.is_some()
        })
    }

    /// Check if any overlay with dismiss-on-click backdrop is visible
    ///
    /// This includes dropdowns, context menus, and other non-blocking overlays
    /// that should be dismissed when clicking outside.
    pub fn has_dismissable_overlay(&self) -> bool {
        self.overlays.values().any(|o| {
            o.state.is_open()
                && o.config
                    .backdrop
                    .as_ref()
                    .map(|b| b.dismiss_on_click)
                    .unwrap_or(false)
        })
    }

    /// Handle backdrop click - close topmost overlay if it has dismiss_on_backdrop_click
    ///
    /// Returns true if a click was handled (overlay closed), false otherwise.
    /// The caller should call this when a mouse click is detected and there's a blocking overlay.
    pub fn handle_backdrop_click(&mut self) -> bool {
        // Find topmost open overlay with backdrop that allows dismissal
        if let Some(handle) = self
            .overlays
            .values()
            .filter(|o| {
                o.state.is_open()
                    && o.config
                        .backdrop
                        .as_ref()
                        .map(|b| b.dismiss_on_click)
                        .unwrap_or(false)
            })
            .max_by_key(|o| o.config.z_priority)
            .map(|o| o.handle)
        {
            if let Some(overlay) = self.overlays.get_mut(&handle) {
                // Get callback before mutating
                let on_close = overlay.on_close.clone();
                if overlay.transition(overlay_events::BACKDROP_CLICK) {
                    self.mark_dirty();
                    // Invoke on_close callback
                    if let Some(cb) = on_close {
                        cb();
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Check if a click at the given position should dismiss an overlay
    ///
    /// This checks if the click is outside the content bounds of any open overlay
    /// with backdrop dismiss enabled. Uses cached content sizes for hit testing.
    ///
    /// # Arguments
    /// * `x` - Logical x coordinate
    /// * `y` - Logical y coordinate
    ///
    /// # Returns
    /// True if the click is on a backdrop (outside content), false if on content or no overlay
    pub fn is_backdrop_click(&self, x: f32, y: f32) -> bool {
        // Find topmost overlay with backdrop dismiss enabled
        if let Some(overlay) = self
            .overlays
            .values()
            .filter(|o| {
                o.state.is_open()
                    && o.config
                        .backdrop
                        .as_ref()
                        .map(|b| b.dismiss_on_click)
                        .unwrap_or(false)
            })
            .max_by_key(|o| o.config.z_priority)
        {
            // Get content size (may be cached or estimated)
            let (content_w, content_h) = overlay.cached_size.unwrap_or_else(|| {
                // Fallback: estimate based on overlay config size or default
                overlay.config.size.unwrap_or((400.0, 300.0))
            });

            // Compute content position based on overlay position type
            let (vp_w, vp_h) = self.viewport;
            let (content_x, content_y) = match &overlay.config.position {
                OverlayPosition::Centered => {
                    // Center the content in viewport
                    ((vp_w - content_w) / 2.0, (vp_h - content_h) / 2.0)
                }
                OverlayPosition::AtPoint { x: px, y: py } => {
                    // Content is positioned at the specified point
                    (*px, *py)
                }
                OverlayPosition::Corner(corner) => {
                    // Position in corner with margin
                    let margin = 16.0;
                    match corner {
                        Corner::TopLeft => (margin, margin),
                        Corner::TopRight => (vp_w - content_w - margin, margin),
                        Corner::BottomLeft => (margin, vp_h - content_h - margin),
                        Corner::BottomRight => {
                            (vp_w - content_w - margin, vp_h - content_h - margin)
                        }
                    }
                }
                OverlayPosition::RelativeToAnchor {
                    offset_x, offset_y, ..
                } => {
                    // For anchor-based positioning, use offset as position
                    // (Anchor lookup not yet implemented, so treat as point)
                    (*offset_x, *offset_y)
                }
            };

            // Check if click is outside content bounds
            let in_content = x >= content_x
                && x <= content_x + content_w
                && y >= content_y
                && y <= content_y + content_h;

            // Click is on backdrop if NOT in content
            !in_content
        } else {
            false
        }
    }

    /// Handle click at position - dismisses if on backdrop
    ///
    /// Convenience method that combines `is_backdrop_click` and `handle_backdrop_click`.
    pub fn handle_click_at(&mut self, x: f32, y: f32) -> bool {
        if self.is_backdrop_click(x, y) {
            self.handle_backdrop_click()
        } else {
            false
        }
    }

    /// Get overlays sorted by z-priority
    pub fn overlays_sorted(&self) -> Vec<&ActiveOverlay> {
        let mut overlays: Vec<_> = self.overlays.values().collect();
        overlays.sort_by_key(|o| o.config.z_priority);
        overlays
    }

    /// Check if there are any visible overlays
    pub fn has_visible_overlays(&self) -> bool {
        self.overlays.values().any(|o| o.is_visible())
    }

    /// Check if any overlay is currently animating (entering or exiting)
    pub fn has_animating_overlays(&self) -> bool {
        self.overlays.values().any(|o| o.state.is_animating())
    }

    /// Get the number of overlays
    pub fn overlay_count(&self) -> usize {
        self.overlays.len()
    }

    /// Build the overlay render tree (DEPRECATED - use build_overlay_layer instead)
    ///
    /// This method creates a separate RenderTree for overlays. Prefer using
    /// `build_overlay_layer()` which returns a Div that can be composed into
    /// the main UI tree for unified event routing and incremental updates.
    pub fn build_overlay_tree(&self) -> Option<RenderTree> {
        if !self.has_visible_overlays() {
            return None;
        }

        let (width, height) = self.viewport;
        if width <= 0.0 || height <= 0.0 {
            tracing::debug!("build_overlay_tree: invalid viewport");
            return None;
        }

        // Build stack with all visible overlays
        let mut root = stack().w(width).h(height);

        for overlay in self.overlays_sorted() {
            if overlay.is_visible() {
                tracing::debug!(
                    "build_overlay_tree: adding overlay {:?}",
                    overlay.config.kind
                );
                root = root.child(self.build_single_overlay(overlay, width, height));
            }
        }

        tracing::debug!("build_overlay_tree: building render tree");
        let mut tree = RenderTree::from_element(&root);
        // CRITICAL: Apply scale factor for HiDPI displays
        tree.set_scale_factor(self.scale_factor);
        // CRITICAL: Compute layout before rendering, otherwise all positions/sizes are zero
        tree.compute_layout(width, height);
        Some(tree)
    }
}

/// The element ID used for the overlay layer container
pub const OVERLAY_LAYER_ID: &'static str = "__blinc_overlay_layer__";

impl OverlayManagerInner {
    /// Build the overlay layer container for the main UI tree
    ///
    /// This ALWAYS returns a Div container with a stable ID, even when empty.
    /// This enables subtree rebuilds to update overlay content without
    /// triggering a full UI rebuild.
    ///
    /// The container uses absolute positioning so it doesn't affect main UI layout.
    /// When empty (no visible overlays), the container has zero size so it doesn't
    /// block events to the UI below.
    pub fn build_overlay_layer(&self) -> Div {
        let (width, height) = self.viewport;
        let has_visible = self.has_visible_overlays();
        let overlay_count = self.overlays.len();

        tracing::debug!(
            "build_overlay_layer: viewport={}x{}, has_visible={}, overlay_count={}",
            width,
            height,
            has_visible,
            overlay_count
        );

        // Container size: full viewport when overlays visible, zero when empty
        // Zero size ensures the empty container doesn't block events to UI below
        let (layer_w, layer_h) = if has_visible && width > 0.0 && height > 0.0 {
            (width, height)
        } else {
            (0.0, 0.0)
        };

        tracing::debug!("build_overlay_layer: layer size={}x{}", layer_w, layer_h);

        // Always return a container with a stable ID
        // This allows subtree rebuilds to find and update it
        // Use .stack_layer() to ensure overlay content renders above main UI
        // through z_layer increment in the interleaved rendering system
        let mut layer = div()
            .id(OVERLAY_LAYER_ID)
            .w(layer_w)
            .h(layer_h)
            .absolute()
            .left(0.0)
            .top(0.0)
            .stack_layer();

        // Add visible overlays as children
        if has_visible && width > 0.0 && height > 0.0 {
            for overlay in self.overlays_sorted() {
                if overlay.is_visible() {
                    layer = layer.child(self.build_single_overlay(overlay, width, height));
                }
            }
        }

        layer
    }

    /// Build overlay layer content for subtree rebuild
    ///
    /// This is called when overlay content changes to queue a subtree rebuild
    /// instead of triggering a full UI rebuild.
    pub fn build_overlay_content(&self) -> Div {
        self.build_overlay_layer()
    }

    /// Build a single overlay with backdrop and content
    fn build_single_overlay(&self, overlay: &ActiveOverlay, vp_width: f32, vp_height: f32) -> Div {
        // Content is built by the user - they should wrap it in motion() for animations
        // Motion exit is triggered explicitly via query_motion(key).exit() when
        // transitioning to Closing state (see transition() method).
        let content = overlay.build_content();

        // Apply size constraints if specified
        let content = if let Some((w, h)) = overlay.config.size {
            content.w(w).h(h)
        } else {
            content
        };

        // Wrap content with hover leave handler if dismiss_on_hover_leave is enabled
        let content = if overlay.config.dismiss_on_hover_leave {
            let overlay_handle = overlay.handle;
            let has_close_delay = overlay.config.close_delay_ms.is_some();
            let on_close_callback = overlay.on_close.clone();
            content.on_hover_leave(move |_| {
                tracing::debug!("OVERLAY dismiss_on_hover_leave handler fired");
                if let Some(ctx) = crate::overlay_state::OverlayContext::try_get() {
                    let mgr = ctx.overlay_manager();
                    let mut inner = mgr.lock().unwrap();
                    if has_close_delay {
                        // Use hover_leave to start close delay countdown
                        tracing::debug!("OVERLAY: calling hover_leave (has_close_delay=true)");
                        inner.hover_leave(overlay_handle);
                    } else {
                        // Close immediately (no delay configured)
                        inner.close(overlay_handle);
                        // Call the on_close callback if provided
                        if let Some(ref cb) = on_close_callback {
                            cb();
                        }
                    }
                }
            })
        } else {
            content
        };

        // Calculate backdrop opacity based on animation state
        let backdrop_opacity = if let Some((progress, is_entering)) =
            overlay.animation_progress(self.current_time_ms)
        {
            if is_entering {
                progress // Fade in: 0 -> 1
            } else {
                1.0 - progress // Fade out: 1 -> 0
            }
        } else {
            match overlay.state {
                OverlayState::Open | OverlayState::PendingClose => 1.0,
                OverlayState::Closed => 0.0,
                OverlayState::Opening => 0.0,
                OverlayState::Closing => 1.0,
            }
        };

        // Build the layer with optional backdrop
        if let Some(ref backdrop_config) = overlay.config.backdrop {
            // Apply opacity to backdrop color
            let backdrop_color = backdrop_config
                .color
                .with_alpha(backdrop_config.color.a * backdrop_opacity);

            // Build backdrop div with click-to-dismiss if enabled
            let backdrop_div = if backdrop_config.dismiss_on_click {
                let overlay_handle = overlay.handle;
                let on_close_callback = overlay.on_close.clone();
                let backdrop_key =
                    InstanceKey::explicit(format!("overlay_backdrop_{}", overlay_handle.0));

                div()
                    .id(backdrop_key.get())
                    .absolute()
                    .left(0.0)
                    .top(0.0)
                    .w(vp_width)
                    .h(vp_height)
                    .bg(backdrop_color)
                    .on_click(move |_| {
                        println!("Backdrop clicked! Dismissing overlay {:?}", overlay_handle);
                        // Close this overlay via the global overlay manager
                        if let Some(ctx) = crate::overlay_state::OverlayContext::try_get() {
                            ctx.overlay_manager().lock().unwrap().close(overlay_handle);
                        }
                        // Call the on_close callback if provided
                        if let Some(ref cb) = on_close_callback {
                            cb();
                        }
                    })
            } else {
                div()
                    .absolute()
                    .left(0.0)
                    .top(0.0)
                    .w(vp_width)
                    .h(vp_height)
                    .bg(backdrop_color)
            };

            // Use stack: first child (backdrop) renders behind, second child (content) on top
            div().w(vp_width).h(vp_height).child(
                stack()
                    .w(vp_width)
                    .h(vp_height)
                    // Backdrop layer (behind) - fills entire viewport with animated opacity
                    .child(backdrop_div)
                    // Content layer (on top) - positioned according to config
                    // Content animation is handled by user via motion() container
                    .child(self.position_content(overlay, content, vp_width, vp_height)),
            )
        } else {
            // // No backdrop - wrap in viewport-sized container for proper z-ordering
            // // The container is pointer-events:none equivalent (no event handlers)
            // // so it doesn't block events to UI below
            // div()
            //     .w(vp_width)
            //     .h(vp_height)
            //     .absolute()
            //     .left(0.0)
            //     .top(0.0)
            //     .child()

            self.position_content(overlay, content, vp_width, vp_height)
        }
    }

    /// Position content according to overlay position config
    fn position_content(
        &self,
        overlay: &ActiveOverlay,
        content: Div,
        vp_width: f32,
        vp_height: f32,
    ) -> Div {
        match &overlay.config.position {
            OverlayPosition::Centered => {
                // Center using flexbox
                div()
                    .w(vp_width)
                    .h(vp_height)
                    .items_center()
                    .justify_center()
                    .child(content)
            }

            OverlayPosition::AtPoint { x, y } => {
                // Position content at specific point using absolute positioning within viewport
                content.absolute().left(*x).top(*y)
            }

            OverlayPosition::Corner(corner) => {
                // Position in corner with margin
                let margin = 16.0;
                self.position_in_corner(content, *corner, vp_width, vp_height, margin)
            }

            OverlayPosition::RelativeToAnchor {
                offset_x, offset_y, ..
            } => {
                // For now, treat as point position
                // TODO: Look up anchor bounds from tree
                div()
                    .w(vp_width)
                    .h(vp_height)
                    .child(content.ml(*offset_x).mt(*offset_y))
            }
        }
    }

    /// Position content in a corner
    fn position_in_corner(
        &self,
        content: Div,
        corner: Corner,
        vp_width: f32,
        vp_height: f32,
        margin: f32,
    ) -> Div {
        let container = div().w(vp_width).h(vp_height);

        match corner {
            Corner::TopLeft => container
                .items_start()
                .justify_start()
                .child(content.m(margin)),
            Corner::TopRight => container
                .items_end()
                .justify_start()
                .child(content.m(margin)),
            Corner::BottomLeft => container
                .items_start()
                .justify_end()
                .child(content.m(margin)),
            Corner::BottomRight => container.items_end().justify_end().child(content.m(margin)),
        }
    }

    /// Layout toasts in a stack
    pub fn layout_toasts(&self) -> Vec<(OverlayHandle, f32, f32)> {
        let (vp_width, vp_height) = self.viewport;
        let toasts: Vec<_> = self
            .overlays
            .values()
            .filter(|o| o.config.kind == OverlayKind::Toast && o.is_visible())
            .collect();

        let margin = 16.0;
        let mut positions = Vec::new();
        let mut y_offset = margin;

        for (i, toast) in toasts.iter().take(self.max_toasts).enumerate() {
            // Estimate toast height (will be refined after layout)
            let estimated_height = toast.cached_size.map(|(_, h)| h).unwrap_or(60.0);

            let (x, y) = match self.toast_corner {
                Corner::TopLeft => (margin, y_offset),
                Corner::TopRight => (vp_width - margin - 300.0, y_offset), // Assume 300px width
                Corner::BottomLeft => (margin, vp_height - y_offset - estimated_height),
                Corner::BottomRight => (
                    vp_width - margin - 300.0,
                    vp_height - y_offset - estimated_height,
                ),
            };

            positions.push((toast.handle, x, y));

            // Stack vertically
            y_offset += estimated_height + self.toast_gap;
        }

        positions
    }
}

impl Default for OverlayManagerInner {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// OverlayManager
// =============================================================================

/// Thread-safe overlay manager
pub type OverlayManager = Arc<Mutex<OverlayManagerInner>>;

/// Create a new overlay manager
pub fn overlay_manager() -> OverlayManager {
    Arc::new(Mutex::new(OverlayManagerInner::new()))
}

// =============================================================================
// Builder Extension Trait
// =============================================================================

/// Extension trait for OverlayManager to create builders
pub trait OverlayManagerExt {
    /// Start building a modal overlay
    fn modal(&self) -> ModalBuilder;
    /// Start building a dialog overlay
    fn dialog(&self) -> DialogBuilder;
    /// Start building a context menu overlay
    fn context_menu(&self) -> ContextMenuBuilder;
    /// Start building a toast overlay
    fn toast(&self) -> ToastBuilder;
    /// Start building a dropdown overlay
    fn dropdown(&self) -> DropdownBuilder;
    /// Start building a hover card overlay (dropdown that closes on mouse leave)
    fn hover_card(&self) -> DropdownBuilder;

    /// Close an overlay by handle
    fn close(&self, handle: OverlayHandle);
    /// Cancel a pending close (Closing -> Open)
    ///
    /// Used when mouse re-enters a hover card during exit animation.
    fn cancel_close(&self, handle: OverlayHandle);
    /// Trigger hover leave - starts close delay countdown (Open -> PendingClose)
    ///
    /// For overlays with close_delay_ms configured, starts the countdown.
    /// Use hover_enter to cancel before the delay expires.
    fn hover_leave(&self, handle: OverlayHandle);
    /// Trigger hover enter - cancels close delay countdown (PendingClose -> Open)
    ///
    /// If overlay is in PendingClose state, cancels the delay and stays open.
    fn hover_enter(&self, handle: OverlayHandle);
    /// Check if an overlay is in PendingClose state (waiting for close delay)
    fn is_pending_close(&self, handle: OverlayHandle) -> bool;
    /// Close the topmost overlay
    fn close_top(&self);
    /// Close all overlays of a kind
    fn close_all_of(&self, kind: OverlayKind);
    /// Close all overlays
    fn close_all(&self);
    /// Handle escape key
    fn handle_escape(&self) -> bool;
    /// Handle backdrop click (dismiss if applicable)
    fn handle_backdrop_click(&self) -> bool;
    /// Handle click at position - dismisses if on backdrop
    fn handle_click_at(&self, x: f32, y: f32) -> bool;
    /// Update viewport dimensions (logical pixels)
    fn set_viewport(&self, width: f32, height: f32);
    /// Update viewport dimensions with scale factor
    fn set_viewport_with_scale(&self, width: f32, height: f32, scale_factor: f32);
    /// Build overlay render tree (DEPRECATED - use build_overlay_layer instead)
    fn build_overlay_tree(&self) -> Option<RenderTree>;
    /// Build overlay layer as a Div for composing into main UI tree (always returns a container)
    fn build_overlay_layer(&self) -> Div;
    /// Check if any blocking overlay is active
    fn has_blocking_overlay(&self) -> bool;
    /// Check if any dismissable overlay is visible (dropdown, context menu, etc.)
    fn has_dismissable_overlay(&self) -> bool;
    /// Check if any overlay is visible
    fn has_visible_overlays(&self) -> bool;
    /// Check if any overlay is currently animating (entering or exiting)
    fn has_animating_overlays(&self) -> bool;
    /// Check if a specific overlay handle is still visible
    fn is_visible(&self, handle: OverlayHandle) -> bool;
    /// Update overlay states - call every frame for animations and auto-dismiss
    fn update(&self, current_time_ms: u64);
    /// Take the dirty flag (returns true if content changed and needs full rebuild)
    fn take_dirty(&self) -> bool;
    /// Check dirty flag without clearing (for peeking before render)
    fn is_dirty(&self) -> bool;
    /// Take the animation dirty flag (returns true if animation changed but content is same)
    fn take_animation_dirty(&self) -> bool;
    /// Check if needs any kind of redraw (content or animation)
    fn needs_redraw(&self) -> bool;
    /// Set the cached content size for an overlay (for hit testing)
    fn set_content_size(&self, handle: OverlayHandle, width: f32, height: f32);
    /// Mark overlay content as dirty (triggers full rebuild)
    ///
    /// Call this when state used in overlay content changes and needs to be reflected.
    /// This is needed because overlay content is built once and cached.
    ///
    /// WARNING: This triggers a full content rebuild which re-initializes motion animations.
    /// For simple visual updates like hover state changes, use `request_redraw()` instead.
    fn mark_content_dirty(&self);

    /// Request a redraw without rebuilding content
    ///
    /// Use this for visual state updates that don't require the overlay tree to be rebuilt,
    /// such as hover state changes. This avoids re-triggering motion animations.
    fn request_redraw(&self);

    /// Get bounds of all visible overlays for occlusion testing
    ///
    /// Returns a list of (x, y, width, height) rectangles for all visible overlay content.
    /// Use this for overlay-aware hit testing to prevent hover events on elements
    /// that are covered by overlays.
    fn get_visible_overlay_bounds(&self) -> Vec<(f32, f32, f32, f32)>;
}

impl OverlayManagerExt for OverlayManager {
    fn modal(&self) -> ModalBuilder {
        ModalBuilder::new(Arc::clone(self))
    }

    fn dialog(&self) -> DialogBuilder {
        DialogBuilder::new(Arc::clone(self))
    }

    fn context_menu(&self) -> ContextMenuBuilder {
        ContextMenuBuilder::new(Arc::clone(self))
    }

    fn toast(&self) -> ToastBuilder {
        ToastBuilder::new(Arc::clone(self))
    }

    fn dropdown(&self) -> DropdownBuilder {
        DropdownBuilder::new(Arc::clone(self))
    }

    fn hover_card(&self) -> DropdownBuilder {
        DropdownBuilder::new_hover_card(Arc::clone(self))
    }

    fn close(&self, handle: OverlayHandle) {
        self.lock().unwrap().close(handle);
    }

    fn cancel_close(&self, handle: OverlayHandle) {
        self.lock().unwrap().cancel_close(handle);
    }

    fn hover_leave(&self, handle: OverlayHandle) {
        self.lock().unwrap().hover_leave(handle);
    }

    fn hover_enter(&self, handle: OverlayHandle) {
        self.lock().unwrap().hover_enter(handle);
    }

    fn is_pending_close(&self, handle: OverlayHandle) -> bool {
        self.lock().unwrap().is_pending_close(handle)
    }

    fn close_top(&self) {
        self.lock().unwrap().close_top();
    }

    fn close_all_of(&self, kind: OverlayKind) {
        self.lock().unwrap().close_all_of(kind);
    }

    fn close_all(&self) {
        self.lock().unwrap().close_all();
    }

    fn handle_escape(&self) -> bool {
        self.lock().unwrap().handle_escape()
    }

    fn handle_backdrop_click(&self) -> bool {
        self.lock().unwrap().handle_backdrop_click()
    }

    fn handle_click_at(&self, x: f32, y: f32) -> bool {
        self.lock().unwrap().handle_click_at(x, y)
    }

    fn set_viewport(&self, width: f32, height: f32) {
        self.lock().unwrap().set_viewport(width, height);
    }

    fn set_viewport_with_scale(&self, width: f32, height: f32, scale_factor: f32) {
        self.lock()
            .unwrap()
            .set_viewport_with_scale(width, height, scale_factor);
    }

    fn build_overlay_tree(&self) -> Option<RenderTree> {
        self.lock().unwrap().build_overlay_tree()
    }

    fn build_overlay_layer(&self) -> Div {
        self.lock().unwrap().build_overlay_layer()
    }

    fn has_blocking_overlay(&self) -> bool {
        self.lock().unwrap().has_blocking_overlay()
    }

    fn has_dismissable_overlay(&self) -> bool {
        self.lock().unwrap().has_dismissable_overlay()
    }

    fn has_visible_overlays(&self) -> bool {
        self.lock().unwrap().has_visible_overlays()
    }

    fn has_animating_overlays(&self) -> bool {
        self.lock().unwrap().has_animating_overlays()
    }

    fn is_visible(&self, handle: OverlayHandle) -> bool {
        self.lock()
            .unwrap()
            .overlays
            .get(&handle)
            .map(|o| o.is_visible())
            .unwrap_or(false)
    }

    fn update(&self, current_time_ms: u64) {
        self.lock().unwrap().update(current_time_ms);
    }

    fn take_dirty(&self) -> bool {
        self.lock().unwrap().take_dirty()
    }

    fn is_dirty(&self) -> bool {
        self.lock().unwrap().is_dirty()
    }

    fn take_animation_dirty(&self) -> bool {
        self.lock().unwrap().take_animation_dirty()
    }

    fn needs_redraw(&self) -> bool {
        self.lock().unwrap().needs_redraw()
    }

    fn set_content_size(&self, handle: OverlayHandle, width: f32, height: f32) {
        self.lock().unwrap().set_content_size(handle, width, height);
    }

    fn mark_content_dirty(&self) {
        self.lock().unwrap().mark_dirty();
    }

    fn request_redraw(&self) {
        self.lock().unwrap().mark_animation_dirty();
    }

    fn get_visible_overlay_bounds(&self) -> Vec<(f32, f32, f32, f32)> {
        self.lock().unwrap().get_visible_overlay_bounds()
    }
}

// =============================================================================
// Builders
// =============================================================================

/// Builder for modal overlays
pub struct ModalBuilder {
    manager: OverlayManager,
    config: OverlayConfig,
    content: Option<Box<dyn Fn() -> Div + Send + Sync>>,
}

impl ModalBuilder {
    fn new(manager: OverlayManager) -> Self {
        Self {
            manager,
            config: OverlayConfig::modal(),
            content: None,
        }
    }

    /// Set the content using a builder function
    ///
    /// # Example
    /// ```ignore
    /// overlay_manager.modal()
    ///     .content(|| {
    ///         div().p(20.0).bg(Color::WHITE)
    ///             .child(text("Modal Content"))
    ///     })
    ///     .show()
    /// ```
    pub fn content<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.content = Some(Box::new(f));
        self
    }

    /// Set explicit size
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.config.size = Some((width, height));
        self
    }

    /// Set backdrop configuration
    pub fn backdrop(mut self, config: BackdropConfig) -> Self {
        self.config.backdrop = Some(config);
        self
    }

    /// Remove backdrop
    pub fn no_backdrop(mut self) -> Self {
        self.config.backdrop = None;
        self
    }

    /// Set dismiss on escape
    pub fn dismiss_on_escape(mut self, dismiss: bool) -> Self {
        self.config.dismiss_on_escape = dismiss;
        self
    }

    /// Set animation
    pub fn animation(mut self, animation: OverlayAnimation) -> Self {
        self.config.animation = animation;
        self
    }

    /// Show the modal
    pub fn show(self) -> OverlayHandle {
        let content = self.content.unwrap_or_else(|| Box::new(|| div()));
        self.manager.lock().unwrap().add(self.config, content)
    }
}

/// Builder for dialog overlays
pub struct DialogBuilder {
    manager: OverlayManager,
    config: OverlayConfig,
    content: Option<Box<dyn Fn() -> Div + Send + Sync>>,
}

impl DialogBuilder {
    fn new(manager: OverlayManager) -> Self {
        Self {
            manager,
            config: OverlayConfig::dialog(),
            content: None,
        }
    }

    /// Set the content using a builder function
    pub fn content<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.content = Some(Box::new(f));
        self
    }

    /// Set explicit size
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.config.size = Some((width, height));
        self
    }

    /// Show the dialog
    pub fn show(self) -> OverlayHandle {
        let content = self.content.unwrap_or_else(|| Box::new(|| div()));
        self.manager.lock().unwrap().add(self.config, content)
    }
}

/// Builder for context menu overlays
pub struct ContextMenuBuilder {
    manager: OverlayManager,
    config: OverlayConfig,
    content: Option<Box<dyn Fn() -> Div + Send + Sync>>,
}

impl ContextMenuBuilder {
    fn new(manager: OverlayManager) -> Self {
        Self {
            manager,
            config: OverlayConfig::context_menu(),
            content: None,
        }
    }

    /// Position at coordinates
    pub fn at(mut self, x: f32, y: f32) -> Self {
        self.config.position = OverlayPosition::AtPoint { x, y };
        self
    }

    /// Set the content using a builder function
    pub fn content<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.content = Some(Box::new(f));
        self
    }

    /// Show the context menu
    pub fn show(self) -> OverlayHandle {
        let content = self.content.unwrap_or_else(|| Box::new(|| div()));
        self.manager.lock().unwrap().add(self.config, content)
    }
}

/// Builder for toast overlays
pub struct ToastBuilder {
    manager: OverlayManager,
    config: OverlayConfig,
    content: Option<Box<dyn Fn() -> Div + Send + Sync>>,
}

impl ToastBuilder {
    fn new(manager: OverlayManager) -> Self {
        Self {
            manager,
            config: OverlayConfig::toast(),
            content: None,
        }
    }

    /// Set auto-dismiss duration in milliseconds
    pub fn duration_ms(mut self, ms: u32) -> Self {
        self.config.auto_dismiss_ms = Some(ms);
        self
    }

    /// Set corner position
    pub fn corner(mut self, corner: Corner) -> Self {
        self.config.position = OverlayPosition::Corner(corner);
        self
    }

    /// Set the content using a builder function
    pub fn content<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.content = Some(Box::new(f));
        self
    }

    /// Show the toast
    pub fn show(self) -> OverlayHandle {
        let content = self.content.unwrap_or_else(|| Box::new(|| div()));
        self.manager.lock().unwrap().add(self.config, content)
    }
}

/// Builder for dropdown overlays
pub struct DropdownBuilder {
    manager: OverlayManager,
    config: OverlayConfig,
    content: Option<Box<dyn Fn() -> Div + Send + Sync>>,
    on_close: Option<OnCloseCallback>,
}

impl DropdownBuilder {
    fn new(manager: OverlayManager) -> Self {
        Self {
            manager,
            config: OverlayConfig::dropdown(),
            content: None,
            on_close: None,
        }
    }

    fn new_hover_card(manager: OverlayManager) -> Self {
        Self {
            manager,
            config: OverlayConfig::hover_card(),
            content: None,
            on_close: None,
        }
    }

    /// Position at specific coordinates
    ///
    /// This is useful when the dropdown position is calculated from mouse position
    /// or other dynamic sources.
    pub fn at(mut self, x: f32, y: f32) -> Self {
        self.config.position = OverlayPosition::AtPoint { x, y };
        self
    }

    /// Position relative to an anchor element
    pub fn anchor(mut self, node: LayoutNodeId) -> Self {
        self.config.position = OverlayPosition::RelativeToAnchor {
            anchor: node,
            offset_x: 0.0,
            offset_y: 0.0,
        };
        self
    }

    /// Set offset from anchor
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        if let OverlayPosition::RelativeToAnchor {
            offset_x, offset_y, ..
        } = &mut self.config.position
        {
            *offset_x = x;
            *offset_y = y;
        }
        self
    }

    /// Enable dismiss on escape key
    pub fn dismiss_on_escape(mut self, dismiss: bool) -> Self {
        self.config.dismiss_on_escape = dismiss;
        self
    }

    /// Enable dismiss when mouse leaves the overlay content (for hover cards)
    pub fn dismiss_on_hover_leave(mut self, dismiss: bool) -> Self {
        self.config.dismiss_on_hover_leave = dismiss;
        // When using hover leave dismiss, we typically don't want a backdrop
        if dismiss {
            self.config.backdrop = None;
        }
        self
    }

    /// Set the expected content size for hit testing
    ///
    /// This helps with backdrop click detection by providing the expected
    /// size of the dropdown content. Without this, a default size is used.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.config.size = Some((width, height));
        self
    }

    /// Set the content using a builder function
    pub fn content<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.content = Some(Box::new(f));
        self
    }

    /// Set a callback to be invoked when the dropdown is closed
    ///
    /// This is called when the dropdown is dismissed via backdrop click, escape key, etc.
    pub fn on_close<F>(mut self, f: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_close = Some(Arc::new(f));
        self
    }

    /// Set the motion key for triggering content exit animations
    ///
    /// When the overlay transitions to Closing state, it will automatically
    /// trigger exit animation on the motion with this key. Use the same key
    /// with `motion_derived(key)` in your content builder.
    ///
    /// # Example
    ///
    /// ```ignore
    /// mgr.hover_card()
    ///     .motion_key("my_hover_card")
    ///     .content(move || {
    ///         motion_derived("my_hover_card")
    ///             .enter_animation(AnimationPreset::grow_in(150))
    ///             .exit_animation(AnimationPreset::grow_out(100))
    ///             .child(card_content)
    ///     })
    ///     .show()
    /// ```
    pub fn motion_key(mut self, key: impl Into<String>) -> Self {
        self.config.motion_key = Some(key.into());
        self
    }

    /// Set the anchor direction for correct occlusion testing
    ///
    /// For positioned overlays (via `.at(x, y)`), this specifies which edge
    /// the (x, y) point represents:
    /// - `Top`: overlay appears above trigger, y is the bottom edge
    /// - `Bottom`: overlay appears below trigger, y is the top edge (default)
    /// - `Left`: overlay appears left of trigger, x is the right edge
    /// - `Right`: overlay appears right of trigger, x is the left edge
    ///
    /// This is used to calculate correct bounds for hit test occlusion.
    pub fn anchor_direction(mut self, direction: AnchorDirection) -> Self {
        self.config.anchor_direction = direction;
        self
    }

    /// Show the dropdown
    pub fn show(self) -> OverlayHandle {
        let content = self.content.unwrap_or_else(|| Box::new(|| div()));
        self.manager
            .lock()
            .unwrap()
            .add_with_close_callback(self.config, content, self.on_close)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_state_transitions() {
        use overlay_events::*;

        let mut state = OverlayState::Closed;

        // Closed -> Opening
        state = state.on_event(OPEN).unwrap();
        assert_eq!(state, OverlayState::Opening);

        // Opening -> Open
        state = state.on_event(ANIMATION_COMPLETE).unwrap();
        assert_eq!(state, OverlayState::Open);

        // Open -> Closing
        state = state.on_event(CLOSE).unwrap();
        assert_eq!(state, OverlayState::Closing);

        // Closing -> Closed
        state = state.on_event(ANIMATION_COMPLETE).unwrap();
        assert_eq!(state, OverlayState::Closed);
    }

    #[test]
    fn test_overlay_manager_basic() {
        let mgr = overlay_manager();

        // Add a modal
        let handle = mgr.lock().unwrap().add(OverlayConfig::modal(), || div());

        assert!(mgr.lock().unwrap().has_visible_overlays());

        // Close it
        mgr.close(handle);

        // Should still be visible (Closing state)
        assert!(mgr.lock().unwrap().has_visible_overlays());
    }

    #[test]
    fn test_overlay_escape() {
        let mgr = overlay_manager();

        // Add modal with dismiss_on_escape
        let _handle = {
            let mut m = mgr.lock().unwrap();
            let h = m.add(OverlayConfig::modal(), || div());
            // Manually transition to Open state
            if let Some(o) = m.overlays.get_mut(&h) {
                o.state = OverlayState::Open;
            }
            h
        };

        // Escape should close it
        assert!(mgr.handle_escape());
    }

    #[test]
    fn test_overlay_config_defaults() {
        let modal = OverlayConfig::modal();
        assert!(modal.backdrop.is_some());
        assert!(modal.dismiss_on_escape);
        assert!(modal.focus_trap);

        let toast = OverlayConfig::toast();
        assert!(toast.backdrop.is_none());
        assert!(!toast.dismiss_on_escape);
        assert!(toast.auto_dismiss_ms.is_some());

        let context = OverlayConfig::context_menu();
        assert!(context.backdrop.is_none());
        assert!(context.dismiss_on_escape);
    }
}
