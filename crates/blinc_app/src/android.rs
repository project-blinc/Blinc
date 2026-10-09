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
    Arc, Mutex,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

/// Latest soft-keyboard inset reported by the JVM in **logical pixels**.
///
/// Set from Kotlin via the JNI export
/// `Java_com_blinc_BlincNativeBridge_nativeDispatchKeyboardInset` (defined
/// below in `blinc_app::android`), which `BlincNativeBridge` invokes from
/// its `setOnApplyWindowInsetsListener` whenever
/// `WindowInsets.Type.ime().bottom` changes. The Kotlin side does the
/// `pixels_raw / display_density` division before pushing, so this value
/// is directly comparable to `WindowedContext.height`.
///
/// `i32` because `AtomicF32` isn't in stable std; we round to the nearest
/// pixel on the Kotlin side, which is more than enough resolution for
/// keyboard-inset triggered scroll-into-view.
///
/// `-1` is the sentinel meaning "no value pushed yet" (so a stale `0`
/// from a previous run can't accidentally suppress the first real
/// notification). The android_main loop converts `-1` to `0.0`.
static PENDING_IME_INSET_PX: AtomicI32 = AtomicI32::new(-1);

/// Latest system-bar safe-area insets (notch, status bar, navigation bar,
/// gesture bar) reported by `WindowInsets` in **logical pixels**.
///
/// Set from Kotlin via `Java_com_blinc_BlincNativeBridge_nativeDispatchSafeArea`,
/// which `BlincNativeBridge` invokes from its shared
/// `setOnApplyWindowInsetsListener`. The Kotlin side reads
/// `WindowInsets.Type.systemBars()` (API 30+) or
/// `WindowInsets.systemWindowInset{Top,Right,Bottom,Left}` (API 24–29),
/// divides by display density, and pushes the four logical-pixel values
/// here.
///
/// `-1` is the sentinel meaning "no value pushed yet". The android_main
/// loop diffs these atomics against `last_applied_safe_area_*` on every
/// tick and, if any value changed, writes the new tuple into
/// `WindowedContext.safe_area`.
static PENDING_SAFE_AREA_TOP_PX: AtomicI32 = AtomicI32::new(-1);
static PENDING_SAFE_AREA_RIGHT_PX: AtomicI32 = AtomicI32::new(-1);
static PENDING_SAFE_AREA_BOTTOM_PX: AtomicI32 = AtomicI32::new(-1);
static PENDING_SAFE_AREA_LEFT_PX: AtomicI32 = AtomicI32::new(-1);

/// Queue of synthesized key-down events waiting to be dispatched on
/// the next android_main poll loop tick.
///
/// JNI handlers run on whichever thread Kotlin called them from
/// (typically the UI thread for `BlincNativeBridge` callbacks), but
/// the `RenderTree` lives on the `android_main` thread. We can't
/// touch the tree from the JNI handler directly, so the handler
/// pushes a `(key_code, modifiers)` tuple here and android_main
/// drains the queue every tick, dispatching each entry through
/// `tree.broadcast_key_event`.
///
/// Used by the native edit menu (Cut / Copy / Paste / Select All)
/// callbacks in `BlincEditMenuHelper.kt` to route into the existing
/// Cmd-shortcut branch of every Blinc text-editable widget's
/// `on_key_down` handler.
static PENDING_KEY_DOWN_EVENTS: Mutex<Vec<(u32, u32)>> = Mutex::new(Vec::new());

use android_activity::input::{
    InputEvent as AndroidInputEvent, KeyAction, KeyMapChar, Keycode, MotionAction,
};
use android_activity::{AndroidApp as NdkAndroidApp, InputStatus, MainEvent, PollEvent};
use ndk::native_window::NativeWindow;

use blinc_animation::AnimationScheduler;
use blinc_core::context_state::{BlincContextState, HookState, SharedHookState};
use blinc_core::reactive::{ReactiveGraph, SignalId};
use blinc_layout::event_router::MouseButton;
use blinc_layout::overlay_state::OverlayContext;
use blinc_layout::prelude::*;
use blinc_layout::widgets::overlay::{OverlayManager, overlay_manager};
use blinc_platform::assets::set_global_asset_loader;
use blinc_platform_android::AndroidAssetLoader;
use blinc_platform_android::input::{PinchPhase, PinchState, TouchPointer, detect_pinch};

use crate::app::BlincApp;
use crate::error::{BlincError, Result};
use crate::windowed::{
    RefDirtyFlag, SharedAnimationScheduler, SharedElementRegistry, SharedReactiveGraph,
    SharedReadyCallbacks, WindowedContext,
};

