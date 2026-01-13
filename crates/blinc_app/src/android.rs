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

use android_activity::{AndroidApp as NdkAndroidApp, MainEvent, PollEvent};
use ndk::native_window::NativeWindow;

use blinc_animation::AnimationScheduler;
use blinc_core::context_state::{BlincContextState, HookState, SharedHookState};
use blinc_core::reactive::{ReactiveGraph, SignalId};
use blinc_layout::overlay_state::OverlayContext;
use blinc_layout::prelude::*;
use blinc_layout::widgets::overlay::{overlay_manager, OverlayManager};
use blinc_platform::assets::set_global_asset_loader;
use blinc_platform_android::{AndroidAssetLoader, AndroidWakeProxy};

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
    fn init_asset_loader(app: &NdkAndroidApp) {
        if let Some(asset_manager) = app.asset_manager() {
            let loader = AndroidAssetLoader::new(asset_manager);
            let _ = set_global_asset_loader(Box::new(loader));
        }
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

    /// Initialize Android logging
    fn init_logging() {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Debug)
                .with_tag("Blinc"),
        );
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
        Self::init_asset_loader(&app);

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

        // Set up wake proxy for Android - this allows the animation thread to wake the event loop
        // The ForeignLooper is obtained from the current thread (the main event loop thread)
        if let Some(wake_proxy) = AndroidWakeProxy::new() {
            tracing::info!("Android WakeProxy enabled for animations");
            scheduler.set_wake_callback(move || wake_proxy.wake());
        } else {
            tracing::warn!("Failed to create Android WakeProxy - using polling fallback");
        }

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

        // Application state
        let mut blinc_app: Option<BlincApp> = None;
        let mut surface: Option<wgpu::Surface<'static>> = None;
        let mut surface_config: Option<wgpu::SurfaceConfiguration> = None;
        let mut ctx: Option<WindowedContext> = None;
        let mut render_tree: Option<RenderTree> = None;
        let mut render_state: Option<blinc_layout::RenderState> = None;
        let mut native_window: Option<NativeWindow> = None;
        let mut needs_rebuild = true;
        let mut running = true;
        let mut focused = false;

        tracing::info!("Entering Android event loop");

        while running {
            // Poll for events with 16ms timeout (~60fps)
            app.poll_events(Some(std::time::Duration::from_millis(16)), |event| {
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
                        // Animation thread wake - request redraw
                        needs_rebuild = true;
                    }

                    _ => {}
                }
            });

            // Check for input events
            if let Some(ref mut windowed_ctx) = ctx {
                // Process touch events from android-activity
                // TODO: Implement touch event routing through EventRouter
            }

            // Check dirty flag
            if ref_dirty_flag.swap(false, Ordering::SeqCst) {
                needs_rebuild = true;
            }

            // Tick animations
            let animations_active = {
                if let Ok(mut sched) = animations.lock() {
                    sched.tick()
                } else {
                    false
                }
            };
            if animations_active {
                needs_rebuild = true;
            }

            // Render if we have everything and need to rebuild
            if needs_rebuild && focused {
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
                    let tree =
                        render_tree.get_or_insert_with(|| RenderTree::from_element(&element));
                    tree.rebuild(&element);
                    tree.compute_layout(windowed_ctx.width, windowed_ctx.height);

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

                    needs_rebuild = false;
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
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });

        // Create surface from native window
        // Safety: The native window handle is valid for the lifetime of the window
        let surface_target = wgpu::SurfaceTargetUnsafe::from_window(window)
            .map_err(|e| BlincError::GpuInit(format!("Failed to get window handle: {}", e)))?;
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
        for font_path in crate::system_font_paths() {
            let path = std::path::Path::new(font_path);
            if path.exists() {
                if let Ok(data) = std::fs::read(path) {
                    let _ = text_ctx.load_font_data(data);
                    break;
                }
            }
        }

        // Preload common fonts
        text_ctx.preload_fonts(&["Roboto", "Noto Sans", "Droid Sans"]);
        text_ctx.preload_generic_styles(blinc_gpu::GenericFont::SansSerif, &[400, 700], false);

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
