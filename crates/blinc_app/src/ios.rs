//! iOS application runner
//!
//! Provides a unified API for running Blinc applications on iOS.
//!
//! # Example
//!
//! ```ignore
//! use blinc_app::prelude::*;
//! use blinc_app::ios::IOSApp;
//!
//! // Called from your Swift/Objective-C app delegate
//! IOSApp::run_with_metal_layer(metal_layer, width, height, scale, |ctx| {
//!     div().w(ctx.width).h(ctx.height)
//!         .bg([0.1, 0.1, 0.15, 1.0])
//!         .flex_center()
//!         .child(text("Hello iOS!").size(48.0))
//! }).unwrap();
//! ```
//!
//! # iOS Integration
//!
//! Unlike Android where Blinc can run as a native activity, on iOS you must
//! integrate Blinc into your existing UIKit application. The typical flow is:
//!
//! 1. Create a `UIView` subclass with `CAMetalLayer` as its layer class
//! 2. Set up a `CADisplayLink` for frame callbacks
//! 3. Call `IOSApp::render_frame()` on each display link callback
//! 4. Forward touch events to `IOSApp::handle_touch()`

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use blinc_animation::AnimationScheduler;
use blinc_core::context_state::{BlincContextState, HookState, SharedHookState};
use blinc_core::reactive::{ReactiveGraph, SignalId};
use blinc_layout::event_router::MouseButton;
use blinc_layout::overlay_state::OverlayContext;
use blinc_layout::prelude::*;
use blinc_layout::widgets::overlay::{overlay_manager, OverlayManager};
use blinc_platform::assets::set_global_asset_loader;
use blinc_platform_ios::{IOSAssetLoader, IOSWakeProxy, TouchPhase};

use crate::app::BlincApp;
use crate::error::{BlincError, Result};
use crate::windowed::{
    RefDirtyFlag, SharedAnimationScheduler, SharedElementRegistry, SharedReactiveGraph,
    SharedReadyCallbacks, WindowedContext,
};

/// iOS application runner
///
/// Provides methods for running a Blinc application on iOS with Metal rendering.
/// Unlike desktop or Android, iOS apps must integrate Blinc into their existing
/// UIKit application lifecycle.
pub struct IOSApp;

impl IOSApp {
    /// Initialize the iOS asset loader
    fn init_asset_loader() {
        let loader = IOSAssetLoader::new();
        let _ = set_global_asset_loader(Box::new(loader));
    }

    /// Initialize the theme system
    fn init_theme() {
        use blinc_theme::{
            detect_system_color_scheme, platform_theme_bundle, set_redraw_callback, ThemeState,
        };

        // Only initialize if not already initialized
        if ThemeState::try_get().is_none() {
            let bundle = platform_theme_bundle();
            let scheme = detect_system_color_scheme();
            ThemeState::init(bundle, scheme);
        }

        // Set up the redraw callback
        set_redraw_callback(|| {
            tracing::debug!("Theme changed - requesting full rebuild");
            blinc_layout::widgets::request_full_rebuild();
        });
    }