struct AndroidSurfaceState {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    width: u32,
    height: u32,
    logical_width: f32,
    logical_height: f32,
    scale_factor: f64,
}

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
            ThemeState, detect_system_color_scheme, platform_theme_bundle, set_redraw_callback,
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

    fn force_opaque_window_format(window: &NativeWindow) {
        // Force the underlying ANativeWindow to use an opaque (no-alpha)
        // pixel format BEFORE we create the wgpu/Vulkan swapchain on it.
        //
        // Why: NativeActivity windows default to a TRANSLUCENT pixel
        // format on modern Android, which makes SurfaceFlinger composite
        // our framebuffer using its alpha channel. On the Pixel 10 Pro /
        // Tensor G5 PowerVR Vulkan driver this combines with `Inherit`
        // composite alpha to produce a fully invisible window even though
        // wgpu is rendering opaque content. `R8G8B8X8_UNORM` (the modern
        // alias of the legacy `WINDOW_FORMAT_RGBX_8888`) tells the
        // compositor "this surface has no alpha — treat every pixel as
        // opaque". The Java-side `window.setFormat(PixelFormat.OPAQUE)`
        // we set in MainActivity is silently overridden once the
        // NativeActivity's native window comes up, so this NDK-side call
        // (which lives in the same process and runs after InitWindow) is
        // the authoritative one. Calling it on a window that's already
        // RGBA8888 is harmless on devices where the bug doesn't apply.
        if let Err(e) = window.set_buffers_geometry(
            0,
            0,
            Some(ndk::hardware_buffer_format::HardwareBufferFormat::R8G8B8X8_UNORM),
        ) {
            tracing::warn!(
                "ANativeWindow_setBuffersGeometry(R8G8B8X8_UNORM) failed: {} \
                — surface may composite with alpha and appear blank on PowerVR-class GPUs",
                e
            );
        } else {
            tracing::info!("ANativeWindow buffer format forced to R8G8B8X8_UNORM (opaque)");
        }
    }

    fn surface_target_from_native_window(
        window: &NativeWindow,
    ) -> Result<wgpu::SurfaceTargetUnsafe> {
        use raw_window_handle::{
            AndroidDisplayHandle, AndroidNdkWindowHandle, RawDisplayHandle, RawWindowHandle,
        };
        use std::ptr::NonNull;

        let raw_window = NonNull::new(window.ptr().as_ptr() as *mut std::ffi::c_void)
            .ok_or_else(|| BlincError::GpuInit("Invalid native window pointer".to_string()))?;

        let window_handle = AndroidNdkWindowHandle::new(raw_window);
        let display_handle = AndroidDisplayHandle::new();

        Ok(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: RawDisplayHandle::Android(display_handle),
            raw_window_handle: RawWindowHandle::AndroidNdk(window_handle),
        })
    }

    fn android_surface_config(
        app_instance: &BlincApp,
        width: u32,
        height: u32,
    ) -> wgpu::SurfaceConfiguration {
        // Use `Inherit` rather than `Auto`.
        //
        // The Pixel 10 Pro / Tensor G5 PowerVR Vulkan driver
        // (25.1@6794074) ONLY reports `[Inherit]` as a supported
        // composite alpha mode — `Opaque` is rejected at
        // `Surface::configure` with a validation error. `Auto` also
        // resolves to `Inherit` here, but goes through a code path that
        // produced a blank/black surface. Forcing `Inherit` explicitly
        // works around it. Per the Vulkan spec
        // (VK_COMPOSITE_ALPHA_INHERIT_BIT_KHR), the application is
        // responsible for configuring the host window's alpha treatment;
        // `force_opaque_window_format` handles that NDK-side.
        let format = app_instance.texture_format();
        let alpha_mode = wgpu::CompositeAlphaMode::Inherit;
        tracing::info!(
            "Android surface: format={:?}, alpha_mode={:?}, size={}x{}",
            format,
            alpha_mode,
            width,
            height,
        );

        wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        }
    }

    fn create_surface_for_native_window(
        app_instance: &BlincApp,
        window: &NativeWindow,
    ) -> Result<wgpu::Surface<'static>> {
        Self::force_opaque_window_format(window);
        let surface_target = Self::surface_target_from_native_window(window)?;
        unsafe { app_instance.create_surface_unsafe(surface_target) }
            .map_err(|e| BlincError::GpuInit(e.to_string()))
    }

    fn attach_native_window(
        app_instance: &BlincApp,
        app: &NdkAndroidApp,
        window: &NativeWindow,
    ) -> Result<AndroidSurfaceState> {
        let width = window.width() as u32;
        let height = window.height() as u32;
        if width == 0 || height == 0 {
            return Err(BlincError::GpuInit(format!(
                "Invalid native window size: {}x{}",
                width, height
            )));
        }

        let surface = Self::create_surface_for_native_window(app_instance, window)?;
        let config = Self::android_surface_config(app_instance, width, height);
        surface.configure(app_instance.device(), &config);

        let scale_factor = blinc_platform_android::get_display_density(app);
        let logical_width = width as f32 / scale_factor as f32;
        let logical_height = height as f32 / scale_factor as f32;

        Ok(AndroidSurfaceState {
            surface,
            config,
            width,
            height,
            logical_width,
            logical_height,
            scale_factor,
        })
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

        // Persistent GPU pipeline-cache location (app-private storage). The
        // first launch compiles ~18 Vulkan pipelines serially (the one-time
        // "black screen" on cold install); wgpu::PipelineCache reloads them on
        // subsequent launches. Borrowed here because `app` is mutably borrowed
        // by `poll_events` inside the loop.
        let pipeline_cache_path = app
            .internal_data_path()
            .map(|p| p.join("blinc_pipeline_cache.bin"));

        // Initialize the text measurer
        blinc_layout::init_text_measurer();

        // Initialize the theme system
        Self::init_theme();

        // Shared state — process-global reactive graph so bare
        // `signal()` interops with `State<T>` / `Stateful::deps`.
        let ref_dirty_flag: RefDirtyFlag = Arc::new(AtomicBool::new(false));
        let reactive: SharedReactiveGraph = blinc_core::reactive::global_graph();
        let hooks: SharedHookState = Arc::new(Mutex::new(HookState::new()));

        // Initialize global context state singleton
        if !BlincContextState::is_initialized() {
            #[allow(clippy::type_complexity)]
            let stateful_callback: Arc<dyn Fn(&[SignalId]) + Send + Sync> =
                Arc::new(|signal_ids| {
                    blinc_layout::check_stateful_deps(signal_ids);
                });
            BlincContextState::init_with_callback(
                Arc::clone(&reactive),
                Arc::clone(&hooks),
                Arc::clone(&ref_dirty_flag),
                stateful_callback.clone(),
            );
            blinc_core::reactive::set_stateful_deps_notifier(move |ids| stateful_callback(ids));
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
        let mut last_frame_time_ms: u64 = 0;
        let mut running = true;
        let mut focused = false;
        // Latest keyboard inset already pushed into `windowed_ctx.keyboard_inset`.
        // The poll loop reads `PENDING_IME_INSET_PX` (set from Kotlin via the
        // `Java_com_blinc_BlincNativeBridge_nativeDispatchKeyboardInset` JNI
        // export) and only triggers `scroll_focused_text_input_above_keyboard`
        // when the value changes — otherwise we'd re-clamp the focused
        // input's container scroll offset every vsync tick and fight the
        // user trying to pan around.
        let mut last_applied_keyboard_inset_px: i32 = -1;
        // Last system-bar safe-area insets we copied into `WindowedContext.safe_area`.
        // Diffed against `PENDING_SAFE_AREA_*_PX` every tick; when any
        // edge changes we push the full tuple into the context. `-1`
        // until the first dispatch from Kotlin arrives.
        let mut last_applied_safe_area_top_px: i32 = -1;
        let mut last_applied_safe_area_right_px: i32 = -1;
        let mut last_applied_safe_area_bottom_px: i32 = -1;
        let mut last_applied_safe_area_left_px: i32 = -1;
        // Last value of `text_input::focus_tap_generation()` we processed.
        // The widget bumps that counter on every `on_mouse_down` that
        // lands on a text input — that's the signal we use to detect
        // re-taps and same-frame focus swaps that
        // `take_keyboard_state_change` misses (because that flag only
        // fires on `0 → 1` / `1 → 0` focus-count transitions).
        let mut last_focus_tap_generation: u64 = 0;

        // Touch tracking for scroll delta calculation
        // On mobile, scroll happens via touch drag, not wheel events
        let mut last_touch_x: Option<f32> = None;
        let mut last_touch_y: Option<f32> = None;
        let mut is_scrolling = false;
        // Down position + move count for the current gesture, used to tell a
        // real tap (stationary, no moves) from a short/fast flick (which moves
        // but may not trip the per-move `is_scrolling` threshold). Only a true
        // tap should grab-to-stop a coast.
        let mut gesture_down: Option<(f32, f32)> = None;
        let mut gesture_moves: u32 = 0;
        let mut pinch_state = PinchState::default();

        tracing::info!("Entering Android event loop");

        while running {
            // When animating: don't wait - vsync in present() handles frame pacing
            // When idle: wait for events to save power
            // When a text-input long-press is armed: poll at ~30 Hz so
            // the 500 ms deadline fires within ~33 ms of the actual
            // deadline (the user holding their finger still on a text
            // input emits no input events, so without this clamp the
            // 100 ms idle poll could miss the deadline by up to 100 ms).
            let long_press_pending = blinc_layout::widgets::text_input::is_long_press_armed();
            let poll_timeout = if needs_rebuild || needs_redraw_next_frame {
                Some(std::time::Duration::ZERO) // Don't wait - vsync paces us
            } else if long_press_pending {
                Some(std::time::Duration::from_millis(33))
            } else {
                Some(std::time::Duration::from_millis(100)) // Idle - save power
            };
            let mut needs_redraw = needs_redraw_next_frame;
            needs_redraw_next_frame = false;

            app.poll_events(poll_timeout, |event| {
                match event {
                    PollEvent::Main(main_event) => match main_event {
                        MainEvent::InitWindow { .. } => {
                            tracing::info!("Native window initialized");
                            if let Some(window) = app.native_window() {
                                let initializing_gpu = blinc_app.is_none();

                                if initializing_gpu {
                                    match Self::init_gpu(&window, pipeline_cache_path.clone()) {
                                        Ok(app_instance) => {
                                            blinc_layout::init_text_measurer_with_registry(
                                                app_instance.font_registry(),
                                            );
                                            blinc_app = Some(app_instance);
                                            tracing::info!("GPU initialized successfully");
                                        }
                                        Err(e) => {
                                            tracing::error!("Failed to initialize GPU: {}", e);
                                            return;
                                        }
                                    }
                                }

                                if let Some(ref app_instance) = blinc_app {
                                    match Self::attach_native_window(app_instance, &app, &window) {
                                        Ok(surface_state) => {
                                            let AndroidSurfaceState {
                                                surface: new_surface,
                                                config: new_config,
                                                width,
                                                height,
                                                logical_width,
                                                logical_height,
                                                scale_factor,
                                            } = surface_state;

                                            surface = Some(new_surface);
                                            surface_config = Some(new_config);
                                            native_window = Some(window);

                                            if let Some(ref mut windowed_ctx) = ctx {
                                                let viewport_changed = (windowed_ctx.width
                                                    - logical_width)
                                                    .abs()
                                                    >= 0.5
                                                    || (windowed_ctx.height - logical_height).abs()
                                                        >= 0.5
                                                    || (windowed_ctx.scale_factor - scale_factor)
                                                        .abs()
                                                        >= 0.001;
                                                windowed_ctx.width = logical_width;
                                                windowed_ctx.height = logical_height;
                                                windowed_ctx.scale_factor = scale_factor;
                                                windowed_ctx.physical_width = width as f32;
                                                windowed_ctx.physical_height = height as f32;
                                                windowed_ctx.focused = focused;

                                                if let Some(ref mut tree) = render_tree {
                                                    tree.set_scale_factor(scale_factor as f32);
                                                }
                                                needs_rebuild |= viewport_changed;
                                            } else {
                                                // Initial safe_area = zeros; Kotlin's
                                                // setOnApplyWindowInsetsListener will push
                                                // the real values within the first vsync
                                                // via `nativeDispatchSafeArea`, and the
                                                // poll loop below copies them into the
                                                // context on the next tick.
                                                ctx = Some(WindowedContext::new_android(
                                                    logical_width,
                                                    logical_height,
                                                    scale_factor,
                                                    width as f32,
                                                    height as f32,
                                                    focused,
                                                    (0.0, 0.0, 0.0, 0.0),
                                                    Arc::clone(&animations),
                                                    Arc::clone(&ref_dirty_flag),
                                                    Arc::clone(&reactive),
                                                    Arc::clone(&hooks),
                                                    Arc::clone(&overlays),
                                                    Arc::clone(&element_registry),
                                                    Arc::clone(&ready_callbacks),
                                                ));
                                                needs_rebuild = true;
                                            }

                                            BlincContextState::get()
                                                .set_viewport_size(logical_width, logical_height);

                                            if render_state.is_none() {
                                                let mut rs = blinc_layout::RenderState::new(
                                                    Arc::clone(&animations),
                                                );
                                                rs.set_shared_motion_states(Arc::clone(
                                                    &shared_motion_states,
                                                ));
                                                render_state = Some(rs);
                                                needs_rebuild = true;
                                            }

                                            needs_rebuild |= render_tree.is_none();
                                            needs_redraw_next_frame = true;
                                        }
                                        Err(e) => {
                                            tracing::error!(
                                                "Failed to attach native window surface: {}",
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                        }

                        MainEvent::TerminateWindow { .. } => {
                            tracing::info!("Native window terminated");
                            // Android destroys only the ANativeWindow here.
                            // Keep the renderer, UI context, render tree, and
                            // motion state alive so returning from the
                            // background only recreates the surface.
                            focused = false;
                            if let Some(ref mut windowed_ctx) = ctx {
                                windowed_ctx.focused = false;
                                windowed_ctx.pointer_query.set_pressure(0.0);
                                windowed_ctx.pointer_query.set_touch_count(0);
                            }
                            blinc_layout::widgets::text_input::cancel_long_press_timer();
                            pinch_state.reset();
                            last_touch_x = None;
                            last_touch_y = None;
                            is_scrolling = false;
                            native_window = None;
                            surface = None;
                            surface_config = None;
                        }

                        MainEvent::WindowResized { .. } => {
                            if let Some(ref window) = native_window {
                                let width = window.width() as u32;
                                let height = window.height() as u32;
                                tracing::info!("Window resized: {}x{}", width, height);

                                if let (Some(app_instance), Some(surf), Some(config)) =
                                    (&blinc_app, &surface, &mut surface_config)
                                {
                                    if width > 0 && height > 0 {
                                        // Android emits a WindowResized on every
                                        // resume even when the dimensions are
                                        // unchanged. Only the actual-size-change
                                        // case needs a full UI rebuild; a same-size
                                        // resume just reconfigures the swapchain and
                                        // repaints the preserved tree. Rebuilding
                                        // unconditionally reconstructed the entire UI
                                        // (and flashed) on every resume.
                                        let size_changed =
                                            config.width != width || config.height != height;
                                        config.width = width;
                                        config.height = height;
                                        surf.configure(app_instance.device(), config);

                                        if size_changed {
                                            if let Some(ref mut windowed_ctx) = ctx {
                                                let scale_factor = windowed_ctx.scale_factor;
                                                windowed_ctx.width =
                                                    width as f32 / scale_factor as f32;
                                                windowed_ctx.height =
                                                    height as f32 / scale_factor as f32;

                                                BlincContextState::get().set_viewport_size(
                                                    windowed_ctx.width,
                                                    windowed_ctx.height,
                                                );
                                            }
                                            needs_rebuild = true;
                                        }
                                        needs_redraw_next_frame = true;
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
                            needs_redraw_next_frame = true;
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
                            if let Some(ref mut windowed_ctx) = ctx {
                                windowed_ctx.focused = true;
                            }
                            needs_redraw_next_frame = true;
                        }

                        MainEvent::Pause => {
                            tracing::info!("App paused");
                            focused = false;
                            if let Some(ref mut windowed_ctx) = ctx {
                                windowed_ctx.focused = false;
                            }
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
            // Track if touch ended (for scroll physics)
            let mut touch_ended = false;
            // A tap — touch up WITHOUT any drag — requests grab-to-stop of a
            // coasting list. A swipe must NOT stop momentum (it should carry
            // and accumulate across successive flicks), so this is set only on
            // a no-move release, carrying the up coords for a post-loop hit test.
            let mut tap_to_stop: Option<(f32, f32)> = None;

            if let (Some(windowed_ctx), Some(tree)) = (&mut ctx, &mut render_tree) {
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

                                    if pointer_count == 0 {
                                        if action == MotionAction::Cancel {
                                            tracing::debug!("Touch CANCEL");
                                            windowed_ctx.pointer_query.set_pressure(0.0);
                                            windowed_ctx.pointer_query.set_touch_count(0);
                                            router.on_mouse_leave(&*tree);
                                            pinch_state.reset();
                                            last_touch_x = None;
                                            last_touch_y = None;
                                            if is_scrolling {
                                                touch_ended = true;
                                            }
                                            is_scrolling = false;
                                            return InputStatus::Handled;
                                        }
                                        return InputStatus::Unhandled;
                                    }

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
                                        let pointers: Vec<TouchPointer> =
                                            (0..pointer_count)
                                                .map(|i| {
                                                    let p = motion_event.pointer_at_index(i);
                                                    TouchPointer {
                                                        id: p.pointer_id(),
                                                        x: p.x() / scale,
                                                        y: p.y() / scale,
                                                        pressure: p.pressure(),
                                                        size: p.size(),
                                                    }
                                                })
                                                .collect();

                                        // Forward pressure and touch count to pointer query
                                        if let Some(primary) = pointers.first() {
                                            windowed_ctx.pointer_query.set_pressure(primary.pressure);
                                        }
                                        windowed_ctx.pointer_query.set_touch_count(pointers.len() as u32);

                                        let pinch_gesture = detect_pinch(&pointers, &mut pinch_state);
                                        if let Some(gesture) = pinch_gesture {
                                            if matches!(gesture.phase, PinchPhase::Started | PinchPhase::Moved)
                                            {
                                                if let Some(hit) =
                                                    router.hit_test(tree, gesture.center.0, gesture.center.1)
                                                {
                                                    tree.dispatch_pinch_chain(
                                                        &hit,
                                                        gesture.center.0,
                                                        gesture.center.1,
                                                        gesture.scale,
                                                    );
                                                    needs_redraw_next_frame = true;
                                                }
                                            }

                                            if matches!(gesture.phase, PinchPhase::Started | PinchPhase::Ended)
                                            {
                                                last_touch_x = None;
                                                last_touch_y = None;
                                                is_scrolling = false;
                                            }
                                        }

                                        match action {
                                            MotionAction::Down | MotionAction::PointerDown => {
                                                tracing::debug!(
                                                    "Touch DOWN at logical ({:.1}, {:.1})",
                                                    lx,
                                                    ly
                                                );
                                                // Mark this event as touch input
                                                // so editable widgets can branch
                                                // their drag / double-tap logic
                                                // for mobile semantics. See
                                                // `widgets::text_input::is_touch_input`.
                                                blinc_layout::widgets::text_input::set_touch_input(true);
                                                // Blur any focused text inputs
                                                // BEFORE processing the touch.
                                                // The text input that gets
                                                // tapped re-focuses itself via
                                                // its own `on_mouse_down`
                                                // handler; tapping outside
                                                // any input clears focus,
                                                // which decrements the focus
                                                // count, which fires
                                                // `take_keyboard_state_change`
                                                // on the next frame so the
                                                // soft keyboard hides
                                                // automatically. Mirrors the
                                                // desktop runner at
                                                // `windowed.rs:2913` and the
                                                // iOS runner.
                                                blinc_layout::widgets::blur_all_text_inputs();
                                                router.on_mouse_down(
                                                    &*tree,
                                                    lx,
                                                    ly,
                                                    MouseButton::Left,
                                                );
                                                // Initialize touch tracking for scroll
                                                if pointer_count == 1 {
                                                    last_touch_x = Some(lx);
                                                    last_touch_y = Some(ly);
                                                    is_scrolling = false;
                                                    gesture_down = Some((lx, ly));
                                                    gesture_moves = 0;
                                                }
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
                                                router.on_mouse_move(&*tree, lx, ly);

                                                if pointer_count == 1 {
                                                    gesture_moves += 1;
                                                    // Calculate scroll delta from touch movement
                                                    // Touch: dragging down = positive delta = content scrolls up (shows below)
                                                    if let (Some(prev_x), Some(prev_y)) =
                                                        (last_touch_x, last_touch_y)
                                                    {
                                                        let delta_x = lx - prev_x;
                                                        let delta_y = ly - prev_y;

                                                        // Only collect scroll if there's actual movement
                                                        // Small threshold to avoid jitter
                                                        if delta_x.abs() > 0.5 || delta_y.abs() > 0.5 {
                                                            is_scrolling = true;
                                                            // ACCUMULATE across every move this
                                                            // frame. `poll_events` drains many
                                                            // MotionAction::Move events per loop
                                                            // (touch samples at 120-240 Hz vs a
                                                            // ~35 fps loop), and each delta is
                                                            // from the previous move. Overwriting
                                                            // dropped all but the last, so the
                                                            // content moved a fraction of the
                                                            // finger distance — the "scroll is
                                                            // slow / slower than desktop" report.
                                                            // Summing applies the full gesture and
                                                            // gives the physics the true velocity.
                                                            let (ax, ay) = scroll_info
                                                                .map(|(_, _, dx, dy)| (dx, dy))
                                                                .unwrap_or((0.0, 0.0));
                                                            scroll_info = Some((
                                                                lx,
                                                                ly,
                                                                ax + delta_x,
                                                                ay + delta_y,
                                                            ));
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
                                                // Cancel any armed text-input long-press
                                                // timer. Lifting the finger before the
                                                // 500 ms deadline means the user wasn't
                                                // trying to long-press.
                                                blinc_layout::widgets::text_input::cancel_long_press_timer();
                                                windowed_ctx.pointer_query.set_pressure(0.0);
                                                router.on_mouse_up(&*tree, lx, ly, MouseButton::Left);

                                                // Classify: only a real TAP — essentially
                                                // stationary (no scroll flag, ≤1 move, tiny
                                                // total displacement) — grabs-to-stop a coast.
                                                // A short or fast flick moves enough to be a
                                                // SWIPE and must keep/continue its momentum.
                                                let displacement = gesture_down
                                                    .map(|(dx, dy)| {
                                                        ((lx - dx).powi(2) + (ly - dy).powi(2))
                                                            .sqrt()
                                                    })
                                                    .unwrap_or(0.0);
                                                const TAP_SLOP: f32 = 12.0;
                                                let is_tap = !is_scrolling
                                                    && gesture_moves <= 1
                                                    && displacement < TAP_SLOP;
                                                if is_tap {
                                                    tap_to_stop = Some((lx, ly));
                                                } else {
                                                    touch_ended = true;
                                                }
                                                // Clear touch tracking
                                                last_touch_x = None;
                                                last_touch_y = None;
                                                is_scrolling = false;
                                                gesture_down = None;
                                                gesture_moves = 0;

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
                                                blinc_layout::widgets::text_input::cancel_long_press_timer();
                                                windowed_ctx.pointer_query.set_pressure(0.0);
                                                windowed_ctx.pointer_query.set_touch_count(0);
                                                router.on_mouse_leave(&*tree);
                                                pinch_state.reset();
                                                // Clear touch tracking on cancel too
                                                last_touch_x = None;
                                                last_touch_y = None;
                                                if is_scrolling {
                                                    touch_ended = true;
                                                }
                                                is_scrolling = false;
                                            }
                                            _ => {}
                                        }
                                        InputStatus::Handled
                                    } else {
                                        InputStatus::Unhandled
                                    }
                                }
                                AndroidInputEvent::KeyEvent(key_event) => {
                                    // Forward soft-keyboard / hardware-keyboard
                                    // input to the Rust text-input widget. The
                                    // mirror of the iOS path in `ios.rs::handle_text_input`
                                    // / `handle_key_down` — without this, the
                                    // Android IME pops up correctly (the runner
                                    // already calls `app.show_soft_input(true)`
                                    // when `take_keyboard_state_change()` returns
                                    // `Some(true)`) but every typed character is
                                    // silently dropped because nothing forwards
                                    // it through to `broadcast_text_input_event`.
                                    //
                                    // We only react to `KeyAction::Down`. Up
                                    // events are uninteresting for the
                                    // text-input widget which advances state on
                                    // press, not release.
                                    if key_event.action() == KeyAction::Down {
                                        let key_code = key_event.key_code();
                                        let meta_state = key_event.meta_state();

                                        // Map Android `Keycode` -> the virtual
                                        // key codes the desktop runner uses
                                        // (which the `text_input` widget's
                                        // `on_key_down` handler matches against
                                        // at line 1639 of `text_input.rs`):
                                        //   8  = Backspace / Delete
                                        //   13 = Enter / Return
                                        //   27 = Escape
                                        //   37/38/39/40 = ←/↑/→/↓
                                        //   36 = Home, 35 = End
                                        // Anything not in the table falls
                                        // through to the unicode-character
                                        // path below, which broadcasts as
                                        // TEXT_INPUT instead.
                                        let virtual_key = match key_code {
                                            Keycode::Del => Some(8u32),
                                            Keycode::Enter | Keycode::NumpadEnter => Some(13u32),
                                            Keycode::Escape => Some(27u32),
                                            Keycode::DpadLeft => Some(37u32),
                                            Keycode::DpadUp => Some(38u32),
                                            Keycode::DpadRight => Some(39u32),
                                            Keycode::DpadDown => Some(40u32),
                                            Keycode::MoveHome => Some(36u32),
                                            Keycode::MoveEnd => Some(35u32),
                                            _ => None,
                                        };

                                        if let Some(vkey) = virtual_key {
                                            tree.broadcast_key_event(
                                                blinc_core::events::event_types::KEY_DOWN,
                                                vkey,
                                                false,
                                                false,
                                                false,
                                                false,
                                            );
                                        } else {
                                            // Unicode character path. The
                                            // android-activity API exposes a
                                            // per-device `KeyCharacterMap` that
                                            // maps `(key_code, meta_state)` to
                                            // a unicode char (or a combining
                                            // accent). For now we ignore
                                            // combining accents and
                                            // forward only direct unicode
                                            // characters — full dead-key
                                            // composition would track an
                                            // accent buffer across events,
                                            // mirroring the desktop runner.
                                            if let Ok(map) = app.device_key_character_map(
                                                key_event.device_id(),
                                            ) {
                                                if let Ok(KeyMapChar::Unicode(ch)) =
                                                    map.get(key_code, meta_state)
                                                {
                                                    tree.broadcast_text_input_event(
                                                        ch, false, false, false, false,
                                                    );
                                                }
                                            }
                                        }
                                    }
                                    InputStatus::Handled
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
            //
            // Use `dispatch_event_full` (matches the desktop and iOS runners)
            // so the receiving handler sees `EventContext::local_x/local_y`
            // and `bounds_*` populated. The simpler `dispatch_event` only
            // forwards `mouse_x/mouse_y` and leaves the local-coordinate
            // fields at their default `0.0`, which silently broke any
            // handler that does click-to-position cursor placement,
            // hit-test math, or in-element coordinate work — most
            // visibly the `text_input` widget, which compiled
            // `cursor_position_from_x(0.0, _) == 0` on every tap and
            // dropped the caret at the start of the field on every
            // refocus. Look up the actual node bounds via
            // `EventRouter::get_node_bounds` so the local coordinates
            // remain correct even when the event has bubbled to an
            // ancestor whose bounds differ from the original hit target.
            if !pending_events.is_empty() {
                if let (Some(tree), Some(windowed_ctx)) = (&mut render_tree, &ctx) {
                    let router = &windowed_ctx.event_router;
                    for event in pending_events {
                        let (bounds_x, bounds_y, bounds_width, bounds_height) = router
                            .get_node_bounds(event.node_id)
                            .unwrap_or((0.0, 0.0, 0.0, 0.0));
                        let local_x = event.mouse_x - bounds_x;
                        let local_y = event.mouse_y - bounds_y;
                        tracing::debug!(
                            "Dispatching event: node={:?}, type={}, pos=({:.1}, {:.1}), local=({:.1}, {:.1})",
                            event.node_id,
                            event.event_type,
                            event.mouse_x,
                            event.mouse_y,
                            local_x,
                            local_y,
                        );
                        tree.dispatch_event_full(
                            event.node_id,
                            event.event_type,
                            event.mouse_x,
                            event.mouse_y,
                            local_x,
                            local_y,
                            bounds_x,
                            bounds_y,
                            bounds_width,
                            bounds_height,
                            0.0,   // drag_delta_x — touch drag uses scroll path
                            0.0,   // drag_delta_y
                            1.0,   // pinch_scale
                            false, // shift — no software-keyboard modifier tracking yet
                            false, // ctrl
                            false, // alt
                            false, // meta
                        );
                    }
                }
            }

            // Grab-to-stop, TAP ONLY: a no-move release on a coasting list
            // halts it (native "tap to stop"). A swipe deliberately does not
            // land here — its momentum carries and accumulates across flicks.
            if let Some((tap_x, tap_y)) = tap_to_stop {
                if let (Some(windowed_ctx), Some(tree)) = (&mut ctx, &mut render_tree) {
                    let router = &mut windowed_ctx.event_router;
                    if let Some(hit) = router.hit_test(tree, tap_x, tap_y) {
                        tree.cancel_scroll_animation_in_chain(hit.node, &hit.ancestors);
                    }
                }
            }

            // Dispatch scroll events (touch scrolling)
            // NOTE: Do NOT set needs_rebuild here - that triggers full UI rebuild!
            // Scroll just updates internal offset and needs redraw, not rebuild.
            if let Some((mouse_x, mouse_y, delta_x, delta_y)) = scroll_info {
                if let (Some(windowed_ctx), Some(tree)) = (&mut ctx, &mut render_tree) {
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

            // Tick scroll physics for momentum/bounce animations.
            // OR in `process_pending_scroll_refs`'s return value so a
            // freshly-fired `scroll_to_with_options` keeps the redraw
            // chain alive on its own frame (otherwise the spring sits
            // in Bouncing until the next stray input).
            let scroll_animating = if let Some(ref mut tree) = render_tree {
                let current_time = blinc_layout::prelude::elapsed_ms();
                let ticking = tree.tick_scroll_physics(current_time);
                let just_started = tree.process_pending_scroll_refs();
                ticking || just_started
            } else {
                false
            };
            if scroll_animating {
                needs_redraw_next_frame = true;
            }

            // =========================================================
            // Soft keyboard: show/hide based on text widget focus
            // =========================================================
            //
            // We prefer routing through the JVM `BlincNativeBridge`
            // (`keyboard.show` / `keyboard.hide`) which uses
            // `InputMethodManager.showSoftInput` against the Activity's
            // decor view. The NDK helper `ANativeActivity_showSoftInput`
            // (which `app.show_soft_input(true)` wraps) is famously
            // unreliable on modern Android — on most devices it silently
            // no-ops because the NativeActivity decor view is not
            // focusable in touch mode.
            //
            // The bridge is wired up by user code in `android_main` via
            // `blinc_platform_android::init_android_native_bridge(&app)`.
            // If it isn't initialized, we fall back to the unreliable NDK
            // path so apps that opt out of the bridge still get *something*.
            // Show / hide the soft keyboard when focus crosses the global
            // 0 / 1 boundary. `take_keyboard_state_change` returns
            // `Some(true)` on the first text input gaining focus and
            // `Some(false)` when the last focused input loses it; it does
            // NOT fire on re-taps or focus-swaps between two inputs.
            // That's fine for show/hide signaling — the keyboard is
            // already up in those cases. Re-tap detection lives below
            // via `focus_tap_generation`.
            if let Some(show) = blinc_layout::widgets::text_input::take_keyboard_state_change() {
                let bridge_ready = blinc_core::native_bridge::NativeBridgeState::is_initialized();
                let routed_via_bridge = if bridge_ready {
                    let result: blinc_core::native_bridge::NativeResult<()> =
                        blinc_core::native_bridge::native_call(
                            "keyboard",
                            if show { "show" } else { "hide" },
                            (),
                        );
                    match result {
                        Ok(()) => true,
                        Err(e) => {
                            tracing::warn!(
                                "BlincNativeBridge keyboard.{} failed: {:?} — falling back to NDK helper",
                                if show { "show" } else { "hide" },
                                e
                            );
                            false
                        }
                    }
                } else {
                    false
                };

                if !routed_via_bridge {
                    if show {
                        app.show_soft_input(true);
                    } else {
                        app.hide_soft_input(true);
                    }
                }
            }

            // =========================================================
            // System-bar safe-area insets → WindowedContext.safe_area
            //
            // Driven by `PENDING_SAFE_AREA_*_PX`, set from Kotlin via the
            // `Java_com_blinc_BlincNativeBridge_nativeDispatchSafeArea`
            // JNI export. Kotlin reads `WindowInsets.Type.systemBars()`
            // (API 30+) or `WindowInsets.systemWindowInset{Top,Right,
            // Bottom,Left}` (API 24–29), divides by display density, and
            // pushes the four logical-pixel values here.
            //
            // We diff every tick and copy into the context only when
            // something changed — safe-area updates are rare (rotation,
            // split-screen, PiP), but the read is cheap. The sentinel
            // `-1` is ignored so we don't clobber the context with a
            // zero tuple before the first dispatch arrives.
            // =========================================================
            let pending_sa_top = PENDING_SAFE_AREA_TOP_PX.load(Ordering::Relaxed);
            let pending_sa_right = PENDING_SAFE_AREA_RIGHT_PX.load(Ordering::Relaxed);
            let pending_sa_bottom = PENDING_SAFE_AREA_BOTTOM_PX.load(Ordering::Relaxed);
            let pending_sa_left = PENDING_SAFE_AREA_LEFT_PX.load(Ordering::Relaxed);
            let safe_area_ready = pending_sa_top >= 0
                && pending_sa_right >= 0
                && pending_sa_bottom >= 0
                && pending_sa_left >= 0;
            let safe_area_changed = safe_area_ready
                && (pending_sa_top != last_applied_safe_area_top_px
                    || pending_sa_right != last_applied_safe_area_right_px
                    || pending_sa_bottom != last_applied_safe_area_bottom_px
                    || pending_sa_left != last_applied_safe_area_left_px);
            if safe_area_changed {
                if let Some(ref mut windowed_ctx) = ctx {
                    windowed_ctx.safe_area = (
                        pending_sa_top as f32,
                        pending_sa_right as f32,
                        pending_sa_bottom as f32,
                        pending_sa_left as f32,
                    );
                    needs_redraw_next_frame = true;
                    tracing::debug!(
                        "Android safe area updated: top={} right={} bottom={} left={}",
                        pending_sa_top,
                        pending_sa_right,
                        pending_sa_bottom,
                        pending_sa_left,
                    );
                }
                last_applied_safe_area_top_px = pending_sa_top;
                last_applied_safe_area_right_px = pending_sa_right;
                last_applied_safe_area_bottom_px = pending_sa_bottom;
                last_applied_safe_area_left_px = pending_sa_left;
            }

            // =========================================================
            // Soft-keyboard inset → scroll the focused text input above the
            // keyboard.
            //
            // Driven by `PENDING_IME_INSET_PX`, set from Kotlin via the
            // `Java_com_blinc_BlincNativeBridge_nativeDispatchKeyboardInset`
            // JNI export. The Kotlin side reads
            // `WindowInsets.Type.ime().bottom`, divides by display density,
            // and pushes the logical-pixel value here.
            //
            // We need TWO independent triggers because the focus-count
            // signal `take_keyboard_state_change` is not enough on its
            // own:
            //
            //   1. **Inset change** — keyboard slides in or out.
            //      Caught by diffing `pending_inset_px` against
            //      `last_applied_keyboard_inset_px`.
            //
            //   2. **Tap-on-text-input generation bump** — the user
            //      tapped a text input (any of them, including
            //      re-tapping the same one) and we should re-evaluate
            //      whether the focused input is currently obscured.
            //      `take_keyboard_state_change` misses this because it
            //      only fires on `0 → 1` / `1 → 0` focus-count
            //      transitions; re-tapping a focused input stays at
            //      count = 1 the whole time. We use a separate
            //      `focus_tap_generation` counter that bumps in the
            //      `text_input` widget's `on_mouse_down` handler.
            // =========================================================
            let pending_inset_px = PENDING_IME_INSET_PX.load(Ordering::Relaxed);
            let current_tap_gen = blinc_layout::widgets::text_input::focus_tap_generation();
            let inset_changed =
                pending_inset_px >= 0 && pending_inset_px != last_applied_keyboard_inset_px;
            let tap_changed = current_tap_gen != last_focus_tap_generation;
            let needs_scroll_pass = inset_changed || (tap_changed && pending_inset_px > 0);

            if needs_scroll_pass {
                let inset_to_apply = pending_inset_px.max(0) as f32;
                if let Some(ref mut windowed_ctx) = ctx {
                    windowed_ctx.keyboard_inset = inset_to_apply;
                    let viewport_h = windowed_ctx.height;
                    if let Some(ref mut tree) = render_tree {
                        let scrolled = tree
                            .scroll_focused_text_input_above_keyboard(viewport_h, inset_to_apply);
                        if scrolled {
                            needs_redraw_next_frame = true;
                        }
                    }
                    tracing::debug!(
                        "Android keyboard inset: last={} pending={} viewport_h={} tap_gen={}->{} inset_changed={} tap_changed={}",
                        last_applied_keyboard_inset_px,
                        pending_inset_px,
                        windowed_ctx.height,
                        last_focus_tap_generation,
                        current_tap_gen,
                        inset_changed,
                        tap_changed,
                    );
                }
                if pending_inset_px >= 0 {
                    last_applied_keyboard_inset_px = pending_inset_px;
                }
                last_focus_tap_generation = current_tap_gen;
            }

            // =========================================================
            // Drain pending key-down events from JNI handlers.
            //
            // The native edit menu (`BlincEditMenuHelper`) pushes
            // synthesized Cmd+key events here when the user picks
            // Cut / Copy / Paste / Select All. JNI callbacks run on
            // Kotlin's UI thread and can't touch the render tree, so
            // they queue the event and we dispatch it here on the
            // android_main thread, where the tree lives.
            // =========================================================
            let key_events_to_dispatch: Vec<(u32, u32)> = {
                if let Ok(mut queue) = PENDING_KEY_DOWN_EVENTS.lock() {
                    std::mem::take(&mut *queue)
                } else {
                    Vec::new()
                }
            };
            if !key_events_to_dispatch.is_empty() {
                if let Some(ref mut tree) = render_tree {
                    for (key_code, modifiers) in key_events_to_dispatch {
                        let shift = modifiers & 0x01 != 0;
                        let ctrl = modifiers & 0x02 != 0;
                        let alt = modifiers & 0x04 != 0;
                        let meta = modifiers & 0x08 != 0;
                        tracing::debug!(
                            "Dispatching synthesized KEY_DOWN: key_code={}, mods={:#x}",
                            key_code,
                            modifiers
                        );
                        tree.broadcast_key_event(
                            blinc_core::events::event_types::KEY_DOWN,
                            key_code,
                            shift,
                            ctrl,
                            alt,
                            meta,
                        );
                    }
                    needs_redraw_next_frame = true;
                }
            }

            // Long-press timer poll. Editable widgets arm this from
            // their `on_mouse_down` handlers when `is_touch_input()`
            // is true; the user holding their finger still for 500
            // ms (with no drift past 10 px) fires the helper, which
            // calls `show_edit_menu` with PASTE available — matching
            // the EditText long-press-to-paste UX. Cancellation
            // happens via `MotionAction::Up` / `Cancel` and
            // drift-detection inside the touch-move branch above.
            if blinc_layout::widgets::text_input::fire_long_press_timer_if_due() {
                needs_redraw_next_frame = true;
            }

            // =========================================================
            // PHASE 1: Check for incremental updates (prop changes, subtree rebuilds)
            // This avoids full rebuild for simple state changes
            // =========================================================
            // Check if stateful elements requested a redraw (hover/press/state changes)
            let has_stateful_updates = blinc_layout::take_needs_redraw();
            let has_pending_rebuilds = blinc_layout::has_pending_subtree_rebuilds();
            let has_prop_updates = blinc_layout::has_pending_partial_prop_updates();

            if has_stateful_updates || has_pending_rebuilds || has_prop_updates {
                if has_stateful_updates {
                    tracing::debug!("Redraw requested by: stateful state change");
                }

                // Drain the unified property channel
                // ([[project-reactive-architecture-v2]]).
                let prop_updates = blinc_layout::take_pending_partial_prop_updates();
                let had_prop_updates = !prop_updates.is_empty();
                let mut prop_effects = blinc_layout::SideEffects::default();
                if let Some(ref mut tree) = render_tree {
                    prop_effects = tree.apply_partial_property_updates(prop_updates);
                }

                // Process subtree rebuilds
                let mut needs_layout = prop_effects.needs_layout;
                if let Some(ref mut tree) = render_tree {
                    needs_layout |= tree.process_pending_subtree_rebuilds();
                }

                if needs_layout {
                    if let Some(ref mut tree) = render_tree {
                        if let Some(ref windowed_ctx) = ctx {
                            tracing::debug!("Subtree rebuilds processed, recomputing layout");
                            tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                            tree.apply_flip_transitions();
                            tree.update_flip_bounds();
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
                    Some(app_instance),
                    Some(surf),
                    Some(config),
                    Some(windowed_ctx),
                    Some(rs),
                ) = (
                    &mut blinc_app,
                    &surface,
                    &surface_config,
                    &mut ctx,
                    &render_state,
                ) {
                    // Tick OverlayStack + ToastTray each frame so motion / TTL
                    // state advances. Mirrors `windowed.rs:4937-4949` and the
                    // matching block in `web.rs`/`ios.rs`. Without this,
                    // overlays pushed via `OverlayStack::present()` never
                    // animate in or auto-dismiss.
                    let now = blinc_layout::prelude::elapsed_ms();
                    {
                        use blinc_layout::overlay_state::{overlay_stack, toast_tray};
                        if let Ok(mut s) = overlay_stack().lock() {
                            s.set_viewport_with_scale(
                                windowed_ctx.width,
                                windowed_ctx.height,
                                windowed_ctx.scale_factor as f32,
                            );
                            s.update(now);
                        }
                        if let Ok(mut t) = toast_tray().lock() {
                            t.update(now);
                        }
                    }

                    // Drain CSS queued by `ThemeBundle::with_css` →
                    // `ThemeState::init` (and DSL/plugin `queue_stylesheet`
                    // calls) before the tree is built so `cn_bundle()`
                    // styles land on the first frame. Same pattern as
                    // `windowed.rs:5228-5234`.
                    {
                        let queued = blinc_core::BlincContextState::get().drain_stylesheets();
                        for css in queued {
                            windowed_ctx.add_css(&css);
                        }
                    }

                    // Build UI — compose user UI with all overlay surfaces
                    // (legacy + new OverlayStack + ToastTray). Without
                    // these children the `.show()` / `.present()` calls
                    // would push content into managers that never reach
                    // the tree.
                    let element = {
                        let user_ui = ui_builder(windowed_ctx);
                        let overlay_layer = windowed_ctx.overlay_manager.build_overlay_layer();
                        let stack_layer = {
                            use blinc_layout::overlay_state::overlay_stack;
                            match overlay_stack().lock() {
                                Ok(mut s) => {
                                    s.set_viewport_with_scale(
                                        windowed_ctx.width,
                                        windowed_ctx.height,
                                        windowed_ctx.scale_factor as f32,
                                    );
                                    s.build_overlay_layer()
                                }
                                Err(_) => blinc_layout::div::Div::new(),
                            }
                        };
                        let viewport = (windowed_ctx.width, windowed_ctx.height);
                        let tray_layer = {
                            use blinc_layout::overlay_state::toast_tray;
                            toast_tray()
                                .lock()
                                .ok()
                                .map(|t| t.build_tray_layer(viewport))
                                .unwrap_or_else(blinc_layout::div::Div::new)
                        };
                        blinc_layout::div::Div::new()
                            .w(windowed_ctx.width)
                            .h(windowed_ctx.height)
                            .relative()
                            .child(user_ui)
                            .child(overlay_layer)
                            .child(stack_layer)
                            .child(tray_layer)
                    };

                    // Clear stale Stateful base_render_props updaters before rebuild
                    blinc_layout::clear_stateful_base_updaters();
                    blinc_layout::click_outside::clear_click_outside_handlers();

                    // Create or update render tree
                    if render_tree.is_none() {
                        // First time: create tree
                        let mut tree = RenderTree::from_element(&element);
                        tree.set_scale_factor(windowed_ctx.scale_factor as f32);
                        if let Some(ref stylesheet) = windowed_ctx.stylesheet {
                            tree.set_stylesheet_arc(stylesheet.clone());
                        }
                        tree.apply_all_stylesheet_styles();
                        tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                        tree.update_flip_bounds();
                        tree.start_all_css_animations();
                        tree.clear_dirty(); // Start clean
                        render_tree = Some(tree);
                    } else if let Some(ref mut tree) = render_tree {
                        // Full rebuild
                        *tree = RenderTree::from_element(&element);
                        tree.set_scale_factor(windowed_ctx.scale_factor as f32);
                        if let Some(ref stylesheet) = windowed_ctx.stylesheet {
                            tree.set_stylesheet_arc(stylesheet.clone());
                        }
                        tree.apply_all_stylesheet_styles();
                        tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                        tree.update_flip_bounds();
                        tree.start_all_css_animations();
                        // Clear dirty on the NEW tree to prevent immediate re-rebuild
                        tree.clear_dirty();
                    }
                    needs_redraw = true;
                }
                // Reset rebuild flag after successful rebuild
                needs_rebuild = false;
            }

            // Tick CSS animations and apply state styles (visual-only)
            {
                let current_time = blinc_layout::prelude::elapsed_ms();
                if let Some(ref mut tree) = render_tree {
                    // Tick CSS animations synchronously to stay in phase with rendering
                    let dt_ms = if last_frame_time_ms > 0 {
                        (current_time - last_frame_time_ms) as f32
                    } else {
                        16.0
                    };
                    {
                        let store = tree.css_anim_store();
                        let mut s = store.lock().unwrap();
                        s.tick(dt_ms);
                    }
                    let flip_active = tree.tick_flip_animations(dt_ms);
                    let css_active =
                        tree.css_has_active() || !tree.css_transitions_empty() || flip_active;

                    // Apply CSS state styles (:hover, :active, :focus)
                    // This also detects property changes and starts new transitions
                    if let Some(ref windowed_ctx) = ctx {
                        if tree.stylesheet().is_some() {
                            tree.apply_stylesheet_state_styles(&windowed_ctx.event_router);
                        }
                    }
                    if css_active
                        || !tree.css_transitions_empty()
                        || tree.has_active_flip_animations()
                    {
                        tree.apply_all_css_animation_props();
                        tree.apply_all_css_transition_props();
                        tree.apply_flip_animation_props();
                        needs_redraw = true;
                        needs_redraw_next_frame = true;
                        // Apply animated layout properties and recompute layout if needed
                        if tree.apply_animated_layout_props() {
                            if let Some(ref windowed_ctx) = ctx {
                                tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                                tree.update_flip_bounds();
                            }
                        }
                    }
                }
                last_frame_time_ms = current_time;
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
                    Some(app_instance),
                    Some(surf),
                    Some(config),
                    Some(windowed_ctx),
                    Some(rs),
                    Some(tree),
                ) = (
                    &mut blinc_app,
                    &surface,
                    &surface_config,
                    &mut ctx,
                    &render_state,
                    &render_tree,
                ) {
                    // Render
                    //
                    // The PowerVR Vulkan driver on the Pixel 10 Pro
                    // appears to mark every acquired SurfaceTexture as
                    // `suboptimal`, and on some drivers a suboptimal
                    // texture's contents are silently discarded during
                    // presentation. We log + reconfigure when that
                    // happens, mirroring the desktop runner. We also
                    // explicitly handle `Outdated`, which the wgpu 26
                    // surface API can return after the swapchain becomes
                    // stale (e.g. after a window resize the runner
                    // hasn't picked up yet).
                    let frame = surf.get_current_texture();
                    static SUBOPTIMAL_LOGGED: std::sync::atomic::AtomicBool =
                        std::sync::atomic::AtomicBool::new(false);
                    match frame {
                        Ok(output) => {
                            if output.suboptimal
                                && !SUBOPTIMAL_LOGGED
                                    .swap(true, std::sync::atomic::Ordering::Relaxed)
                            {
                                tracing::warn!(
                                    "SurfaceTexture is suboptimal — will reconfigure swapchain"
                                );
                            }
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
                            let was_suboptimal = output.suboptimal;
                            output.present();
                            if was_suboptimal {
                                surf.configure(app_instance.device(), config);
                            }
                        }
                        Err(wgpu::SurfaceError::Lost) | Err(wgpu::SurfaceError::Outdated) => {
                            tracing::warn!("Surface lost / outdated — reconfiguring swapchain");
                            surf.configure(app_instance.device(), config);
                            needs_redraw_next_frame = true;
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
    fn init_gpu(
        window: &NativeWindow,
        pipeline_cache_path: Option<std::path::PathBuf>,
    ) -> Result<BlincApp> {
        use blinc_gpu::{GpuRenderer, RendererConfig, TextRenderingContext};

        Self::force_opaque_window_format(window);

        let config = crate::BlincConfig::default();

        let renderer_config = RendererConfig {
            max_primitives: config.max_primitives,
            max_glass_primitives: config.max_glass_primitives,
            max_glyphs: config.max_glyphs,
            sample_count: 1,
            texture_format: None,
            unified_text_rendering: true,
            pipeline_cache_path,
            ..RendererConfig::default()
        };

        // Create instance with Vulkan backend
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });

        let surface_target = Self::surface_target_from_native_window(window)?;
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

        Ok(app)
    }
}

// ============================================================================
// Deep link handling
// ============================================================================

/// Dispatch a deep link URI from JNI intent data.
///
/// Auto-dispatches to the router registered via `RouterBuilder::build()`.
/// No user setup required.
pub fn dispatch_deep_link(uri: &str) {
    tracing::info!("Android deep link received: {}", uri);
    blinc_router::dispatch_deep_link(uri);
}

/// Receive stream data from JNI (camera frames, audio buffers).
///
/// Called from Kotlin via JNI:
/// ```kotlin
/// external fun nativeDispatchStreamData(streamId: Long, data: ByteArray)
/// ```
pub fn dispatch_stream_data(stream_id: u64, data: &[u8]) {
    blinc_core::native_bridge::dispatch_stream_data(
        stream_id,
        blinc_core::native_bridge::NativeValue::Bytes(data.to_vec()),
    );
}

/// JNI export — receive a soft-keyboard inset update from Kotlin.
///
/// Called from `BlincNativeBridge`'s `setOnApplyWindowInsetsListener`
/// whenever `WindowInsets.Type.ime().bottom` changes (the Android
/// equivalent of `UIKeyboardWillChangeFrameNotification`).
///
/// The Kotlin side converts the raw pixel value (which Android reports
/// in physical pixels) to **logical pixels** by dividing by the display
/// density before pushing here, so the value stored in
/// `PENDING_IME_INSET_PX` is directly comparable to
/// `WindowedContext.height` and `width` (which the Android runner
/// also stores in logical pixels).
///
/// The android_main poll loop reads this atomic on every tick and
/// pushes the value into `WindowedContext.keyboard_inset` if it
/// changed. From there the layout / scroll-into-focused-input
/// machinery picks it up via the same path the iOS runner uses.
///
/// # Kotlin declaration
/// ```kotlin
/// external fun nativeDispatchKeyboardInset(insetLogicalPx: Int)
/// ```
///
/// # JNI signature
/// `(I)V`
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_blinc_BlincNativeBridge_nativeDispatchKeyboardInset(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
    inset_logical_px: jni::sys::jint,
) {
    // Clamp anything negative (which would be nonsense from the IME
    // API but worth defending against) to zero. Sentinel `-1` is
    // reserved for "not yet pushed", so anything we accept here is
    // a real keyboard-inset update.
    let clamped = inset_logical_px.max(0);
    PENDING_IME_INSET_PX.store(clamped, Ordering::Relaxed);
}

/// JNI export — receive a system-bar safe-area inset update from Kotlin.
///
/// Called from `BlincNativeBridge`'s `setOnApplyWindowInsetsListener`
/// whenever the status bar, navigation bar, notch cutout, or gesture
/// bar inset changes (rotation, split-screen, picture-in-picture exit,
/// immersive-mode toggle, etc.).
///
/// The Kotlin side converts each edge's raw pixel value to **logical
/// pixels** by dividing by display density before pushing, so the
/// values stored in `PENDING_SAFE_AREA_*_PX` are directly comparable
/// to `WindowedContext.width` / `height`.
///
/// All four edges are pushed together — a partial dispatch could leave
/// the context in an inconsistent state for a frame.
///
/// # Kotlin declaration
/// ```kotlin
/// external fun nativeDispatchSafeArea(top: Int, right: Int, bottom: Int, left: Int)
/// ```
///
/// # JNI signature
/// `(IIII)V`
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_blinc_BlincNativeBridge_nativeDispatchSafeArea(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
    top_logical_px: jni::sys::jint,
    right_logical_px: jni::sys::jint,
    bottom_logical_px: jni::sys::jint,
    left_logical_px: jni::sys::jint,
) {
    PENDING_SAFE_AREA_TOP_PX.store(top_logical_px.max(0), Ordering::Relaxed);
    PENDING_SAFE_AREA_RIGHT_PX.store(right_logical_px.max(0), Ordering::Relaxed);
    PENDING_SAFE_AREA_BOTTOM_PX.store(bottom_logical_px.max(0), Ordering::Relaxed);
    PENDING_SAFE_AREA_LEFT_PX.store(left_logical_px.max(0), Ordering::Relaxed);
}

/// JNI export — receive a synthesized key-down event from Kotlin.
///
/// Called from `BlincEditMenuHelper.onActionItemClicked` when the user
/// picks Cut / Copy / Paste / Select All from the native edit menu.
/// The Kotlin side passes the matching desktop-style key code:
///
///   Cut        → 88 (Cmd+X)
///   Copy       → 67 (Cmd+C)
///   Paste      → 86 (Cmd+V)
///   Select All → 65 (Cmd+A)
///
/// `modifiers` is the same bitmask iOS uses (shift=0x01, ctrl=0x02,
/// alt=0x04, meta=0x08); the menu callbacks always set the meta bit
/// (`0x08`) so the dispatch routes through the existing Cmd-shortcut
/// branch of every Blinc text-editable widget's `on_key_down` handler.
///
/// JNI handlers run on Kotlin's UI thread, but the `RenderTree` only
/// lives on the `android_main` thread. We push the event onto a
/// `PENDING_KEY_DOWN_EVENTS` queue and android_main drains it on the
/// next poll-loop tick.
///
/// # Kotlin declaration
/// ```kotlin
/// external fun nativeDispatchKeyDownWithModifiers(keyCode: Int, modifiers: Int)
/// ```
///
/// # JNI signature
/// `(II)V`
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_blinc_BlincNativeBridge_nativeDispatchKeyDownWithModifiers(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
    key_code: jni::sys::jint,
    modifiers: jni::sys::jint,
) {
    if let Ok(mut queue) = PENDING_KEY_DOWN_EVENTS.lock() {
        queue.push((key_code.max(0) as u32, modifiers.max(0) as u32));
    }
}
