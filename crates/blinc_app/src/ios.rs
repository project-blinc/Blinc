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
use blinc_layout::overlay_state::OverlayContext;
use blinc_layout::prelude::*;
use blinc_layout::widgets::overlay::{overlay_manager, OverlayManager};
use blinc_platform::assets::set_global_asset_loader;
use blinc_platform_ios::{IOSAssetLoader, IOSWakeProxy};

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
        use blinc_theme::{platform::detect_system_color_scheme, ThemeBundle, ThemeState};

        // Only initialize if not already initialized
        if ThemeState::try_get().is_none() {
            let bundle = ThemeBundle::default();
            let scheme = detect_system_color_scheme();
            ThemeState::init(bundle, scheme);
        }

        // Set up the redraw callback
        blinc_theme::set_redraw_callback(|| {
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
        tracing::info!("IOSApp::create_context starting");

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
    /// - Reactive state changed
    /// - Animations are active
    /// - Wake was requested by animation thread
    pub fn needs_render(&self) -> bool {
        let dirty = self.ref_dirty_flag.load(Ordering::SeqCst);
        let wake_requested = self.wake_proxy.take_wake_request();
        let animations_active = self
            .animations
            .lock()
            .map(|sched| sched.has_active())
            .unwrap_or(false);

        dirty || wake_requested || animations_active
    }

    /// Update the window size
    ///
    /// Call this when the view's bounds change.
    pub fn update_size(&mut self, width: u32, height: u32, scale_factor: f64) {
        let logical_width = width as f32 / scale_factor as f32;
        let logical_height = height as f32 / scale_factor as f32;

        self.windowed_ctx.width = logical_width;
        self.windowed_ctx.height = logical_height;
        self.windowed_ctx.scale_factor = scale_factor;
        self.windowed_ctx.physical_width = width as f32;
        self.windowed_ctx.physical_height = height as f32;

        BlincContextState::get().set_viewport_size(logical_width, logical_height);
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
        let tree = self
            .render_tree
            .get_or_insert_with(|| RenderTree::from_element(&element));
        tree.rebuild(&element);
        tree.compute_layout(self.windowed_ctx.width, self.windowed_ctx.height);

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
    pub fn handle_touch(&mut self, _touch: blinc_platform_ios::Touch) {
        // TODO: Route touch to event router
        // This will be implemented when the event routing system is connected
    }

    /// Set focus state
    pub fn set_focused(&mut self, focused: bool) {
        self.windowed_ctx.focused = focused;
    }
}