    /// Create a new Blinc context for iOS rendering
    ///
    /// This sets up all the shared state needed for Blinc rendering.
    /// Call this once when your app starts, then use the returned
    /// `IOSRenderContext` for rendering frames.
    ///
    /// # Arguments
    ///
    /// * `width` - Physical width in pixels
    /// * `height` - Physical height in pixels
    /// * `scale_factor` - Display scale factor (UIScreen.scale)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let render_ctx = IOSApp::create_context(
    ///     screen_width,
    ///     screen_height,
    ///     UIScreen.mainScreen.scale,
    /// )?;
    /// ```
    pub fn create_context(width: u32, height: u32, scale_factor: f64) -> Result<IOSRenderContext> {
        tracing::info!(
            "IOSApp::create_context: {}x{} physical pixels, scale_factor={}",
            width, height, scale_factor
        );

        let logical_width = width as f32 / scale_factor as f32;
        let logical_height = height as f32 / scale_factor as f32;
        tracing::info!(
            "IOSApp::create_context: {:.1}x{:.1} logical points",
            logical_width, logical_height
        );

        // Initialize the asset loader
        Self::init_asset_loader();

        // Initialize the text measurer
        crate::text_measurer::init_text_measurer();

        // Initialize the theme system
        Self::init_theme();

        // Shared state
        let ref_dirty_flag: RefDirtyFlag = Arc::new(AtomicBool::new(false));
        let reactive: SharedReactiveGraph = Arc::new(Mutex::new(ReactiveGraph::new()));
        let hooks: SharedHookState = Arc::new(Mutex::new(HookState::new()));

        // Initialize global context state singleton
        if !BlincContextState::is_initialized() {
            let stateful_callback: Arc<dyn Fn(&[SignalId]) + Send + Sync> =
                Arc::new(|signal_ids| {
                    blinc_layout::check_stateful_deps(signal_ids);
                });
            BlincContextState::init_with_callback(
                Arc::clone(&reactive),
                Arc::clone(&hooks),
                Arc::clone(&ref_dirty_flag),
                stateful_callback,
            );
        }

        // Animation scheduler with wake proxy
        let mut scheduler = AnimationScheduler::new();

        // Set up wake proxy for iOS
        let wake_proxy = IOSWakeProxy::new();
        let wake_proxy_clone = wake_proxy.clone();
        scheduler.set_wake_callback(move || wake_proxy_clone.wake());

        scheduler.start_background();
        let animations: SharedAnimationScheduler = Arc::new(Mutex::new(scheduler));

        // Set global scheduler handle
        {
            let scheduler_handle = animations.lock().unwrap().handle();
            blinc_animation::set_global_scheduler(scheduler_handle);
        }

        // Element registry for query API
        let element_registry: SharedElementRegistry =
            Arc::new(blinc_layout::selector::ElementRegistry::new());

        // Set up query callback
        {
            let registry_for_query = Arc::clone(&element_registry);
            let query_callback: blinc_core::QueryCallback = Arc::new(move |id: &str| {
                registry_for_query.get(id).map(|node_id| node_id.to_raw())
            });
            BlincContextState::get().set_query_callback(query_callback);
        }

        // Set up bounds callback
        {
            let registry_for_bounds = Arc::clone(&element_registry);
            let bounds_callback: blinc_core::BoundsCallback =
                Arc::new(move |id: &str| registry_for_bounds.get_bounds(id));
            BlincContextState::get().set_bounds_callback(bounds_callback);
        }

        // Store element registry in BlincContextState
        BlincContextState::get()
            .set_element_registry(Arc::clone(&element_registry) as blinc_core::AnyElementRegistry);

        // Ready callbacks
        let ready_callbacks: SharedReadyCallbacks = Arc::new(Mutex::new(Vec::new()));

        // Overlay manager
        let overlays: OverlayManager = overlay_manager();
        if !OverlayContext::is_initialized() {
            OverlayContext::init(Arc::clone(&overlays));
        }

        // Connect theme animation to scheduler
        blinc_theme::ThemeState::get().set_scheduler(&animations);

        // Render state and motion states
        let shared_motion_states = blinc_layout::create_shared_motion_states();

        // Set up motion state callback
        {
            let motion_states_for_callback = Arc::clone(&shared_motion_states);
            let motion_callback: blinc_core::MotionStateCallback = Arc::new(move |key: &str| {
                motion_states_for_callback
                    .read()
                    .ok()
                    .and_then(|states| states.get(key).copied())
                    .unwrap_or(blinc_core::MotionAnimationState::NotFound)
            });
            BlincContextState::get().set_motion_state_callback(motion_callback);
        }

        // Calculate logical dimensions
        let logical_width = width as f32 / scale_factor as f32;
        let logical_height = height as f32 / scale_factor as f32;

        // Set viewport size
        BlincContextState::get().set_viewport_size(logical_width, logical_height);

        // Create windowed context
        let windowed_ctx = WindowedContext::new_ios(
            logical_width,
            logical_height,
            scale_factor,
            width as f32,
            height as f32,
            true, // focused
            Arc::clone(&animations),
            Arc::clone(&ref_dirty_flag),
            Arc::clone(&reactive),
            Arc::clone(&hooks),
            Arc::clone(&overlays),
            Arc::clone(&element_registry),
            Arc::clone(&ready_callbacks),
        );

        // Initialize render state
        let mut render_state = blinc_layout::RenderState::new(Arc::clone(&animations));
        render_state.set_shared_motion_states(Arc::clone(&shared_motion_states));

        Ok(IOSRenderContext {
            windowed_ctx,
            render_state,
            render_tree: None,
            ref_dirty_flag,
            animations,
            ready_callbacks,
            wake_proxy,
            rebuild_count: 0,
        })
    }

    /// iOS system font paths
    pub fn system_font_paths() -> &'static [&'static str] {
        blinc_platform_ios::system_font_paths()
    }
}

/// iOS render context
///
/// Holds all the state needed to render Blinc UI on iOS.
/// Create this once and reuse it for each frame.
pub struct IOSRenderContext {
    /// Windowed context for UI building
    pub windowed_ctx: WindowedContext,
    /// Render state for animations
    render_state: blinc_layout::RenderState,
    /// Render tree (created on first render)
    render_tree: Option<RenderTree>,
    /// Dirty flag for reactive updates
    ref_dirty_flag: RefDirtyFlag,
    /// Animation scheduler
    animations: SharedAnimationScheduler,
    /// Ready callbacks
    ready_callbacks: SharedReadyCallbacks,
    /// Wake proxy for animation thread
    wake_proxy: IOSWakeProxy,
    /// Number of rebuilds
    rebuild_count: u64,
}

