//! Fuchsia application runner
//!
//! Provides a unified API for running Blinc applications on Fuchsia OS.
//!
//! # Example
//!
//! ```ignore
//! use blinc_app::prelude::*;
//! use blinc_app::fuchsia::FuchsiaApp;
//!
//! #[no_mangle]
//! fn main() {
//!     FuchsiaApp::run(|ctx| {
//!         div().w(ctx.width).h(ctx.height)
//!             .bg([0.1, 0.1, 0.15, 1.0])
//!             .flex_center()
//!             .child(text("Hello Fuchsia!").size(48.0))
//!     }).unwrap();
//! }
//! ```
//!
//! # Architecture
//!
//! Fuchsia applications integrate with the system through:
//!
//! - **Scenic/Flatland** - Window compositing via Views
//! - **fuchsia-async** - Async executor for event handling
//! - **FIDL** - IPC with system services
//! - **Vulkan** - GPU rendering via ImagePipe2
//!
//! # Building
//!
//! Requires the Fuchsia SDK and target:
//!
//! ```bash
//! rustup target add x86_64-unknown-fuchsia
//! cargo build --target x86_64-unknown-fuchsia --features fuchsia
//! ```

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
use blinc_platform_fuchsia::{
    AsyncEventLoop, BufferFormat, ContentId, EventLoopConfig, FlatlandSession,
    FrameSchedulingState, FuchsiaAssetLoader, FuchsiaEvent, FuchsiaEventSources, FuchsiaPlatform,
    FuchsiaWakeProxy, GpuConfig, ImagePipeClient, InputSources, MouseInteraction, TouchInteraction,
    TouchPhase, TouchResponse, TransformId, ViewEvent, ViewProperties, ViewProvider, WakeEvent,
};

use crate::app::BlincApp;
use crate::error::{BlincError, Result};
use crate::windowed::{
    RefDirtyFlag, SharedAnimationScheduler, SharedElementRegistry, SharedReactiveGraph,
    SharedReadyCallbacks, WindowedContext,
};

/// Fuchsia application runner
///
/// Provides a simple way to run a Blinc application on Fuchsia OS
/// with automatic event handling and rendering via Scenic.
pub struct FuchsiaApp;

/// Handle a touch event from fuchsia.ui.pointer.TouchSource
///
/// Call this from the async event loop when touch events are received.
/// Routes the touch through the event router for hit testing and callbacks.
fn handle_touch_event(
    touch: &TouchInteraction,
    render_tree: Option<&RenderTree>,
    ctx: &mut WindowedContext,
    scale_factor: f64,
) -> bool {
    let tree = match render_tree {
        Some(t) => t,
        None => return false,
    };

    // Convert physical coordinates to logical (touch coords are already in logical DIP)
    let lx = touch.position.0;
    let ly = touch.position.1;

    match touch.phase {
        TouchPhase::Began => {
            ctx.event_router.on_mouse_down(tree, lx, ly, MouseButton::Left);
        }
        TouchPhase::Moved => {
            ctx.event_router.on_mouse_move(tree, lx, ly);
        }
        TouchPhase::Ended => {
            ctx.event_router.on_mouse_up(tree, lx, ly, MouseButton::Left);
        }
        TouchPhase::Cancelled => {
            ctx.event_router.on_mouse_leave();
        }
    }

    true // Needs rebuild after touch
}

/// Handle a mouse event from fuchsia.ui.pointer.MouseSource
///
/// Routes mouse interactions through the event router.
fn handle_mouse_event(
    mouse: &MouseInteraction,
    render_tree: Option<&RenderTree>,
    ctx: &mut WindowedContext,
) -> bool {
    let tree = match render_tree {
        Some(t) => t,
        None => return false,
    };

    let (x, y) = mouse.position;

    // Handle button presses
    if mouse.newly_pressed & 0x01 != 0 {
        ctx.event_router.on_mouse_down(tree, x, y, MouseButton::Left);
    }
    if mouse.newly_pressed & 0x02 != 0 {
        ctx.event_router.on_mouse_down(tree, x, y, MouseButton::Right);
    }
    if mouse.newly_pressed & 0x04 != 0 {
        ctx.event_router.on_mouse_down(tree, x, y, MouseButton::Middle);
    }

    // Handle button releases
    if mouse.newly_released & 0x01 != 0 {
        ctx.event_router.on_mouse_up(tree, x, y, MouseButton::Left);
    }
    if mouse.newly_released & 0x02 != 0 {
        ctx.event_router.on_mouse_up(tree, x, y, MouseButton::Right);
    }
    if mouse.newly_released & 0x04 != 0 {
        ctx.event_router.on_mouse_up(tree, x, y, MouseButton::Middle);
    }

    // Handle scroll
    if let Some((dx, dy)) = mouse.scroll_v {
        ctx.event_router.on_scroll(tree, x, y, dx as f32, dy as f32);
    }

    // Always update position for hover
    ctx.event_router.on_mouse_move(tree, x, y);

    true // Needs rebuild after mouse event
}

