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
        matches!(self, OverlayState::Open)
    }

    /// Check if overlay is animating
    pub fn is_animating(&self) -> bool {
        matches!(self, OverlayState::Opening | OverlayState::Closing)
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

            // Open -> Closing: Start hide animation
            (Open, CLOSE) | (Open, ESCAPE) | (Open, BACKDROP_CLICK) => Some(Closing),

            // Closing -> Closed: Animation finished, remove overlay
            (Closing, ANIMATION_COMPLETE) => Some(Closed),

            // Interrupt opening with close
            (Opening, CLOSE) | (Opening, ESCAPE) => Some(Closing),

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
    /// Auto-dismiss after duration (for toasts)
    pub auto_dismiss_ms: Option<u32>,
    /// Trap focus within overlay (for modals)
    pub focus_trap: bool,
    /// Z-priority (higher = more on top)
    pub z_priority: i32,
    /// Explicit size (None = content-sized)
    pub size: Option<(f32, f32)>,
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
            auto_dismiss_ms: None,
            focus_trap: true,
            z_priority: 100,
            size: None,
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
            auto_dismiss_ms: None,
            focus_trap: false,
            z_priority: 200,
            size: None,
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
            auto_dismiss_ms: Some(3000),
            focus_trap: false,
            z_priority: 300,
            size: None,
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
            auto_dismiss_ms: None,
            focus_trap: false,
            z_priority: 150,
            size: None,
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
    /// Cached content size after layout (for positioning)
    pub cached_size: Option<(f32, f32)>,
    /// Callback invoked when the overlay is closed (backdrop click, escape, etc.)
    on_close: Option<OnCloseCallback>,
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
    pub fn transition(&mut self, event: u32) -> bool {
        if let Some(new_state) = self.state.on_event(event) {
            self.state = new_state;
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
            cached_size: None,
            on_close,
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
                    let exit_duration = overlay.config.animation.exit.duration_ms();
                    if let Some(close_started) = overlay.close_started_at_ms {
                        let elapsed = current_time_ms.saturating_sub(close_started);
                        if elapsed >= exit_duration as u64 {
                            // Animation complete, transition to Closed
                            tracing::debug!(
                                "Overlay {:?} exit animation complete, elapsed={}ms",
                                handle,
                                elapsed
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

    /// Set the cached content size for an overlay (for hit testing)
    ///
    /// This is typically called from an `on_ready` callback after the overlay
    /// content has been laid out, providing accurate size for backdrop click detection.
    pub fn set_content_size(&mut self, handle: OverlayHandle, width: f32, height: f32) {
        if let Some(overlay) = self.overlays.get_mut(&handle) {
            overlay.cached_size = Some((width, height));
        }
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

    /// Get the number of overlays
    pub fn overlay_count(&self) -> usize {
        self.overlays.len()
    }

    /// Build the overlay render tree
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
                root = root.child(self.build_overlay_layer(overlay, width, height));
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

    /// Build a single overlay layer
    fn build_overlay_layer(&self, overlay: &ActiveOverlay, vp_width: f32, vp_height: f32) -> Div {
        // Set the closing flag if this overlay is closing, so motion() containers
        // know to start their exit animations instead of reinitializing
        let is_closing = overlay.state == OverlayState::Closing;
        crate::overlay_state::set_overlay_closing(is_closing);

        // Content is built by the user - they should wrap it in motion() for animations
        let content = overlay.build_content();

        // Reset the closing flag after building content
        crate::overlay_state::set_overlay_closing(false);

        // Apply size constraints if specified
        let content = if let Some((w, h)) = overlay.config.size {
            content.w(w).h(h)
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
                OverlayState::Open => 1.0,
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

            // Use stack: first child (backdrop) renders behind, second child (content) on top
            div().w(vp_width).h(vp_height).child(
                stack()
                    .w_full()
                    .h_full()
                    // Backdrop layer (behind) - fills entire viewport with animated opacity
                    .child(div().w_full().h_full().bg(backdrop_color))
                    // Content layer (on top) - positioned according to config
                    // Content animation is handled by user via motion() container
                    .child(self.position_content(overlay, content, vp_width, vp_height)),
            )
        } else {
            // No backdrop - position content according to config
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
                // Wrap in a full viewport container with top-left alignment so margins work correctly
                div()
                    .w(vp_width)
                    .h(vp_height)
                    .items_start()
                    .justify_start()
                    .child(content.absolute().left(*x).top(*y))
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

    /// Close an overlay by handle
    fn close(&self, handle: OverlayHandle);
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
    /// Build overlay render tree
    fn build_overlay_tree(&self) -> Option<RenderTree>;
    /// Check if any blocking overlay is active
    fn has_blocking_overlay(&self) -> bool;
    /// Check if any dismissable overlay is visible (dropdown, context menu, etc.)
    fn has_dismissable_overlay(&self) -> bool;
    /// Check if any overlay is visible
    fn has_visible_overlays(&self) -> bool;
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

    fn close(&self, handle: OverlayHandle) {
        self.lock().unwrap().close(handle);
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

    fn has_blocking_overlay(&self) -> bool {
        self.lock().unwrap().has_blocking_overlay()
    }

    fn has_dismissable_overlay(&self) -> bool {
        self.lock().unwrap().has_dismissable_overlay()
    }

    fn has_visible_overlays(&self) -> bool {
        self.lock().unwrap().has_visible_overlays()
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