impl IOSRenderContext {
    /// Check if a frame needs to be rendered
    ///
    /// Returns true if:
    /// - Reactive state changed (dirty flag)
    /// - Stateful elements need redraw (ButtonState changes, etc.)
    /// - Animations are active
    /// - Wake was requested by animation thread
    pub fn needs_render(&self) -> bool {
        let dirty = self.ref_dirty_flag.load(Ordering::SeqCst);
        let wake_requested = self.wake_proxy.take_wake_request();
        let animations_active = self
            .animations
            .lock()
            .map(|sched| sched.has_active_animations())
            .unwrap_or(false);

        // Check if stateful elements need incremental updates (visual state changes)
        let has_stateful_updates = blinc_layout::peek_needs_redraw();
        let has_pending_rebuilds = blinc_layout::has_pending_subtree_rebuilds();

        dirty || wake_requested || animations_active || has_stateful_updates || has_pending_rebuilds
    }
    /// Update the window size
    ///
    /// Call this when the view's bounds change.
    /// `width` and `height` are in physical pixels.
    pub fn update_size(&mut self, width: u32, height: u32, scale_factor: f64) {
        let physical_width = width as f32;
        let physical_height = height as f32;
        let logical_width = physical_width / scale_factor as f32;
        let logical_height = physical_height / scale_factor as f32;

        // Only mark dirty if something actually changed
        let changed = (self.windowed_ctx.width - logical_width).abs() > 0.1
            || (self.windowed_ctx.height - logical_height).abs() > 0.1
            || (self.windowed_ctx.scale_factor - scale_factor).abs() > 0.001;

        self.windowed_ctx.width = logical_width;
        self.windowed_ctx.height = logical_height;
        self.windowed_ctx.physical_width = physical_width;
        self.windowed_ctx.physical_height = physical_height;
        self.windowed_ctx.scale_factor = scale_factor;

        BlincContextState::get().set_viewport_size(logical_width, logical_height);

        // Mark dirty to trigger rebuild with new dimensions
        if changed {
            tracing::debug!(
                "iOS update_size: {:.1}x{:.1} logical ({:.0}x{:.0} physical) @ {:.1}x scale",
                logical_width, logical_height, physical_width, physical_height, scale_factor
            );
            self.ref_dirty_flag.store(true, Ordering::SeqCst);
        }
    }

    /// Build and layout the UI tree
    ///
    /// Call this before rendering each frame.
    pub fn build_ui<F, E>(&mut self, ui_builder: F)
    where
        F: FnOnce(&mut WindowedContext) -> E,
        E: ElementBuilder,
    {
        // Clear dirty flag
        self.ref_dirty_flag.swap(false, Ordering::SeqCst);

        // Tick animations
        if let Ok(mut sched) = self.animations.lock() {
            sched.tick();
        }

        // Build UI
        let element = ui_builder(&mut self.windowed_ctx);

        // Create or update render tree
        if self.render_tree.is_none() {
            // First time: create tree
            tracing::debug!(
                "iOS build_ui: Creating tree with scale_factor={}, layout={:.1}x{:.1}",
                self.windowed_ctx.scale_factor,
                self.windowed_ctx.width,
                self.windowed_ctx.height
            );
            let mut tree = RenderTree::from_element(&element);
            tree.set_scale_factor(self.windowed_ctx.scale_factor as f32);
            tree.compute_layout(self.windowed_ctx.width, self.windowed_ctx.height);
            self.render_tree = Some(tree);
        } else if let Some(ref mut tree) = self.render_tree {
            // Full rebuild
            tree.clear_dirty();
            *tree = RenderTree::from_element(&element);
            tree.set_scale_factor(self.windowed_ctx.scale_factor as f32);
            tree.compute_layout(self.windowed_ctx.width, self.windowed_ctx.height);
        }

        // Increment rebuild count
        self.rebuild_count += 1;

        // Execute ready callbacks after first rebuild
        if self.rebuild_count == 1 {
            if let Ok(mut callbacks) = self.ready_callbacks.lock() {
                for callback in callbacks.drain(..) {
                    callback();
                }
            }
        }
    }

    /// Get the render tree for rendering
    ///
    /// Returns None if build_ui hasn't been called yet.
    pub fn render_tree(&self) -> Option<&RenderTree> {
        self.render_tree.as_ref()
    }

    /// Get the render state for motion animations
    pub fn render_state(&self) -> &blinc_layout::RenderState {
        &self.render_state
    }

