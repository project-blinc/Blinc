//! Android application runner
//!
//! Provides a unified API for running Blinc applications on Android.
//!
//! # Example
//!
//! ```ignore
//! use blinc_app::prelude::*;
//! use blinc_app::android::AndroidApp;
//!
//! #[no_mangle]
//! fn android_main(app: android_activity::AndroidApp) {
//!     AndroidApp::run(app, |ctx| {
//!         div().w(ctx.width).h(ctx.height)
//!             .bg([0.1, 0.1, 0.15, 1.0])
//!             .flex_center()
//!             .child(text("Hello Android!").size(48.0))
//!     }).unwrap();
//! }
//! ```

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use android_activity::input::{InputEvent as AndroidInputEvent, MotionAction};
use android_activity::{AndroidApp as NdkAndroidApp, InputStatus, MainEvent, PollEvent};
use ndk::native_window::NativeWindow;

use blinc_animation::AnimationScheduler;
use blinc_core::context_state::{BlincContextState, HookState, SharedHookState};
use blinc_core::reactive::{ReactiveGraph, SignalId};
use blinc_layout::event_router::MouseButton;
use blinc_layout::overlay_state::OverlayContext;
use blinc_layout::prelude::*;
use blinc_layout::widgets::overlay::{overlay_manager, OverlayManager};
use blinc_platform::assets::set_global_asset_loader;
use blinc_platform_android::AndroidAssetLoader;

use crate::app::BlincApp;
use crate::error::{BlincError, Result};
use crate::windowed::{
    RefDirtyFlag, SharedAnimationScheduler, SharedElementRegistry, SharedReactiveGraph,
    SharedReadyCallbacks, WindowedContext,
};

/// Android application runner
///
/// Provides a simple way to run a Blinc application on Android
/// with automatic event handling and rendering.
pub struct AndroidApp;

impl AndroidApp {
    /// Initialize the Android asset loader
    fn init_asset_loader(app: NdkAndroidApp) {
        let loader = AndroidAssetLoader::new(app);
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

    /// Initialize Android logging
    fn init_logging() {
        // Initialize android_logger for log crate
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Debug)
                .with_tag("Blinc"),
        );

