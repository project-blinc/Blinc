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
use crate::motion::motion;
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
    pub fn modal() -> Self {
        Self {
            enter: AnimationPreset::scale_in(200),
            exit: AnimationPreset::fade_out(150),
        }
    }

    /// Context menu animation (pop in)
    pub fn context_menu() -> Self {
        Self {
            enter: AnimationPreset::pop_in(150),
            exit: AnimationPreset::fade_out(100),
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
    pub fn dropdown() -> Self {
        Self {
            enter: AnimationPreset::fade_in(100),
            exit: AnimationPreset::fade_out(75),
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
            position: OverlayPosition::Centered, // Will be overridden by anchor
            backdrop: None,
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

    /// Get the raw ID
    pub fn id(&self) -> u64 {
        self.0
    }
}

// =============================================================================
// ActiveOverlay
// =============================================================================

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
    /// Time when overlay was opened (for auto-dismiss)
    opened_at_ms: Option<u64>,
    /// Cached content size after layout (for positioning)
    pub cached_size: Option<(f32, f32)>,
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
    /// Dirty flag - set when overlays change
    dirty: AtomicBool,
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
}

impl OverlayManagerInner {
    /// Create a new overlay manager
    pub fn new() -> Self {
        Self {
            overlays: IndexMap::new(),
            next_id: AtomicU64::new(1),
            dirty: AtomicBool::new(false),
            viewport: (0.0, 0.0),
            scale_factor: 1.0,
            toast_corner: Corner::TopRight,
            max_toasts: 5,
            toast_gap: 8.0,
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

    /// Check and clear dirty flag
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::SeqCst)
    }

    /// Mark as dirty
    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// Add a new overlay
    pub fn add(
        &mut self,
        config: OverlayConfig,
        content: impl Fn() -> Div + Send + Sync + 'static,
    ) -> OverlayHandle {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let handle = OverlayHandle::new(id);

        tracing::info!(
            "OverlayManager::add - adding {:?} overlay with handle {:?}",
            config.kind,
            handle
        );

        let overlay = ActiveOverlay {
            handle,
            config,
            state: OverlayState::Opening,
            content_builder: Box::new(content),
            opened_at_ms: None,
            cached_size: None,
        };

        self.overlays.insert(handle, overlay);
        self.mark_dirty();

        tracing::info!(
            "OverlayManager::add - now have {} overlays",
            self.overlays.len()
        );

        handle
    }

    /// Update overlay states - call this every frame
    ///
    /// This handles:
    /// - Transitioning Opening -> Open after animation
    /// - Auto-dismissing toasts after their duration expires
    /// - Removing closed overlays
    pub fn update(&mut self, current_time_ms: u64) {
        let mut to_close = Vec::new();
        let mut dirty = false;

        for (handle, overlay) in self.overlays.iter_mut() {
            match overlay.state {
                OverlayState::Opening => {
                    // Transition to Open immediately (skip animation for now)
                    if overlay.transition(overlay_events::ANIMATION_COMPLETE) {
                        overlay.opened_at_ms = Some(current_time_ms);
                        dirty = true;
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
                    // Transition to Closed immediately (skip animation for now)
                    if overlay.transition(overlay_events::ANIMATION_COMPLETE) {
                        dirty = true;
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
                dirty = true;
            }
        }

        // Remove closed overlays
        self.overlays.retain(|_, o| o.state != OverlayState::Closed);

        if dirty {
            self.mark_dirty();
        }
    }

    /// Close an overlay by handle
    pub fn close(&mut self, handle: OverlayHandle) {
        if let Some(overlay) = self.overlays.get_mut(&handle) {
            if overlay.transition(overlay_events::CLOSE) {
                self.mark_dirty();
            }
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
        self.overlays
            .retain(|_, o| o.state != OverlayState::Closed);
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
                if overlay.transition(overlay_events::ESCAPE) {
                    self.mark_dirty();
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
                    && o.config.backdrop.as_ref().map(|b| b.dismiss_on_click).unwrap_or(false)
            })
            .max_by_key(|o| o.config.z_priority)
            .map(|o| o.handle)
        {
            if let Some(overlay) = self.overlays.get_mut(&handle) {
                if overlay.transition(overlay_events::BACKDROP_CLICK) {
                    self.mark_dirty();
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
                    && o.config.backdrop.as_ref().map(|b| b.dismiss_on_click).unwrap_or(false)
            })
            .max_by_key(|o| o.config.z_priority)
        {
            // Get content size (may be cached or estimated)
            let (content_w, content_h) = overlay.cached_size.unwrap_or_else(|| {
                // Fallback: estimate based on overlay config size or default
                overlay.config.size.unwrap_or((400.0, 300.0))
            });

            // For centered overlays, compute content bounds
            let (vp_w, vp_h) = self.viewport;
            let content_x = (vp_w - content_w) / 2.0;
            let content_y = (vp_h - content_h) / 2.0;

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
        tracing::debug!(
            "build_overlay_tree: {} overlays, viewport=({}, {})",
            self.overlays.len(),
            self.viewport.0,
            self.viewport.1
        );

        if !self.has_visible_overlays() {
            tracing::debug!("build_overlay_tree: no visible overlays");
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
                tracing::debug!("build_overlay_tree: adding overlay {:?}", overlay.config.kind);
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
        let content = overlay.build_content();

        // Apply size constraints if specified
        let content = if let Some((w, h)) = overlay.config.size {
            content.w(w).h(h)
        } else {
            content
        };

        // TODO: Re-enable motion animations for enter/exit
        // let animated_content = motion()
        //     .enter_animation(overlay.config.animation.enter.clone())
        //     .exit_animation(overlay.config.animation.exit.clone())
        //     .child(content);

        // Build the layer with optional backdrop
        if let Some(ref backdrop_config) = overlay.config.backdrop {
            tracing::debug!(
                "build_overlay_layer: using backdrop color {:?}",
                backdrop_config.color
            );
            // Use stack: first child (backdrop) renders behind, second child (content) on top
            div()
                .w(vp_width)
                .h(vp_height)
                .child(
                    stack()
                        .w_full()
                        .h_full()
                        // Backdrop layer (behind)
                        .child(
                            div()
                                .w_full()
                                .h_full()
                                .bg(backdrop_config.color),
                        )
                        // Content layer (on top) - centered
                        .child(
                            div()
                                .w_full()
                                .h_full()
                                .items_center()
                                .justify_center()
                                .child(content),
                        ),
                )
        } else {
            // No backdrop - position content according to config
            div()
                .w(vp_width)
                .h(vp_height)
                .items_center()
                .justify_center()
                .child(content)
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
                // Absolute positioning at point
                div()
                    .w(vp_width)
                    .h(vp_height)
                    .child(content.ml(*x).mt(*y))
            }

            OverlayPosition::Corner(corner) => {
                // Position in corner with margin
                let margin = 16.0;
                self.position_in_corner(content, *corner, vp_width, vp_height, margin)
            }

            OverlayPosition::RelativeToAnchor {
                offset_x,
                offset_y,
                ..
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
            Corner::TopLeft => container.items_start().justify_start().child(content.m(margin)),
            Corner::TopRight => container.items_end().justify_start().child(content.m(margin)),
            Corner::BottomLeft => container.items_start().justify_end().child(content.m(margin)),
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
                Corner::BottomRight => {
                    (vp_width - margin - 300.0, vp_height - y_offset - estimated_height)
                }
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
    /// Update overlay states - call every frame for animations and auto-dismiss
    fn update(&self, current_time_ms: u64);
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
        self.lock().unwrap().set_viewport_with_scale(width, height, scale_factor);
    }

    fn build_overlay_tree(&self) -> Option<RenderTree> {
        self.lock().unwrap().build_overlay_tree()
    }

    fn has_blocking_overlay(&self) -> bool {
        self.lock().unwrap().has_blocking_overlay()
    }

    fn update(&self, current_time_ms: u64) {
        self.lock().unwrap().update(current_time_ms);
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
}

impl DropdownBuilder {
    fn new(manager: OverlayManager) -> Self {
        Self {
            manager,
            config: OverlayConfig::dropdown(),
            content: None,
        }
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
            offset_x,
            offset_y,
            ..
        } = &mut self.config.position
        {
            *offset_x = x;
            *offset_y = y;
        }
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

    /// Show the dropdown
    pub fn show(self) -> OverlayHandle {
        let content = self.content.unwrap_or_else(|| Box::new(|| div()));
        self.manager.lock().unwrap().add(self.config, content)
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