    /// Handle a touch event
    ///
    /// Call this from your UIView's touch handling methods.
    /// Touch coordinates should be in logical points (not physical pixels).
    ///
    /// # Example (Swift)
    ///
    /// ```swift
    /// override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
    ///     for touch in touches {
    ///         let point = touch.location(in: self)
    ///         blinc_handle_touch(context, 0, Float(point.x), Float(point.y), 0) // 0 = began
    ///     }
    /// }
    /// ```
    pub fn handle_touch(&mut self, touch: blinc_platform_ios::Touch) {
        use blinc_layout::tree::LayoutNodeId;

        // Pending event structure for deferred dispatch
        #[derive(Clone, Default)]
        struct PendingEvent {
            node_id: LayoutNodeId,
            event_type: u32,
        }

        let tree = match &self.render_tree {
            Some(t) => t,
            None => {
                eprintln!("[Blinc] iOS handle_touch: No render tree yet, ignoring touch");
                return;
            }
        };

        // Touch coordinates are already in logical points on iOS
        let lx = touch.x;
        let ly = touch.y;

        // Log tree info for debugging
        if let Some(root) = tree.root() {
            if let Some(bounds) = tree.layout().get_bounds(root, (0.0, 0.0)) {
                eprintln!(
                    "[Blinc] iOS Touch at ({:.1}, {:.1}) - tree root bounds: ({:.1}, {:.1}, {:.1}x{:.1})",
                    lx, ly, bounds.x, bounds.y, bounds.width, bounds.height
                );
            } else {
                eprintln!("[Blinc] iOS Touch: tree root has no bounds!");
            }
        } else {
            eprintln!("[Blinc] iOS Touch: tree has no root!");
        }

        // Collect pending events via callback
        let mut pending_events: Vec<PendingEvent> = Vec::new();

        // Set up callback to collect events
        self.windowed_ctx.event_router.set_event_callback({
            let events = &mut pending_events as *mut Vec<PendingEvent>;
            move |node, event_type| {
                // SAFETY: This callback is only used within this scope
                unsafe {
                    (*events).push(PendingEvent { node_id: node, event_type });
                }
            }
        });

        // Route touch event through event router
        match touch.phase {
            TouchPhase::Began => {
                eprintln!("[Blinc] iOS Touch BEGAN at ({:.1}, {:.1})", lx, ly);
                self.windowed_ctx
                    .event_router
                    .on_mouse_down(tree, lx, ly, MouseButton::Left);
            }
            TouchPhase::Moved => {
                self.windowed_ctx.event_router.on_mouse_move(tree, lx, ly);
            }
            TouchPhase::Ended => {
                eprintln!("[Blinc] iOS Touch ENDED at ({:.1}, {:.1})", lx, ly);
                self.windowed_ctx
                    .event_router
                    .on_mouse_up(tree, lx, ly, MouseButton::Left);
                // On touch devices, finger lift means pointer leaves too
                // This transitions ButtonState from Hovered back to Idle
                self.windowed_ctx.event_router.on_mouse_leave();
            }
            TouchPhase::Cancelled => {
                eprintln!("[Blinc] iOS Touch CANCELLED");
                self.windowed_ctx.event_router.on_mouse_leave();
            }
        }

        // Clear callback
        self.windowed_ctx.event_router.clear_event_callback();

        eprintln!("[Blinc] iOS Touch: collected {} pending events", pending_events.len());

        // Dispatch collected events to the tree
        if !pending_events.is_empty() {
            eprintln!("[Blinc] iOS dispatching {} events", pending_events.len());

            if let Some(ref mut tree) = self.render_tree {
                let router = &self.windowed_ctx.event_router;
                for event in pending_events {
                    // Get bounds for local coordinate calculation
                    let (bounds_x, bounds_y, bounds_width, bounds_height) =
                        router.get_node_bounds(event.node_id).unwrap_or((0.0, 0.0, 0.0, 0.0));
                    let local_x = lx - bounds_x;
                    let local_y = ly - bounds_y;

                    tree.dispatch_event_full(
                        event.node_id,
                        event.event_type,
                        lx,
                        ly,
                        local_x,
                        local_y,
                        bounds_x,
                        bounds_y,
                        bounds_width,
                        bounds_height,
                        0.0, // drag_delta_x
                        0.0, // drag_delta_y
                    );
                }
            }
            // Stateful elements will call request_redraw() internally when state changes
            // The needs_render() check will pick this up for the next frame
        }
    }

    /// Set focus state
    pub fn set_focused(&mut self, focused: bool) {
        self.windowed_ctx.focused = focused;
    }
}

// =============================================================================
// Rust UI Builder Registration
// =============================================================================

use std::sync::OnceLock;

/// Type for Rust UI builder function that directly creates/updates the render tree
type RustUIBuilder = Box<dyn Fn(&mut WindowedContext, Option<&mut RenderTree>) -> RenderTree + Send + Sync>;

/// Global storage for Rust UI builder
static RUST_UI_BUILDER: OnceLock<RustUIBuilder> = OnceLock::new();

/// Register a Rust UI builder function
///
/// This is called from the example app's iOS entry point to register
/// the UI builder closure. The closure should return an ElementBuilder,
/// which will be converted to a RenderTree.
///
/// # Example
///
/// ```ignore
/// #[cfg(target_os = "ios")]
/// #[no_mangle]
/// pub extern "C" fn ios_app_init() {
///     blinc_app::ios::register_rust_ui_builder(|ctx| {
///         my_app_ui(ctx)
///     });
/// }
/// ```
pub fn register_rust_ui_builder<F, E>(builder: F)
where
    F: Fn(&mut WindowedContext) -> E + Send + Sync + 'static,
    E: ElementBuilder + 'static,
{
    let boxed_builder: RustUIBuilder = Box::new(move |ctx, _existing_tree| {
        let element = builder(ctx);
        let mut tree = RenderTree::from_element(&element);
        tree.set_scale_factor(ctx.scale_factor as f32);
        tree.compute_layout(ctx.width, ctx.height);
        tree
    });
    let _ = RUST_UI_BUILDER.set(boxed_builder);
}

/// Get the registered Rust UI builder
fn get_rust_ui_builder() -> Option<&'static RustUIBuilder> {
    RUST_UI_BUILDER.get()
}

// =============================================================================
// C FFI for Swift/Objective-C Integration
// =============================================================================