        // Initialize tracing-android for tracing crate
        use tracing_subscriber::layer::SubscriberExt;
        let subscriber =
            tracing_subscriber::registry().with(tracing_android::layer("Blinc").unwrap());
        let _ = tracing::subscriber::set_global_default(subscriber);
    }

    /// Run an Android Blinc application
    ///
    /// This is the main entry point for Android applications. It sets up
    /// the GPU renderer, handles lifecycle events, and runs the event loop.
    ///
    /// # Arguments
    ///
    /// * `app` - The AndroidApp from android-activity
    /// * `ui_builder` - Function that builds the UI tree given the window context
    ///
    /// # Example
    ///
    /// ```ignore
    /// AndroidApp::run(app, |ctx| {
    ///     div()
    ///         .w(ctx.width).h(ctx.height)
    ///         .bg([0.1, 0.1, 0.15, 1.0])
    ///         .flex_center()
    ///         .child(text("Hello Android!").size(32.0))
    /// })
    /// ```
    pub fn run<F, E>(app: NdkAndroidApp, mut ui_builder: F) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder + 'static,
    {
        // Initialize logging first
        Self::init_logging();
        tracing::info!("AndroidApp::run starting");

        // Initialize the asset loader
        Self::init_asset_loader(app.clone());

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

        // Animation scheduler - single-threaded for mobile efficiency
        // Unlike desktop, we tick animations on main thread to avoid mutex contention
        // and high CPU usage from background thread + main thread fighting
        let scheduler = AnimationScheduler::new();
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

        // Application state
        let mut blinc_app: Option<BlincApp> = None;
        let mut surface: Option<wgpu::Surface<'static>> = None;
        let mut surface_config: Option<wgpu::SurfaceConfiguration> = None;
        let mut ctx: Option<WindowedContext> = None;
        let mut render_tree: Option<RenderTree> = None;
        let mut render_state: Option<blinc_layout::RenderState> = None;
        let mut native_window: Option<NativeWindow> = None;
        let mut needs_rebuild = true;
        let mut needs_redraw_next_frame = false;
        let mut running = true;
        let mut focused = false;

        // Touch tracking for scroll delta calculation
        // On mobile, scroll happens via touch drag, not wheel events
        let mut last_touch_x: Option<f32> = None;
        let mut last_touch_y: Option<f32> = None;
        let mut is_scrolling = false;
        let mut pinch_prev_span: Option<f32> = None;

        tracing::info!("Entering Android event loop");

        while running {
            // When animating: don't wait - vsync in present() handles frame pacing
            // When idle: wait for events to save power
            let poll_timeout = if needs_rebuild || needs_redraw_next_frame {
                Some(std::time::Duration::ZERO) // Don't wait - vsync paces us
            } else {
                Some(std::time::Duration::from_millis(100)) // Idle - save power
            };
            needs_redraw_next_frame = false;

            app.poll_events(poll_timeout, |event| {
                match event {
                    PollEvent::Main(main_event) => match main_event {
                        MainEvent::InitWindow { .. } => {
                            tracing::info!("Native window initialized");
                            if let Some(window) = app.native_window() {
                                let width = window.width() as u32;
                                let height = window.height() as u32;
                                tracing::info!("Window size: {}x{}", width, height);

                                // Initialize GPU with native window
                                match Self::init_gpu(&window) {
                                    Ok((app_instance, surf)) => {
                                        let format = app_instance.texture_format();
                                        let config = wgpu::SurfaceConfiguration {
                                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                                            format,
                                            width,
                                            height,
                                            present_mode: wgpu::PresentMode::AutoVsync,
                                            alpha_mode: wgpu::CompositeAlphaMode::Auto,
                                            view_formats: vec![],
                                            desired_maximum_frame_latency: 2,
                                        };
                                        surf.configure(&app_instance.device(), &config);

                                        // Update text measurer
                                        crate::text_measurer::init_text_measurer_with_registry(
                                            app_instance.font_registry(),
                                        );

                                        surface = Some(surf);
                                        surface_config = Some(config);
                                        blinc_app = Some(app_instance);
                                        native_window = Some(window);

                                        // Create WindowedContext with actual display density
                                        let scale_factor =
                                            blinc_platform_android::get_display_density(&app);
                                        let logical_width = width as f32 / scale_factor as f32;
                                        let logical_height = height as f32 / scale_factor as f32;

                                        ctx = Some(WindowedContext::new_android(
                                            logical_width,
                                            logical_height,
                                            scale_factor,
                                            width as f32,
                                            height as f32,
                                            focused,
                                            Arc::clone(&animations),
                                            Arc::clone(&ref_dirty_flag),
                                            Arc::clone(&reactive),
                                            Arc::clone(&hooks),
                                            Arc::clone(&overlays),
                                            Arc::clone(&element_registry),
                                            Arc::clone(&ready_callbacks),
                                        ));

                                        // Set viewport size
                                        BlincContextState::get()
                                            .set_viewport_size(logical_width, logical_height);

                                        // Initialize render state
                                        let mut rs =
                                            blinc_layout::RenderState::new(Arc::clone(&animations));
                                        rs.set_shared_motion_states(Arc::clone(
                                            &shared_motion_states,
                                        ));
                                        render_state = Some(rs);

                                        needs_rebuild = true;
                                        tracing::info!("GPU initialized successfully");
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to initialize GPU: {}", e);
                                    }
                                }
                            }
                        }

                        MainEvent::TerminateWindow { .. } => {
                            tracing::info!("Native window terminated");
                            native_window = None;
                            surface = None;
                            surface_config = None;
                            blinc_app = None;
                            ctx = None;
                            render_tree = None;
                            render_state = None;
                        }

                        MainEvent::WindowResized { .. } => {
                            if let Some(ref window) = native_window {
                                let width = window.width() as u32;
                                let height = window.height() as u32;
                                tracing::info!("Window resized: {}x{}", width, height);

                                if let (
                                    Some(ref app_instance),
                                    Some(ref surf),
                                    Some(ref mut config),
                                ) = (&blinc_app, &surface, &mut surface_config)
                                {
                                    if width > 0 && height > 0 {
                                        config.width = width;
                                        config.height = height;
                                        surf.configure(&app_instance.device(), config);

                                        if let Some(ref mut windowed_ctx) = ctx {
                                            let scale_factor = windowed_ctx.scale_factor;
                                            windowed_ctx.width = width as f32 / scale_factor as f32;
                                            windowed_ctx.height =
                                                height as f32 / scale_factor as f32;

                                            BlincContextState::get().set_viewport_size(
                                                windowed_ctx.width,
                                                windowed_ctx.height,
                                            );
                                        }

                                        needs_rebuild = true;
                                    }
                                }
                            }
                        }

                        MainEvent::GainedFocus => {
                            tracing::info!("App gained focus");
                            focused = true;
                            if let Some(ref mut windowed_ctx) = ctx {
                                windowed_ctx.focused = true;
                            }
                        }

                        MainEvent::LostFocus => {
                            tracing::info!("App lost focus");
                            focused = false;
                            if let Some(ref mut windowed_ctx) = ctx {
                                windowed_ctx.focused = false;
                            }
                        }

                        MainEvent::Resume { .. } => {
                            tracing::info!("App resumed");
                            focused = true;
                        }

                        MainEvent::Pause => {
                            tracing::info!("App paused");
                            focused = false;
                        }

                        MainEvent::Destroy => {
                            tracing::info!("App destroyed");
                            running = false;
                        }

                        MainEvent::LowMemory => {
                            tracing::warn!("Low memory warning");
                            // TODO: Release caches
                        }

                        _ => {}
                    },

                    PollEvent::Wake => {
                        // Animation thread wake - request redraw only (NOT rebuild)
                        needs_redraw_next_frame = true;
                    }

                    _ => {}
                }
            });

            // Process touch/input events from android-activity
            // Collect pending events for dispatch (like desktop windowed.rs)
            #[derive(Clone, Default)]
            struct PendingEvent {
                node_id: blinc_layout::LayoutNodeId,
                event_type: u32,
                mouse_x: f32,
                mouse_y: f32,
            }

            let mut pending_events: Vec<PendingEvent> = Vec::new();
            // Track scroll info for dispatch after event processing (mouse_x, mouse_y, delta_x, delta_y)
            let mut scroll_info: Option<(f32, f32, f32, f32)> = None;
            // Track pinch scroll info for dispatch after event processing (mouse_x, mouse_y, delta_y)
            let mut pinch_scroll_info: Option<(f32, f32, f32)> = None;
            // Track if touch ended (for scroll physics)
            let mut touch_ended = false;

            if let (Some(ref mut windowed_ctx), Some(ref tree)) = (&mut ctx, &render_tree) {
                // Get the scale factor for coordinate conversion
                let scale = windowed_ctx.scale_factor as f32;
                let router = &mut windowed_ctx.event_router;

                // Set up callback to collect events (like desktop)
                router.set_event_callback({
                    let events = &mut pending_events as *mut Vec<PendingEvent>;
                    move |node, event_type| {
                        // SAFETY: This callback is only used within this scope
                        unsafe {
                            (*events).push(PendingEvent {
                                node_id: node,
                                event_type,
                                ..Default::default()
                            });
                        }
                    }
                });

                // Process all pending input events
                match app.input_events_iter() {
                    Ok(mut input_iter) => {
                        // Handle input events using android-activity 0.6 API
                        while input_iter.next(|event| {
                            match event {
                                AndroidInputEvent::MotionEvent(motion_event) => {
                                    let action = motion_event.action();
                                    let pointer_count = motion_event.pointer_count();
                                    let action_index = motion_event.pointer_index();

                                    if pointer_count > 0 {
                                        let pointer_idx = match action {
                                            MotionAction::PointerDown | MotionAction::PointerUp => {
                                                action_index
                                            }
                                            _ => 0,
                                        };

                                        let pointer = motion_event.pointer_at_index(pointer_idx);
                                        let lx = pointer.x() / scale;
                                        let ly = pointer.y() / scale;
                                        let pinch_info = if pointer_count >= 2 {
                                            let pointer0 = motion_event.pointer_at_index(0);
                                            let pointer1 = motion_event.pointer_at_index(1);
                                            let x0 = pointer0.x() / scale;
                                            let y0 = pointer0.y() / scale;
                                            let x1 = pointer1.x() / scale;
                                            let y1 = pointer1.y() / scale;
                                            let dx = x1 - x0;
                                            let dy = y1 - y0;
                                            let span = (dx * dx + dy * dy).sqrt();
                                            let center_x = (x0 + x1) * 0.5;
                                            let center_y = (y0 + y1) * 0.5;
                                            Some((center_x, center_y, span))
                                        } else {
                                            None
                                        };

                                        match action {
                                            MotionAction::Down | MotionAction::PointerDown => {
                                                tracing::debug!(
                                                    "Touch DOWN at logical ({:.1}, {:.1})",
                                                    lx,
                                                    ly
                                                );
                                                router.on_mouse_down(
                                                    tree,
                                                    lx,
                                                    ly,
                                                    MouseButton::Left,
                                                );
                                                // Initialize touch tracking for scroll
                                                if let Some((_center_x, _center_y, span)) = pinch_info
                                                {
                                                    pinch_prev_span = Some(span);
                                                    last_touch_x = None;
                                                    last_touch_y = None;
                                                    is_scrolling = false;
                                                } else {
                                                    last_touch_x = Some(lx);
                                                    last_touch_y = Some(ly);
                                                }
                                                is_scrolling = false;
                                                // Update pending events with coordinates
                                                unsafe {
                                                    let events = &mut pending_events
                                                        as *mut Vec<PendingEvent>;
                                                    for event in (*events).iter_mut() {
                                                        event.mouse_x = lx;
                                                        event.mouse_y = ly;
                                                    }
                                                }
                                            }
                                            MotionAction::Move => {
                                                router.on_mouse_move(tree, lx, ly);

                                                if let Some((center_x, center_y, span)) = pinch_info
                                                {
                                                    if let Some(prev_span) = pinch_prev_span {
                                                        if prev_span > 0.0 && span > 0.0 {
                                                            let mut scale_ratio = span / prev_span;
                                                            if scale_ratio.is_finite() {
                                                                scale_ratio =
                                                                    scale_ratio.clamp(0.90, 1.10);
                                                                let zoom_delta_y =
                                                                    -(scale_ratio.ln()) / 0.0015;
                                                                pinch_scroll_info = Some((
                                                                    center_x,
                                                                    center_y,
                                                                    zoom_delta_y,
                                                                ));
                                                            }
                                                        }
                                                        pinch_prev_span = Some(span);
                                                    } else {
                                                        pinch_prev_span = Some(span);
                                                        last_touch_x = None;
                                                        last_touch_y = None;
                                                        is_scrolling = false;
                                                    }
                                                } else {
                                                    // Calculate scroll delta from touch movement
                                                    // Touch: dragging down = positive delta = content scrolls up (shows below)
                                                    if let (Some(prev_x), Some(prev_y)) =
                                                        (last_touch_x, last_touch_y)
                                                    {
                                                        let delta_x = lx - prev_x;
                                                        let delta_y = ly - prev_y;

                                                        // Only collect scroll if there's actual movement
                                                        // Small threshold to avoid jitter
                                                        if delta_x.abs() > 0.5
                                                            || delta_y.abs() > 0.5
                                                        {
                                                            is_scrolling = true;
                                                            // Store scroll info for dispatch after event loop
                                                            scroll_info =
                                                                Some((lx, ly, delta_x, delta_y));
                                                            tracing::trace!(
                                                                "Touch scroll: delta=({:.1}, {:.1})",
                                                                delta_x,
                                                                delta_y
                                                            );
                                                        }
                                                    }

                                                    // Update last touch position
                                                    last_touch_x = Some(lx);
                                                    last_touch_y = Some(ly);
                                                }

                                                unsafe {
                                                    let events = &mut pending_events
                                                        as *mut Vec<PendingEvent>;
                                                    for event in (*events).iter_mut() {
                                                        event.mouse_x = lx;
                                                        event.mouse_y = ly;
                                                    }
                                                }
                                            }
                                            MotionAction::Up | MotionAction::PointerUp => {
                                                tracing::debug!(
                                                    "Touch UP at logical ({:.1}, {:.1})",
                                                    lx,
                                                    ly
                                                );
                                                router.on_mouse_up(tree, lx, ly, MouseButton::Left);

                                                // Mark touch ended for scroll physics
                                                if is_scrolling {
                                                    touch_ended = true;
                                                }
                                                // Clear touch tracking
                                                last_touch_x = None;
                                                last_touch_y = None;
                                                is_scrolling = false;
                                                pinch_prev_span = None;

                                                unsafe {
                                                    let events = &mut pending_events
                                                        as *mut Vec<PendingEvent>;
                                                    for event in (*events).iter_mut() {
                                                        event.mouse_x = lx;
                                                        event.mouse_y = ly;
                                                    }
                                                }
                                            }
                                            MotionAction::Cancel => {
                                                tracing::debug!("Touch CANCEL");
                                                router.on_mouse_leave();
                                                // Clear touch tracking on cancel too
                                                last_touch_x = None;
                                                last_touch_y = None;
                                                if is_scrolling {
                                                    touch_ended = true;
                                                }
                                                is_scrolling = false;
                                                pinch_prev_span = None;
                                            }
                                            _ => {}
                                        }
                                        InputStatus::Handled
                                    } else {
                                        InputStatus::Unhandled
                                    }
                                }
                                _ => InputStatus::Unhandled,
                            }
                        }) {
                            // Event was processed, continue loop
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get input events iterator: {:?}", e);
                    }
                }

                // Clear the callback
                router.clear_event_callback();
            } else {
                // Log when we can't process input (missing context or tree)
                if ctx.is_none() {
                    tracing::trace!("Input: ctx is None");
                }
                if render_tree.is_none() {
                    tracing::trace!("Input: render_tree is None");
                }
            }

            // Dispatch collected events to the tree (critical for click handlers!)
            if !pending_events.is_empty() {
                if let Some(ref mut tree) = render_tree {
                    for event in pending_events {
                        tracing::debug!(
                            "Dispatching event: node={:?}, type={}, pos=({:.1}, {:.1})",
                            event.node_id,
                            event.event_type,
                            event.mouse_x,
                            event.mouse_y
                        );
                        tree.dispatch_event(
                            event.node_id,
                            event.event_type,
                            event.mouse_x,
                            event.mouse_y,
                        );
                    }
                }
            }

            // Dispatch pinch zoom as scroll without time (scroll_time=None)
            if let Some((mouse_x, mouse_y, delta_y)) = pinch_scroll_info {
                if let (Some(ref mut windowed_ctx), Some(ref mut tree)) =
                    (&mut ctx, &mut render_tree)
                {
                    let router = &mut windowed_ctx.event_router;
                    if let Some(hit) = router.hit_test(tree, mouse_x, mouse_y) {
                        let mut has_scroll_container =
                            tree.get_scroll_direction(hit.node).is_some();
                        if !has_scroll_container {
                            for ancestor in &hit.ancestors {
                                if tree.get_scroll_direction(*ancestor).is_some() {
                                    has_scroll_container = true;
                                    break;
                                }
                            }
                        }

                        if has_scroll_container {
                            tracing::trace!(
                                "Skipping pinch zoom over scroll container: hit={:?}",
                                hit.node
                            );
                        } else {
                            tracing::debug!(
                                "Dispatching pinch zoom: hit={:?}, delta_y={:.3}",
                                hit.node,
                                delta_y
                            );
                            tree.dispatch_scroll_chain(
                                hit.node,
                                &hit.ancestors,
                                mouse_x,
                                mouse_y,
                                0.0,
                                delta_y,
                            );
                            needs_redraw_next_frame = true;
                        }
                    }
                }
            }

            // Dispatch scroll events (touch scrolling)
            // NOTE: Do NOT set needs_rebuild here - that triggers full UI rebuild!
            // Scroll just updates internal offset and needs redraw, not rebuild.
            if let Some((mouse_x, mouse_y, delta_x, delta_y)) = scroll_info {
                if let (Some(ref mut windowed_ctx), Some(ref mut tree)) =
                    (&mut ctx, &mut render_tree)
                {
                    let router = &mut windowed_ctx.event_router;
                    // Hit test to get node chain for nested scroll dispatch
                    if let Some(hit) = router.hit_test(tree, mouse_x, mouse_y) {
                        // Get current time for velocity tracking (momentum scrolling)
                        let scroll_time = blinc_layout::prelude::elapsed_ms() as f64;
                        tracing::debug!(
                            "Dispatching scroll: hit={:?}, delta=({:.1}, {:.1})",
                            hit.node,
                            delta_x,
                            delta_y
                        );
                        tree.dispatch_scroll_chain_with_time(
                            hit.node,
                            &hit.ancestors,
                            mouse_x,
                            mouse_y,
                            delta_x,
                            delta_y,
                            scroll_time,
                        );
                        // Trigger redraw (NOT rebuild)
                        needs_redraw_next_frame = true;
                    }
                }
            }

            // Handle touch end - notify scroll physics for bounce/momentum
            if touch_ended {
                if let Some(ref mut tree) = render_tree {
                    tracing::debug!("Touch ended - notifying scroll physics");
                    tree.on_scroll_end();
                    // Trigger redraw for bounce animation (NOT rebuild)
                    needs_redraw_next_frame = true;
                }
            }

            // Tick scroll physics for momentum/bounce animations
            let scroll_animating = if let Some(ref mut tree) = render_tree {
                let current_time = blinc_layout::prelude::elapsed_ms();
                let animating = tree.tick_scroll_physics(current_time);
                tree.process_pending_scroll_refs();
                animating
            } else {
                false
            };
            if scroll_animating {
                needs_redraw_next_frame = true;
            }

            // =========================================================
            // PHASE 1: Check for incremental updates (prop changes, subtree rebuilds)
            // This avoids full rebuild for simple state changes
            // =========================================================
            let mut needs_redraw = false;

            // Check if stateful elements requested a redraw (hover/press/state changes)
            let has_stateful_updates = blinc_layout::take_needs_redraw();
            let has_pending_rebuilds = blinc_layout::has_pending_subtree_rebuilds();

            if has_stateful_updates || has_pending_rebuilds {
                if has_stateful_updates {
                    tracing::debug!("Redraw requested by: stateful state change");
                }

                // Get all pending prop updates
                let prop_updates = blinc_layout::take_pending_prop_updates();
                let had_prop_updates = !prop_updates.is_empty();

                // Apply prop updates to the tree
                if let Some(ref mut tree) = render_tree {
                    for (node_id, props) in &prop_updates {
                        tree.update_render_props(*node_id, |p| *p = props.clone());
                    }
                }

                // Process subtree rebuilds
                let mut needs_layout = false;
                if let Some(ref mut tree) = render_tree {
                    needs_layout = tree.process_pending_subtree_rebuilds();
                }

                if needs_layout {
                    if let Some(ref mut tree) = render_tree {
                        if let Some(ref windowed_ctx) = ctx {
                            tracing::debug!("Subtree rebuilds processed, recomputing layout");
                            tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                        }
                    }
                }

                if had_prop_updates && !needs_layout {
                    tracing::trace!("Visual-only prop updates, skipping layout");
                }

                needs_redraw = true;
            }

            // Check dirty flag from State::set() calls
            if ref_dirty_flag.swap(false, Ordering::SeqCst) {
                tracing::debug!("Rebuild triggered by: ref_dirty_flag (State::set)");
                needs_rebuild = true;
            }

            // Check if tree was marked dirty by event handlers
            if let Some(ref tree) = render_tree {
                if tree.needs_rebuild() {
                    tracing::debug!("Rebuild triggered by: tree.needs_rebuild()");
                    needs_rebuild = true;
                }
            }

            // Tick animations on main thread (single-threaded for mobile)
            let animations_active = {
                if let Ok(mut sched) = animations.lock() {
                    sched.tick()
                } else {
                    false
                }
            };
            if animations_active {
                needs_redraw = true;
                needs_redraw_next_frame = true;
            }

            // =========================================================
            // PHASE 2: Full rebuild only when structure changes
            // =========================================================
            static REBUILD_COUNT: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(0);
            if needs_rebuild && focused {
                let count = REBUILD_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if count % 60 == 0 {
                    tracing::warn!("REBUILD #{} (every 60th logged)", count);
                }
                if let (
                    Some(ref mut app_instance),
                    Some(ref surf),
                    Some(ref config),
                    Some(ref mut windowed_ctx),
                    Some(ref rs),
                ) = (
                    &mut blinc_app,
                    &surface,
                    &surface_config,
                    &mut ctx,
                    &render_state,
                ) {
                    // Build UI
                    let element = ui_builder(windowed_ctx);

                    // Create or update render tree
                    if render_tree.is_none() {
                        // First time: create tree
                        let mut tree = RenderTree::from_element(&element);
                        tree.set_scale_factor(windowed_ctx.scale_factor as f32);
                        tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                        tree.clear_dirty(); // Start clean
                        render_tree = Some(tree);
                    } else if let Some(ref mut tree) = render_tree {
                        // Full rebuild
                        *tree = RenderTree::from_element(&element);
                        tree.set_scale_factor(windowed_ctx.scale_factor as f32);
                        tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                        // Clear dirty on the NEW tree to prevent immediate re-rebuild
                        tree.clear_dirty();
                    }
                    needs_redraw = true;
                }
                // Reset rebuild flag after successful rebuild
                needs_rebuild = false;
            }

            // =========================================================
            // PHASE 3: Render if we need redraw
            // =========================================================
            static REDRAW_COUNT: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(0);
            if needs_redraw && focused {
                let count = REDRAW_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if count % 120 == 0 {
                    tracing::info!("REDRAW #{} (every 120th logged)", count);
                }
                if let (
                    Some(ref mut app_instance),
                    Some(ref surf),
                    Some(ref config),
                    Some(ref mut windowed_ctx),
                    Some(ref rs),
                    Some(ref tree),
                ) = (
                    &mut blinc_app,
                    &surface,
                    &surface_config,
                    &mut ctx,
                    &render_state,
                    &render_tree,
                ) {
                    // Render
                    match surf.get_current_texture() {
                        Ok(output) => {
                            let view = output.texture.create_view(&Default::default());
                            if let Err(e) = app_instance.render_tree_with_motion(
                                tree,
                                rs,
                                &view,
                                config.width,
                                config.height,
                            ) {
                                tracing::error!("Render error: {}", e);
                            }
                            output.present();
                        }
                        Err(wgpu::SurfaceError::Lost) => {
                            surf.configure(&app_instance.device(), config);
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            tracing::error!("Out of GPU memory");
                            running = false;
                        }
                        Err(e) => {
                            tracing::error!("Surface error: {:?}", e);
                        }
                    }

                    // Increment rebuild count
                    windowed_ctx.rebuild_count += 1;

                    // Execute ready callbacks after first rebuild
                    if windowed_ctx.rebuild_count == 1 {
                        if let Ok(mut callbacks) = ready_callbacks.lock() {
                            for callback in callbacks.drain(..) {
                                callback();
                            }
                        }
                    }
                }

                needs_rebuild = false;
            }

            // =========================================================
            // PHASE 4: Check if we need another frame for animations
            // =========================================================
            {
                // Check animation scheduler for active animations
                if let Ok(scheduler) = animations.lock() {
                    if scheduler.has_active_animations() {
                        needs_redraw_next_frame = true;
                    }
                }

                // Check for animating stateful elements (spring animations, state transitions)
                if blinc_layout::has_animating_statefuls() {
                    needs_redraw_next_frame = true;
                }

                // Check for pending subtree rebuilds that might need processing
                if blinc_layout::has_pending_subtree_rebuilds() {
                    needs_redraw_next_frame = true;
                }
            }
        }

        tracing::info!("AndroidApp::run exiting");
        Ok(())
    }

    /// Initialize GPU with a native window
    fn init_gpu(window: &NativeWindow) -> Result<(BlincApp, wgpu::Surface<'static>)> {
        use blinc_gpu::{GpuRenderer, RendererConfig, TextRenderingContext};

        let config = crate::BlincConfig::default();

        let renderer_config = RendererConfig {
            max_primitives: config.max_primitives,
            max_glass_primitives: config.max_glass_primitives,
            max_glyphs: config.max_glyphs,
            sample_count: 1,
            texture_format: None,
            unified_text_rendering: true,
        };

        // Create instance with Vulkan backend
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });

        // Create surface from native window using raw handles
        // Safety: The native window handle is valid for the lifetime of the window
        use raw_window_handle::{
            AndroidDisplayHandle, AndroidNdkWindowHandle, RawDisplayHandle, RawWindowHandle,
        };
        use std::ptr::NonNull;

        let raw_window = NonNull::new(window.ptr().as_ptr() as *mut std::ffi::c_void)
            .ok_or_else(|| BlincError::GpuInit("Invalid native window pointer".to_string()))?;

        let window_handle = AndroidNdkWindowHandle::new(raw_window);
        let display_handle = AndroidDisplayHandle::new();

        let surface_target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: RawDisplayHandle::Android(display_handle),
            raw_window_handle: RawWindowHandle::AndroidNdk(window_handle),
        };

        let surface = unsafe {
            instance
                .create_surface_unsafe(surface_target)
                .map_err(|e| BlincError::GpuInit(e.to_string()))?
        };

        // Create renderer
        let renderer = pollster::block_on(async {
            GpuRenderer::with_instance_and_surface(instance, &surface, renderer_config).await
        })
        .map_err(|e| BlincError::GpuInit(e.to_string()))?;

        let device = renderer.device_arc();
        let queue = renderer.queue_arc();

        let mut text_ctx = TextRenderingContext::new(device.clone(), queue.clone());

        // Load Android system fonts
        let mut fonts_loaded = 0;
        for font_path in crate::system_font_paths() {
            let path = std::path::Path::new(font_path);
            tracing::debug!("Checking font path: {}", font_path);
            if path.exists() {
                match std::fs::read(path) {
                    Ok(data) => {
                        tracing::info!("Loading font from: {} ({} bytes)", font_path, data.len());
                        match text_ctx.load_font_data(data) {
                            Ok(_) => {
                                tracing::info!("Successfully loaded font: {}", font_path);
                                fonts_loaded += 1;
                            }
                            Err(e) => {
                                tracing::warn!("Failed to load font {}: {:?}", font_path, e);
                            }
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
        tracing::info!("Loaded {} system fonts", fonts_loaded);

        // Preload common fonts
        text_ctx.preload_fonts(&["Roboto", "Noto Sans", "Droid Sans"]);
        text_ctx.preload_generic_styles(blinc_gpu::GenericFont::SansSerif, &[400, 700], false);
        tracing::info!("Font preloading complete");

        let ctx = crate::context::RenderContext::new(
            renderer,
            text_ctx,
            device,
            queue,
            config.sample_count,
        );
        let app = BlincApp::from_context(ctx, config);

        Ok((app, surface))
    }
}