/// Process a FuchsiaEvent and return whether we need to rebuild the UI
fn process_fuchsia_event(
    event: &FuchsiaEvent,
    ctx: &mut WindowedContext,
    render_tree: Option<&RenderTree>,
    logical_size: &mut (f32, f32),
    scale_factor: &mut f64,
) -> bool {
    match event {
        FuchsiaEvent::FrameReady { .. } => {
            // Frame scheduling update - not a rebuild trigger itself
            false
        }
        FuchsiaEvent::LayoutChanged {
            width,
            height,
            scale_factor: new_scale,
            insets,
        } => {
            // Update dimensions
            *logical_size = (*width, *height);
            *scale_factor = *new_scale;

            // Update context
            ctx.width = *width;
            ctx.height = *height;
            ctx.scale_factor = *new_scale;

            // Update viewport size globally
            BlincContextState::get().set_viewport_size(*width, *height);

            tracing::info!(
                "Layout changed: {}x{} @ {}x, insets: {:?}",
                width,
                height,
                new_scale,
                insets
            );
            true
        }
        FuchsiaEvent::Touch { interaction } => {
            handle_touch_event(interaction, render_tree, ctx, *scale_factor)
        }
        FuchsiaEvent::Mouse { interaction } => handle_mouse_event(interaction, render_tree, ctx),
        FuchsiaEvent::Keyboard { event: key_event } => {
            // TODO: Route keyboard events
            tracing::debug!("Keyboard event: {:?}", key_event);
            false
        }
        FuchsiaEvent::FocusChanged(focused) => {
            ctx.focused = *focused;
            tracing::info!("Focus changed: {}", focused);
            true
        }
        FuchsiaEvent::ViewDetached => {
            tracing::info!("View detached");
            false
        }
        FuchsiaEvent::ViewDestroyed => {
            tracing::info!("View destroyed");
            false
        }
        FuchsiaEvent::WakeRequested => {
            // Animation wake - trigger rebuild
            true
        }
    }
}

impl FuchsiaApp {
    /// Initialize the Fuchsia asset loader
    fn init_asset_loader() {
        let loader = FuchsiaAssetLoader::new();
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

    /// Initialize logging for Fuchsia
    fn init_logging() {
        // Fuchsia uses syslog - set up tracing subscriber
        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_target(true));
        let _ = tracing::subscriber::set_global_default(subscriber);
    }