/// Type alias for UI builder function pointer
///
/// The function receives the WindowedContext pointer and should build/update the UI.
/// It's called each frame when rendering is needed.
///
/// Example Rust implementation:
/// ```ignore
/// #[no_mangle]
/// pub extern "C" fn my_app_build_ui(ctx: *mut WindowedContext) {
///     if ctx.is_null() { return; }
///     let ctx = unsafe { &mut *ctx };
///     // Use ctx.width, ctx.height, etc. to build UI
/// }
/// ```
pub type UIBuilderFn = extern "C" fn(ctx: *mut WindowedContext);

/// Stored UI builder for FFI
static mut UI_BUILDER: Option<UIBuilderFn> = None;

/// Register a UI builder function (C FFI for Swift/Rust interop)
///
/// The builder function will be called each frame to build the UI.
/// Call this once during initialization before any rendering.
///
/// # Safety
/// The function pointer must remain valid for the lifetime of the application.
#[no_mangle]
pub extern "C" fn blinc_set_ui_builder(builder: UIBuilderFn) {
    unsafe {
        UI_BUILDER = Some(builder);
    }
}

/// Get the registered UI builder (internal use)
fn get_ui_builder() -> Option<UIBuilderFn> {
    unsafe { UI_BUILDER }
}

/// Build a frame using the registered UI builder (C FFI for Swift)
///
/// This handles both incremental updates (prop changes, subtree rebuilds) and
/// full rebuilds. Call this each frame when blinc_needs_render() is true.
///
/// The function:
/// 1. First processes incremental updates (prop changes from stateful elements)
/// 2. Only does a full rebuild if the dirty flag is set (State::set_rebuild)
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
/// A UI builder must have been registered via `blinc_set_ui_builder` or `register_rust_ui_builder`.
#[no_mangle]
pub extern "C" fn blinc_build_frame(ctx: *mut IOSRenderContext) {
    if ctx.is_null() {
        return;
    }

    unsafe {
        let ctx = &mut *ctx;

        // Tick animations
        if let Ok(sched) = ctx.animations.lock() {
            sched.tick();
        }

        // PHASE 1: Process incremental updates (prop changes, subtree rebuilds)
        // This avoids full rebuild for simple state changes like ButtonState
        let has_stateful_updates = blinc_layout::take_needs_redraw();
        let has_pending_rebuilds = blinc_layout::has_pending_subtree_rebuilds();

        if has_stateful_updates || has_pending_rebuilds {
            // Get all pending prop updates
            let prop_updates = blinc_layout::take_pending_prop_updates();

            // Apply prop updates to the tree
            if let Some(ref mut tree) = ctx.render_tree {
                for (node_id, props) in &prop_updates {
                    tree.update_render_props(*node_id, |p| *p = props.clone());
                }
            }

            // Process subtree rebuilds
            let mut needs_layout = false;
            if let Some(ref mut tree) = ctx.render_tree {
                needs_layout = tree.process_pending_subtree_rebuilds();
            }

            if needs_layout {
                if let Some(ref mut tree) = ctx.render_tree {
                    tree.compute_layout(ctx.windowed_ctx.width, ctx.windowed_ctx.height);
                }
            }
        }

        // PHASE 2: Check if full rebuild is needed
        let needs_rebuild = ctx.ref_dirty_flag.swap(false, Ordering::SeqCst);
        let no_tree_yet = ctx.render_tree.is_none();

        if !needs_rebuild && !no_tree_yet {
            // No full rebuild needed - incremental updates already applied
            return;
        }

        // PHASE 3: Full rebuild using UI builder (required on first load or when dirty)
        if let Some(rust_builder) = get_rust_ui_builder() {
            // The builder creates the RenderTree for us
            let tree = rust_builder(&mut ctx.windowed_ctx, ctx.render_tree.as_mut());
            ctx.render_tree = Some(tree);
        } else if let Some(builder) = get_ui_builder() {
            builder(&mut ctx.windowed_ctx as *mut WindowedContext);
        }
    }
}

/// Create an iOS render context (C FFI for Swift)
///
/// # Arguments
/// * `width` - Physical width in pixels
/// * `height` - Physical height in pixels
/// * `scale_factor` - Display scale factor (UIScreen.scale)
///
/// # Returns
/// Pointer to the render context, or null on failure
///
/// # Safety
/// The returned pointer must be freed with `blinc_destroy_context`.
#[no_mangle]
pub extern "C" fn blinc_create_context(
    width: u32,
    height: u32,
    scale_factor: f64,
) -> *mut IOSRenderContext {
    match IOSApp::create_context(width, height, scale_factor) {
        Ok(ctx) => Box::into_raw(Box::new(ctx)),
        Err(e) => {
            tracing::error!("Failed to create iOS render context: {}", e);
            std::ptr::null_mut()
        }
    }
}

/// Check if a frame needs to be rendered (C FFI for Swift)
///
/// Returns true if reactive state changed, animations are active,
/// or a wake was requested.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[no_mangle]
pub extern "C" fn blinc_needs_render(ctx: *mut IOSRenderContext) -> bool {
    if ctx.is_null() {
        return false;
    }
    unsafe { (*ctx).needs_render() }
}

/// Update the window size (C FFI for Swift)
///
/// Call this when the view's bounds change.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[no_mangle]
pub extern "C" fn blinc_update_size(
    ctx: *mut IOSRenderContext,
    width: u32,
    height: u32,
    scale_factor: f64,
) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        (*ctx).update_size(width, height, scale_factor);
    }
}

/// Handle a touch event (C FFI for Swift)
///
/// # Arguments
/// * `ctx` - Render context pointer
/// * `touch_id` - Unique touch identifier (from UITouch)
/// * `x` - X position in logical points
/// * `y` - Y position in logical points
/// * `phase` - Touch phase: 0=began, 1=moved, 2=ended, 3=cancelled
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[no_mangle]
pub extern "C" fn blinc_handle_touch(
    ctx: *mut IOSRenderContext,
    touch_id: u64,
    x: f32,
    y: f32,
    phase: i32,
) {
    eprintln!("[Blinc FFI] blinc_handle_touch called: x={}, y={}, phase={}", x, y, phase);

    if ctx.is_null() {
        eprintln!("[Blinc FFI] blinc_handle_touch: ctx is NULL!");
        return;
    }

    let touch_phase = match phase {
        0 => TouchPhase::Began,
        1 => TouchPhase::Moved,
        2 => TouchPhase::Ended,
        _ => TouchPhase::Cancelled,
    };

    let touch = blinc_platform_ios::Touch::new(touch_id, x, y, touch_phase);
    unsafe {
        (*ctx).handle_touch(touch);
    }
    eprintln!("[Blinc FFI] blinc_handle_touch completed");
}

/// Set the focus state (C FFI for Swift)
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[no_mangle]
pub extern "C" fn blinc_set_focused(ctx: *mut IOSRenderContext, focused: bool) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        (*ctx).set_focused(focused);
    }
}

/// Destroy the render context (C FFI for Swift)
///
/// Frees all resources associated with the context.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`,
/// and must not be used after this call.
#[no_mangle]
pub extern "C" fn blinc_destroy_context(ctx: *mut IOSRenderContext) {
    if !ctx.is_null() {
        unsafe {
            drop(Box::from_raw(ctx));
        }
    }
}

/// Tick animations (C FFI for Swift)
///
/// Call this each frame before building UI. Returns true if any animations
/// are active (meaning you should continue rendering).
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[no_mangle]
pub extern "C" fn blinc_tick_animations(ctx: *mut IOSRenderContext) -> bool {
    if ctx.is_null() {
        return false;
    }
    unsafe {
        let ctx = &mut *ctx;
        if let Ok(mut sched) = ctx.animations.lock() {
            sched.tick()
        } else {
            false
        }
    }
}

/// Get the logical width for UI layout (C FFI for Swift)
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[no_mangle]
pub extern "C" fn blinc_get_width(ctx: *mut IOSRenderContext) -> f32 {
    if ctx.is_null() {
        return 0.0;
    }
    unsafe { (*ctx).windowed_ctx.width }
}

/// Get the logical height for UI layout (C FFI for Swift)
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[no_mangle]
pub extern "C" fn blinc_get_height(ctx: *mut IOSRenderContext) -> f32 {
    if ctx.is_null() {
        return 0.0;
    }
    unsafe { (*ctx).windowed_ctx.height }
}

/// Get the scale factor (C FFI for Swift)
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[no_mangle]
pub extern "C" fn blinc_get_scale_factor(ctx: *mut IOSRenderContext) -> f64 {
    if ctx.is_null() {
        return 1.0;
    }
    unsafe { (*ctx).windowed_ctx.scale_factor }
}

/// Get a pointer to the WindowedContext for UI building (C FFI for Swift)
///
/// Use this to pass to a Rust UI builder function.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
/// The returned pointer is only valid while `ctx` is valid.
#[no_mangle]
pub extern "C" fn blinc_get_windowed_context(ctx: *mut IOSRenderContext) -> *mut WindowedContext {
    if ctx.is_null() {
        return std::ptr::null_mut();
    }
    unsafe { &mut (*ctx).windowed_ctx as *mut WindowedContext }
}

/// Mark the context as needing a rebuild (C FFI for Swift)
///
/// Call this when external state changes that should trigger a UI update.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[no_mangle]
pub extern "C" fn blinc_mark_dirty(ctx: *mut IOSRenderContext) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        (*ctx).ref_dirty_flag.store(true, Ordering::SeqCst);
    }
}

/// Clear the dirty flag (C FFI for Swift)
///
/// Call this after processing a rebuild.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[no_mangle]
pub extern "C" fn blinc_clear_dirty(ctx: *mut IOSRenderContext) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        (*ctx).ref_dirty_flag.store(false, Ordering::SeqCst);
    }
}

/// Get the physical width in pixels (C FFI for Swift)
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[no_mangle]
pub extern "C" fn blinc_get_physical_width(ctx: *mut IOSRenderContext) -> u32 {
    if ctx.is_null() {
        return 0;
    }
    unsafe {
        let ctx = &*ctx;
        (ctx.windowed_ctx.width * ctx.windowed_ctx.scale_factor as f32) as u32
    }
}

/// Get the physical height in pixels (C FFI for Swift)
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[no_mangle]
pub extern "C" fn blinc_get_physical_height(ctx: *mut IOSRenderContext) -> u32 {
    if ctx.is_null() {
        return 0;
    }
    unsafe {
        let ctx = &*ctx;
        (ctx.windowed_ctx.height * ctx.windowed_ctx.scale_factor as f32) as u32
    }
}