    /// Run a Fuchsia Blinc application
    ///
    /// This is the main entry point for Fuchsia applications. It sets up
    /// the GPU renderer via Scenic, handles lifecycle events, and runs the event loop.
    ///
    /// # Arguments
    ///
    /// * `ui_builder` - Function that builds the UI tree given the window context
    ///
    /// # Example
    ///
    /// ```ignore
    /// FuchsiaApp::run(|ctx| {
    ///     div()
    ///         .w(ctx.width).h(ctx.height)
    ///         .bg([0.1, 0.1, 0.15, 1.0])
    ///         .flex_center()
    ///         .child(text("Hello Fuchsia!").size(32.0))
    /// })
    /// ```
    #[cfg(target_os = "fuchsia")]
    pub fn run<F, E>(mut ui_builder: F) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder + 'static,
    {
        // Initialize logging first
        Self::init_logging();
        tracing::info!("FuchsiaApp::run starting");

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

        // Animation scheduler
        let mut scheduler = AnimationScheduler::new();

        // Set up wake proxy for Fuchsia - allows animation thread to wake event loop
        let wake_proxy = FuchsiaWakeProxy::new();
        let wake_proxy_clone = wake_proxy.clone();
        scheduler.set_wake_callback(move || wake_proxy_clone.wake());
        tracing::info!("Fuchsia WakeProxy enabled for animations");

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

        // Create Flatland session for scene graph management
        let mut flatland = FlatlandSession::new()
            .map_err(|e| BlincError::PlatformError(format!("Flatland init failed: {}", e)))?;

        // Create ImagePipe for GPU rendering
        // Double-buffered, sRGB format for correct color
        let mut image_pipe = ImagePipeClient::new(1920, 1080, 2, BufferFormat::B8G8R8A8Srgb);

        // Initialize the image pipe (allocates GPU buffers via sysmem)
        image_pipe.initialize_sync()
            .map_err(|e| BlincError::PlatformError(format!("ImagePipe init failed: {}", e)))?;

        // Register with Flatland to get ContentId
        let content_id = image_pipe.register_with_flatland(&flatland)
            .map_err(|e| BlincError::PlatformError(format!("Flatland registration failed: {}", e)))?;

        // Create scene graph: root transform with image content
        let root_transform = flatland.create_transform();
        flatland.set_root_transform(root_transform);
        flatland.set_content(root_transform, content_id);

        // Configure full-view hit region for input
        flatland.set_infinite_hit_region(root_transform);

        tracing::info!("Flatland scene graph created: root={:?}, content={:?}", root_transform, content_id);

        // Frame scheduling state (updated by Flatland.OnNextFrameBegin)
        let mut frame_state = FrameSchedulingState::default();

        // Event loop configuration
        let event_config = EventLoopConfig {
            use_frame_scheduling: true,
            poll_timeout: std::time::Duration::from_millis(16), // ~60fps fallback
            max_pending_frames: 2,
        };

        // Input sources available on this device
        let input_sources = InputSources::default();
        tracing::info!("Input sources: touch={}, mouse={}, keyboard={}",
            input_sources.touch_available,
            input_sources.mouse_available,
            input_sources.keyboard_available);

        // Default window size (will be updated from ViewProperties on first layout)
        let width = 1920u32;
        let height = 1080u32;
        let scale_factor = 1.0f64;
        let logical_width = width as f32 / scale_factor as f32;
        let logical_height = height as f32 / scale_factor as f32;

        // Create WindowedContext with default size
        let mut ctx = WindowedContext::new_fuchsia(
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

        // Set viewport size
        BlincContextState::get().set_viewport_size(logical_width, logical_height);

        // Application and render state
        let mut blinc_app: Option<BlincApp> = None;
        let mut render_tree: Option<RenderTree> = None;
        let mut render_state: Option<blinc_layout::RenderState> = None;

        // Running state
        let mut running = true;
        let mut needs_rebuild = true;

        tracing::info!("Entering Fuchsia event loop");

        // Main event loop
        // On Fuchsia, this would be an async loop using fuchsia-async executor
        // with select! on multiple FIDL event streams
        while running {
            // Check for wake requests from animation thread
            if wake_proxy.take_wake_request() {
                needs_rebuild = true;
            }

            // Check for reactive state changes
            if ref_dirty_flag.swap(false, Ordering::AcqRel) {
                needs_rebuild = true;
            }

            // Pump animations
            {
                let mut scheduler = animations.lock().unwrap();
                if scheduler.tick() {
                    needs_rebuild = true;
                }
            }

            // Build UI if needed
            if needs_rebuild {
                // Clear hook state for fresh build
                {
                    let mut hooks_guard = hooks.lock().unwrap();
                    hooks_guard.reset_cursor();
                }

                // Build the UI
                let element = ui_builder(&mut ctx);

                // Prepare for rendering
                let mut tree = LayoutTree::new();
                element.build(&mut tree);

                // Compute layout
                let available_size = taffy::Size {
                    width: taffy::AvailableSpace::Definite(logical_width),
                    height: taffy::AvailableSpace::Definite(logical_height),
                };

                if let Some(root_id) = tree.root() {
                    tree.compute_layout(root_id, available_size);

                    // Collect render tree
                    if let Some(new_render_tree) = tree.collect_render_tree(root_id) {
                        render_tree = Some(new_render_tree);
                    }
                }

                needs_rebuild = false;
            }

            // Render if we have a frame slot available
            if frame_state.should_render() {
                if let Some(ref tree) = render_tree {
                    // Acquire buffer from ImagePipe
                    let buffer = image_pipe.acquire_next_buffer();

                    // On Fuchsia with actual GPU:
                    // 1. Get wgpu texture view for this buffer
                    // 2. Render using blinc_app.render_frame(texture_view)
                    // 3. Present via image_pipe.present()

                    // For now, log render stats
                    let node_count = tree.nodes().len();
                    tracing::trace!("Frame {} rendered: {} nodes", image_pipe.present_count(), node_count);

                    // Present the frame
                    let _ = image_pipe.present_sync(0);
                }
            }

            // Process input events
            // On Fuchsia, would receive from fuchsia.ui.pointer.TouchSource/MouseSource
            // For now, this is where touch events would be routed through EventRouter

            // Sleep to avoid busy-looping (real implementation awaits on FIDL events)
            std::thread::sleep(event_config.poll_timeout);
        }

        tracing::info!("FuchsiaApp::run exiting");
        Ok(())
    }

    /// Placeholder for non-Fuchsia builds
    #[cfg(not(target_os = "fuchsia"))]
    pub fn run<F, E>(_ui_builder: F) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder + 'static,
    {
        Err(BlincError::PlatformUnsupported(
            "Fuchsia apps can only run on Fuchsia OS".to_string(),
        ))
    }

    /// Get the system font paths for Fuchsia
    pub fn system_font_paths() -> &'static [&'static str] {
        FuchsiaPlatform::system_font_paths()
    }
}