// =============================================================================
// GPU Rendering (C FFI for Swift)
// =============================================================================

/// GPU renderer state for iOS
pub struct IOSGpuRenderer {
    /// The Blinc application (includes renderer, text context, image context)
    app: BlincApp,
    /// The wgpu surface
    surface: wgpu::Surface<'static>,
    /// Surface configuration
    surface_config: wgpu::SurfaceConfiguration,
    /// Render context reference
    render_ctx: *mut IOSRenderContext,
}

/// Initialize the GPU renderer with a CAMetalLayer (C FFI for Swift)
///
/// # Arguments
/// * `ctx` - Render context pointer from `blinc_create_context`
/// * `metal_layer` - Pointer to CAMetalLayer (from UIView.layer)
/// * `width` - Drawable width in pixels
/// * `height` - Drawable height in pixels
///
/// # Returns
/// Pointer to GPU renderer, or null on failure
///
/// # Safety
/// * `ctx` must be a valid pointer returned by `blinc_create_context`
/// * `metal_layer` must be a valid pointer to a CAMetalLayer
#[no_mangle]
pub extern "C" fn blinc_init_gpu(
    ctx: *mut IOSRenderContext,
    metal_layer: *mut std::ffi::c_void,
    width: u32,
    height: u32,
) -> *mut IOSGpuRenderer {
    use blinc_gpu::{GpuRenderer, RendererConfig, TextRenderingContext};

    if ctx.is_null() || metal_layer.is_null() {
        tracing::error!("blinc_init_gpu: null context or metal_layer");
        return std::ptr::null_mut();
    }

    let config = crate::BlincConfig::default();

    let renderer_config = RendererConfig {
        max_primitives: config.max_primitives,
        max_glass_primitives: config.max_glass_primitives,
        max_glyphs: config.max_glyphs,
        sample_count: 1,
        texture_format: None,
        unified_text_rendering: true,
    };

    // Create wgpu instance with Metal backend
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..Default::default()
    });

    // Create surface from CAMetalLayer
    // CoreAnimationLayer takes a raw *mut c_void pointer
    let surface_target = wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(metal_layer);
    let surface = match unsafe { instance.create_surface_unsafe(surface_target) } {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("blinc_init_gpu: failed to create surface: {}", e);
            return std::ptr::null_mut();
        }
    };

    // Create renderer
    let renderer = match pollster::block_on(async {
        GpuRenderer::with_instance_and_surface(instance, &surface, renderer_config).await
    }) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("blinc_init_gpu: failed to create renderer: {}", e);
            return std::ptr::null_mut();
        }
    };

    let device = renderer.device_arc();
    let queue = renderer.queue_arc();

    let mut text_ctx = TextRenderingContext::new(device.clone(), queue.clone());

    // Load iOS system fonts into the registry
    // Note: The FontRegistry already tries to load from KNOWN_FONT_PATHS,
    // but we also load from system_font_paths() to ensure fonts are available.
    let mut fonts_loaded = 0;
    for font_path in IOSApp::system_font_paths() {
        let path = std::path::Path::new(font_path);
        tracing::debug!("Checking font path: {}", font_path);
        if path.exists() {
            match std::fs::read(path) {
                Ok(data) => {
                    tracing::info!("Loading font from: {} ({} bytes)", font_path, data.len());
                    // Use load_font_data_to_registry to add to the font registry
                    // (not load_font_data which only sets the default font)
                    let loaded = text_ctx.load_font_data_to_registry(data);
                    if loaded > 0 {
                        tracing::info!("Successfully loaded {} faces from: {}", loaded, font_path);
                        fonts_loaded += loaded;
                    } else {
                        tracing::warn!("No faces loaded from font {}", font_path);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to read font file {}: {}", font_path, e);
                }
            }
        } else {
            tracing::debug!("Font path does not exist: {}", font_path);
        }
    }
    tracing::info!("Loaded {} font faces total", fonts_loaded);

    // Preload common fonts with iOS family names
    // Note: iOS uses ".SF UI" family names (with leading dot for system fonts)
    text_ctx.preload_fonts(&[
        ".SF UI",            // iOS system font
        ".SF UI Text",       // iOS system font (text)
        ".SF UI Display",    // iOS system font (display)
        "Helvetica",         // Helvetica
        "Helvetica Neue",    // Helvetica Neue
        "Avenir",            // Avenir
        "Avenir Next",       // Avenir Next
        "Menlo",             // Monospace
        "Courier New",       // Courier
    ]);
    text_ctx.preload_generic_styles(blinc_gpu::GenericFont::SansSerif, &[400, 700], false);
    tracing::info!("Font preloading complete, {} fonts loaded", fonts_loaded);

    // Create RenderContext with text rendering support
    let render_context = crate::context::RenderContext::new(
        renderer,
        text_ctx,
        device,
        queue,
        config.sample_count,
    );
    let app = BlincApp::from_context(render_context, config);

    // Configure surface with the format the renderer selected
    let format = app.texture_format();
    let surface_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width,
        height,
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(app.device(), &surface_config);

    tracing::info!(
        "blinc_init_gpu: GPU initialized ({}x{}, format: {:?})",
        width,
        height,
        format
    );

    Box::into_raw(Box::new(IOSGpuRenderer {
        app,
        surface,
        surface_config,
        render_ctx: ctx,
    }))
}

/// Resize the GPU surface (C FFI for Swift)
///
/// Call this when the Metal layer's drawable size changes.
///
/// # Safety
/// `gpu` must be a valid pointer returned by `blinc_init_gpu`.
#[no_mangle]
pub extern "C" fn blinc_gpu_resize(gpu: *mut IOSGpuRenderer, width: u32, height: u32) {
    if gpu.is_null() {
        return;
    }

    unsafe {
        let gpu = &mut *gpu;
        if width > 0 && height > 0 && (gpu.surface_config.width != width || gpu.surface_config.height != height) {
            gpu.surface_config.width = width;
            gpu.surface_config.height = height;
            gpu.surface.configure(gpu.app.device(), &gpu.surface_config);
            tracing::debug!("blinc_gpu_resize: {}x{}", width, height);
        }
    }
}

/// Render a frame (C FFI for Swift)
///
/// This builds the UI if needed and renders to the current surface.
/// Call this from your CADisplayLink callback when `blinc_needs_render()` is true.
///
/// # Returns
/// true if frame was rendered successfully, false on error
///
/// # Safety
/// * `gpu` must be a valid pointer returned by `blinc_init_gpu`
/// * Must be called on the main thread
#[no_mangle]
pub extern "C" fn blinc_render_frame(gpu: *mut IOSGpuRenderer) -> bool {
    if gpu.is_null() {
        return false;
    }

    unsafe {
        let gpu = &mut *gpu;
        let ctx = match gpu.render_ctx.as_mut() {
            Some(c) => c,
            None => return false,
        };

        // Get surface texture
        let surface_texture = match gpu.surface.get_current_texture() {
            Ok(st) => st,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                // Reconfigure surface and try again
                gpu.surface.configure(gpu.app.device(), &gpu.surface_config);
                match gpu.surface.get_current_texture() {
                    Ok(st) => st,
                    Err(e) => {
                        tracing::error!("blinc_render_frame: surface error: {:?}", e);
                        return false;
                    }
                }
            }
            Err(e) => {
                tracing::error!("blinc_render_frame: surface error: {:?}", e);
                return false;
            }
        };

        // Get render tree
        let tree = match ctx.render_tree.as_ref() {
            Some(t) => t,
            None => {
                surface_texture.present();
                return true; // No tree yet, just present empty frame
            }
        };

        // Render
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        if let Err(e) = gpu.app.render_tree_with_motion(
            tree,
            &ctx.render_state,
            &view,
            gpu.surface_config.width,
            gpu.surface_config.height,
        ) {
            tracing::error!("blinc_render_frame: render error: {}", e);
            surface_texture.present();
            return false;
        }

        surface_texture.present();
        true
    }
}

/// Destroy the GPU renderer (C FFI for Swift)
///
/// # Safety
/// `gpu` must be a valid pointer returned by `blinc_init_gpu`,
/// and must not be used after this call.
#[no_mangle]
pub extern "C" fn blinc_destroy_gpu(gpu: *mut IOSGpuRenderer) {
    if !gpu.is_null() {
        unsafe {
            drop(Box::from_raw(gpu));
        }
    }
}

/// Load a bundled font from the app bundle (C FFI for Swift)
///
/// Call this after `blinc_init_gpu` to load fonts from the app bundle.
/// The font will be added to the font registry and available for text rendering.
///
/// # Arguments
/// * `gpu` - GPU renderer pointer from `blinc_init_gpu`
/// * `path` - Path to the font file (null-terminated C string)
///
/// # Returns
/// Number of font faces loaded (0 on failure)
///
/// # Safety
/// * `gpu` must be a valid pointer returned by `blinc_init_gpu`
/// * `path` must be a valid null-terminated C string
#[no_mangle]
pub extern "C" fn blinc_load_bundled_font(
    gpu: *mut IOSGpuRenderer,
    path: *const std::ffi::c_char,
) -> u32 {
    if gpu.is_null() || path.is_null() {
        return 0;
    }

    unsafe {
        let gpu = &mut *gpu;
        let path_str = match std::ffi::CStr::from_ptr(path).to_str() {
            Ok(s) => s,
            Err(_) => {
                tracing::error!("blinc_load_bundled_font: invalid path string");
                return 0;
            }
        };

        tracing::info!("Loading bundled font from: {}", path_str);

        let path = std::path::Path::new(path_str);
        if !path.exists() {
            tracing::error!("blinc_load_bundled_font: font file does not exist: {}", path_str);
            return 0;
        }

        match std::fs::read(path) {
            Ok(data) => {
                tracing::info!("Read {} bytes from bundled font", data.len());
                let loaded = gpu.app.load_font_data_to_registry(data);
                tracing::info!("Loaded {} font faces from bundled font", loaded);
                loaded as u32
            }
            Err(e) => {
                tracing::error!("blinc_load_bundled_font: failed to read font: {}", e);
                0
            }
        }
    }
}
