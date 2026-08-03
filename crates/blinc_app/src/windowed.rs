//! Windowed application runner
//!
//! Provides a unified API for running windowed Blinc applications across
//! desktop and Android platforms.
//!
//! # Example
//!
//! ```ignore
//! use blinc_app::prelude::*;
//! use blinc_app::windowed::WindowedApp;
//!
//! fn main() -> Result<()> {
//!     WindowedApp::run(WindowConfig::default(), |ctx| {
//!         // Build your UI using reactive signals
//!         let count = ctx.use_signal(0);
//!         let doubled = ctx.use_derived(move |cx| cx.get(count).unwrap_or(0) * 2);
//!
//!         div().w_full().h_full()
//!             .flex_center()
//!             .child(text(&format!("Count: {}", ctx.get(count).unwrap_or(0))).size(48.0))
//!     })
//! }
//! ```

use std::hash::Hash;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use blinc_animation::{
    AnimatedTimeline, AnimatedValue, AnimationContext, AnimationScheduler, SchedulerHandle,
    SharedAnimatedTimeline, SharedAnimatedValue, SpringConfig,
};
use blinc_core::context_state::{BlincContextState, HookState, SharedHookState, StateKey};
use blinc_core::reactive::{Derived, ReactiveGraph, Signal, SignalId, State, StatefulDepsCallback};
use blinc_layout::overlay_state::{OverlayContext, get_overlay_manager};
use blinc_layout::prelude::*;
use blinc_layout::widgets::overlay::{OverlayManager, OverlayManagerExt, overlay_manager};
use blinc_platform::{
    ControlFlow, Event, EventLoop, InputEvent, Key, KeyState, LifecycleEvent, MouseEvent, Platform,
    TouchEvent, Window, WindowConfig, WindowEvent,
};

use crate::app::BlincApp;
use crate::error::{BlincError, Result};

/// Shared animation scheduler for the application (thread-safe)
pub type SharedAnimationScheduler = Arc<Mutex<AnimationScheduler>>;

/// Pick an initial animation-FPS cap for `AnimationFps::Adaptive`
/// based on coarse system characteristics.
///
/// Currently looks at logical-core count via
/// `std::thread::available_parallelism()`. Future revisions can add
/// GPU class (via wgpu adapter info), total RAM, and battery /
/// power-mode signals.
///
/// Thresholds are conservative: the dynamic adaptation step (when it
/// lands) will raise the cap if frames are coming in well under
/// budget, so we err on the side of starting lower and letting the
/// adapter climb. The animation_fps_cap field clamping further
/// enforces sane bounds.
///
/// Returns:
/// * `<= 4` cores  → 30 fps  (typical low-end laptops, older
///   Chromebooks, single-board computers)
/// * `<= 8` cores  → 60 fps  (mainstream desktops, recent laptops)
/// * `>  8` cores  → 60 fps  (high-end; could be 120 but conservative)
///
/// Apps that want vsync regardless can set
/// `WindowConfig::animation_fps = AnimationFps::Refresh`.
pub(crate) fn detect_initial_fps_cap() -> u32 {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    if cores <= 4 { 30 } else { 60 }
}

/// Allowed cap values for dynamic adaptation, ascending. The adapter
/// walks one step at a time.
const FPS_LADDER: &[u32] = &[15, 20, 30, 45, 60, 90, 120];
/// Multiple of the current per-frame budget (1000/cap ms) above
/// which the window's median frame time counts as overshooting.
const FPS_OVERSHOOT_MULT: f32 = 1.2;
/// Multiple of the budget below which the median counts as headroom.
const FPS_HEADROOM_MULT: f32 = 0.45;
/// Consecutive evaluation windows of consistent signal needed before
/// the cap moves (hysteresis to dampen transient spikes).
const FPS_WINDOWS_TO_DROP: u32 = 2;
const FPS_WINDOWS_TO_RAISE: u32 = 6;
/// Frames per evaluation window (≈ 1 second at 60 fps).
const FPS_WINDOW_FRAMES: usize = 60;

/// Adapter that adjusts the animation FPS cap based on observed
/// frame timings. Drops when the window-median frame time
/// consistently overshoots the budget; raises when frames have
/// substantial headroom. Only consulted under
/// [`AnimationFps::Adaptive`]; `Fixed` / `Refresh` skip it.
pub(crate) struct FpsAdapter {
    current_cap: u32,
    /// Upper bound the cap may raise to — set to the display refresh
    /// rate. Raising above the panel refresh wastes renders and, when
    /// the cap isn't an integer multiple of vsync (90 on a 60 Hz
    /// panel), beats against the compositor's presentation cadence →
    /// visible judder in continuous animations (spinner, scroll decel).
    ceiling: u32,
    window: Vec<u64>,
    consecutive_overshoot_windows: u32,
    consecutive_headroom_windows: u32,
}

impl FpsAdapter {
    pub(crate) fn new(initial_cap: u32) -> Self {
        Self {
            current_cap: clamp_to_ladder(initial_cap),
            // Until the real refresh rate is known, allow the full
            // ladder; `set_ceiling` narrows it at window resume.
            ceiling: *FPS_LADDER.last().unwrap(),
            window: Vec::with_capacity(FPS_WINDOW_FRAMES),
            consecutive_overshoot_windows: 0,
            consecutive_headroom_windows: 0,
        }
    }

    pub(crate) fn current_cap(&self) -> u32 {
        self.current_cap
    }

    /// Clamp the adaptive cap to the display refresh rate. The adapter
    /// never raises past this, and an already-raised cap is pulled back
    /// down immediately. Called at window resume once the panel's
    /// refresh is known.
    pub(crate) fn set_ceiling(&mut self, ceiling: u32) {
        self.ceiling = ceiling.max(FPS_LADDER[0]);
        if self.current_cap > self.ceiling {
            self.current_cap = self.ceiling;
        }
    }

    /// Record a frame's total wall-clock time. Returns the cap to
    /// use going forward (only changes after a hysteresis tally
    /// crosses its threshold).
    pub(crate) fn record(&mut self, total_us: u64) -> u32 {
        self.window.push(total_us);
        if self.window.len() >= FPS_WINDOW_FRAMES {
            self.evaluate();
            self.window.clear();
        }
        self.current_cap
    }

    fn evaluate(&mut self) {
        let median = window_median(&mut self.window);
        let budget_us = (1_000_000.0 / self.current_cap as f32) as u64;
        let drop_thresh = (budget_us as f32 * FPS_OVERSHOOT_MULT) as u64;
        let raise_thresh = (budget_us as f32 * FPS_HEADROOM_MULT) as u64;
        if median > drop_thresh {
            self.consecutive_overshoot_windows =
                self.consecutive_overshoot_windows.saturating_add(1);
            self.consecutive_headroom_windows = 0;
            if self.consecutive_overshoot_windows >= FPS_WINDOWS_TO_DROP {
                let new_cap = ladder_step_down(self.current_cap);
                if new_cap != self.current_cap {
                    tracing::info!(
                        "fps_adapter: dropping cap {} -> {} (median={}us budget={}us)",
                        self.current_cap,
                        new_cap,
                        median,
                        budget_us
                    );
                    self.current_cap = new_cap;
                    self.consecutive_overshoot_windows = 0;
                }
            }
        } else if median < raise_thresh {
            self.consecutive_headroom_windows = self.consecutive_headroom_windows.saturating_add(1);
            self.consecutive_overshoot_windows = 0;
            if self.consecutive_headroom_windows >= FPS_WINDOWS_TO_RAISE {
                let new_cap = ladder_step_up(self.current_cap).min(self.ceiling);
                if new_cap != self.current_cap {
                    tracing::info!(
                        "fps_adapter: raising cap {} -> {} (median={}us budget={}us)",
                        self.current_cap,
                        new_cap,
                        median,
                        budget_us
                    );
                    self.current_cap = new_cap;
                    self.consecutive_headroom_windows = 0;
                }
            }
        } else {
            self.consecutive_overshoot_windows = 0;
            self.consecutive_headroom_windows = 0;
        }
    }
}

fn clamp_to_ladder(target: u32) -> u32 {
    let mut best = FPS_LADDER[0];
    let mut best_diff = (FPS_LADDER[0] as i64 - target as i64).abs();
    for &rung in FPS_LADDER.iter().skip(1) {
        let diff = (rung as i64 - target as i64).abs();
        if diff < best_diff {
            best = rung;
            best_diff = diff;
        }
    }
    best
}

fn ladder_step_down(cap: u32) -> u32 {
    let mut prev = cap;
    for &rung in FPS_LADDER {
        if rung >= cap {
            return prev;
        }
        prev = rung;
    }
    prev
}

fn ladder_step_up(cap: u32) -> u32 {
    for &rung in FPS_LADDER {
        if rung > cap {
            return rung;
        }
    }
    cap
}

fn window_median(window: &mut [u64]) -> u64 {
    if window.is_empty() {
        return 0;
    }
    window.sort_unstable();
    window[window.len() / 2]
}

// SharedAnimatedValue and SharedAnimatedTimeline are re-exported from blinc_animation

#[cfg(all(feature = "windowed", not(target_os = "android")))]
use blinc_platform_desktop::DesktopPlatform;

/// Shared dirty flag type for element refs
pub type RefDirtyFlag = Arc<AtomicBool>;

/// Shared reactive graph for the application (thread-safe)
pub type SharedReactiveGraph = Arc<Mutex<ReactiveGraph>>;

/// Shared element registry for query API (thread-safe)
pub type SharedElementRegistry = Arc<blinc_layout::selector::ElementRegistry>;

/// Callback type for on_ready handlers
pub type ReadyCallback = Box<dyn FnOnce() + Send + Sync>;

/// Shared storage for ready callbacks
pub type SharedReadyCallbacks = Arc<Mutex<Vec<ReadyCallback>>>;

/// UI builder function for a window. Called each frame to produce the UI tree.
/// Returns a `Div` (the root element type for all Blinc UIs).
pub type WindowBuilder = Box<dyn FnMut(&mut WindowedContext) -> Div + Send>;

/// Pending window request: config + optional UI builder
struct PendingWindowRequest {
    config: WindowConfig,
    builder: Option<WindowBuilder>,
}

/// Queue of pending window requests (builder closures waiting to be picked up
/// by the event loop after AppCommand::CreateWindow fires).
static PENDING_WINDOW_BUILDERS: std::sync::OnceLock<Mutex<Vec<PendingWindowRequest>>> =
    std::sync::OnceLock::new();

fn pending_builders() -> &'static Mutex<Vec<PendingWindowRequest>> {
    PENDING_WINDOW_BUILDERS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Global callback for sending CreateWindow command to the event loop.
static OPEN_WINDOW_FN: std::sync::OnceLock<Arc<dyn Fn(WindowConfig) + Send + Sync>> =
    std::sync::OnceLock::new();

/// Global callback for initiating a window drag operation (custom title bars).
static DRAG_WINDOW_FN: std::sync::OnceLock<Arc<dyn Fn() + Send + Sync>> =
    std::sync::OnceLock::new();

/// Start a window drag operation (for custom title bars).
///
/// Call this from a mouse-down handler on a draggable element.
/// The OS takes over and the window follows the cursor until release.
pub fn drag_window() {
    if let Some(f) = DRAG_WINDOW_FN.get() {
        f();
    }
}

/// Open a new window with a UI builder from anywhere in the application.
///
/// The builder closure is called each frame to produce the window's UI.
///
/// # Example
/// ```ignore
/// use blinc_app::windowed::open_window_with;
///
/// open_window_with(
///     WindowConfig::new("Settings").size(400, 300),
///     |ctx| {
///         Box::new(div()
///             .w(ctx.width).h(ctx.height)
///             .bg(Color::rgb(0.1, 0.1, 0.15))
///             .child(text("Settings Window").size(24.0).color(Color::WHITE)))
///     },
/// );
/// ```
pub fn open_window_with<F>(config: WindowConfig, builder: F)
where
    F: FnMut(&mut WindowedContext) -> Div + Send + 'static,
{
    // Queue the builder so the event loop can pick it up
    pending_builders()
        .lock()
        .unwrap()
        .push(PendingWindowRequest {
            config: config.clone(),
            builder: Some(Box::new(builder)),
        });

    // Send the CreateWindow command to the event loop
    if let Some(f) = OPEN_WINDOW_FN.get() {
        f(config);
    } else {
        tracing::warn!("open_window_with() called before app initialization");
    }
}

/// Open a new window with a default blank UI.
///
/// For windows with custom UI, use `open_window_with()` instead.
pub fn open_window(config: WindowConfig) {
    // Queue without builder (uses default UI)
    pending_builders()
        .lock()
        .unwrap()
        .push(PendingWindowRequest {
            config: config.clone(),
            builder: None,
        });

    if let Some(f) = OPEN_WINDOW_FN.get() {
        f(config);
    } else {
        tracing::warn!("open_window() called before app initialization");
    }
}

/// Per-window state bundle.
///
/// Groups all state that is specific to a single window, extracted from the
/// monolithic event loop closure. This is the foundation for multi-window support.
#[cfg(all(feature = "windowed", not(target_os = "android")))]
pub(crate) struct WindowState {
    /// GPU app (renderer, device, queue)
    pub app: Option<BlincApp>,
    /// Window surface for rendering
    pub surface: Option<wgpu::Surface<'static>>,
    /// Surface configuration
    pub surface_config: Option<wgpu::SurfaceConfiguration>,
    /// UI context (dimensions, event router, shared handles)
    pub ctx: Option<WindowedContext>,
    /// Render tree (layout + render nodes)
    pub render_tree: Option<RenderTree>,
    /// Render state (cursor blink, animated values, motion)
    pub render_state: Option<blinc_layout::RenderState>,
    /// CSS animation/transition store
    pub css_anim_store: Arc<Mutex<blinc_layout::CssAnimationStore>>,
    /// Shared motion animation states
    pub shared_motion_states:
        Arc<std::sync::RwLock<std::collections::HashMap<String, blinc_core::MotionAnimationState>>>,
    /// Whether the UI tree needs rebuilding
    pub needs_rebuild: bool,
    /// Whether layout needs recomputing
    pub needs_relayout: bool,
    /// A hot reload landed: re-run the stylesheet passes on the next
    /// update regardless of what the diff reports, since an edited CSS
    /// rule changes no element hash.
    pub hot_reload_restyle: bool,
    /// Last frame timestamp for CSS animation delta
    pub last_frame_time_ms: u64,
    /// Timestamp of the last frame that actually ran Phase 4 (render +
    /// GPU submit + present). Distinct from `last_frame_time_ms`,
    /// which advances every Frame event we processed — including
    /// tick-only frames where scheduler animations were ticked but
    /// the paint pipeline was skipped under `animation_fps_cap`.
    /// Drives the decoupled-tick decision: we render only when the
    /// elapsed time since this stamp ≥ the cap interval (or some
    /// non-spring source needs a frame, in which case we render
    /// regardless of the cap). `0` until the first paint.
    pub last_paint_time_ms: u64,
    /// Active touch point IDs
    pub active_touch_ids: std::collections::HashSet<u64>,
    /// UI builder for this window (None = default static UI)
    pub ui_builder: Option<WindowBuilder>,
    /// Whether this window was created with a transparent surface.
    /// Drives the wgpu `CompositeAlphaMode` selection at surface config
    /// time and the per-frame clear-color alpha. Mirrors
    /// `WindowConfig::transparent`.
    pub transparent: bool,
    /// Last cursor style we asked the OS to display, so per-frame
    /// `set_cursor()` calls become a no-op when the cursor hasn't
    /// changed (the mouse-move handler may run hundreds of times a
    /// second during a drag — we don't want to syscall every iteration).
    pub last_cursor: Option<blinc_platform::Cursor>,
    /// Last-known keyboard modifier state. Updated from every
    /// `InputEvent::Keyboard` because winit fires `KeyboardInput`
    /// for modifier keys themselves (Shift / Cmd / Ctrl / Alt
    /// down + up); subsequent pointer events stamp this onto their
    /// `EventContext` via `dispatch_event_full(..., shift, ctrl,
    /// alt, meta)` so downstream handlers (canvas-kit `on_drag` /
    /// `on_drag_end`, application widgets) can branch on modifier
    /// state. Without this, pointer EventContexts always reported
    /// `shift: false` regardless of what was held during the click.
    pub cached_modifiers: blinc_platform::Modifiers,
    /// Last event-router state fingerprint (hovered + pressed + focused).
    /// Phase 4 skips `apply_stylesheet_state_styles` whenever the
    /// router state is identical to the previous frame — a major win
    /// on `cn_demo` and similar pages with hundreds of CSS-state-styled
    /// elements where the steady-state animation tick would otherwise
    /// iterate every registered id 60×/s to apply zero changes.
    /// `None` until the first state-style pass runs (forces the first
    /// frame to execute).
    pub last_router_state_fp: Option<u64>,
    /// Wall-clock instant of the most recent heavy pointer-move
    /// dispatch (the `on_mouse_move_with_occlusion` / `on_mouse_drag_fast`
    /// path). High-rate mice (1000 Hz polling) fire moves at >> the
    /// display refresh rate; once dispatched, subsequent moves within
    /// ~8 ms add no useful information (no element bound can have
    /// changed; the next paint still reads the latest cursor position).
    /// The Moved-arm uses this to skip the heavy dispatch when the
    /// previous one fired very recently — sub-frame coalescing without
    /// the full event-buffer refactor.
    /// ([[project-reactive-architecture-v2]] Phase 3.1.)
    pub last_pointer_dispatch: Option<std::time::Instant>,
    /// EXPERIMENTAL: hand-rolled Wayland `wl_surface::frame()` gate.
    /// `Some(...)` only when (a) the `wayland-frame-gate` feature is
    /// enabled, (b) we're on Linux/Wayland, and (c) the gate's
    /// construction from the raw display + surface pointers
    /// succeeded. When present, takes over the frame-callback
    /// gating from winit's `pre_present_notify`. See
    /// `crate::wayland_frame_gate` module docs.
    #[cfg(all(feature = "wayland-frame-gate", target_os = "linux"))]
    pub wayland_gate: Option<crate::wayland_frame_gate::WaylandFrameGate>,
    #[cfg(target_os = "linux")]
    /// Wayland only: keep the swapchain cycling when the scene is static
    /// (self-driven, vsync-paced presents) so the compositor never starves
    /// the next `get_current_texture()`. Set at surface creation from the
    /// detected display backend; override with `BLINC_KEEP_ALIVE=1/0`.
    pub wayland_keep_alive: bool,
    /// Whether any CSS animation/transition was active last frame.
    /// Falling edge triggers a one-shot render-cache invalidation so
    /// composite-promoted subtrees demote and re-emit at their exact
    /// final state (the fast path never demotes on its own).
    pub prev_css_active: bool,
}

#[cfg(all(feature = "windowed", not(target_os = "android")))]
impl WindowState {
    /// Create a new empty WindowState with shared resources
    pub fn new(
        css_anim_store: Arc<Mutex<blinc_layout::CssAnimationStore>>,
        shared_motion_states: Arc<
            std::sync::RwLock<std::collections::HashMap<String, blinc_core::MotionAnimationState>>,
        >,
    ) -> Self {
        Self {
            app: None,
            surface: None,
            surface_config: None,
            ctx: None,
            render_tree: None,
            render_state: None,
            css_anim_store,
            shared_motion_states,
            needs_rebuild: true,
            needs_relayout: false,
            hot_reload_restyle: false,
            last_frame_time_ms: 0,
            last_paint_time_ms: 0,
            active_touch_ids: std::collections::HashSet::new(),
            ui_builder: None,
            transparent: false,
            last_cursor: None,
            cached_modifiers: blinc_platform::Modifiers::default(),
            last_router_state_fp: None,
            last_pointer_dispatch: None,
            #[cfg(all(feature = "wayland-frame-gate", target_os = "linux"))]
            wayland_gate: None,
            #[cfg(target_os = "linux")]
            wayland_keep_alive: false,
            prev_css_active: false,
        }
    }
}

/// Pick the best present mode for the current surface.
///
/// Preference order on Linux:
/// 1. `FifoRelaxed` — vsync-paced, but a frame that lands late tears
///    instead of blocking. Preferred over strict Fifo: strict Fifo makes
///    the next present WAIT for the following vsync whenever one frame
///    runs late, surfacing as an occasional animation hitch/pause;
///    FifoRelaxed lets that single late frame through (a brief tear) and
///    keeps the cadence steady. It is still vsync-synchronised in the
///    steady state, so it reads cleanly under compositor screen-capture
///    (remote desktop / RDP) — unlike unsynchronised `Immediate`, which
///    is captured mid-flip and judders even though local rendering is
///    fine.
/// 2. `Fifo` — strict vsync (fallback when FifoRelaxed is unavailable).
/// 3. `Mailbox` — non-blocking with frame replacement.
/// 4. `Immediate` — non-blocking, may tear. Last resort.
/// 5. `AutoVsync`.
///
/// HISTORY: this list was previously inverted (`Immediate` first).
/// `Fifo` can BLOCK `get_current_texture()` for ~1 s per acquire when
/// a Wayland compositor (Mesa/Mutter) transiently can't release a
/// swapchain image — a stretch of those timeouts starved the winit
/// loop and froze the UI, and `Immediate` sidestepped it by taking a
/// different Mesa present-mode path. That starvation is now fixed at
/// its source: the Wayland keep-alive self-drive keeps presents
/// flowing so the compositor never withholds an image, and
/// reconfigure-on-`Timeout` recovers if one still slips through (see
/// the keep-alive notes at the frame loop). So vsync is safe again,
/// and `Immediate` — which the user could observe as judder over RDP —
/// is demoted. `BLINC_PRESENT_MODE=immediate` restores the old
/// behaviour if a specific compositor still misbehaves.
///
/// The chosen mode is logged at startup so end users can confirm
/// which path their compositor surfaced. Other platforms keep
/// `AutoVsync`.
#[cfg(all(feature = "windowed", not(target_os = "android")))]
fn preferred_present_mode(
    surface: &wgpu::Surface<'static>,
    adapter: &wgpu::Adapter,
) -> wgpu::PresentMode {
    #[cfg(target_os = "linux")]
    {
        let caps = surface.get_capabilities(adapter);
        let modes = caps.present_modes;
        // Debug override: BLINC_PRESENT_MODE=fifo|fifo_relaxed|mailbox|immediate
        // forces a specific mode when the surface offers it; otherwise the
        // normal auto ladder applies.
        let forced = std::env::var("BLINC_PRESENT_MODE").ok().and_then(|w| {
            match w.trim().to_ascii_lowercase().as_str() {
                "fifo" | "autovsync" => Some(wgpu::PresentMode::Fifo),
                "fifo_relaxed" | "fiforelaxed" => Some(wgpu::PresentMode::FifoRelaxed),
                "mailbox" => Some(wgpu::PresentMode::Mailbox),
                "immediate" => Some(wgpu::PresentMode::Immediate),
                _ => None,
            }
        });
        let pick = if let Some(m) = forced.filter(|m| modes.contains(m)) {
            m
        } else if modes.contains(&wgpu::PresentMode::FifoRelaxed) {
            wgpu::PresentMode::FifoRelaxed
        } else if modes.contains(&wgpu::PresentMode::Fifo) {
            wgpu::PresentMode::Fifo
        } else if modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else if modes.contains(&wgpu::PresentMode::Immediate) {
            wgpu::PresentMode::Immediate
        } else {
            wgpu::PresentMode::AutoVsync
        };
        tracing::info!(
            available = ?modes,
            chosen = ?pick,
            forced = ?forced,
            "Linux surface present-mode selection",
        );
        pick
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (surface, adapter);
        wgpu::PresentMode::AutoVsync
    }
}

/// Context passed to the UI builder function
pub struct WindowedContext {
    /// Current window width in logical pixels (for UI layout)
    ///
    /// This is the width you should use when building UI. It automatically
    /// accounts for DPI scaling, so elements sized to `ctx.width` will
    /// fill the window regardless of display scale factor.
    pub width: f32,
    /// Current window height in logical pixels (for UI layout)
    pub height: f32,
    /// Current scale factor (physical / logical)
    pub scale_factor: f64,
    /// Safe area insets (top, right, bottom, left) in logical pixels.
    /// On mobile: notch, status bar, home indicator.
    /// On desktop: all zeros.
    pub safe_area: (f32, f32, f32, f32),
    /// Soft-keyboard inset, in **logical** pixels — height in pixels of
    /// the area at the bottom of the screen currently obscured by an
    /// on-screen keyboard. Zero when the keyboard is hidden.
    ///
    /// Updated by the platform runner from native keyboard events:
    ///
    ///   - iOS: parsed from
    ///     `UIKeyboardWillChangeFrameNotification.userInfo[UIKeyboardFrameEndUserInfoKey]`
    ///     in `BlincKeyboardHelper`, pushed via the
    ///     `blinc_ios_set_keyboard_inset` FFI export.
    ///   - Android: read from
    ///     `WindowInsets.Type.ime().bottom` in
    ///     `BlincNativeBridge.kt`, dispatched into Rust through the
    ///     `keyboard.set_inset` native-bridge handler.
    ///   - Desktop / web / Fuchsia: always zero.
    ///
    /// The text-input refocus path consumes this to scroll the focused
    /// input above the keyboard when it appears, mirroring the iOS UIKit
    /// `UIScrollView.contentInset` adjustment dance.
    pub keyboard_inset: f32,
    /// Physical window width (for internal use)
    pub(crate) physical_width: f32,
    /// Physical window height (for internal use)
    pub(crate) physical_height: f32,
    /// Whether the window is focused
    pub focused: bool,
    /// Number of completed UI rebuilds (0 = first build in progress)
    ///
    /// Use `is_ready()` to check if the UI has been built at least once.
    /// This is useful for triggering animations after motion bindings are registered.
    pub rebuild_count: u32,
    /// Event router for input event handling
    pub event_router: EventRouter,
    /// Animation scheduler for spring/keyframe animations
    pub animations: SharedAnimationScheduler,
    /// Shared dirty flag for element refs - when set, triggers UI rebuild
    ref_dirty_flag: RefDirtyFlag,
    /// Reactive graph for signal-based state management
    reactive: SharedReactiveGraph,
    /// Hook state for call-order based signal persistence
    hooks: SharedHookState,
    /// Overlay manager for modals, dialogs, toasts, etc.
    pub(crate) overlay_manager: OverlayManager,
    /// Whether overlays were visible last frame (for triggering rebuilds)
    pub(crate) had_visible_overlays: bool,
    /// Element registry for query API (shared with RenderTree)
    element_registry: SharedElementRegistry,
    /// Callbacks to run after UI is ready (motion bindings registered)
    ready_callbacks: SharedReadyCallbacks,
    /// CSS stylesheet for automatic style application (hover, animations, base styles)
    /// Multiple stylesheets cascade — later rules override earlier ones.
    pub stylesheet: Option<Arc<blinc_layout::css_parser::Stylesheet>>,
    /// Raw CSS source strings, preserved for reparsing on theme changes.
    /// Each entry corresponds to one `add_css()` call, in order.
    css_sources: Vec<String>,
    /// Continuous pointer query state (per-element pointer tracking)
    pub pointer_query: blinc_layout::pointer_query::PointerQueryState,
    /// Callback to request opening a new window (set by desktop runner)
    open_window_fn: Option<Arc<dyn Fn(WindowConfig) + Send + Sync>>,
    /// Per-window close callback (sends CloseWindow command for THIS window)
    close_fn: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Per-window drag callback (starts OS drag for THIS window)
    drag_fn: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Per-window minimize callback
    minimize_fn: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Per-window maximize callback
    maximize_fn: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl WindowedContext {
    #[allow(clippy::too_many_arguments)]
    fn from_window<W: Window>(
        window: &W,
        event_router: EventRouter,
        animations: SharedAnimationScheduler,
        ref_dirty_flag: RefDirtyFlag,
        reactive: SharedReactiveGraph,
        hooks: SharedHookState,
        overlay_mgr: OverlayManager,
        element_registry: SharedElementRegistry,
        ready_callbacks: SharedReadyCallbacks,
    ) -> Self {
        // Get physical size (actual surface pixels) and scale factor
        let (physical_width, physical_height) = window.size();
        let scale_factor = window.scale_factor();

        // Compute logical size (what users work with in their UI code)
        // This ensures elements sized with ctx.width/height fill the window
        // regardless of DPI, and font sizes appear consistent across displays
        let logical_width = physical_width as f32 / scale_factor as f32;
        let logical_height = physical_height as f32 / scale_factor as f32;

        // Publish to the shared context so CSS viewport units (`vh` /
        // `vw`) resolve. iOS and web already did this; desktop did not,
        // so `height: 100vh` in a stylesheet parsed against a zero
        // viewport and was dropped.
        if blinc_core::context_state::BlincContextState::is_initialized() {
            blinc_core::context_state::BlincContextState::get()
                .set_viewport_size(logical_width, logical_height);
        }

        Self {
            width: logical_width,
            height: logical_height,
            scale_factor,
            safe_area: window.safe_area_insets(),
            keyboard_inset: 0.0,
            physical_width: physical_width as f32,
            physical_height: physical_height as f32,
            focused: window.is_focused(),
            rebuild_count: 0,
            event_router,
            animations,
            ref_dirty_flag,
            reactive,
            hooks,
            overlay_manager: overlay_mgr,
            had_visible_overlays: false,
            element_registry,
            ready_callbacks,
            stylesheet: None,
            css_sources: Vec::new(),
            pointer_query: blinc_layout::pointer_query::PointerQueryState::new(),
            open_window_fn: None,
            close_fn: None,
            drag_fn: None,
            minimize_fn: None,
            maximize_fn: None,
        }
    }

    /// Create a WindowedContext for Android
    ///
    /// This is used by the Android runner since it doesn't have a Window trait implementation.
    #[cfg(all(feature = "android", target_os = "android"))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_android(
        logical_width: f32,
        logical_height: f32,
        scale_factor: f64,
        physical_width: f32,
        physical_height: f32,
        focused: bool,
        safe_area: (f32, f32, f32, f32),
        animations: SharedAnimationScheduler,
        ref_dirty_flag: RefDirtyFlag,
        reactive: SharedReactiveGraph,
        hooks: SharedHookState,
        overlay_mgr: OverlayManager,
        element_registry: SharedElementRegistry,
        ready_callbacks: SharedReadyCallbacks,
    ) -> Self {
        Self {
            width: logical_width,
            height: logical_height,
            scale_factor,
            safe_area,
            keyboard_inset: 0.0,
            physical_width,
            physical_height,
            focused,
            rebuild_count: 0,
            event_router: EventRouter::new(),
            animations,
            ref_dirty_flag,
            reactive,
            hooks,
            overlay_manager: overlay_mgr,
            had_visible_overlays: false,
            element_registry,
            ready_callbacks,
            stylesheet: None,
            css_sources: Vec::new(),
            pointer_query: blinc_layout::pointer_query::PointerQueryState::new(),
            open_window_fn: None,
            close_fn: None,
            drag_fn: None,
            minimize_fn: None,
            maximize_fn: None,
        }
    }

    /// Create a WindowedContext for iOS
    ///
    /// This is used by the iOS runner since it doesn't have a Window trait implementation.
    #[cfg(all(feature = "ios", target_os = "ios"))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_ios(
        logical_width: f32,
        logical_height: f32,
        scale_factor: f64,
        physical_width: f32,
        physical_height: f32,
        focused: bool,
        safe_area: (f32, f32, f32, f32),
        animations: SharedAnimationScheduler,
        ref_dirty_flag: RefDirtyFlag,
        reactive: SharedReactiveGraph,
        hooks: SharedHookState,
        overlay_mgr: OverlayManager,
        element_registry: SharedElementRegistry,
        ready_callbacks: SharedReadyCallbacks,
    ) -> Self {
        Self {
            width: logical_width,
            height: logical_height,
            scale_factor,
            safe_area,
            keyboard_inset: 0.0,
            physical_width,
            physical_height,
            focused,
            rebuild_count: 0,
            event_router: EventRouter::new(),
            animations,
            ref_dirty_flag,
            reactive,
            hooks,
            overlay_manager: overlay_mgr,
            had_visible_overlays: false,
            element_registry,
            ready_callbacks,
            stylesheet: None,
            css_sources: Vec::new(),
            pointer_query: blinc_layout::pointer_query::PointerQueryState::new(),
            open_window_fn: None,
            close_fn: None,
            drag_fn: None,
            minimize_fn: None,
            maximize_fn: None,
        }
    }

    /// Create a WindowedContext for the web target.
    ///
    /// Mirrors [`Self::new_android`] / [`Self::new_ios`] / [`Self::new_fuchsia`]:
    /// the web runner extracts canvas dimensions and `devicePixelRatio`
    /// from the browser before calling, instead of going through the
    /// `Window` trait (which requires `raw-window-handle` types that
    /// `HtmlCanvasElement` doesn't implement).
    ///
    /// Wired into the `web` feature so this constructor is invisible to
    /// non-wasm builds. The shared / animation / overlay parameters are
    /// the same as the other `new_*` constructors so the wasm runner
    /// can build the same `WindowedContext` shape every other platform
    /// gets.
    #[cfg(all(feature = "web", target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_web(
        logical_width: f32,
        logical_height: f32,
        scale_factor: f64,
        physical_width: f32,
        physical_height: f32,
        focused: bool,
        animations: SharedAnimationScheduler,
        ref_dirty_flag: RefDirtyFlag,
        reactive: SharedReactiveGraph,
        hooks: SharedHookState,
        overlay_mgr: OverlayManager,
        element_registry: SharedElementRegistry,
        ready_callbacks: SharedReadyCallbacks,
    ) -> Self {
        Self {
            width: logical_width,
            height: logical_height,
            scale_factor,
            safe_area: (0.0, 0.0, 0.0, 0.0),
            keyboard_inset: 0.0,
            physical_width,
            physical_height,
            focused,
            rebuild_count: 0,
            event_router: EventRouter::new(),
            animations,
            ref_dirty_flag,
            reactive,
            hooks,
            overlay_manager: overlay_mgr,
            had_visible_overlays: false,
            element_registry,
            ready_callbacks,
            stylesheet: None,
            css_sources: Vec::new(),
            pointer_query: blinc_layout::pointer_query::PointerQueryState::new(),
            open_window_fn: None,
            close_fn: None,
            drag_fn: None,
            minimize_fn: None,
            maximize_fn: None,
        }
    }

    /// Create a WindowedContext for Fuchsia
    ///
    /// This is used by the Fuchsia runner since it doesn't have a Window trait implementation.
    #[cfg(all(feature = "fuchsia", target_os = "fuchsia"))]
    pub(crate) fn new_fuchsia(
        logical_width: f32,
        logical_height: f32,
        scale_factor: f64,
        physical_width: f32,
        physical_height: f32,
        focused: bool,
        animations: SharedAnimationScheduler,
        ref_dirty_flag: RefDirtyFlag,
        reactive: SharedReactiveGraph,
        hooks: SharedHookState,
        overlay_mgr: OverlayManager,
        element_registry: SharedElementRegistry,
        ready_callbacks: SharedReadyCallbacks,
    ) -> Self {
        Self {
            width: logical_width,
            height: logical_height,
            scale_factor,
            safe_area: (0.0, 0.0, 0.0, 0.0),
            keyboard_inset: 0.0,
            physical_width,
            physical_height,
            focused,
            rebuild_count: 0,
            event_router: EventRouter::new(),
            animations,
            ref_dirty_flag,
            reactive,
            hooks,
            overlay_manager: overlay_mgr,
            had_visible_overlays: false,
            element_registry,
            ready_callbacks,
            stylesheet: None,
            css_sources: Vec::new(),
            pointer_query: blinc_layout::pointer_query::PointerQueryState::new(),
            open_window_fn: None,
            close_fn: None,
            drag_fn: None,
            minimize_fn: None,
            maximize_fn: None,
        }
    }

    /// Update context from window (preserving event router, dirty flag, and reactive graph)
    fn update_from_window<W: Window>(&mut self, window: &W) {
        let (physical_width, physical_height) = window.size();
        let scale_factor = window.scale_factor();

        self.physical_width = physical_width as f32;
        self.physical_height = physical_height as f32;
        self.width = physical_width as f32 / scale_factor as f32;
        self.height = physical_height as f32 / scale_factor as f32;
        self.scale_factor = scale_factor;
        self.focused = window.is_focused();
    }

    // =========================================================================
    // DPI-Related Helpers
    // =========================================================================

    /// Get the physical window width (for advanced use cases)
    ///
    /// Most users should use `ctx.width` which is in logical pixels.
    /// Physical dimensions are only needed when directly interfacing
    /// with GPU surfaces or platform-specific code.
    pub fn physical_width(&self) -> f32 {
        self.physical_width
    }

    /// Get the physical window height (for advanced use cases)
    pub fn physical_height(&self) -> f32 {
        self.physical_height
    }

    /// Check if the UI is ready (has completed at least one rebuild)
    ///
    /// This is useful for triggering animations after the first UI build,
    /// when motion bindings have been registered with the renderer.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn my_component(ctx: &WindowedContext) -> impl ElementBuilder {
    ///     let progress = ctx.use_animated_value_for("progress", 0.0, SpringConfig::gentle());
    ///
    ///     // Only trigger animation after UI is ready
    ///     let triggered = ctx.use_state_keyed("triggered", || false);
    ///     if ctx.is_ready() && !triggered.get() {
    ///         triggered.set(true);
    ///         progress.lock().unwrap().set_target(100.0);
    ///     }
    ///
    ///     // ... build UI ...
    /// }
    /// ```
    pub fn is_ready(&self) -> bool {
        self.rebuild_count > 0
    }

    /// Safe area inset from the top (status bar, notch)
    pub fn safe_top(&self) -> f32 {
        self.safe_area.0
    }

    /// Safe area inset from the right
    pub fn safe_right(&self) -> f32 {
        self.safe_area.1
    }

    /// Safe area inset from the bottom (home indicator)
    pub fn safe_bottom(&self) -> f32 {
        self.safe_area.2
    }

    /// Safe area inset from the left
    pub fn safe_left(&self) -> f32 {
        self.safe_area.3
    }

    /// Content width excluding safe area insets
    pub fn safe_width(&self) -> f32 {
        self.width - self.safe_area.1 - self.safe_area.3
    }

    /// Content height excluding safe area insets
    pub fn safe_height(&self) -> f32 {
        self.height - self.safe_area.0 - self.safe_area.2
    }

    /// Open a new window with the given configuration.
    ///
    /// The window is created asynchronously on the next event loop tick.
    /// Only available on desktop platforms.
    ///
    /// # Example
    ///
    /// ```ignore
    /// ctx.open_window(WindowConfig::new("Settings").size(400, 300));
    /// ```
    pub fn open_window(&self, config: WindowConfig) {
        if let Some(ref open_fn) = self.open_window_fn {
            open_fn(config);
        } else {
            tracing::warn!(
                "open_window() called but no window creation callback is set (not on desktop?)"
            );
        }
    }

    /// Set the callback for opening new windows (called by the desktop runner)
    pub(crate) fn set_open_window_fn(&mut self, f: Arc<dyn Fn(WindowConfig) + Send + Sync>) {
        self.open_window_fn = Some(f);
    }

    /// Set per-window action callbacks (called by the desktop runner)
    pub(crate) fn set_window_actions(
        &mut self,
        close: Arc<dyn Fn() + Send + Sync>,
        drag: Arc<dyn Fn() + Send + Sync>,
        minimize: Arc<dyn Fn() + Send + Sync>,
        maximize: Arc<dyn Fn() + Send + Sync>,
    ) {
        self.close_fn = Some(close);
        self.drag_fn = Some(drag);
        self.minimize_fn = Some(minimize);
        self.maximize_fn = Some(maximize);
    }

    /// Close THIS window. Safe to call from any click handler.
    pub fn close(&self) {
        if let Some(ref f) = self.close_fn {
            f();
        }
    }

    /// Start dragging THIS window (for custom title bars).
    pub fn drag(&self) {
        if let Some(ref f) = self.drag_fn {
            f();
        }
    }

    /// Minimize THIS window.
    pub fn minimize(&self) {
        if let Some(ref f) = self.minimize_fn {
            f();
        }
    }

    /// Maximize/restore THIS window.
    pub fn maximize(&self) {
        if let Some(ref f) = self.maximize_fn {
            f();
        }
    }

    /// Get a cloneable close callback for THIS window.
    /// Use this to capture the close action in event handler closures.
    pub fn close_callback(&self) -> Arc<dyn Fn() + Send + Sync> {
        self.close_fn.clone().unwrap_or_else(|| Arc::new(|| {}))
    }

    /// Get a cloneable drag callback for THIS window.
    pub fn drag_callback(&self) -> Arc<dyn Fn() + Send + Sync> {
        self.drag_fn.clone().unwrap_or_else(|| Arc::new(|| {}))
    }

    /// Get a cloneable minimize callback for THIS window.
    pub fn minimize_callback(&self) -> Arc<dyn Fn() + Send + Sync> {
        self.minimize_fn.clone().unwrap_or_else(|| Arc::new(|| {}))
    }

    /// Get a cloneable maximize callback for THIS window.
    pub fn maximize_callback(&self) -> Arc<dyn Fn() + Send + Sync> {
        self.maximize_fn.clone().unwrap_or_else(|| Arc::new(|| {}))
    }

    /// Register a callback to run once after the UI is ready
    ///
    /// The callback will be executed after the first UI rebuild completes,
    /// when motion bindings have been registered with the renderer.
    /// This is the recommended way to trigger initial animations.
    ///
    /// Callbacks are executed once and then discarded. If `is_ready()` is
    /// already true, the callback will run on the next frame.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn my_component(ctx: &WindowedContext) -> impl ElementBuilder {
    ///     let progress = ctx.use_animated_value_for("progress", 0.0, SpringConfig::gentle());
    ///
    ///     // Register animation to trigger when UI is ready
    ///     let progress_clone = progress.clone();
    ///     ctx.on_ready(move || {
    ///         if let Ok(mut value) = progress_clone.lock() {
    ///             value.set_target(100.0);
    ///         }
    ///     });
    ///
    ///     // ... build UI ...
    /// }
    /// ```
    /// Register a callback to run once when the UI is ready (context-level).
    ///
    /// **Note:** For element-specific callbacks, prefer using the query API:
    /// ```ignore
    /// ctx.query_element("my-element").on_ready(|bounds| {
    ///     // Triggered once after element is laid out
    /// });
    /// ```
    /// The query-based approach uses stable string IDs that survive tree rebuilds.
    ///
    /// This context-level callback runs after the first rebuild completes.
    /// If called after the UI is already ready, executes immediately.
    pub fn on_ready<F>(&self, callback: F)
    where
        F: FnOnce() + Send + Sync + 'static,
    {
        // If already ready, execute immediately
        if self.rebuild_count > 0 {
            callback();
            return;
        }
        // Otherwise queue for execution after first rebuild
        if let Ok(mut callbacks) = self.ready_callbacks.lock() {
            callbacks.push(Box::new(callback));
        }
    }

    // =========================================================================
    // Reactive Signal API
    // =========================================================================

    /// Create a persistent state value that survives across UI rebuilds (keyed)
    ///
    /// This creates component-level state identified by a unique string key.
    /// Returns a `State<T>` with direct `.get()` and `.set()` methods.
    ///
    /// For stateful UI elements with `StateTransitions`, prefer `use_state(initial)`
    /// which auto-keys by source location.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn my_button(ctx: &WindowedContext, id: &str) -> impl ElementBuilder {
    ///     // Each button gets its own hover state, keyed by id
    ///     let hovered = ctx.use_state_keyed(id, || false);
    ///
    ///     div()
    ///         .bg(if hovered.get() { Color::RED } else { Color::BLUE })
    ///         .on_hover_enter({
    ///             let hovered = hovered.clone();
    ///             move |_| hovered.set(true)
    ///         })
    ///         .on_hover_leave({
    ///             let hovered = hovered.clone();
    ///             move |_| hovered.set(false)
    ///         })
    /// }
    /// ```
    pub fn use_state_keyed<T, F>(&self, key: &str, init: F) -> State<T>
    where
        T: Clone + Send + 'static,
        F: FnOnce() -> T,
    {
        use blinc_core::reactive::SignalId;

        let state_key = StateKey::from_string::<T>(key);
        let mut hooks = self.hooks.lock().unwrap();

        // Check if we have an existing signal with this key
        let signal = if let Some(raw_id) = hooks.get(&state_key) {
            // Reconstruct the signal from stored ID
            let signal_id = SignalId::from_raw(raw_id);
            Signal::from_id(signal_id)
        } else {
            // First time - create a new signal and store it
            let signal = self.reactive.lock().unwrap().create_signal(init());
            let raw_id = signal.id().to_raw();
            hooks.insert(state_key, raw_id);
            signal
        };

        // Create callback for stateful deps notification
        let callback: StatefulDepsCallback = Arc::new(|signal_ids| {
            blinc_layout::check_stateful_deps(signal_ids);
        });

        State::with_stateful_callback(
            signal,
            Arc::clone(&self.reactive),
            Arc::clone(&self.ref_dirty_flag),
            callback,
        )
    }

    /// Create a persistent signal that survives across UI rebuilds (keyed)
    ///
    /// Unlike `use_signal()` which creates a new signal each call, this method
    /// persists the signal using a unique string key. Use this for simple
    /// reactive values that need to survive rebuilds.
    ///
    /// For FSM-based state with `StateTransitions`, use `use_state_keyed()` instead.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let current_index = ctx.use_signal_keyed("current_index", || 0usize);
    ///
    /// // Read the value
    /// let index = ctx.get(current_index).unwrap_or(0);
    ///
    /// // Set the value (in an event handler)
    /// ctx.set(current_index, 1);
    /// ```
    pub fn use_signal_keyed<T, F>(&self, key: &str, init: F) -> Signal<T>
    where
        T: Clone + Send + 'static,
        F: FnOnce() -> T,
    {
        use blinc_core::reactive::SignalId;

        let state_key = StateKey::from_string::<T>(key);
        let mut hooks = self.hooks.lock().unwrap();

        // Check if we have an existing signal with this key
        if let Some(raw_id) = hooks.get(&state_key) {
            // Reconstruct the signal from stored ID
            let signal_id = SignalId::from_raw(raw_id);
            Signal::from_id(signal_id)
        } else {
            // First time - create a new signal and store it
            let signal = self.reactive.lock().unwrap().create_signal(init());
            let raw_id = signal.id().to_raw();
            hooks.insert(state_key, raw_id);
            signal
        }
    }

    /// Bare auto-keyed [`ScrollRef`] — uses `#[track_caller]` to
    /// derive a unique key from the source location, so the handle
    /// survives UI rebuilds without a manual key. For loops or
    /// repeated calls from the same line, use
    /// [`Self::use_scroll_ref_keyed`].
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    ///     let scroll_ref = ctx.use_scroll_ref();
    ///
    ///     div()
    ///         .child(
    ///             scroll()
    ///                 .bind(&scroll_ref)
    ///                 .child(items.iter().map(|i| div().id(format!("item-{}", i.id))))
    ///         )
    ///         .child(
    ///             button("Scroll to item 5").on_click({
    ///                 let scroll_ref = scroll_ref.clone();
    ///                 move |_| scroll_ref.scroll_to("item-5")
    ///             })
    ///         )
    /// }
    /// ```
    #[track_caller]
    pub fn use_scroll_ref(&self) -> blinc_layout::selector::ScrollRef {
        blinc_layout::selector::use_scroll_ref()
    }

    /// Hash-keyed [`ScrollRef`] for loops / list items / reusable
    /// component factories called multiple times from one line. The
    /// key can be any `Hash` type (`&str`, `u32`, tuples,
    /// `InstanceKey`, ...).
    ///
    /// Prefer [`Self::use_scroll_ref`] for the common one-call-site
    /// case.
    pub fn use_scroll_ref_keyed<K: Hash>(&self, key: K) -> blinc_layout::selector::ScrollRef {
        blinc_layout::selector::use_scroll_ref_keyed(key)
    }

    /// Create or retrieve a persistent reactive signal, auto-keyed
    /// by the caller's source location via `#[track_caller]`. The
    /// signal survives UI rebuilds — first call from a given line
    /// mints it, subsequent calls return the same handle.
    ///
    /// For loops or factories called multiple times from one line,
    /// use [`Self::use_signal_keyed`].
    ///
    /// # Example
    ///
    /// ```ignore
    /// let count = ctx.use_signal(0i32);
    /// // In an event handler:
    /// ctx.set(count, ctx.get(count).unwrap_or(0) + 1);
    /// ```
    #[track_caller]
    pub fn use_signal<T: Clone + Send + 'static>(&self, initial: T) -> Signal<T> {
        blinc_core::context_state::BlincContextState::get().use_signal(initial)
    }

    /// Get the current value of a signal
    ///
    /// This automatically tracks the signal as a dependency when called
    /// within a derived computation or effect.
    pub fn get<T: Clone + 'static>(&self, signal: Signal<T>) -> Option<T> {
        self.reactive.lock().unwrap().get(signal)
    }

    /// Set the value of a signal, triggering reactive updates
    ///
    /// This will automatically trigger a UI rebuild.
    pub fn set<T: Send + 'static>(&self, signal: Signal<T>, value: T) {
        self.reactive.lock().unwrap().set(signal, value);
        // Mark dirty to trigger rebuild
        self.ref_dirty_flag.store(true, Ordering::SeqCst);
    }

    /// Update a signal using a function
    ///
    /// This is useful for incrementing counters or modifying state based
    /// on the current value.
    ///
    /// # Example
    ///
    /// ```ignore
    /// ctx.update(count, |n| n + 1);
    /// ```
    pub fn update<T: Clone + Send + 'static, F: FnOnce(T) -> T>(&self, signal: Signal<T>, f: F) {
        self.reactive.lock().unwrap().update(signal, f);
        self.ref_dirty_flag.store(true, Ordering::SeqCst);
    }

    /// Create a derived (computed) value
    ///
    /// Derived values are lazily computed and cached. They automatically
    /// track their signal dependencies and recompute when those signals change.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let count = ctx.use_signal(5);
    /// let doubled = ctx.use_derived(move |cx| cx.get(count).unwrap_or(0) * 2);
    ///
    /// assert_eq!(ctx.get_derived(doubled), Some(10));
    /// ```
    pub fn use_derived<T, F>(&self, compute: F) -> Derived<T>
    where
        T: Clone + Send + 'static,
        F: Fn(&ReactiveGraph) -> T + Send + 'static,
    {
        self.reactive.lock().unwrap().create_derived(compute)
    }

    /// Get the value of a derived computation
    pub fn get_derived<T: Clone + 'static>(&self, derived: Derived<T>) -> Option<T> {
        self.reactive.lock().unwrap().get_derived(derived)
    }

    /// Batch multiple signal updates into a single reactive update
    ///
    /// This is useful when updating multiple signals at once to avoid
    /// redundant recomputations.
    ///
    /// # Example
    ///
    /// ```ignore
    /// ctx.batch(|g| {
    ///     g.set(x, 10);
    ///     g.set(y, 20);
    ///     g.set(z, 30);
    /// });
    /// // Only one UI rebuild triggered
    /// ```
    pub fn batch<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut ReactiveGraph) -> R,
    {
        let result = self.reactive.lock().unwrap().batch(f);
        self.ref_dirty_flag.store(true, Ordering::SeqCst);
        result
    }

    /// Get the shared reactive graph for advanced usage
    ///
    /// This is useful when you need to pass the graph to closures or
    /// store it for later use.
    pub fn reactive(&self) -> SharedReactiveGraph {
        Arc::clone(&self.reactive)
    }

    /// Create a new DivRef that will trigger rebuilds when modified
    ///
    /// Use this to create refs that can be mutated in event handlers.
    /// When you call `.borrow_mut()` or `.with_mut()` on the returned ref,
    /// the UI will automatically rebuild when the mutation completes.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let card_ref = ctx.create_ref::<Div>();
    ///
    /// div()
    ///     .child(
    ///         div()
    ///             .bind(&card_ref)
    ///             .on_hover_enter({
    ///                 let r = card_ref.clone();
    ///                 move |_| {
    ///                     // This automatically triggers a rebuild
    ///                     r.with_mut(|d| *d = d.swap().bg(Color::RED));
    ///                 }
    ///             })
    ///     )
    /// ```
    pub fn create_ref<T>(&self) -> ElementRef<T> {
        ElementRef::with_dirty_flag(Arc::clone(&self.ref_dirty_flag))
    }

    /// Create a new DivRef (convenience method)
    pub fn div_ref(&self) -> DivRef {
        self.create_ref::<Div>()
    }

    /// Get the shared dirty flag for manual state management
    ///
    /// Use this when you want to create your own state types that trigger
    /// UI rebuilds when modified. When you modify state, set this flag to true.
    ///
    /// # Example
    ///
    /// ```ignore
    /// struct MyState {
    ///     value: i32,
    ///     dirty_flag: RefDirtyFlag,
    /// }
    ///
    /// impl MyState {
    ///     fn set_value(&mut self, v: i32) {
    ///         self.value = v;
    ///         self.dirty_flag.store(true, Ordering::SeqCst);
    ///     }
    /// }
    /// ```
    pub fn dirty_flag(&self) -> RefDirtyFlag {
        Arc::clone(&self.ref_dirty_flag)
    }

    /// Get a handle to the animation scheduler for creating animated values
    ///
    /// Components use this handle to create `AnimatedValue`s that automatically
    /// register with the scheduler. The scheduler ticks all animations each frame
    /// and triggers UI rebuilds while animations are active.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_animation::{AnimatedValue, SpringConfig};
    ///
    /// let opacity = AnimatedValue::new(ctx.animations(), 1.0, SpringConfig::stiff());
    /// opacity.set_target(0.5); // Auto-registers and animates
    /// let current = opacity.get(); // Get interpolated value
    /// ```
    pub fn animation_handle(&self) -> SchedulerHandle {
        self.animations.lock().unwrap().handle()
    }

    /// Get the overlay manager for showing modals, dialogs, toasts, etc.
    ///
    /// The overlay manager provides a fluent API for creating overlays that
    /// render in a separate pass after the main UI tree, ensuring they always
    /// appear on top.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_layout::prelude::*;
    ///
    /// fn my_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    ///     let overlay_mgr = ctx.overlay_manager();
    ///
    ///     div()
    ///         .child(
    ///             button("Show Modal").on_click({
    ///                 let mgr = overlay_mgr.clone();
    ///                 move |_| {
    ///                     mgr.modal()
    ///                         .content(|| {
    ///                             div().p(20.0).bg(Color::WHITE)
    ///                                 .child(text("Hello from modal!"))
    ///                         })
    ///                         .show();
    ///                 }
    ///             })
    ///         )
    /// }
    /// ```
    pub fn overlay_manager(&self) -> OverlayManager {
        Arc::clone(&self.overlay_manager)
    }

    // =========================================================================
    // Query API
    // =========================================================================

    /// Query an element by ID and get an ElementHandle for programmatic manipulation
    ///
    /// Returns an `ElementHandle` for interacting with the element. The handle
    /// provides methods like `scroll_into_view()`, `focus()`, `click()`, `on_ready()`,
    /// and tree traversal.
    ///
    /// The handle works even if the element doesn't exist yet - operations like
    /// `on_ready()` will queue until the element is laid out. Use `handle.exists()`
    /// to check if the element currently exists.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Register on_ready callback (works before element exists):
    /// ctx.query("progress-bar").on_ready(|bounds| {
    ///     progress_anim.lock().unwrap().set_target(bounds.width * 0.75);
    /// });
    ///
    /// // In UI builder:
    /// div().id("progress-bar").child(...)
    ///
    /// // Later, interact with existing element:
    /// let handle = ctx.query("my-element");
    /// if handle.exists() {
    ///     handle.scroll_into_view();
    ///     handle.focus();
    /// }
    /// ```
    pub fn query(&self, id: &str) -> blinc_layout::selector::ElementHandle<()> {
        blinc_layout::selector::ElementHandle::new(id, self.element_registry.clone())
    }

    /// Get the shared element registry
    ///
    /// This provides access to the element registry for advanced query operations.
    pub fn element_registry(&self) -> &SharedElementRegistry {
        &self.element_registry
    }

    /// Create or retrieve a persistent fine-grained reactive cell,
    /// auto-keyed by the caller's source location via
    /// `#[track_caller]`. Returns a [`blinc_core::State`] — a
    /// signal-backed slot with `.get()` / `.set()` / `.update()`
    /// methods. Survives UI rebuilds.
    ///
    /// `State<T>` is the fine-grained reactive primitive: each
    /// `.set()` notifies only the things that registered a
    /// dependency on this signal. Use it for values like counters,
    /// flags, form fields — anything you'd want to read in one
    /// place and mutate from another.
    ///
    /// For Stateful FSM handles (hover / press / drag / custom
    /// state machines), reach for [`Self::use_state_for`] /
    /// `use_state_for(initial)` instead — those return a
    /// `SharedState<S>` (an `Arc<Mutex<StatefulInner<S>>>`), which
    /// is a different abstraction.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn counter(ctx: &WindowedContext) -> impl ElementBuilder {
    ///     let count = ctx.use_state(0i32);
    ///     stateful::<NoState>()
    ///         .deps([count.signal_id()])
    ///         .on_state(move |_| {
    ///             div()
    ///                 .child(text(&format!("Clicks: {}", count.get())))
    ///                 .on_click({
    ///                     let count = count.clone();
    ///                     move |_| count.update(|n| n + 1)
    ///                 })
    ///         })
    /// }
    /// ```
    #[track_caller]
    pub fn use_state<T>(&self, initial: T) -> blinc_core::State<T>
    where
        T: Clone + Send + 'static,
    {
        // Forward to the global context's `use_state`. Both this
        // method and `BlincContextState::use_state` are
        // `#[track_caller]` so `Location::caller()` lands on the
        // ORIGINAL caller of `ctx.use_state(...)`, not on this
        // wrapper line.
        blinc_core::context_state::BlincContextState::get().use_state(initial)
    }

    /// Create or retrieve a persistent FSM handle (`SharedState<S>`)
    /// with an explicit hash key. Use this for reusable components
    /// called multiple times from the same source line (loops,
    /// list items, factory functions).
    ///
    /// The key can be any type that implements `Hash` (strings,
    /// numbers, tuples, `InstanceKey`, etc).
    ///
    /// For the common "one widget per source line" case, prefer
    /// the bare [`Self::use_fsm`] / `use_fsm(initial)` — it derives
    /// the key from `#[track_caller]` automatically.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Reusable component — string key per instance
    /// fn feature_card(ctx: &WindowedContext, id: &str) -> impl ElementBuilder {
    ///     let handle = ctx.use_fsm_keyed(id, ButtonState::Idle);
    ///     stateful_from_handle(handle).on_state(|state, div| { /* … */ })
    /// }
    ///
    /// // Or with a numeric key in a loop
    /// for i in 0..3 {
    ///     let handle = ctx.use_fsm_keyed(i, ButtonState::Idle);
    ///     // …
    /// }
    /// ```
    pub fn use_fsm_keyed<K, S>(&self, key: K, initial: S) -> blinc_layout::SharedState<S>
    where
        K: Hash,
        S: blinc_layout::StateTransitions + Clone + Send + 'static,
    {
        // Forward to the standalone keyed version so both API
        // surfaces share one implementation and can't drift.
        blinc_layout::stateful::use_fsm_keyed(key, initial)
    }

    /// **Deprecated.** Renamed to [`Self::use_fsm_keyed`].
    ///
    /// The new name says what the method returns (a `SharedState<S>`
    /// FSM handle, not a `State<T>` reactive cell). Existing call
    /// sites still compile; they just emit a deprecation warning.
    #[deprecated(
        since = "0.5.2",
        note = "use `use_fsm_keyed(key, initial)` instead — returns a SharedState<S> (FSM handle)"
    )]
    #[track_caller]
    pub fn use_state_for<K, S>(&self, key: K, initial: S) -> blinc_layout::SharedState<S>
    where
        K: Hash,
        S: blinc_layout::StateTransitions + Clone + Send + 'static,
    {
        self.use_fsm_keyed(key, initial)
    }

    /// Create or retrieve a persistent FSM handle (`SharedState<S>`),
    /// auto-keyed by the caller's source location via
    /// `#[track_caller]`. The common case for "one Stateful per call
    /// site" — no manual key plumbing required.
    ///
    /// For loops or reusable factories called multiple times from
    /// the same source line, use [`Self::use_fsm_keyed`] with an
    /// explicit per-instance key (collisions on the auto-derived
    /// `(file, line, column)` key would otherwise alias every
    /// iteration onto the same slot).
    ///
    /// `Sync` is not required on `S` — the value lives behind
    /// `Arc<Mutex<…>>`.
    #[track_caller]
    pub fn use_fsm<S>(&self, initial: S) -> blinc_layout::SharedState<S>
    where
        S: blinc_layout::StateTransitions + Clone + Send + 'static,
    {
        blinc_layout::stateful::use_fsm(initial)
    }

    /// Create a persistent animated value using caller location as key
    ///
    /// The animated value survives UI rebuilds, preserving its current value
    /// and active spring animations. This is essential for continuous animations
    /// driven by state changes.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Animated value persists across rebuilds
    /// let offset_y = ctx.use_animated_value(0.0, SpringConfig::wobbly());
    ///
    /// // Can be used in motion bindings
    /// motion().translate_y(offset_y.clone()).child(content)
    /// ```
    #[track_caller]
    pub fn use_animated_value(&self, initial: f32, config: SpringConfig) -> SharedAnimatedValue {
        let location = std::panic::Location::caller();
        let key = format!(
            "{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
        self.use_animated_value_for(&key, initial, config)
    }

    /// Create a persistent animated value with an explicit key
    ///
    /// Use this for reusable components or when creating multiple animated
    /// values at the same source location (e.g., in a loop).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Multiple animated values with unique keys
    /// for i in 0..3 {
    ///     let scale = ctx.use_animated_value_for(
    ///         format!("item_{}_scale", i),
    ///         1.0,
    ///         SpringConfig::snappy(),
    ///     );
    /// }
    /// ```
    pub fn use_animated_value_for<K: Hash>(
        &self,
        key: K,
        initial: f32,
        config: SpringConfig,
    ) -> SharedAnimatedValue {
        use blinc_core::reactive::SignalId;

        // Use a type marker for SharedAnimatedValue
        let state_key = StateKey::new::<SharedAnimatedValue, _>(&key);
        let mut hooks = self.hooks.lock().unwrap();

        if let Some(raw_id) = hooks.get(&state_key) {
            // Existing animated value - retrieve from signal
            let signal_id = SignalId::from_raw(raw_id);
            let signal: Signal<SharedAnimatedValue> = Signal::from_id(signal_id);
            self.reactive.lock().unwrap().get(signal).unwrap()
        } else {
            // New animated value - create and store in signal
            let animated_value: SharedAnimatedValue = Arc::new(Mutex::new(AnimatedValue::new(
                self.animation_handle(),
                initial,
                config,
            )));
            let signal = self
                .reactive
                .lock()
                .unwrap()
                .create_signal(animated_value.clone());
            let raw_id = signal.id().to_raw();
            hooks.insert(state_key, raw_id);
            animated_value
        }
    }

    /// Create or retrieve a persistent animated timeline
    ///
    /// AnimatedTimeline provides keyframe-based animations that persist across
    /// UI rebuilds. Use this for timeline animations that need to survive
    /// layout changes and window resizes.
    ///
    /// The returned timeline is empty on first call - add keyframes using
    /// `timeline.add()` then call `start()`. Use `has_entries()` to check
    /// if the timeline needs configuration.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let timeline = ctx.use_animated_timeline();
    /// let entry_id = {
    ///     let mut t = timeline.lock().unwrap();
    ///     if !t.has_entries() {
    ///         let id = t.add(0, 2000, 0.0, 1.0);
    ///         t.start();
    ///         id
    ///     } else {
    ///         t.entry_ids().first().copied().unwrap()
    ///     }
    /// };
    /// ```
    #[track_caller]
    pub fn use_animated_timeline(&self) -> SharedAnimatedTimeline {
        let location = std::panic::Location::caller();
        let key = format!(
            "{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
        self.use_animated_timeline_for(&key)
    }

    /// Create or retrieve a persistent animated timeline with an explicit key
    ///
    /// Use this for reusable components or when creating multiple timelines
    /// at the same source location (e.g., in a loop).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Multiple timelines with unique keys
    /// for i in 0..3 {
    ///     let timeline = ctx.use_animated_timeline_for(format!("dot_{}", i));
    ///     // ...
    /// }
    /// ```
    pub fn use_animated_timeline_for<K: Hash>(&self, key: K) -> SharedAnimatedTimeline {
        use blinc_core::reactive::SignalId;

        // Use a type marker for SharedAnimatedTimeline
        let state_key = StateKey::new::<SharedAnimatedTimeline, _>(&key);
        let mut hooks = self.hooks.lock().unwrap();

        if let Some(raw_id) = hooks.get(&state_key) {
            // Existing timeline - retrieve from signal
            let signal_id = SignalId::from_raw(raw_id);
            let signal: Signal<SharedAnimatedTimeline> = Signal::from_id(signal_id);
            self.reactive.lock().unwrap().get(signal).unwrap()
        } else {
            // New timeline - create and store in signal
            let timeline: SharedAnimatedTimeline =
                Arc::new(Mutex::new(AnimatedTimeline::new(self.animation_handle())));
            let signal = self
                .reactive
                .lock()
                .unwrap()
                .create_signal(timeline.clone());
            let raw_id = signal.id().to_raw();
            hooks.insert(state_key, raw_id);
            timeline
        }
    }

    // =========================================================================
    // Tick Callback API (for per-frame updates like ECS systems)
    // =========================================================================

    /// Register a callback that runs each frame in the animation scheduler
    ///
    /// This creates a persistent tick callback keyed by source location.
    /// The callback receives delta time in seconds and runs on the animation
    /// scheduler's background thread at 120fps.
    ///
    /// Use this for ECS systems, physics, or any per-frame updates.
    /// The callback is registered once and persists across UI rebuilds.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Create ECS world (persisted via use_state)
    /// let world = ctx.use_state_keyed("world", || Arc::new(Mutex::new(World::new())));
    ///
    /// // Register tick callback to run ECS systems
    /// ctx.use_tick_callback({
    ///     let world = world.get();
    ///     move |dt| {
    ///         let mut w = world.lock().unwrap();
    ///         // Run ECS systems with delta time
    ///         w.run_systems(dt);
    ///     }
    /// });
    /// ```
    #[track_caller]
    pub fn use_tick_callback<F>(&self, callback: F) -> blinc_animation::TickCallbackId
    where
        F: FnMut(f32) + Send + Sync + 'static,
    {
        let location = std::panic::Location::caller();
        let key = format!(
            "tick_{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
        self.use_tick_callback_for(&key, callback)
    }

    /// Register a tick callback with an explicit key
    ///
    /// Use this when you need to create multiple tick callbacks at the same
    /// source location (e.g., in a loop) or in reusable components.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Multiple tick callbacks with unique keys
    /// for i in 0..3 {
    ///     ctx.use_tick_callback_for(format!("system_{}", i), move |dt| {
    ///         // Per-frame update
    ///     });
    /// }
    /// ```
    pub fn use_tick_callback_for<K: Hash, F>(
        &self,
        key: K,
        callback: F,
    ) -> blinc_animation::TickCallbackId
    where
        F: FnMut(f32) + Send + Sync + 'static,
    {
        // Marker type for TickCallbackId storage
        struct TickCallbackMarker;

        let state_key = StateKey::new::<TickCallbackMarker, _>(&key);
        let mut hooks = self.hooks.lock().unwrap();

        if let Some(raw_id) = hooks.get(&state_key) {
            // Already registered - return existing ID
            blinc_animation::TickCallbackId::from_raw(raw_id)
        } else {
            // First time - register the callback with the scheduler
            let id = self
                .animation_handle()
                .register_tick_callback(callback)
                .expect("Animation scheduler should be alive");
            hooks.insert(state_key, id.to_raw());
            id
        }
    }

    // =========================================================================
    // Theme API
    // =========================================================================

    /// Get the current color scheme (light or dark)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let scheme = ctx.color_scheme();
    /// match scheme {
    ///     ColorScheme::Light => println!("Light mode"),
    ///     ColorScheme::Dark => println!("Dark mode"),
    /// }
    /// ```
    pub fn color_scheme(&self) -> blinc_theme::ColorScheme {
        blinc_theme::ThemeState::get().scheme()
    }

    /// Set the color scheme (triggers smooth theme transition)
    ///
    /// # Example
    ///
    /// ```ignore
    /// ctx.set_color_scheme(ColorScheme::Dark);
    /// ```
    pub fn set_color_scheme(&self, scheme: blinc_theme::ColorScheme) {
        blinc_theme::ThemeState::get().set_scheme(scheme);
    }

    /// Toggle between light and dark mode
    ///
    /// # Example
    ///
    /// ```ignore
    /// button("Toggle Theme").on_click(|ctx| {
    ///     ctx.toggle_color_scheme();
    /// })
    /// ```
    pub fn toggle_color_scheme(&self) {
        blinc_theme::ThemeState::get().toggle_scheme();
    }

    /// Get a color from the current theme
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_theme::ColorToken;
    ///
    /// let primary = ctx.theme_color(ColorToken::Primary);
    /// let bg = ctx.theme_color(ColorToken::Background);
    /// ```
    pub fn theme_color(&self, token: blinc_theme::ColorToken) -> blinc_core::Color {
        blinc_theme::ThemeState::get().color(token)
    }

    /// Get spacing from the current theme
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_theme::SpacingToken;
    ///
    /// let padding = ctx.theme_spacing(SpacingToken::Space4); // 16px
    /// ```
    pub fn theme_spacing(&self, token: blinc_theme::SpacingToken) -> f32 {
        blinc_theme::ThemeState::get().spacing_value(token)
    }

    /// Get border radius from the current theme
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_theme::RadiusToken;
    ///
    /// let radius = ctx.theme_radius(RadiusToken::Lg); // 8px
    /// ```
    pub fn theme_radius(&self, token: blinc_theme::RadiusToken) -> f32 {
        blinc_theme::ThemeState::get().radius(token)
    }

    // =========================================================================
    // CSS Stylesheet API
    // =========================================================================

    /// Add inline CSS to the application stylesheet.
    ///
    /// Multiple calls cascade — later rules override earlier ones.
    /// Stylesheets are visual-only: they update render props on existing nodes
    /// and trigger redraws. They never cause tree rebuilds.
    pub fn add_css(&mut self, css: &str) {
        // Store raw CSS for reparsing on theme changes
        self.css_sources.push(css.to_string());

        // Seed parser with theme variables + any previously defined CSS variables
        let mut external_vars = blinc_theme::ThemeState::try_get()
            .map(|t| t.to_css_variable_map())
            .unwrap_or_default();
        if let Some(existing) = &self.stylesheet {
            for (k, v) in existing.variables() {
                external_vars.insert(k.clone(), v.clone());
            }
        }
        match blinc_layout::css_parser::Stylesheet::parse_with_variables(css, &external_vars) {
            Ok(sheet) => self.add_stylesheet(sheet),
            Err(e) => {
                tracing::warn!("Failed to parse CSS: {}", e);
            }
        }
    }

    /// Load and add a `.css` file to the application stylesheet.
    ///
    /// Multiple calls cascade — later rules override earlier ones.
    pub fn load_css(&mut self, path: &str) {
        match std::fs::read_to_string(path) {
            Ok(css) => self.add_css(&css),
            Err(e) => {
                tracing::warn!("Failed to load CSS file '{}': {}", path, e);
            }
        }
    }

    /// Add a pre-parsed stylesheet to the application.
    ///
    /// Multiple calls cascade — later rules override earlier ones.
    pub fn add_stylesheet(&mut self, sheet: blinc_layout::css_parser::Stylesheet) {
        match self.stylesheet.as_mut() {
            Some(existing) => {
                // Cascade: merge into existing (Arc::make_mut for COW)
                Arc::make_mut(existing).merge(sheet);
            }
            None => {
                self.stylesheet = Some(Arc::new(sheet));
            }
        }
        // Publish to global so stateful widgets (buttons, etc.) can read CSS
        // overrides during tree construction, before set_stylesheet_arc() runs
        if let Some(ref stylesheet) = self.stylesheet {
            blinc_layout::css_parser::set_active_stylesheet(std::sync::Arc::clone(stylesheet));
        }
    }

    /// Drop every accumulated stylesheet and reset `rebuild_count` so a
    /// subsequent rebuild looks like the very first one.
    ///
    /// Called by the hot-reload runner before re-invoking the user's UI
    /// closure under the freshly-applied subsecond patch. Without this:
    ///
    /// - `add_css` cascades, so deleted rules from the pre-patch run
    ///   would linger in the merged stylesheet.
    /// - `css_sources` would grow unboundedly across every patch.
    /// - The common `if ctx.rebuild_count == 0 { ctx.add_css(...) }`
    ///   guard in app code would skip re-registration entirely, so
    ///   even live rules wouldn't refresh.
    ///
    /// Also drops the global `ACTIVE_STYLESHEET` so widgets reaching
    /// for CSS overrides during the rebuild don't see a stale Arc.
    /// Outside hot-reload this would throw away cascaded user state;
    /// the method is `#[doc(hidden)]` and only invoked from the
    /// hot-reload trigger.
    #[doc(hidden)]
    pub fn reset_for_hot_reload(&mut self) {
        self.stylesheet = None;
        self.css_sources.clear();
        self.rebuild_count = 0;
        blinc_layout::css_parser::clear_active_stylesheet();
    }

    /// Reparse all stored CSS sources with fresh theme variables.
    ///
    /// Called automatically when the theme color scheme changes to ensure
    /// CSS `var()` and `theme()` references resolve to the new colors.
    pub fn reparse_css(&mut self) {
        if self.css_sources.is_empty() {
            return;
        }

        tracing::debug!(
            "Reparsing {} CSS sources with updated theme variables",
            self.css_sources.len()
        );

        // Clear existing stylesheet
        self.stylesheet = None;

        // Reparse each CSS source with fresh theme variables
        for css in self.css_sources.clone() {
            let mut external_vars = blinc_theme::ThemeState::try_get()
                .map(|t| t.to_css_variable_map())
                .unwrap_or_default();
            if let Some(existing) = &self.stylesheet {
                for (k, v) in existing.variables() {
                    external_vars.insert(k.clone(), v.clone());
                }
            }
            match blinc_layout::css_parser::Stylesheet::parse_with_variables(&css, &external_vars) {
                Ok(sheet) => self.add_stylesheet(sheet),
                Err(e) => {
                    tracing::warn!("Failed to reparse CSS on theme change: {}", e);
                }
            }
        }
    }

    /// Set a style for an element by ID.
    ///
    /// This is the Rust-native alternative to `add_css()`. Use with `css!` or `style!`
    /// macros to define styles in Rust syntax that are applied automatically to matching
    /// elements — just like CSS stylesheets.
    ///
    /// # Example
    ///
    /// ```ignore
    /// ctx.set_style("card", css! {
    ///     background: Color::BLUE;
    ///     border-radius: 12.0;
    ///     box-shadow: md;
    /// });
    ///
    /// // Then just give the element an ID:
    /// div().id("card").w(200.0).h(100.0)
    /// ```
    pub fn set_style(&mut self, id: &str, style: blinc_layout::element_style::ElementStyle) {
        match self.stylesheet.as_mut() {
            Some(existing) => {
                Arc::make_mut(existing).insert(id, style);
            }
            None => {
                let mut sheet = blinc_layout::css_parser::Stylesheet::new();
                sheet.insert(id, style);
                self.stylesheet = Some(Arc::new(sheet));
            }
        }
    }

    /// Set a state-specific style for an element by ID.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_layout::css_parser::ElementState;
    ///
    /// ctx.set_style("button", style! { bg: Color::BLUE, rounded: 8.0 });
    /// ctx.set_state_style("button", ElementState::Hover, style! {
    ///     bg: Color::from_hex(0x2563EB),
    ///     shadow_md,
    /// });
    /// ```
    pub fn set_state_style(
        &mut self,
        id: &str,
        state: blinc_layout::css_parser::ElementState,
        style: blinc_layout::element_style::ElementStyle,
    ) {
        match self.stylesheet.as_mut() {
            Some(existing) => {
                Arc::make_mut(existing).insert_with_state(id, state, style);
            }
            None => {
                let mut sheet = blinc_layout::css_parser::Stylesheet::new();
                sheet.insert_with_state(id, state, style);
                self.stylesheet = Some(Arc::new(sheet));
            }
        }
    }
}

// =============================================================================
// BlincContext Implementation
// =============================================================================

impl blinc_core::BlincContext for WindowedContext {
    fn use_state_keyed<T, F>(&self, key: &str, init: F) -> State<T>
    where
        T: Clone + Send + 'static,
        F: FnOnce() -> T,
    {
        // Delegate to the existing method
        WindowedContext::use_state_keyed(self, key, init)
    }

    fn use_signal_keyed<T, F>(&self, key: &str, init: F) -> Signal<T>
    where
        T: Clone + Send + 'static,
        F: FnOnce() -> T,
    {
        WindowedContext::use_signal_keyed(self, key, init)
    }

    #[track_caller]
    fn use_signal<T: Clone + Send + 'static>(&self, initial: T) -> Signal<T> {
        WindowedContext::use_signal(self, initial)
    }

    fn get<T: Clone + 'static>(&self, signal: Signal<T>) -> Option<T> {
        WindowedContext::get(self, signal)
    }

    fn set<T: Send + 'static>(&self, signal: Signal<T>, value: T) {
        WindowedContext::set(self, signal, value)
    }

    fn update<T: Clone + Send + 'static, F: FnOnce(T) -> T>(&self, signal: Signal<T>, f: F) {
        WindowedContext::update(self, signal, f)
    }

    fn use_derived<T, F>(&self, compute: F) -> Derived<T>
    where
        T: Clone + Send + 'static,
        F: Fn(&ReactiveGraph) -> T + Send + 'static,
    {
        WindowedContext::use_derived(self, compute)
    }

    fn get_derived<T: Clone + 'static>(&self, derived: Derived<T>) -> Option<T> {
        WindowedContext::get_derived(self, derived)
    }

    fn batch<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut ReactiveGraph) -> R,
    {
        WindowedContext::batch(self, f)
    }

    fn dirty_flag(&self) -> blinc_core::DirtyFlag {
        WindowedContext::dirty_flag(self)
    }

    fn request_rebuild(&self) {
        self.ref_dirty_flag.store(true, Ordering::SeqCst);
    }

    fn width(&self) -> f32 {
        self.width
    }

    fn height(&self) -> f32 {
        self.height
    }

    fn scale_factor(&self) -> f64 {
        self.scale_factor
    }
}

// =============================================================================
// AnimationContext Implementation
// =============================================================================

impl AnimationContext for WindowedContext {
    fn animation_handle(&self) -> SchedulerHandle {
        WindowedContext::animation_handle(self)
    }

    fn use_animated_value_for<K: Hash>(
        &self,
        key: K,
        initial: f32,
        config: SpringConfig,
    ) -> SharedAnimatedValue {
        WindowedContext::use_animated_value_for(self, key, initial, config)
    }

    fn use_animated_timeline_for<K: Hash>(&self, key: K) -> SharedAnimatedTimeline {
        WindowedContext::use_animated_timeline_for(self, key)
    }
}

/// Windowed application runner
///
/// Provides a simple way to run a Blinc application in a window
/// with automatic event handling and rendering.
pub struct WindowedApp;

impl WindowedApp {
    /// Initialize the platform asset loader
    ///
    /// On desktop, this sets up a filesystem-based loader.
    /// On Android, this would use the NDK AssetManager.
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    fn init_asset_loader() {
        use blinc_platform::assets::{FilesystemAssetLoader, set_global_asset_loader};

        // Create a filesystem loader (uses current directory as base)
        let loader = FilesystemAssetLoader::new();

        // Try to set the global loader (ignore error if already set)
        let _ = set_global_asset_loader(Box::new(loader));
    }

    /// Initialize the theme system with platform detection
    ///
    /// This sets up the global ThemeState with:
    /// - Platform-appropriate theme bundle (macOS, Windows, Linux, etc.)
    /// - System color scheme detection (light/dark mode)
    /// - Redraw callback to trigger UI updates on theme changes
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    fn init_theme() {
        use blinc_theme::{
            ThemeState, detect_system_color_scheme, platform_theme_bundle, set_redraw_callback,
        };

        // Only seed the platform default if no theme has been
        // installed yet — users who call `ThemeState::init` before
        // `WindowedApp::run` keep their bundle. Users who call it
        // *after* (e.g. from inside the UI builder) are handled by
        // `ThemeState::init`'s replace-when-already-set path.
        if ThemeState::try_get().is_none() {
            let bundle = platform_theme_bundle();
            let scheme = detect_system_color_scheme();
            ThemeState::init(bundle, scheme);
        }

        // Register the redraw callback unconditionally — needed for
        // every theme swap (color scheme toggle, runtime bundle
        // replace) regardless of who installed the initial theme.
        // Previously this lived inside the `is_none()` branch, so a
        // pre-init user theme silently disabled CSS reparse + tree
        // rebuild on subsequent theme changes.
        set_redraw_callback(|| {
            tracing::debug!("Theme changed - requesting full rebuild + CSS reparse");
            blinc_layout::widgets::request_css_reparse();
            blinc_layout::widgets::request_full_rebuild();
        });
    }

    /// Run a windowed Blinc application on desktop platforms
    ///
    /// This is the main entry point for desktop applications. It creates
    /// a window, sets up GPU rendering, and runs the event loop.
    ///
    /// # Arguments
    ///
    /// * `config` - Window configuration (title, size, etc.)
    /// * `ui_builder` - Function that builds the UI tree given the window context
    ///
    /// # Example
    ///
    /// ```ignore
    /// WindowedApp::run(WindowConfig::default(), |ctx| {
    ///     div()
    ///         .w(ctx.width).h(ctx.height)
    ///         .bg([0.1, 0.1, 0.15, 1.0])
    ///         .flex_center()
    ///         .child(
    ///             div().glass().rounded(16.0).p(24.0)
    ///                 .child(text("Hello Blinc!").size(32.0))
    ///         )
    /// })
    /// ```
    ///
    /// # Named-fn signature (edition 2024)
    ///
    /// On edition 2024, your `build_ui` function needs `+ use<>` on
    /// its return type:
    /// ```ignore
    /// fn build_ui(ctx: &mut WindowedContext) -> impl Element + use<> {
    ///     /* … */
    /// }
    /// WindowedApp::run(config, build_ui)?;
    /// ```
    ///
    /// **Why `+ use<>` is required and not just a workaround.** The
    /// runner stores the built tree across frames and incrementally
    /// updates it — every element inside the tree therefore has to be
    /// `'static` (it can't borrow from `ctx`, which is re-borrowed
    /// mutably on every frame). Edition 2024 changed RPIT capture
    /// rules so that `-> impl Element` in a free fn *implicitly*
    /// captures input lifetimes (it means `-> impl Element + use<'_>`),
    /// which the compiler reads as "may borrow from `ctx`". `+ use<>`
    /// explicitly says "captures nothing", restoring `'static`.
    ///
    /// This is the right signature for every UI builder in practice —
    /// they construct new owned elements from values read out of
    /// `ctx`, never references into it.
    ///
    /// Edition 2021 callers don't need the annotation (free-fn RPIT
    /// didn't capture by default there), but adding it is harmless
    /// and forward-compatible with the 2024 migration.
    ///
    /// See `gotcha-run-with-theme-hrtb-fnmut` in dev memory for
    /// reproduction, full error shapes, and why a closure wrap
    /// (`|cx| build_ui(cx)`) and a `Box` wrap don't help — both inherit
    /// the named fn's RPIT capture and produce the same error in a
    /// different shape.
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    pub fn run<F, E>(config: WindowConfig, ui_builder: F) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder + 'static,
    {
        Self::run_desktop(config, ui_builder)
    }

    /// Run a windowed app with an explicit theme bundle.
    ///
    /// Equivalent to `ThemeState::init(bundle, scheme)` immediately
    /// followed by [`Self::run`], but lets the caller install the
    /// theme without having to do it from inside the UI builder
    /// (where `init_theme`'s platform-default has already run and
    /// the user's bundle has to fight that path).
    ///
    /// Pass any value that implements `Into<ThemeBundle>` — the
    /// built-in [`BlincTheme::bundle()`](blinc_theme::BlincTheme::bundle)
    /// or a custom bundle built by your app.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_theme::{BlincTheme, ColorScheme};
    ///
    /// WindowedApp::run_with_theme(
    ///     WindowConfig::default(),
    ///     BlincTheme::bundle(),
    ///     ColorScheme::Dark,
    ///     |ctx| my_ui(ctx),
    /// )
    /// ```
    ///
    /// # Named-fn signature (edition 2024)
    ///
    /// Same requirement as [`Self::run`]: on edition 2024 your
    /// `build_ui` function needs `+ use<>` on the return type, e.g.
    /// `fn build_ui(ctx: &mut WindowedContext) -> impl Element + use<>`.
    /// See [`Self::run`]'s doc for the soundness rationale.
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    pub fn run_with_theme<F, E>(
        config: WindowConfig,
        bundle: blinc_theme::ThemeBundle,
        scheme: blinc_theme::ColorScheme,
        ui_builder: F,
    ) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder + 'static,
    {
        blinc_theme::ThemeState::init(bundle, scheme);
        Self::run_desktop(config, ui_builder)
    }

    /// Create per-window action closures (close, drag, minimize, maximize).
    /// Returns (close, drag, minimize, maximize) Arcs.
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    #[allow(clippy::type_complexity)]
    fn make_window_actions(
        win: std::sync::Arc<winit::window::Window>,
        wake: blinc_platform_desktop::WakeProxy,
    ) -> (
        Arc<dyn Fn() + Send + Sync>,
        Arc<dyn Fn() + Send + Sync>,
        Arc<dyn Fn() + Send + Sync>,
        Arc<dyn Fn() + Send + Sync>,
    ) {
        let d = Arc::downgrade(&win);
        let mi = Arc::downgrade(&win);
        let ma = Arc::downgrade(&win);
        let cl = Arc::downgrade(&win);
        let wake_for_close = wake;
        (
            Arc::new(move || {
                if let Some(w) = cl.upgrade() {
                    use std::hash::{Hash, Hasher};
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    w.id().hash(&mut hasher);
                    wake_for_close.close_window(blinc_platform::WindowId(hasher.finish()));
                }
            }),
            Arc::new(move || {
                if let Some(w) = d.upgrade() {
                    let _ = w.drag_window();
                }
            }),
            Arc::new(move || {
                if let Some(w) = mi.upgrade() {
                    w.set_minimized(true);
                }
            }),
            Arc::new(move || {
                if let Some(w) = ma.upgrade() {
                    w.set_maximized(!w.is_maximized());
                }
            }),
        )
    }

    /// Register global window action callbacks (for drag_region() on Div).
    /// Called for both primary and secondary windows, and on focus changes.
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    fn register_window_actions_static(
        win: std::sync::Arc<winit::window::Window>,
        wake: blinc_platform_desktop::WakeProxy,
    ) {
        let d = win.clone();
        let mi = win.clone();
        let ma = win.clone();
        let cl = win;
        blinc_layout::window_actions::set_active_window_actions(
            move || {
                let _ = d.drag_window();
            },
            move || mi.set_minimized(true),
            move || ma.set_maximized(!ma.is_maximized()),
            move || {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                cl.id().hash(&mut hasher);
                wake.close_window(blinc_platform::WindowId(hasher.finish()));
            },
        );
    }

    /// Pick the wgpu `CompositeAlphaMode` to configure the surface with.
    ///
    /// Queries the surface's actual supported modes via
    /// [`wgpu::Surface::get_capabilities`] and picks the best match.
    /// Hardcoding `PostMultiplied` for non-Windows panicked on Linux
    /// Mesa, which only exposes `[Opaque, PreMultiplied]` (GH #34).
    ///
    /// Preference when `transparent` is requested:
    /// `PostMultiplied` → `PreMultiplied` → `Inherit` → `Auto` →
    /// `Opaque`. `PostMultiplied` is correct for our non-premultiplied
    /// shader output; `PreMultiplied` may slightly wash colors out
    /// against the desktop background but doesn't panic and works on
    /// Linux Mesa + Windows DWM.
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    fn pick_alpha_mode(
        surface: &wgpu::Surface,
        adapter: &wgpu::Adapter,
        transparent: bool,
    ) -> wgpu::CompositeAlphaMode {
        if !transparent {
            return wgpu::CompositeAlphaMode::Opaque;
        }
        let supported = surface.get_capabilities(adapter).alpha_modes;
        let preference = [
            wgpu::CompositeAlphaMode::PostMultiplied,
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::Inherit,
            wgpu::CompositeAlphaMode::Auto,
        ];
        for mode in preference {
            if supported.contains(&mode) {
                return mode;
            }
        }
        tracing::warn!(
            "transparent window requested but the surface exposes no transparent alpha mode \
             (supported: {:?}) — falling back to Opaque",
            supported
        );
        wgpu::CompositeAlphaMode::Opaque
    }

    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    fn run_desktop<F, E>(config: WindowConfig, mut ui_builder: F) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder + 'static,
    {
        // Initialize the platform asset loader for cross-platform asset loading
        Self::init_asset_loader();

        // Initialize the text measurer for accurate text layout
        crate::text_measurer::init_text_measurer();

        // Initialize the theme system with platform detection
        Self::init_theme();

        let platform = DesktopPlatform::new().map_err(|e| BlincError::Platform(e.to_string()))?;
        let primary_transparent = config.transparent;
        let primary_max_frame_latency = config.max_frame_latency.clamp(1, 3);
        // Snapshot the animation FPS policy before `config` moves into
        // the event loop. Reconcile the two paths:
        //
        // * Legacy `animation_fps_cap: Option<u32>` — when set, wins
        //   unconditionally (treated as `AnimationFps::Fixed(N)`). Lets
        //   existing apps keep working without code changes.
        //
        // * New `animation_fps: AnimationFps`:
        //   - `Fixed(N)`     → cap at N, framework never overrides.
        //   - `Refresh`      → no cap, vsync-paced animations.
        //   - `Adaptive`     → framework picks a starting cap from a
        //                       static heuristic (GPU class proxy via
        //                       core count + total RAM) and adjusts at
        //                       runtime based on observed frame times.
        //
        // `animation_fps_cap_atomic` is shared with the Phase 4
        // adaptation logic below so the adapter can mutate the cap
        // at runtime; the per-frame paint gate and the scheduler
        // both read through it.
        let initial_animation_fps_cap: Option<u32> = match config.animation_fps_cap {
            Some(n) => Some(n),
            None => match config.animation_fps {
                blinc_platform::AnimationFps::Fixed(n) => Some(n),
                blinc_platform::AnimationFps::Refresh => None,
                blinc_platform::AnimationFps::Adaptive => Some(detect_initial_fps_cap()),
            },
        };
        let animation_fps_is_adaptive = config.animation_fps_cap.is_none()
            && matches!(config.animation_fps, blinc_platform::AnimationFps::Adaptive);
        let animation_fps_cap = initial_animation_fps_cap;
        tracing::info!(
            "animation_fps: source={} cap={:?} adaptive={}",
            if config.animation_fps_cap.is_some() {
                "legacy_animation_fps_cap"
            } else {
                match config.animation_fps {
                    blinc_platform::AnimationFps::Adaptive => "Adaptive",
                    blinc_platform::AnimationFps::Fixed(_) => "Fixed",
                    blinc_platform::AnimationFps::Refresh => "Refresh",
                }
            },
            animation_fps_cap,
            animation_fps_is_adaptive,
        );
        // Atomic cell that all per-frame paths read for the current
        // effective cap (0 = no cap / refresh-paced). The dynamic
        // adapter writes through this so changes flow without
        // recompiling closures.
        let animation_fps_cap_atomic = Arc::new(std::sync::atomic::AtomicU32::new(
            animation_fps_cap.unwrap_or(0),
        ));
        // Adapter only spun up in Adaptive mode. `Fixed` / `Refresh`
        // and legacy `Some(n)` skip it — the framework respects what
        // the app picked.
        let fps_adapter: Option<Arc<Mutex<FpsAdapter>>> = if animation_fps_is_adaptive {
            Some(Arc::new(Mutex::new(FpsAdapter::new(
                animation_fps_cap.unwrap_or(60),
            ))))
        } else {
            None
        };
        // Capture animation_thread_mode pre-move (config gets moved
        // into `create_event_loop_with_config` below). Drives whether
        // the AnimationScheduler spawns its bg thread or relies on
        // the main thread's per-frame `tick()` call (Phase 3) to
        // advance springs / keyframes / timelines / tick_callbacks.
        let animation_thread_mode = config.animation_thread_mode;
        let event_loop = platform
            .create_event_loop_with_config(config)
            .map_err(|e| BlincError::Platform(e.to_string()))?;

        // Get a wake proxy to allow the animation thread to wake up the event loop
        let wake_proxy = event_loop.wake_proxy();
        // Clone for the open_window callback
        let wake_proxy_for_windows = event_loop.wake_proxy();

        // Clone for the redraw-chain pacing path. When `animation_fps_cap`
        // is set, the chain calls `wake_at` on this proxy instead of
        // `request_redraw`, so the platform shim's lazy timer thread
        // delivers the next frame after the configured delay.
        let wake_proxy_for_pacing = event_loop.wake_proxy();

        // Frame-dirty flag. The OS sends `Event::Frame` at vsync to focused
        // windows whether or not we asked for a redraw, which means a
        // statically-rendered focused UI was burning CPU re-rendering an
        // identical scene every ~16 ms. We now skip the entire frame
        // handler when this flag is `false` at frame entry. Any mutation
        // we care about — input event, lifecycle event, scheduler wake
        // (set by the wake callback below), end-of-frame signal indicating
        // ongoing work — flips it back to `true`. Initial value is `true`
        // so the first frame always renders.
        let frame_dirty = Arc::new(AtomicBool::new(true));
        let frame_dirty_for_wake = Arc::clone(&frame_dirty);

        // If the `hot-reload` feature is on AND we're a child of
        // `dx serve --hot-patch`, spawn the websocket client that
        // sends our ASLR offset to the dev-server and applies
        // incoming jump-table patches. The wake closure is what lets
        // the patch thread nudge the event loop out of
        // `ControlFlow::Wait` after a patch lands — without it the
        // render tree only refreshes on the next natural redraw
        // (mouse move, focus change, etc.), which is not what the
        // user wants from a hot-reload signal.
        //
        // The closure also flips `frame_dirty` because Event::Frame
        // returns early when both `frame_dirty` and `peek_needs_redraw`
        // are false. Without that flip, `wake_proxy.wake()` reaches
        // the event loop and `request_redraw()` fires, but the
        // resulting Event::Frame bails before our rebuild check at
        // line ~4244 ever runs — the patch lands silently and the
        // window doesn't update until something else happens to
        // dirty the frame.
        //
        // No-op when the dx env vars aren't set, which is every
        // normal `cargo run` invocation, so the call is safe to make
        // unconditionally here.
        #[cfg(feature = "hot-reload")]
        {
            let wp = event_loop.wake_proxy();
            let frame_dirty_hr = Arc::clone(&frame_dirty);
            crate::hot_reload::connect(move || {
                frame_dirty_hr.store(true, Ordering::Release);
                wp.wake();
            });
        }

        // Cross-thread mirror of the renderer's `visible_anim_active`
        // flag. The end-of-frame chain (main thread) writes the
        // current frame's value here; the scheduler's wake callback
        // (bg thread) reads it. When `false`, the scheduler's
        // periodic ticks for off-screen-only animations don't kick
        // the main thread — the chain dies until input or scroll
        // brings the animation back into view. Starts `true` so the
        // very first scheduler activity wakes the main thread to
        // render the initial frame.
        let visible_anim_for_wake = Arc::new(AtomicBool::new(true));
        let visible_anim_for_wake_cb = Arc::clone(&visible_anim_for_wake);

        // Shared dirty flag for element refs
        let ref_dirty_flag: RefDirtyFlag = Arc::new(AtomicBool::new(false));
        // Process-global reactive graph + dirty flag — shared with bare
        // `signal()` / `effect()` / `computed()` callers in user code so
        // dependency tracking spans both paths.
        let reactive: SharedReactiveGraph = blinc_core::reactive::global_graph();
        // Shared hook state for use_state persistence
        let hooks: SharedHookState = Arc::new(Mutex::new(HookState::new()));

        // Initialize global context state singleton (if not already initialized)
        // This allows components to create internal state without context parameters
        if !BlincContextState::is_initialized() {
            #[allow(clippy::type_complexity)]
            let stateful_callback: std::sync::Arc<dyn Fn(&[SignalId]) + Send + Sync> =
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

        // Outside the init guard on purpose. Anything that builds a
        // widget before the window opens initialises the context state
        // itself -- `BlincDsl` does, through `ensure_context_state` --
        // and the guard above then skips, leaving no notifier. A bare
        // `Signal::set` would mark the signal dirty and reach no
        // `Stateful`, so a bound widget only caught up when some
        // unrelated interaction rebuilt it.
        {
            #[allow(clippy::type_complexity)]
            let notifier: std::sync::Arc<dyn Fn(&[SignalId]) + Send + Sync> =
                Arc::new(|signal_ids| {
                    blinc_layout::check_stateful_deps(signal_ids);
                });
            blinc_core::reactive::set_stateful_deps_notifier(move |ids| notifier(ids));
        }

        // Shared animation scheduler for spring/keyframe animations
        // Runs on background thread so animations continue even when window loses focus
        let mut scheduler = AnimationScheduler::new();
        // The wake callback gets installed in either thread mode.
        // Used by:
        //   * Bg thread (when `start_background()` is called below) —
        //     fires on idle→active edge so the main thread wakes up
        //     and starts rendering.
        //   * `notify_active()` paths inside the scheduler
        //     (`add_spring` / `add_keyframe` / etc.) — fires whenever
        //     an animation gets registered. Necessary in
        //     `AnimationThreadMode::Main` for off-thread registrations
        //     (custom timer thread, async task) to trigger a render;
        //     redundant-but-harmless in `Background` since the bg
        //     thread also fires it on its idle→active edge.
        // Marks `frame_dirty` so the resulting Event::Frame actually
        // renders.
        let _ = visible_anim_for_wake_cb; // wake gate moved into scheduler edge trigger
        scheduler.set_wake_callback(move || {
            frame_dirty_for_wake.store(true, Ordering::Release);
            wake_proxy.wake();
        });
        // Spawn the bg ticking thread only in `Background` mode. In
        // `Main` (default), the windowed app's Phase 3 calls
        // `RenderState::tick` which in turn calls
        // `AnimationScheduler::tick` synchronously — eliminating phase
        // jitter and removing one thread from the runtime. The
        // `tick()` method itself is a no-op when a bg thread is
        // running, so the two paths are mutually exclusive.
        match animation_thread_mode {
            blinc_platform::AnimationThreadMode::Background => {
                // wasm32 has no background thread — animations tick via
                // requestAnimationFrame — so this is a no-op there.
                #[cfg(not(target_arch = "wasm32"))]
                scheduler.start_background();
            }
            blinc_platform::AnimationThreadMode::Main => {
                // No bg thread. Main-thread tick handles it.
            }
        }
        let animations: SharedAnimationScheduler = Arc::new(Mutex::new(scheduler));

        // Set global scheduler handle for StateContext and component access
        {
            let scheduler_handle = animations.lock().unwrap().handle();
            blinc_animation::set_global_scheduler(scheduler_handle);
        }

        // Shared CSS animation/transition store. CSS ticking happens
        // synchronously on the main thread (Phase 3 of the frame loop)
        // to avoid phase jitter; the bg scheduler thread does not drive
        // it. Once a CSS animation/transition is live, the main thread
        // self-perpetuates via `request_redraw()` at the end of the
        // frame as long as `css_needs_redraw` is true (see Phase 5).
        // No keep-alive scheduler callback is needed — the bg thread
        // can stay parked while only CSS work is in flight.
        let css_anim_store = Arc::new(Mutex::new(blinc_layout::CssAnimationStore::new()));

        // Shared element registry for query API
        let element_registry: SharedElementRegistry =
            Arc::new(blinc_layout::selector::ElementRegistry::new());

        // Set up query callback in BlincContextState so components can query elements globally
        {
            let registry_for_query = Arc::clone(&element_registry);
            let query_callback: blinc_core::QueryCallback = Arc::new(move |id: &str| {
                registry_for_query.get(id).map(|node_id| node_id.to_raw())
            });
            BlincContextState::get().set_query_callback(query_callback);
        }

        // Set up bounds callback for ElementHandle.bounds()
        {
            let registry_for_bounds = Arc::clone(&element_registry);
            let bounds_callback: blinc_core::BoundsCallback =
                Arc::new(move |id: &str| registry_for_bounds.get_bounds(id));
            BlincContextState::get().set_bounds_callback(bounds_callback);
        }

        // Store element registry in BlincContextState for global query() function
        // Cast to Arc<dyn Any + Send + Sync> for type-erased storage
        BlincContextState::get()
            .set_element_registry(Arc::clone(&element_registry) as blinc_core::AnyElementRegistry);

        // Shared storage for on_ready callbacks
        let ready_callbacks: SharedReadyCallbacks = Arc::new(Mutex::new(Vec::new()));

        // Set up continuous redraw callback for text widget cursor animation
        // This bridges text widgets (which track focus) with the animation scheduler (which drives redraws)
        {
            let animations_for_callback = Arc::clone(&animations);
            blinc_layout::widgets::set_continuous_redraw_callback(move |enabled| {
                if let Ok(scheduler) = animations_for_callback.lock() {
                    scheduler.set_continuous_redraw(enabled);
                }
            });
        }

        // Connect theme animation to the animation scheduler
        // This enables smooth color transitions when switching between light/dark mode
        blinc_theme::ThemeState::get().set_scheduler(&animations);

        // Render state: dynamic properties that update every frame without tree rebuild
        // This includes cursor blink, animated colors, hover states, etc.
        let mut render_state: Option<blinc_layout::RenderState> = None;

        // Shared motion states for query API access
        // This allows components to query motion animation state via query_motion()
        let shared_motion_states = blinc_layout::create_shared_motion_states();

        // Set up motion state callback in BlincContextState
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

        // Overlay manager for modals, dialogs, toasts, etc.
        let overlays: OverlayManager = overlay_manager();

        // Initialize overlay context singleton for component access
        if !OverlayContext::is_initialized() {
            OverlayContext::init(Arc::clone(&overlays));
        }

        // Primary window state
        let mut ws = WindowState::new(
            Arc::clone(&css_anim_store),
            Arc::clone(&shared_motion_states),
        );
        ws.transparent = primary_transparent;
        // Track primary window ID once known
        let mut primary_wid: Option<blinc_platform::WindowId> = None;
        // Secondary windows (opened via ctx.open_window())
        let mut secondary_windows: std::collections::HashMap<
            blinc_platform::WindowId,
            WindowState,
        > = std::collections::HashMap::new();
        // UI builders for secondary windows (queued via open_window)
        // For now secondary windows get a blank UI — full UI builder support is future work

        event_loop
            .run(move |event, window| {
                // Mark the next frame dirty for any non-Frame event. Input,
                // lifecycle changes, drag/drop, etc. are all "something
                // happened" signals — the next OS frame should actually
                // render rather than skip. Frame events are the OS asking
                // us to render; whether we should is decided below by the
                // `frame_dirty` swap at the top of `Event::Frame`.
                //
                // Exception: bare mouse moves are too frequent to flip
                // unconditionally (60–120 events/s during drag and hover).
                // For those we let the input handler decide whether anything
                // visible changed; if a hover handler / Stateful dispatch
                // fires, it sets `NEEDS_REDRAW`, which the `Event::Frame`
                // gate also honours. Skipping the blanket flip here keeps
                // a static UI from re-rendering at vsync just because the
                // pointer is in motion.
                //
                // We pair the dirty flip with a `request_redraw()` because
                // under `ControlFlow::Wait` (set by the desktop platform
                // shim — Linux/Wayland/X11 had no other pacing and burned
                // 25% CPU just spinning the loop in Poll) `frame_dirty`
                // alone does nothing; winit only delivers the next
                // `RedrawRequested → Event::Frame` if someone actually
                // asks for it. macOS used to coast on Poll's
                // request_redraw spam, which we removed at the same time.
                // Per-event trace for debugging idle-CPU / mouse-move
                // hot paths. Silent in normal builds (`tracing::trace!`
                // doesn't evaluate args when the target is disabled).
                // Run with
                //   RUST_LOG=blinc_app::events=trace
                // and the output gets one line per event; pipe through
                //   `grep -oE 'kind=[^ ]+' | sort | uniq -c`
                // to see counts per event-kind over a sampling window.
                tracing::trace!(
                    target: "blinc_app::events",
                    kind = match &event {
                        Event::Frame(_) => "frame",
                        Event::Input(_, InputEvent::Mouse(MouseEvent::Moved { .. })) =>
                            "input.mouse.moved",
                        Event::Input(_, InputEvent::Mouse(MouseEvent::ButtonPressed { .. })) =>
                            "input.mouse.pressed",
                        Event::Input(_, InputEvent::Mouse(MouseEvent::ButtonReleased { .. })) =>
                            "input.mouse.released",
                        Event::Input(_, InputEvent::Mouse(MouseEvent::Entered)) =>
                            "input.mouse.entered",
                        Event::Input(_, InputEvent::Mouse(MouseEvent::Left)) =>
                            "input.mouse.left",
                        Event::Input(_, InputEvent::Scroll { .. }) => "input.scroll",
                        Event::Input(_, InputEvent::ScrollEnd) => "input.scroll_end",
                        Event::Input(_, InputEvent::Keyboard(_)) => "input.keyboard",
                        Event::Input(_, InputEvent::Touch(_)) => "input.touch",
                        Event::Input(_, InputEvent::Pinch { .. }) => "input.pinch",
                        Event::Input(_, InputEvent::Rotation { .. }) => "input.rotation",
                        Event::Input(_, _) => "input.other",
                        Event::Window(_, WindowEvent::Resized { .. }) => "window.resized",
                        Event::Window(_, WindowEvent::Moved { .. }) => "window.moved",
                        Event::Window(_, WindowEvent::CloseRequested) => "window.close_requested",
                        Event::Window(_, WindowEvent::Focused(_)) => "window.focused",
                        Event::Window(_, WindowEvent::ScaleFactorChanged { .. }) =>
                            "window.scale_factor_changed",
                        Event::Window(_, WindowEvent::DroppedFileHovered { .. }) =>
                            "window.dropped_file_hovered",
                        Event::Window(_, WindowEvent::DroppedFile { .. }) =>
                            "window.dropped_file",
                        Event::Window(_, WindowEvent::DroppedFileCancelled) =>
                            "window.dropped_file_cancelled",
                        Event::Lifecycle(LifecycleEvent::Resumed) => "lifecycle.resumed",
                        Event::Lifecycle(LifecycleEvent::Suspended) => "lifecycle.suspended",
                        Event::Lifecycle(LifecycleEvent::LowMemory) => "lifecycle.low_memory",
                    },
                    "blinc event"
                );

                let is_bare_mouse_move = matches!(
                    event,
                    Event::Input(_, InputEvent::Mouse(MouseEvent::Moved { .. }))
                );
                let event_wid = match &event {
                    Event::Window(wid, _) | Event::Input(wid, _) | Event::Frame(wid) => Some(*wid),
                    _ => None,
                };
                let is_secondary = event_wid
                    .map(|wid| primary_wid.is_some_and(|p| wid != p))
                    .unwrap_or(false);
                // Mouse button press/release on a non-interactive area
                // doesn't need to schedule a redraw — if the click fires
                // a handler that mutates state, the post-dispatch check
                // (`peek_needs_redraw || has_pending_subtree_rebuilds`)
                // at the bottom of the input branch will arm one. Without
                // this gate, every click on empty space ran two full
                // renders (press + release) on cn_demo's complex tree,
                // burning ~30 % CPU for rapid clicks where nothing
                // visible changes. Scroll/keyboard/touch events kept
                // unconditional because their handlers commonly update
                // physics or input state via paths that don't go through
                // `NEEDS_REDRAW` (scroll offset, IME composition).
                let is_mouse_button = matches!(
                    event,
                    Event::Input(
                        _,
                        InputEvent::Mouse(
                            MouseEvent::ButtonPressed { .. } | MouseEvent::ButtonReleased { .. }
                        )
                    )
                );
                if !matches!(event, Event::Frame(_)) && !is_bare_mouse_move && !is_mouse_button {
                    frame_dirty.store(true, Ordering::Release);
                    window.request_redraw();
                }

                // Bare-mouse-move fast path. Linux high-rate mice
                // (gaming mice on Hyprland in particular) deliver
                // 1 kHz `CursorMoved` events; running the full input
                // pipeline (Vec scratch alloc, `Box<dyn FnMut>`
                // event-callback alloc, hit_test, hover diff, cursor
                // resolve) once per move puts the process at ~60% of
                // a CPU on `hello_blinc` even though no element in
                // the tree could react. The Moved branch already
                // had an early-return guard with the same predicate,
                // but the prelude that gets us there allocated and
                // destructed unconditionally — so the branch ran
                // 1000 times/sec on a UI that needs zero pointer
                // work.
                //
                // The cached predicate on `RenderTree` is one
                // relaxed atomic load; recomputed lazily on next
                // read after a tree mutation invalidates it. For a
                // static UI with no handlers / no `:hover` / no
                // `cursor:` styles it stays `false` from the first
                // call after build until the next rebuild, so this
                // branch returns ~immediately and the closure exits
                // without touching `ws.ctx`, `ws.render_tree`, or
                // any allocator.
                if is_bare_mouse_move && !is_secondary {
                    let pipeline_needed = ws
                        .render_tree
                        .as_ref()
                        .is_some_and(|tree| tree.mouse_move_pipeline_needed());
                    if !pipeline_needed {
                        return ControlFlow::Continue;
                    }
                    // Cursor-only fast path. When the tree has cursor
                    // styles but no pointer handlers / hover rules, the
                    // entire input branch below ends up allocating two
                    // `Vec<PendingEvent>`s + boxing a `FnMut` event
                    // callback per move just to fall into the cursor-
                    // resolve early-return at the very top. For
                    // `hello_blinc` (a single `text()` with the default
                    // I-beam cursor style) that allocation churn was the
                    // entire 11 % CPU baseline during continuous mouse
                    // movement. Resolving cursor inline up here keeps
                    // the bare-move-only-needs-cursor case allocation-
                    // free and reuses the `last_cursor` cache so the
                    // OS `set_cursor` syscall only fires on transitions.
                    let needs_pointer_dispatch = ws
                        .render_tree
                        .as_ref()
                        .is_some_and(|tree| {
                            tree.handler_registry().has_any_pointer_handler()
                                || tree
                                    .stylesheet()
                                    .is_some_and(|s| s.has_pointer_state_rules())
                        });
                    if !needs_pointer_dispatch {
                        if let (Some(blinc_ctx), Some(tree)) =
                            (&mut ws.ctx, &ws.render_tree)
                        {
                            if let Event::Input(
                                _,
                                InputEvent::Mouse(MouseEvent::Moved { x, y }),
                            ) = &event
                            {
                                let scale = blinc_ctx.scale_factor as f32;
                                let lx = *x / scale;
                                let ly = *y / scale;
                                let cursor = tree
                                    .get_cursor_at(&blinc_ctx.event_router, lx, ly)
                                    .unwrap_or(CursorStyle::Default);
                                let want = convert_cursor_style(cursor);
                                if ws.last_cursor != Some(want) {
                                    window.set_cursor(want);
                                    ws.last_cursor = Some(want);
                                }
                            }
                        }
                        return ControlFlow::Continue;
                    }
                }

                // Handle secondary window events
                if is_secondary {
                    let wid = event_wid.unwrap();
                    match event {
                        Event::Window(_, WindowEvent::Resized { width, height }) => {
                            if let Some(sws) = secondary_windows.get_mut(&wid) {
                                if let (Some(surf), Some(config)) =
                                    (&sws.surface, &mut sws.surface_config)
                                {
                                    if width > 0 && height > 0 {
                                        config.width = width;
                                        config.height = height;
                                        if let Some(ref blinc_app) = ws.app {
                                            surf.configure(blinc_app.device(), config);
                                        }
                                        sws.needs_rebuild = true;
                                        if let Some(ref mut ctx) = sws.ctx {
                                            let sf = window.scale_factor();
                                            ctx.width = width as f32 / sf as f32;
                                            ctx.height = height as f32 / sf as f32;
                                            ctx.physical_width = width as f32;
                                            ctx.physical_height = height as f32;
                                            ctx.scale_factor = sf;
                                        }
                                    }
                                }
                            }
                        }
                        Event::Window(_, WindowEvent::CloseRequested) => {
                            secondary_windows.remove(&wid);
                            tracing::info!("Secondary window closed (wid={:?})", wid);
                        }
                        Event::Window(_, WindowEvent::Focused(_focused)) => {}
                        Event::Input(_, ref input_event) => {

                            if let Some(sws) = secondary_windows.get_mut(&wid) {
                                if let (Some(ctx), Some(tree)) =
                                    (&mut sws.ctx, &mut sws.render_tree)
                                {
                                    let sf = ctx.scale_factor as f32;

                                    // Collect events from the router
                                    let mut pending: Vec<(blinc_layout::tree::LayoutNodeId, u32)> =
                                        Vec::new();
                                    ctx.event_router.set_event_callback({
                                        let events =
                                            &mut pending as *mut Vec<(blinc_layout::tree::LayoutNodeId, u32)>;
                                        move |node, event_type| unsafe {
                                            (*events).push((node, event_type));
                                        }
                                    });

                                    let convert_button =
                                        |b: &blinc_platform::MouseButton| match b {
                                            blinc_platform::MouseButton::Left => {
                                                blinc_layout::prelude::MouseButton::Left
                                            }
                                            blinc_platform::MouseButton::Right => {
                                                blinc_layout::prelude::MouseButton::Right
                                            }
                                            blinc_platform::MouseButton::Middle => {
                                                blinc_layout::prelude::MouseButton::Middle
                                            }
                                            _ => blinc_layout::prelude::MouseButton::Left,
                                        };

                                    match input_event {
                                        InputEvent::Mouse(MouseEvent::Moved { x, y }) => {
                                            ctx.event_router
                                                .on_mouse_move(tree, *x / sf, *y / sf);
                                        }
                                        InputEvent::Mouse(MouseEvent::ButtonPressed {
                                            button,
                                            x,
                                            y,
                                        }) => {
                                            // Mark this event as mouse (not
                                            // touch) input so editable widgets
                                            // restore desktop semantics
                                            // (drag = extend selection). The
                                            // flag is sticky between events
                                            // and gets flipped back to true
                                            // by the touch path on
                                            // touchscreens — desktop runners
                                            // don't see touch events at all,
                                            // but a docked tablet running
                                            // the desktop runner could mix
                                            // both, so we set this on every
                                            // mouse press to be safe.
                                            blinc_layout::widgets::text_input::set_touch_input(false);
                                            ctx.event_router.on_mouse_down(
                                                tree,
                                                *x / sf,
                                                *y / sf,
                                                convert_button(button),
                                            );
                                        }
                                        InputEvent::Mouse(MouseEvent::ButtonReleased {
                                            button,
                                            x,
                                            y,
                                        }) => {
                                            ctx.event_router.on_mouse_up(
                                                tree,
                                                *x / sf,
                                                *y / sf,
                                                convert_button(button),
                                            );
                                        }
                                        _ => {}
                                    }

                                    ctx.event_router.clear_event_callback();

                                    // Dispatch collected events through render tree handlers
                                    for (node_id, event_type) in &pending {
                                        tree.dispatch_event(
                                            *node_id,
                                            *event_type,
                                            ctx.event_router.mouse_position().0,
                                            ctx.event_router.mouse_position().1,
                                        );
                                    }
                                }
                            }
                        }
                        Event::Frame(_) => {
                            if let Some(sws) = secondary_windows.get_mut(&wid) {

                                if let (Some(blinc_app), Some(surf), Some(config)) =
                                    (&mut ws.app, &sws.surface, &sws.surface_config)
                                {
                                    // Build render tree on first frame or after resize
                                    if sws.render_tree.is_none() || sws.needs_rebuild {
                                        let (w, h) = sws.ctx.as_ref()
                                            .map(|c| (c.width, c.height))
                                            .unwrap_or((400.0, 300.0));

                                        let ui: Div =
                                            if let Some(ref mut builder) = sws.ui_builder {
                                                if let Some(ref mut sctx) = sws.ctx {
                                                    invoke_window_builder(builder, sctx)
                                                } else {
                                                    div().w(w).h(h)
                                                }
                                            } else {
                                                let title = window.winit_window().title();
                                                div()
                                                    .w(w)
                                                    .h(h)
                                                    .bg(blinc_core::Color::rgba(
                                                        0.06, 0.06, 0.09, 1.0,
                                                    ))
                                                    .flex_col()
                                                    .justify_center()
                                                    .items_center()
                                                    .gap_px(12.0)
                                                    .child(
                                                        text(&title)
                                                            .size(24.0)
                                                            .color(blinc_core::Color::WHITE)
                                                            .bold(),
                                                    )
                                                    .child(
                                                        text(format!("{:.0} x {:.0}", w, h))
                                                            .size(14.0)
                                                            .color(blinc_core::Color::rgba(
                                                                0.5, 0.5, 0.6, 1.0,
                                                            )),
                                                    )
                                            };

                                        let sf = sws
                                            .ctx
                                            .as_ref()
                                            .map(|c| c.scale_factor as f32)
                                            .unwrap_or(1.0);
                                        let mut tree = RenderTree::from_element(&ui);
                                        tree.set_scale_factor(sf);
                                        tree.compute_layout(w, h);
                                        sws.render_tree = Some(tree);
                                        sws.needs_rebuild = false;
                                    }

                                    // Render the tree (skip if minimized / zero size)
                                    if config.width > 0 && config.height > 0 {
                                        if let (Some(tree), Some(rs)) =
                                            (&sws.render_tree, &sws.render_state)
                                        {
                                            match surf.get_current_texture() {
                                                Ok(frame) => {
                                                    let view = frame.texture.create_view(
                                                        &wgpu::TextureViewDescriptor::default(),
                                                    );
                                                    blinc_app.set_clear_alpha(if sws.transparent {
                                                        0.0
                                                    } else {
                                                        1.0
                                                    });
                                                    let _ = blinc_app.render_tree_with_motion(
                                                        tree,
                                                        rs,
                                                        &view,
                                                        config.width,
                                                        config.height,
                                                    );
                                                    window.pre_present_notify();
                                                    frame.present();
                                                }
                                                Err(
                                                    wgpu::SurfaceError::Lost
                                                    | wgpu::SurfaceError::Outdated,
                                                ) => {
                                                    surf.configure(blinc_app.device(), config);
                                                }
                                                Err(_) => {}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    return ControlFlow::Continue;
                }

                match event {
                    Event::Lifecycle(LifecycleEvent::Resumed) => {
                        let wid = window.id();
                        // Initialize GPU if not already done (primary window)
                        if ws.app.is_none() {
                            primary_wid = Some(wid);
                            let winit_window = window.winit_window_arc();

                            match BlincApp::with_window(winit_window, None) {
                                Ok((mut blinc_app, surf)) => {
                                    let (width, height) = window.size();
                                    // Use the same texture format that the renderer's pipelines use
                                    let format = blinc_app.texture_format();
                                    let alpha_mode = Self::pick_alpha_mode(
                                        &surf,
                                        blinc_app.adapter(),
                                        ws.transparent,
                                    );
                                    if ws.transparent {
                                        blinc_app.set_clear_alpha(0.0);
                                    }
                                    let config = wgpu::SurfaceConfiguration {
                                        // COPY_DST is required by the layer
                                        // compositor's `composite_frame` step —
                                        // it `copy_texture_to_texture`s the
                                        // cached static layer into the surface
                                        // before drawing canvas overlays on top.
                                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                                            | wgpu::TextureUsages::COPY_SRC
                                            | wgpu::TextureUsages::COPY_DST,
                                        format,
                                        width,
                                        height,
                                        present_mode: preferred_present_mode(
                                            &surf,
                                            blinc_app.adapter(),
                                        ),
                                        alpha_mode,
                                        view_formats: vec![],
                                        desired_maximum_frame_latency: primary_max_frame_latency,
                                    };
                                    surf.configure(blinc_app.device(), &config);

                                    // Update text measurer with shared font registry for accurate measurement
                                    crate::text_measurer::init_text_measurer_with_registry(
                                        blinc_app.font_registry(),
                                    );

                                    // Adapt the scheduler's tick rate to the display's
                                    // refresh rate, capped further if the app set
                                    // `animation_fps_cap`. The scheduler ticking
                                    // faster than the paint cap just produces
                                    // intermediate values that get overwritten
                                    // before the next paint reads them — wasted
                                    // CPU on every spring / keyframe / timeline
                                    // step. Match the effective paint cadence so
                                    // each scheduler tick corresponds to one paint
                                    // attempt.
                                    //
                                    // Winit returns refresh in millihertz; clamp to a
                                    // sane range so a 240/360 Hz display doesn't pin
                                    // a CPU and a missing/zero report doesn't drop us
                                    // to 0 fps.
                                    {
                                        let refresh = window
                                            .winit_window()
                                            .current_monitor()
                                            .and_then(|m| m.refresh_rate_millihertz())
                                            .map(|mhz| (mhz / 1000).clamp(30, 120))
                                            .unwrap_or(60);
                                        let effective_fps = match animation_fps_cap {
                                            Some(cap) => refresh.min(cap),
                                            None => refresh,
                                        };
                                        // Cap the adaptive ladder at the panel
                                        // refresh so it never climbs past vsync
                                        // (e.g. 60→90→120 on a 60 Hz NUC panel,
                                        // which beat-judders the spinner + scroll
                                        // decel). On Wayland `refresh_rate_*`
                                        // often reports None → 60, the correct
                                        // default ceiling.
                                        if let Some(adapter) = fps_adapter.as_ref() {
                                            if let Ok(mut a) = adapter.lock() {
                                                a.set_ceiling(refresh);
                                            }
                                        }
                                        if let Ok(mut sched) = animations.lock() {
                                            sched.set_target_fps(effective_fps);
                                            tracing::debug!(
                                                "Scheduler target_fps set to {} Hz (refresh={}, cap={:?})",
                                                effective_fps,
                                                refresh,
                                                animation_fps_cap
                                            );
                                        }
                                    }

                                    #[cfg(target_os = "linux")]
                                    {
                                        // On Wayland, once the scene goes static the
                                        // event loop parks (epoll) and stops
                                        // presenting; the compositor then stops
                                        // releasing swapchain buffers and the next
                                        // get_current_texture() starves for ~1s
                                        // ("paints once then dead"). Detect a genuine
                                        // Wayland backend (not X11/XWayland) and keep
                                        // the swapchain cycling: the frame loop
                                        // self-drives request_redraw(), which winit
                                        // paces to vsync through pre_present_notify —
                                        // steady ~7% CPU, not a busy spin. Override
                                        // with BLINC_KEEP_ALIVE=1/0.
                                        let on_wayland = {
                                            use raw_window_handle::{
                                                HasDisplayHandle, RawDisplayHandle,
                                            };
                                            window
                                                .winit_window()
                                                .display_handle()
                                                .map(|dh| {
                                                    matches!(
                                                        dh.as_raw(),
                                                        RawDisplayHandle::Wayland(_)
                                                    )
                                                })
                                                .unwrap_or(false)
                                        };
                                        ws.wayland_keep_alive =
                                            match std::env::var("BLINC_KEEP_ALIVE")
                                                .ok()
                                                .as_deref()
                                            {
                                                Some("0") | Some("false")
                                                | Some("off") => false,
                                                Some("1") | Some("true")
                                                | Some("on") => true,
                                                _ => on_wayland,
                                            };
                                        if ws.wayland_keep_alive {
                                            tracing::info!(
                                                "wayland keep-alive active: \
                                                 self-driven vsync-paced presents \
                                                 keep the swapchain from starving"
                                            );
                                        }
                                        ws.surface = Some(surf);
                                    }
                                    #[cfg(not(target_os = "linux"))]
                                    {
                                        ws.surface = Some(surf);
                                    }
                                    ws.surface_config = Some(config);
                                    ws.app = Some(blinc_app);

                                    // Bring up the experimental Wayland
                                    // frame-callback gate iff the feature
                                    // is enabled AND raw_window_handle
                                    // reports a Wayland surface. Anything
                                    // else (X11, macOS, Windows): the
                                    // factory returns None and the gate
                                    // stays inert, falling back to
                                    // winit's native `pre_present_notify`
                                    // gating path elsewhere in this loop.
                                    #[cfg(all(feature = "wayland-frame-gate", target_os = "linux"))]
                                    {
                                        use raw_window_handle::{
                                            HasDisplayHandle, HasWindowHandle,
                                            RawDisplayHandle, RawWindowHandle,
                                        };
                                        let winit_win = window.winit_window();
                                        if let (Ok(dh), Ok(wh)) = (
                                            winit_win.display_handle(),
                                            winit_win.window_handle(),
                                        ) {
                                            if let (
                                                RawDisplayHandle::Wayland(d),
                                                RawWindowHandle::Wayland(w),
                                            ) = (dh.as_raw(), wh.as_raw())
                                            {
                                                ws.wayland_gate =
                                                    crate::wayland_frame_gate::WaylandFrameGate::try_new_from_raw(
                                                        d.display.as_ptr(),
                                                        w.surface.as_ptr(),
                                                    );
                                                tracing::info!(
                                                    enabled = ws.wayland_gate.is_some(),
                                                    "wayland-frame-gate experimental: \
                                                     hand-rolled wl_surface::frame() callbacks \
                                                     {}",
                                                    if ws.wayland_gate.is_some() {
                                                        "active"
                                                    } else {
                                                        "construction failed — falling back to winit"
                                                    }
                                                );
                                            }
                                        }
                                    }

                                    // Initialize context with event router, animations, dirty flag, reactive graph, hooks, overlay manager, registry, and ready callbacks
                                    ws.ctx = Some(WindowedContext::from_window(
                                        window,
                                        EventRouter::new(),
                                        Arc::clone(&animations),
                                        Arc::clone(&ref_dirty_flag),
                                        Arc::clone(&reactive),
                                        Arc::clone(&hooks),
                                        Arc::clone(&overlays),
                                        Arc::clone(&element_registry),
                                        Arc::clone(&ready_callbacks),
                                    ));

                                    // Wire open_window callback using the event loop's wake proxy
                                    let wp_for_ctx = wake_proxy_for_windows.clone();
                                    let open_fn: Arc<dyn Fn(WindowConfig) + Send + Sync> =
                                        Arc::new(move |config| {
                                            wp_for_ctx.create_window(config);
                                        });
                                    if let Some(ref mut windowed_ctx) = ws.ctx {
                                        windowed_ctx.set_open_window_fn(Arc::clone(&open_fn));
                                        // Per-window action callbacks
                                        let win_actions = Self::make_window_actions(
                                            window.winit_window_arc(),
                                            wake_proxy_for_windows.clone(),
                                        );
                                        windowed_ctx.set_window_actions(
                                            win_actions.0,
                                            win_actions.1,
                                            win_actions.2,
                                            win_actions.3,
                                        );
                                    }
                                    // Register globally so open_window() works from anywhere
                                    let _ = OPEN_WINDOW_FN.set(open_fn);

                                    // Register global window action callbacks (for drag_region() on Div)
                                    Self::register_window_actions_static(window.winit_window_arc(), wake_proxy_for_windows.clone());

                                    // Set initial viewport size in BlincContextState
                                    if let Some(ref windowed_ctx) = ws.ctx {
                                        BlincContextState::get().set_viewport_size(windowed_ctx.width, windowed_ctx.height);
                                    }

                                    // Initialize render state with the shared animation scheduler
                                    // RenderState handles dynamic properties (cursor blink, animations)
                                    // independently from tree structure changes
                                    let mut rs = blinc_layout::RenderState::new(Arc::clone(&animations));
                                    rs.set_shared_motion_states(Arc::clone(&shared_motion_states));
                                    ws.render_state = Some(rs);

                                    tracing::debug!("Blinc windowed ws.app initialized");
                                }
                                Err(e) => {
                                    tracing::error!("Failed to initialize Blinc: {}", e);
                                    return ControlFlow::Exit;
                                }
                            }
                        } else {
                            // Resumed for a secondary window
                            let wid = window.id();
                            #[allow(clippy::map_entry)]
                            if !secondary_windows.contains_key(&wid) {
                                if let Some(ref blinc_app) = ws.app {
                                    // Pop the matching pending request up front so we can use
                                    // its `max_frame_latency` on the surface config below.
                                    // Match by title — winit's `WindowId` isn't known to the
                                    // pending side, and titles are practically unique per
                                    // open_window call.
                                    let pending_req = {
                                        let winit_title = window.winit_window().title();
                                        pending_builders().lock().ok().and_then(|mut p| {
                                            p.iter()
                                                .position(|r| r.config.title == winit_title)
                                                .map(|idx| p.remove(idx))
                                        })
                                    };
                                    let secondary_latency = pending_req
                                        .as_ref()
                                        .map(|r| r.config.max_frame_latency.clamp(1, 3))
                                        .unwrap_or(2);
                                    let winit_window = window.winit_window_arc();
                                    match blinc_app.create_surface_for_window(winit_window) {
                                        Ok(surf) => {
                                            let (w, h) = window.size();
                                            let format = blinc_app.texture_format();
                                            let window_transparent = window.is_transparent();
                                            let alpha_mode = Self::pick_alpha_mode(
                                                &surf,
                                                blinc_app.adapter(),
                                                window_transparent,
                                            );
                                            let config = wgpu::SurfaceConfiguration {
                                                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                                                    | wgpu::TextureUsages::COPY_SRC
                                                    | wgpu::TextureUsages::COPY_DST,
                                                format,
                                                width: w,
                                                height: h,
                                                present_mode: preferred_present_mode(
                                                    &surf,
                                                    blinc_app.adapter(),
                                                ),
                                                alpha_mode,
                                                view_formats: vec![],
                                                desired_maximum_frame_latency: secondary_latency,
                                            };
                                            surf.configure(blinc_app.device(), &config);

                                            let mut sws = WindowState::new(
                                                Arc::clone(&css_anim_store),
                                                Arc::clone(&shared_motion_states),
                                            );
                                            sws.transparent = window_transparent;
                                            sws.surface = Some(surf);
                                            sws.surface_config = Some(config);

                                            sws.ctx = Some(WindowedContext::from_window(
                                                window,
                                                EventRouter::new(),
                                                Arc::clone(&animations),
                                                Arc::clone(&ref_dirty_flag),
                                                Arc::clone(&reactive),
                                                Arc::clone(&hooks),
                                                Arc::clone(&overlays),
                                                Arc::clone(&element_registry),
                                                Arc::clone(&ready_callbacks),
                                            ));

                                            if let Some(ref mut ctx) = sws.ctx {
                                                let wp = wake_proxy_for_windows.clone();
                                                ctx.set_open_window_fn(Arc::new(move |c| {
                                                    wp.create_window(c);
                                                }));
                                                // Per-window actions
                                                let win_actions = Self::make_window_actions(
                                                    window.winit_window_arc(),
                                                    wake_proxy_for_windows.clone(),
                                                );
                                                ctx.set_window_actions(
                                                    win_actions.0,
                                                    win_actions.1,
                                                    win_actions.2,
                                                    win_actions.3,
                                                );
                                            }

                                            let mut rs = blinc_layout::RenderState::new(
                                                Arc::clone(&animations),
                                            );
                                            rs.set_shared_motion_states(
                                                Arc::clone(&shared_motion_states),
                                            );
                                            sws.render_state = Some(rs);

                                            // Adopt the UI builder from the pending request
                                            // that we already popped above (along with the
                                            // surface frame latency).
                                            if let Some(req) = pending_req {
                                                sws.ui_builder = req.builder;
                                            }

                                            // Per-window callbacks are set via set_window_actions above.
                                            // Global window_actions is NOT set — secondary windows
                                            // use ctx.close_callback() etc. instead.

                                            secondary_windows.insert(wid, sws);
                                            tracing::info!(
                                                "Secondary window initialized (wid={:?})",
                                                wid
                                            );
                                        }
                                        Err(e) => {
                                            tracing::error!(
                                                "Failed to create surface for window: {}",
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }

                    Event::Window(_, WindowEvent::Resized { width, height }) => {
                        if let (Some(blinc_app), Some(surf), Some(config)) =
                            (&mut ws.app, &ws.surface, &mut ws.surface_config)
                        {
                            // winit fires a spurious Resized event when the window is first
                            // mapped, with the same dimensions used to configure the surface.
                            // Rebuilding on that no-op resize triggers a double initial build
                            // (visible as duplicated `build_ui` side effects) and — more
                            // critically — clobbers Stateful-handle state from the first build
                            // that downstream canvases depend on, so sketches wired up during
                            // the initial build stop painting after the phantom rebuild.
                            // Short-circuit when neither axis actually changed.
                            let dims_changed =
                                config.width != width || config.height != height;
                            if width > 0 && height > 0 && dims_changed {
                                config.width = width;
                                config.height = height;
                                surf.configure(blinc_app.device(), config);
                                ws.needs_rebuild = true;
                                ws.needs_relayout = true;
                                // Resize invalidates every cached primitive
                                // position. Without this, the slow-path
                                // rebuild repopulates the cache but
                                // canvas-backed subtrees can still get
                                // their bounds resolved against the old
                                // cached layout because the static-cache
                                // texture itself isn't dropped. Match the
                                // ScaleFactorChanged handler's posture by
                                // tagging the invalidation explicitly.
                                blinc_app
                                    .invalidate_render_cache_tagged("window_resized");
                                frame_dirty.store(true, Ordering::Release);

                                // Dispatch RESIZE event to elements (use logical dimensions)
                                if let (Some(windowed_ctx), Some(tree)) =
                                    (&mut ws.ctx, &ws.render_tree)
                                {
                                    let logical_width = width as f32 / windowed_ctx.scale_factor as f32;
                                    let logical_height = height as f32 / windowed_ctx.scale_factor as f32;

                                    // Update windowed context dimensions - CRITICAL for layout computation
                                    // Without this, compute_layout uses stale dimensions
                                    windowed_ctx.width = logical_width;
                                    windowed_ctx.height = logical_height;
                                    windowed_ctx.physical_width = width as f32;
                                    windowed_ctx.physical_height = height as f32;

                                    // Update viewport size in BlincContextState for ElementHandle.is_visible()
                                    BlincContextState::get().set_viewport_size(logical_width, logical_height);

                                    windowed_ctx
                                        .event_router
                                        .on_window_resize(tree, logical_width, logical_height);

                                    // Clear layout bounds storages to force fresh calculations
                                    // This prevents stale cached bounds from influencing the new layout
                                    tree.clear_layout_bounds_storages();
                                }

                                // Request redraw to trigger relayout with new dimensions
                                window.request_redraw();
                            }
                        }
                    }

                    // Multi-monitor: window dragged onto a monitor with a different
                    // DPI / scale factor. Without this handler `windowed_ctx.scale_factor`
                    // stays stale, the next `Resized` computes logical dims against the
                    // old DPI, and the surface (which winit may have invalidated during
                    // the monitor handoff) goes un-reconfigured — observed as a crash
                    // or wrong-sized render the first time the window crosses monitors.
                    //
                    // Guard against the no-op case: macOS fires `ScaleFactorChanged`
                    // early in window setup to report the real backing scale (1.0 → 2.0
                    // on a Retina display). When the reported factor matches what the
                    // context already has, the rebuild this handler used to force was
                    // pure waste — observed as "build_ui called twice on launch" on
                    // cn_demo. Only fire the rebuild path when scale, physical dims,
                    // or surface config actually changed.
                    Event::Window(_, WindowEvent::ScaleFactorChanged { scale_factor }) => {
                        let (phys_w, phys_h) = window.size();
                        let prev_scale = ws.ctx.as_ref().map(|c| c.scale_factor).unwrap_or(0.0);
                        let prev_phys_w = ws.ctx.as_ref().map(|c| c.physical_width).unwrap_or(0.0);
                        let prev_phys_h = ws.ctx.as_ref().map(|c| c.physical_height).unwrap_or(0.0);
                        let scale_changed = (scale_factor - prev_scale).abs() > f64::EPSILON;
                        let dims_changed = (phys_w as f32 - prev_phys_w).abs() > f32::EPSILON
                            || (phys_h as f32 - prev_phys_h).abs() > f32::EPSILON;
                        if !scale_changed && !dims_changed {
                            // No-op event — common at startup on macOS Retina.
                            return ControlFlow::Continue;
                        }

                        if let (Some(blinc_app), Some(surf), Some(config)) =
                            (&ws.app, &ws.surface, &mut ws.surface_config)
                        {
                            if phys_w > 0
                                && phys_h > 0
                                && (config.width != phys_w || config.height != phys_h)
                            {
                                config.width = phys_w;
                                config.height = phys_h;
                                surf.configure(blinc_app.device(), config);
                            }
                        }
                        if let Some(ref mut windowed_ctx) = ws.ctx {
                            windowed_ctx.scale_factor = scale_factor;
                            windowed_ctx.physical_width = phys_w as f32;
                            windowed_ctx.physical_height = phys_h as f32;
                            let logical_width = phys_w as f32 / scale_factor as f32;
                            let logical_height = phys_h as f32 / scale_factor as f32;
                            windowed_ctx.width = logical_width;
                            windowed_ctx.height = logical_height;
                            BlincContextState::get()
                                .set_viewport_size(logical_width, logical_height);
                            if let Some(ref tree) = ws.render_tree {
                                windowed_ctx
                                    .event_router
                                    .on_window_resize(tree, logical_width, logical_height);
                                tree.clear_layout_bounds_storages();
                            }
                        }
                        ws.needs_rebuild = true;
                        ws.needs_relayout = true;
                        frame_dirty.store(true, Ordering::Release);
                        window.request_redraw();
                    }

                    Event::Window(_, WindowEvent::Focused(focused)) => {
                        // Update context focus state
                        if let Some(ref mut windowed_ctx) = ws.ctx {
                            windowed_ctx.focused = focused;
                            windowed_ctx.event_router.on_window_focus(focused);

                            if !focused {
                                blinc_layout::widgets::blur_all_text_inputs();
                            }
                        }
                    }

                    Event::Window(_, WindowEvent::CloseRequested) => {
                        return ControlFlow::Exit;
                    }

                    // File drop events — dispatch to drop handler and render tree
                    Event::Window(_, WindowEvent::DroppedFile { paths }) => {
                        crate::dnd::dispatch_drop_event(crate::dnd::DropEvent::Dropped(paths));
                    }
                    Event::Window(_, WindowEvent::DroppedFileHovered { paths }) => {
                        crate::dnd::dispatch_drop_event(crate::dnd::DropEvent::Hovered(paths));
                    }
                    Event::Window(_, WindowEvent::DroppedFileCancelled) => {
                        crate::dnd::dispatch_drop_event(crate::dnd::DropEvent::Cancelled);
                    }

                    // Handle input events
                    Event::Input(_, input_event) => {
                        // Classify the event up front — `input_event` is
                        // consumed by the match below, so the
                        // post-dispatch redraw gate at the bottom of
                        // this arm can't peek at it. Bool flags are
                        // cheap and survive the move.
                        let is_button_event = matches!(
                            input_event,
                            InputEvent::Mouse(MouseEvent::ButtonPressed { .. })
                                | InputEvent::Mouse(MouseEvent::ButtonReleased { .. })
                                | InputEvent::Touch(_)
                        );
                        let is_move_event = matches!(
                            input_event,
                            InputEvent::Mouse(MouseEvent::Moved { .. })
                                | InputEvent::Mouse(MouseEvent::Entered)
                                | InputEvent::Mouse(MouseEvent::Left),
                        );
                        // Cache invalidation now happens AFTER dispatch
                        // (see the post-dispatch `peek_needs_redraw`
                        // block at the bottom of this arm), gated on a
                        // signal that the dispatch actually changed
                        // render state. Invalidating up front for every
                        // input — including bare mouse-moves that just
                        // hit-test and produce no state change — meant
                        // the cache never lived more than a single
                        // frame on a UI with hover-eligible elements
                        // (cn_demo's buttons make `mouse_move_pipeline_
                        // _needed()` return `true`, so every mouse-move
                        // enters this arm). Fast path was rebuilding
                        // constantly and CPU never dropped.
                        // Pending event structure for deferred dispatch
                        #[derive(Clone)]
                        struct PendingEvent {
                            node_id: LayoutNodeId,
                            event_type: u32,
                            mouse_x: f32,
                            mouse_y: f32,
                            /// Local coordinates relative to element bounds
                            local_x: f32,
                            local_y: f32,
                            /// Absolute position of element bounds (top-left corner)
                            bounds_x: f32,
                            bounds_y: f32,
                            /// Computed bounds dimensions of the element
                            bounds_width: f32,
                            bounds_height: f32,
                            scroll_delta_x: f32,
                            scroll_delta_y: f32,
                            /// Drag delta for DRAG/DRAG_END events
                            drag_delta_x: f32,
                            drag_delta_y: f32,
                            key_char: Option<char>,
                            key_code: u32,
                            shift: bool,
                            ctrl: bool,
                            alt: bool,
                            meta: bool,
                            /// Pinch scale or rotation delta
                            pinch_scale: f32,
                        }

                        impl Default for PendingEvent {
                            fn default() -> Self {
                                Self {
                                    node_id: LayoutNodeId::default(),
                                    event_type: 0,
                                    mouse_x: 0.0,
                                    mouse_y: 0.0,
                                    local_x: 0.0,
                                    local_y: 0.0,
                                    bounds_x: 0.0,
                                    bounds_y: 0.0,
                                    bounds_width: 0.0,
                                    bounds_height: 0.0,
                                    scroll_delta_x: 0.0,
                                    scroll_delta_y: 0.0,
                                    drag_delta_x: 0.0,
                                    drag_delta_y: 0.0,
                                    key_char: None,
                                    key_code: 0,
                                    shift: false,
                                    ctrl: false,
                                    alt: false,
                                    meta: false,
                                    pinch_scale: 1.0,
                                }
                            }
                        }

                        // First phase: collect events using immutable borrow
                        let (pending_events, keyboard_events, scroll_ended, gesture_ended, scroll_info, scroll_cancel_hit) = if let (Some(windowed_ctx), Some(tree)) =
                            (&mut ws.ctx, &ws.render_tree)
                        {
                            let router = &mut windowed_ctx.event_router;

                            // Collect events from router
                            let mut pending_events: Vec<PendingEvent> = Vec::new();
                            // Separate collection for keyboard events (TEXT_INPUT)
                            let mut keyboard_events: Vec<PendingEvent> = Vec::new();
                            // Track if scroll ended (momentum finished)
                            let mut scroll_ended = false;
                            // Track if gesture ended (finger lifted - may still have momentum)
                            let mut gesture_ended = false;
                            // Track scroll info for nested scroll dispatch (mouse_x, mouse_y, delta_x, delta_y)
                            let mut scroll_info: Option<(f32, f32, f32, f32)> = None;
                            // Hit chain (leaf + ancestors) captured at mouse-down so the
                            // mutable phase can cancel any active scroll animation under
                            // the cursor — the "grab-to-stop" affordance.
                            let mut scroll_cancel_hit: Option<(
                                blinc_layout::LayoutNodeId,
                                Vec<blinc_layout::LayoutNodeId>,
                            )> = None;

                            // Set up callback to collect events
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

                            // Note: Overlays are now part of the main tree, so all events
                            // are routed through the single main event router.

                            // Convert physical coordinates to logical for hit testing
                            let scale = windowed_ctx.scale_factor as f32;

                            match input_event {
                                InputEvent::Mouse(mouse_event) => match mouse_event {
                                    MouseEvent::Moved { x, y } => {
                                        // Convert physical to logical coordinates
                                        let lx = x / scale;
                                        let ly = y / scale;

                                        // Short-circuit: cursor stayed inside the deepest
                                        // hit node's AABB from the previous move → the
                                        // hover set can't have changed, so the entire
                                        // dispatch pipeline (hit_test, hover diff, emit,
                                        // cursor lookup) is wasted work. macOS / Linux
                                        // gaming mice fire at hundreds of Hz; without
                                        // this gate cn_demo's tree-walk per event ate
                                        // ~20 % CPU on cursor wiggles.
                                        //
                                        // Conditions to take the short-circuit:
                                        //  - No press in flight (drag handlers need
                                        //    every move).
                                        //  - No `pointer_query` (continuous coord
                                        //    consumers — flow shaders, calc(env(...))).
                                        //  - No POINTER_MOVE handler on the tree (real
                                        //    continuous-tracking subscribers).
                                        //  - Cursor still inside last leaf bounds.
                                        let pressed = router.is_press_in_flight();
                                        let pointer_query_active = !windowed_ctx
                                            .pointer_query
                                            .is_empty();
                                        let has_move_subscriber = tree
                                            .handler_registry()
                                            .has_any_pointer_move_subscriber();
                                        // A node publishing `cursor_regions`
                                        // varies its cursor ALONG itself, so
                                        // "still inside the same leaf" no
                                        // longer means "nothing to re-resolve"
                                        // — the early-out is why a link inside
                                        // rich text never got its pointer while
                                        // its click worked: a click always
                                        // dispatches, the cursor only re-resolves
                                        // when the hovered node changes.
                                        // ANY node in the hovered chain, not
                                        // just the leaf: the node publishing
                                        // regions may be an ancestor of
                                        // whatever hit-testing returned.
                                        let leaf_varies_cursor =
                                            router.last_hit_chain().iter().any(|n| {
                                                tree.get_render_node(*n)
                                                    .is_some_and(|n| n.props.cursor_regions.is_some())
                                            });
                                        if !pressed
                                            && !pointer_query_active
                                            && !has_move_subscriber
                                            && !leaf_varies_cursor
                                            && router.cursor_inside_last_leaf(lx, ly)
                                        {
                                            router.set_mouse_position(lx, ly);
                                            // Drop the event callback before
                                            // returning — it holds a raw ptr
                                            // to `pending_events`'s stack
                                            // slot, which would dangle after
                                            // this scope exits. A later
                                            // event (e.g. Resized broadcast)
                                            // firing the stale callback
                                            // writes to freed memory and
                                            // corrupts the (since-replaced)
                                            // Vec header, surfacing as
                                            // "capacity overflow" on the
                                            // next push.
                                            router.clear_event_callback();
                                            return ControlFlow::Continue;
                                        }

                                        // Skip the heavy mouse-move pipeline (hit_test_all,
                                        // hover-set diff, POINTER_ENTER / LEAVE emission,
                                        // drag-delta tracking) if nothing in the tree could
                                        // react to it: no node with a registered pointer
                                        // handler, no CSS rule keyed on `:hover` / `:active`,
                                        // and no node carries a custom `cursor:` style that
                                        // would need re-resolving when the pointer crosses
                                        // an element boundary.
                                        //
                                        // `hello_blinc` and similar static views now stay
                                        // at near-zero CPU even during a continuous drag.
                                        // Per-move cost was previously: hit_test_all +
                                        // hover diff + DRAG emission + cursor hit_test +
                                        // OS `set_cursor` syscall. With nothing listening,
                                        // all of that is wasted work.
                                        let needs_pointer_dispatch =
                                            tree.handler_registry().has_any_pointer_handler()
                                                || tree.stylesheet().is_some_and(|s| {
                                                    s.has_pointer_state_rules()
                                                });
                                        let needs_cursor_resolve = tree.has_any_cursor_style();
                                        if !needs_pointer_dispatch && !needs_cursor_resolve {
                                            // Reset the OS cursor to Default (only if we
                                            // previously asked for something else — `Default`
                                            // is the OS's idle state). Caches the last
                                            // request so the syscall fires at most once
                                            // when the UI transitions from "had a styled
                                            // cursor" to "no longer does".
                                            let want = blinc_platform::Cursor::Default;
                                            if ws.last_cursor != Some(want) {
                                                window.set_cursor(want);
                                                ws.last_cursor = Some(want);
                                            }
                                            // See the matching clear above for why.
                                            router.clear_event_callback();
                                            return ControlFlow::Continue;
                                        }
                                        if !needs_pointer_dispatch {
                                            // Cursor-only path: do the cheap one-shot
                                            // `hit_test` to resolve `cursor:` styles, but
                                            // skip the full hover-diff machinery.
                                            let cursor = tree
                                                .get_cursor_at(router, lx, ly)
                                                .unwrap_or(CursorStyle::Default);
                                            let want = convert_cursor_style(cursor);
                                            if ws.last_cursor != Some(want) {
                                                window.set_cursor(want);
                                                ws.last_cursor = Some(want);
                                            }
                                            // See the matching clear above for why.
                                            router.clear_event_callback();
                                            return ControlFlow::Continue;
                                        }

                                        // Sub-frame coalescing — Phase 3.1 of
                                        // [[project-reactive-architecture-v2]]. High-rate
                                        // mice (1000 Hz Linux gaming mice in particular)
                                        // fire `MouseEvent::Moved` at >> display refresh,
                                        // and each event runs the full hit_test + hover
                                        // diff + handler dispatch path below — overkill
                                        // since at most one paint per vsync interval can
                                        // surface a visible result. Skip the heavy path
                                        // when the previous dispatch was very recent AND
                                        // dropping this event is safe:
                                        //   - No press in flight (drag handlers need
                                        //     every move to track velocity / position).
                                        //   - No `POINTER_MOVE` subscriber on the tree
                                        //     (continuous-tracking handlers like
                                        //     scroll-velocity tracking, sketch coords).
                                        //   - No pointer_query consumer (flow shaders,
                                        //     calc(env(...)) — already check this in
                                        //     the short-circuit a few branches up).
                                        // The window is conservative — half a vsync at
                                        // 60 Hz. At 1000 Hz mouse rate this turns ~16
                                        // dispatches/frame into 2; at 144 Hz display +
                                        // 1000 Hz mouse, ~7 → 1. Cursor + frame_dirty
                                        // are still updated inline so the next paint
                                        // reflects the latest cursor position.
                                        //
                                        // The last move before a stop won't be lost
                                        // because: any new event after the 8ms window
                                        // dispatches with its own (current) (lx, ly).
                                        // The "user moved fast then stopped exactly
                                        // mid-window" edge case leaves hover state up
                                        // to 8 ms stale until the next event — half a
                                        // 60 Hz frame, imperceptible.
                                        if let Some(last) = ws.last_pointer_dispatch {
                                            const COALESCE_WINDOW: std::time::Duration =
                                                std::time::Duration::from_millis(8);
                                            let elapsed = last.elapsed();
                                            if elapsed < COALESCE_WINDOW
                                                && !router.is_press_in_flight()
                                                && !has_move_subscriber
                                            {
                                                // Cursor resolution reuses the cached
                                                // hit chain from the previous full
                                                // dispatch (`get_cursor_for_last_hit`)
                                                // instead of a fresh `hit_test`. The
                                                // whole point of this gate is to skip
                                                // dispatch cost; running a fresh
                                                // hit_test for cursor would burn most
                                                // of the savings — see
                                                // `get_cursor_for_last_hit`'s docstring
                                                // ("ate ~10 % CPU on its own").
                                                // Tradeoff: cursor style is up to
                                                // 8 ms stale when the user crosses an
                                                // element boundary mid-skip-window —
                                                // half-frame at 60 Hz, imperceptible.
                                                let cursor = tree
                                                    .get_cursor_for_last_hit(router, lx)
                                                    .unwrap_or(CursorStyle::Default);
                                                let want = convert_cursor_style(cursor);
                                                if ws.last_cursor != Some(want) {
                                                    window.set_cursor(want);
                                                    ws.last_cursor = Some(want);
                                                }
                                                // Router-internal cursor tracking
                                                // stays current so
                                                // `cursor_inside_last_leaf` and
                                                // other queries see the latest pos.
                                                router.set_mouse_position(lx, ly);
                                                router.clear_event_callback();
                                                return ControlFlow::Continue;
                                            }
                                        }

                                        // Get overlay bounds and layer ID for occlusion-aware hit testing
                                        // This prevents background elements from receiving hover events
                                        // when they are visually occluded by overlay content
                                        let overlay_bounds = windowed_ctx.overlay_manager.get_visible_overlay_bounds();
                                        let overlay_layer_id = tree.query_by_id(
                                            blinc_layout::widgets::overlay::OVERLAY_LAYER_ID
                                        );

                                        // Drag fast path: while a press is in flight the
                                        // pressed target is fixed at mouse-down, so hover
                                        // diff / ENTER / LEAVE / cursor lookup are all
                                        // unnecessary. Just emit DRAG to the pressed
                                        // target + ancestors. Skips the full tree walk
                                        // that `on_mouse_move_with_occlusion` does — at
                                        // 100+ Hz move rates × cn_demo's tree depth, that
                                        // walk was the dominant remaining cost during a
                                        // drag (the gap between hello_blinc's 11 % drag
                                        // CPU and cn_demo's 25 %).
                                        if router.is_press_in_flight() {
                                            router.on_mouse_drag_fast(tree, lx, ly);
                                        } else {
                                            // Route mouse move through main tree with overlay occlusion awareness
                                            router.on_mouse_move_with_occlusion(
                                                tree,
                                                lx,
                                                ly,
                                                &overlay_bounds,
                                                overlay_layer_id,
                                            );
                                        }

                                        // Stamp the dispatch timestamp so the
                                        // sub-frame coalescing gate above can
                                        // skip subsequent moves within the
                                        // next 8 ms. Phase 3.1.
                                        ws.last_pointer_dispatch = Some(std::time::Instant::now());

                                        // Crossing an element boundary changes CSS `:hover`
                                        // styling and may switch which Stateful is in
                                        // its `Hover` state — flip dirty so the next
                                        // Event::Frame paints the new look.
                                        //
                                        // We deliberately do NOT include `DRAG` /
                                        // `DRAG_END` here: the router emits a `DRAG`
                                        // event for every mouse move while a button is
                                        // held, regardless of whether any handler is
                                        // attached. Including them turned a bare
                                        // mouse-down + drag in `hello_blinc` (no
                                        // handlers anywhere) into a 60–120 Hz redraw
                                        // loop pinning ~30 % CPU. Stateful-driven drag
                                        // (sliders, sortable, splitter panes) is
                                        // already covered by the post-dispatch
                                        // peek-needs-redraw check below — the drag
                                        // handler mutates `State`/`Stateful`, that
                                        // sets `NEEDS_REDRAW`, and we honour it.
                                        // Pre-fix: every POINTER_ENTER / POINTER_LEAVE
                                        // unconditionally invalidated the cache, even when
                                        // the entering / leaving element had no `:hover`
                                        // styling at all — every mouse move that crossed
                                        // any element boundary forced a full slow-path
                                        // repaint. styling_demo logs showed 142 of these
                                        // hover invalidations during a 13-second run.
                                        //
                                        // Now we walk the pending events and ask the tree
                                        // whether each target participates in `:hover`
                                        // styling (its id or one of its classes appears
                                        // in a `:hover`-bearing rule). frame_dirty +
                                        // request_redraw still fire unconditionally so
                                        // pointer handlers and the cursor-style path see
                                        // every move; only the cache-invalidation step
                                        // is per-target.
                                        let mut any_hover_changed = false;
                                        let mut hoverable_changed = false;
                                        for event in &pending_events {
                                            let is_hover_event = matches!(
                                                event.event_type,
                                                blinc_core::events::event_types::POINTER_ENTER
                                                    | blinc_core::events::event_types::POINTER_LEAVE
                                            );
                                            if !is_hover_event {
                                                continue;
                                            }
                                            any_hover_changed = true;
                                            if tree.node_participates_in_hover(event.node_id) {
                                                hoverable_changed = true;
                                                break;
                                            }
                                        }
                                        if any_hover_changed {
                                            frame_dirty.store(true, Ordering::Release);
                                            // Under `ControlFlow::Wait` (Linux/Wayland/X11)
                                            // flipping `frame_dirty` alone doesn't schedule
                                            // anything — we need to actually ask winit to
                                            // deliver a `RedrawRequested` event. macOS happens
                                            // to render anyway because Poll's auto-redraw was
                                            // there; on Linux this is the only path.
                                            window.request_redraw();
                                            // Cache invalidation is the expensive step —
                                            // gate it on the target actually having
                                            // pointer-state styling. Hovering over an
                                            // unstyled element produces no visual change
                                            // to invalidate for.
                                            //
                                            // Symptom this gate originally fixed: cn_demo
                                            // accordion hover lagged visibly behind the
                                            // cursor enter because the fast path just
                                            // blitted the pre-hover cache. We preserve
                                            // that fix by still invalidating when the
                                            // hovered element actually has hover styling.
                                            if hoverable_changed {
                                                if let Some(ref mut app) = ws.app {
                                                    app.invalidate_render_cache_tagged(
                                                        "hover_state_changed",
                                                    );
                                                }
                                            }
                                        }

                                        // Get drag delta from router (for DRAG events)
                                        let (drag_dx, drag_dy) = router.drag_delta();

                                        // Populate bounds for each event from the router's hit test results
                                        // This is needed for POINTER_ENTER/POINTER_LEAVE/POINTER_MOVE events
                                        for event in pending_events.iter_mut() {
                                            event.mouse_x = lx;
                                            event.mouse_y = ly;
                                            // Populate drag delta for DRAG events
                                            if event.event_type == blinc_core::events::event_types::DRAG
                                                || event.event_type == blinc_core::events::event_types::DRAG_END
                                            {
                                                event.drag_delta_x = drag_dx;
                                                event.drag_delta_y = drag_dy;
                                            }
                                            // Populate bounds from hit test results (stored in router)
                                            if let Some((bx, by, bw, bh)) = router.get_node_bounds(event.node_id) {
                                                event.bounds_x = bx;
                                                event.bounds_y = by;
                                                event.bounds_width = bw;
                                                event.bounds_height = bh;
                                                event.local_x = lx - bx;
                                                event.local_y = ly - by;
                                            }
                                        }

                                        // Update cursor based on hovered element. Reuse
                                        // the ancestor chain just cached by
                                        // `on_mouse_move_with_occlusion` instead of
                                        // running a fresh hit_test — at a Magic Mouse's
                                        // 120 Hz move rate × cn_demo's tree depth, the
                                        // second tree walk + HashMap allocation was ~10 %
                                        // CPU on its own. Cached against `last_cursor`
                                        // so a long stable hover doesn't syscall every
                                        // move.
                                        let cursor = tree
                                            .get_cursor_for_last_hit(router, lx)
                                            .unwrap_or(CursorStyle::Default);
                                        let want = convert_cursor_style(cursor);
                                        if ws.last_cursor != Some(want) {
                                            window.set_cursor(want);
                                            ws.last_cursor = Some(want);
                                        }
                                    }
                                    MouseEvent::ButtonPressed { button, x, y } => {
                                        let lx = x / scale;
                                        let ly = y / scale;
                                        let btn = convert_mouse_button(button);
                                        windowed_ctx.pointer_query.set_pressure(1.0);

                                        // Check for backdrop clicks (dismisses overlays)
                                        // This still needs special handling because backdrop clicks should
                                        // not propagate to elements behind the overlay
                                        let overlay_dismissed = if windowed_ctx.overlay_manager.has_blocking_overlay()
                                            || windowed_ctx.overlay_manager.has_dismissable_overlay()
                                        {
                                            windowed_ctx.overlay_manager.handle_click_at(lx, ly)
                                        } else {
                                            false
                                        };

                                        // If overlay was dismissed by backdrop click, don't process further
                                        if !overlay_dismissed {
                                            // Blur any focused text inputs BEFORE processing mouse down
                                            // This mimics HTML behavior where clicking anywhere blurs inputs,
                                            // and clicking on an input then re-focuses it via its own handler
                                            blinc_layout::widgets::blur_all_text_inputs();

                                            // "Grab-to-stop" — record the hit chain so the
                                            // mutable phase below can cancel any scroll
                                            // animation under the cursor before the click
                                            // dispatches. Without this a coasting list keeps
                                            // decelerating past the tap.
                                            scroll_cancel_hit = router
                                                .hit_test(tree, lx, ly)
                                                .map(|h| (h.node, h.ancestors.clone()));

                                            // Route through main tree (includes overlay content)
                                            let _events = router.on_mouse_down(tree, lx, ly, btn);

                                            let (local_x, local_y) = router.last_hit_local();
                                            let (bounds_x, bounds_y) = router.last_hit_bounds_pos();
                                            let (bounds_width, bounds_height) = router.last_hit_bounds();
                                            for event in pending_events.iter_mut() {
                                                event.mouse_x = lx;
                                                event.mouse_y = ly;
                                                event.local_x = local_x;
                                                event.local_y = local_y;
                                                event.bounds_x = bounds_x;
                                                event.bounds_y = bounds_y;
                                                event.bounds_width = bounds_width;
                                                event.bounds_height = bounds_height;
                                            }
                                        }
                                    }
                                    MouseEvent::ButtonReleased { button, x, y } => {
                                        let lx = x / scale;
                                        let ly = y / scale;
                                        let btn = convert_mouse_button(button);
                                        windowed_ctx.pointer_query.set_pressure(0.0);

                                        // Route through main tree (includes overlay content)
                                        router.on_mouse_up(tree, lx, ly, btn);
                                        // Use the local coordinates from when the press started
                                        // (stored by on_mouse_down via last_hit_local)
                                        let (local_x, local_y) = router.last_hit_local();
                                        let (bounds_x, bounds_y) = router.last_hit_bounds_pos();
                                        let (bounds_width, bounds_height) = router.last_hit_bounds();
                                        for event in pending_events.iter_mut() {
                                            event.mouse_x = lx;
                                            event.mouse_y = ly;
                                            event.local_x = local_x;
                                            event.local_y = local_y;
                                            event.bounds_x = bounds_x;
                                            event.bounds_y = bounds_y;
                                            event.bounds_width = bounds_width;
                                            event.bounds_height = bounds_height;
                                        }
                                    }
                                    MouseEvent::Left => {
                                        // on_mouse_leave now emits POINTER_UP if there was a pressed target
                                        // This handles the case where mouse leaves window while dragging
                                        router.on_mouse_leave(tree);
                                        // Reset cursor to default when mouse leaves window
                                        let want = blinc_platform::Cursor::Default;
                                        if ws.last_cursor != Some(want) {
                                            window.set_cursor(want);
                                            ws.last_cursor = Some(want);
                                        }
                                        // Events are collected via the callback set above
                                    }
                                    MouseEvent::Entered => {
                                        let (mx, my) = router.mouse_position();

                                        // Use occlusion-aware hit testing when mouse enters window
                                        let overlay_bounds = windowed_ctx.overlay_manager.get_visible_overlay_bounds();
                                        let overlay_layer_id = tree.query_by_id(
                                            blinc_layout::widgets::overlay::OVERLAY_LAYER_ID
                                        );
                                        router.on_mouse_move_with_occlusion(
                                            tree,
                                            mx,
                                            my,
                                            &overlay_bounds,
                                            overlay_layer_id,
                                        );

                                        for event in pending_events.iter_mut() {
                                            event.mouse_x = mx;
                                            event.mouse_y = my;
                                        }

                                        // Update cursor based on hovered element. See the
                                        // `MouseEvent::Moved` branch for the cache rationale.
                                        let cursor = tree
                                            .get_cursor_at(router, mx, my)
                                            .unwrap_or(CursorStyle::Default);
                                        let want = convert_cursor_style(cursor);
                                        if ws.last_cursor != Some(want) {
                                            window.set_cursor(want);
                                            ws.last_cursor = Some(want);
                                        }
                                    }
                                },
                                InputEvent::Keyboard(kb_event) => {
                                    let mods = &kb_event.modifiers;
                                    // Cache the live modifier state so subsequent
                                    // pointer events can propagate it through to
                                    // their EventContext.
                                    //
                                    // The cache uses TWO inputs to stay
                                    // accurate across backend quirks:
                                    //
                                    // 1. Start from kb_event.modifiers (winit's
                                    //    snapshot at the moment the event was
                                    //    queued). Good when ModifiersChanged
                                    //    fires before the key event.
                                    // 2. Override explicitly for modifier-key
                                    //    transitions: a Pressed Shift sets
                                    //    cache.shift=true, a Released Shift
                                    //    clears it. Handles backends where
                                    //    ModifiersChanged queues AFTER the
                                    //    KeyboardInput (so the snapshot reports
                                    //    the pre-transition state).
                                    let mut next = *mods;
                                    let is_press = matches!(
                                        kb_event.state,
                                        blinc_platform::KeyState::Pressed
                                    );
                                    let is_release = matches!(
                                        kb_event.state,
                                        blinc_platform::KeyState::Released
                                    );
                                    match (&kb_event.key, is_press, is_release) {
                                        (blinc_platform::Key::Shift, true, _) => next.shift = true,
                                        (blinc_platform::Key::Shift, _, true) => next.shift = false,
                                        (blinc_platform::Key::Ctrl, true, _) => next.ctrl = true,
                                        (blinc_platform::Key::Ctrl, _, true) => next.ctrl = false,
                                        (blinc_platform::Key::Alt, true, _) => next.alt = true,
                                        (blinc_platform::Key::Alt, _, true) => next.alt = false,
                                        (blinc_platform::Key::Meta, true, _) => next.meta = true,
                                        (blinc_platform::Key::Meta, _, true) => next.meta = false,
                                        _ => {}
                                    }
                                    if next != ws.cached_modifiers {
                                        tracing::trace!(
                                            target: "blinc_app::modifiers",
                                            shift = next.shift,
                                            ctrl = next.ctrl,
                                            alt = next.alt,
                                            meta = next.meta,
                                            "cached modifiers updated"
                                        );
                                    }
                                    ws.cached_modifiers = next;

                                    // Extract character from key if applicable
                                    let key_char = match &kb_event.key {
                                        Key::Char(c) => Some(*c),
                                        Key::Space => Some(' '),
                                        Key::A => Some(if mods.shift { 'A' } else { 'a' }),
                                        Key::B => Some(if mods.shift { 'B' } else { 'b' }),
                                        Key::C => Some(if mods.shift { 'C' } else { 'c' }),
                                        Key::D => Some(if mods.shift { 'D' } else { 'd' }),
                                        Key::E => Some(if mods.shift { 'E' } else { 'e' }),
                                        Key::F => Some(if mods.shift { 'F' } else { 'f' }),
                                        Key::G => Some(if mods.shift { 'G' } else { 'g' }),
                                        Key::H => Some(if mods.shift { 'H' } else { 'h' }),
                                        Key::I => Some(if mods.shift { 'I' } else { 'i' }),
                                        Key::J => Some(if mods.shift { 'J' } else { 'j' }),
                                        Key::K => Some(if mods.shift { 'K' } else { 'k' }),
                                        Key::L => Some(if mods.shift { 'L' } else { 'l' }),
                                        Key::M => Some(if mods.shift { 'M' } else { 'm' }),
                                        Key::N => Some(if mods.shift { 'N' } else { 'n' }),
                                        Key::O => Some(if mods.shift { 'O' } else { 'o' }),
                                        Key::P => Some(if mods.shift { 'P' } else { 'p' }),
                                        Key::Q => Some(if mods.shift { 'Q' } else { 'q' }),
                                        Key::R => Some(if mods.shift { 'R' } else { 'r' }),
                                        Key::S => Some(if mods.shift { 'S' } else { 's' }),
                                        Key::T => Some(if mods.shift { 'T' } else { 't' }),
                                        Key::U => Some(if mods.shift { 'U' } else { 'u' }),
                                        Key::V => Some(if mods.shift { 'V' } else { 'v' }),
                                        Key::W => Some(if mods.shift { 'W' } else { 'w' }),
                                        Key::X => Some(if mods.shift { 'X' } else { 'x' }),
                                        Key::Y => Some(if mods.shift { 'Y' } else { 'y' }),
                                        Key::Z => Some(if mods.shift { 'Z' } else { 'z' }),
                                        Key::Num0 => Some(if mods.shift { ')' } else { '0' }),
                                        Key::Num1 => Some(if mods.shift { '!' } else { '1' }),
                                        Key::Num2 => Some(if mods.shift { '@' } else { '2' }),
                                        Key::Num3 => Some(if mods.shift { '#' } else { '3' }),
                                        Key::Num4 => Some(if mods.shift { '$' } else { '4' }),
                                        Key::Num5 => Some(if mods.shift { '%' } else { '5' }),
                                        Key::Num6 => Some(if mods.shift { '^' } else { '6' }),
                                        Key::Num7 => Some(if mods.shift { '&' } else { '7' }),
                                        Key::Num8 => Some(if mods.shift { '*' } else { '8' }),
                                        Key::Num9 => Some(if mods.shift { '(' } else { '9' }),
                                        Key::Minus => Some(if mods.shift { '_' } else { '-' }),
                                        Key::Equals => Some(if mods.shift { '+' } else { '=' }),
                                        Key::LeftBracket => Some(if mods.shift { '{' } else { '[' }),
                                        Key::RightBracket => Some(if mods.shift { '}' } else { ']' }),
                                        Key::Backslash => Some(if mods.shift { '|' } else { '\\' }),
                                        Key::Semicolon => Some(if mods.shift { ':' } else { ';' }),
                                        Key::Quote => Some(if mods.shift { '"' } else { '\'' }),
                                        Key::Comma => Some(if mods.shift { '<' } else { ',' }),
                                        Key::Period => Some(if mods.shift { '>' } else { '.' }),
                                        Key::Slash => Some(if mods.shift { '?' } else { '/' }),
                                        Key::Grave => Some(if mods.shift { '~' } else { '`' }),
                                        _ => None,
                                    };

                                    // Key code for special key handling (backspace, arrows, etc)
                                    // Letter keys use ASCII uppercase (65=A, 90=Z) for Cmd+key shortcuts
                                    let key_code = match &kb_event.key {
                                        Key::Backspace => 8,
                                        Key::Delete => 127,
                                        Key::Enter => 13,
                                        Key::Tab => 9,
                                        Key::Escape => 27,
                                        Key::Space => 32,
                                        Key::Left => 37,
                                        Key::Right => 39,
                                        Key::Up => 38,
                                        Key::Down => 40,
                                        Key::Home => 36,
                                        Key::End => 35,
                                        // Map letter keys to ASCII uppercase for Cmd+key shortcuts
                                        Key::A => 65, Key::B => 66, Key::C => 67,
                                        Key::D => 68, Key::E => 69, Key::F => 70,
                                        Key::G => 71, Key::H => 72, Key::I => 73,
                                        Key::J => 74, Key::K => 75, Key::L => 76,
                                        Key::M => 77, Key::N => 78, Key::O => 79,
                                        Key::P => 80, Key::Q => 81, Key::R => 82,
                                        Key::S => 83, Key::T => 84, Key::U => 85,
                                        Key::V => 86, Key::W => 87, Key::X => 88,
                                        Key::Y => 89, Key::Z => 90,
                                        // Digit row — match standard JS
                                        // KeyboardEvent.keyCode for parity
                                        // with web-convention chord tables.
                                        Key::Num0 => 48, Key::Num1 => 49, Key::Num2 => 50,
                                        Key::Num3 => 51, Key::Num4 => 52, Key::Num5 => 53,
                                        Key::Num6 => 54, Key::Num7 => 55, Key::Num8 => 56,
                                        Key::Num9 => 57,
                                        // Punctuation / symbol keys — JS
                                        // keyCode values so chord tables
                                        // that bind `,` / `=` / `-` etc
                                        // resolve cleanly.
                                        Key::Semicolon => 186,
                                        Key::Equals => 187,
                                        Key::Comma => 188,
                                        Key::Minus => 189,
                                        Key::Period => 190,
                                        Key::Slash => 191,
                                        Key::Grave => 192,
                                        Key::LeftBracket => 219,
                                        Key::Backslash => 220,
                                        Key::RightBracket => 221,
                                        Key::Quote => 222,
                                        Key::Back => {
                                            // System back button — dispatch through back handler
                                            if blinc_layout::back_handler::dispatch_back() {
                                                // See the matching clear above for why.
                                                router.clear_event_callback();
                                                return ControlFlow::Continue;
                                            }
                                            // Not consumed — let default handling proceed
                                            0
                                        }
                                        _ => 0,
                                    };

                                    match kb_event.state {
                                        KeyState::Pressed => {
                                            // Handle Escape key for overlays first
                                            // If an overlay handles it, don't propagate further
                                            if kb_event.key == Key::Escape {
                                                // Legacy manager first (covers widgets not yet
                                                // migrated). Then the new OverlayStack.
                                                let legacy_handled =
                                                    windowed_ctx.overlay_manager.handle_escape();
                                                let stack_handled = blinc_layout::overlay_state::overlay_stack()
                                                    .lock()
                                                    .map(|mut s| s.handle_escape())
                                                    .unwrap_or(false);
                                                let _ = legacy_handled || stack_handled;
                                            }

                                            // Dispatch KEY_DOWN for all keys
                                            router.on_key_down(key_code);

                                            // For character-producing keys, dispatch TEXT_INPUT
                                            // We use broadcast dispatch so any focused text input can receive it
                                            if let Some(c) = key_char {
                                                // Don't send text input if ctrl/cmd is held (shortcuts)
                                                if !mods.ctrl && !mods.meta {
                                                    keyboard_events.push(PendingEvent {
                                                        event_type: blinc_core::events::event_types::TEXT_INPUT,
                                                        key_char: Some(c),
                                                        key_code,
                                                        shift: mods.shift,
                                                        ctrl: mods.ctrl,
                                                        alt: mods.alt,
                                                        meta: mods.meta,
                                                        ..Default::default()
                                                    });
                                                }
                                            }

                                            // For KEY_DOWN events with special keys (backspace, arrows)
                                            if key_code != 0 {
                                                keyboard_events.push(PendingEvent {
                                                    event_type: blinc_core::events::event_types::KEY_DOWN,
                                                    key_char: None,
                                                    key_code,
                                                    shift: mods.shift,
                                                    ctrl: mods.ctrl,
                                                    alt: mods.alt,
                                                    meta: mods.meta,
                                                    ..Default::default()
                                                });
                                            }
                                        }
                                        KeyState::Released => {
                                            router.on_key_up(key_code);

                                            // Also broadcast KEY_UP through the
                                            // `keyboard_events` path. Without this the
                                            // focus-targeted dispatch only fires on the
                                            // focused leaf node, so ancestor handlers
                                            // (e.g. `blinc_input::DivInputExt::capture_input`
                                            // attached to a viewport Div) never see
                                            // releases and their internal
                                            // `keys_down`-tracking sets never clear —
                                            // which in turn makes polling consumers see
                                            // every key as permanently-held after the
                                            // first press. Matches the broadcast path
                                            // KEY_DOWN already uses below.
                                            if key_code != 0 {
                                                keyboard_events.push(PendingEvent {
                                                    event_type: blinc_core::events::event_types::KEY_UP,
                                                    key_char: None,
                                                    key_code,
                                                    shift: mods.shift,
                                                    ctrl: mods.ctrl,
                                                    alt: mods.alt,
                                                    meta: mods.meta,
                                                    ..Default::default()
                                                });
                                            }
                                        }
                                    }
                                },
                                InputEvent::Touch(touch_event) => {
                                    // Track active touch IDs for touch count
                                    match &touch_event {
                                        TouchEvent::Started { .. } => {
                                            ws.active_touch_ids.insert(touch_event.id());
                                            windowed_ctx.pointer_query.set_touch_count(ws.active_touch_ids.len() as u32);
                                        }
                                        TouchEvent::Ended { .. } => {
                                            ws.active_touch_ids.remove(&touch_event.id());
                                            windowed_ctx.pointer_query.set_touch_count(ws.active_touch_ids.len() as u32);
                                        }
                                        TouchEvent::Cancelled { .. } => {
                                            ws.active_touch_ids.remove(&touch_event.id());
                                            windowed_ctx.pointer_query.set_touch_count(ws.active_touch_ids.len() as u32);
                                        }
                                        _ => {}
                                    }
                                    match touch_event {
                                        TouchEvent::Started { x, y, pressure, .. } => {
                                            let lx = x / scale;
                                            let ly = y / scale;
                                            windowed_ctx.pointer_query.set_pressure(pressure);
                                            router.on_mouse_down(tree, lx, ly, MouseButton::Left);
                                            let (local_x, local_y) = router.last_hit_local();
                                            let (bounds_x, bounds_y) = router.last_hit_bounds_pos();
                                            let (bounds_width, bounds_height) = router.last_hit_bounds();
                                            for event in pending_events.iter_mut() {
                                                event.mouse_x = lx;
                                                event.mouse_y = ly;
                                                event.local_x = local_x;
                                                event.local_y = local_y;
                                                event.bounds_x = bounds_x;
                                                event.bounds_y = bounds_y;
                                                event.bounds_width = bounds_width;
                                                event.bounds_height = bounds_height;
                                            }
                                        }
                                        TouchEvent::Moved { x, y, pressure, .. } => {
                                            let lx = x / scale;
                                            let ly = y / scale;
                                            windowed_ctx.pointer_query.set_pressure(pressure);

                                            // Use occlusion-aware hit testing for touch move as well
                                            let overlay_bounds = windowed_ctx.overlay_manager.get_visible_overlay_bounds();
                                            let overlay_layer_id = tree.query_by_id(
                                                blinc_layout::widgets::overlay::OVERLAY_LAYER_ID
                                            );
                                            router.on_mouse_move_with_occlusion(
                                                tree,
                                                lx,
                                                ly,
                                                &overlay_bounds,
                                                overlay_layer_id,
                                            );

                                            for event in pending_events.iter_mut() {
                                                event.mouse_x = lx;
                                                event.mouse_y = ly;
                                            }
                                        }
                                        TouchEvent::Ended { x, y, .. } => {
                                            let lx = x / scale;
                                            let ly = y / scale;
                                            windowed_ctx.pointer_query.set_pressure(0.0);
                                            router.on_mouse_up(tree, lx, ly, MouseButton::Left);
                                            for event in pending_events.iter_mut() {
                                                event.mouse_x = lx;
                                                event.mouse_y = ly;
                                            }
                                        }
                                        TouchEvent::Cancelled { .. } => {
                                            // Touch cancelled - treat like mouse leave
                                            // This will emit POINTER_UP if there was a pressed target
                                            windowed_ctx.pointer_query.set_pressure(0.0);
                                            windowed_ctx.pointer_query.set_touch_count(0);
                                            router.on_mouse_leave(tree);
                                        }
                                    }
                                }
                                InputEvent::Scroll { delta_x, delta_y, phase } => {
                                    let (mx, my) = router.mouse_position();
                                    // Scroll deltas are also in physical pixels, convert to logical
                                    let ldx = delta_x;
                                    let ldy = delta_y;

                                    tracing::trace!(
                                        "InputEvent::Scroll received: pos=({:.1}, {:.1}) delta=({:.1}, {:.1}) phase={:?}",
                                        mx, my, ldx, ldy, phase
                                    );

                                    // Check if gesture ended (finger lifted from trackpad)
                                    // This happens before momentum ends
                                    if phase == blinc_platform::ScrollPhase::Ended {
                                        gesture_ended = true;
                                    }

                                    // Use nested scroll support - get hit result for smart dispatch
                                    // Store mouse position and delta for dispatch phase
                                    // We'll re-do hit test in dispatch phase since we need mutable borrow
                                    scroll_info = Some((mx, my, ldx, ldy));
                                }
                                InputEvent::ScrollEnd => {
                                    // Scroll momentum ended - full stop
                                    scroll_ended = true;
                                }
                                InputEvent::Pinch { scale, .. } => {
                                    let (mx, my) = router.mouse_position();
                                    pending_events.push(PendingEvent {
                                        event_type: blinc_core::events::event_types::PINCH,
                                        mouse_x: mx,
                                        mouse_y: my,
                                        pinch_scale: scale,
                                        ..Default::default()
                                    });
                                }
                                InputEvent::Rotation { angle, .. } => {
                                    let (mx, my) = router.mouse_position();
                                    pending_events.push(PendingEvent {
                                        event_type: blinc_core::events::event_types::ROTATE,
                                        mouse_x: mx,
                                        mouse_y: my,
                                        pinch_scale: angle,
                                        ..Default::default()
                                    });
                                }
                                InputEvent::ModifiersChanged(mods) => {
                                    // Authoritative modifier-state
                                    // update from the platform layer
                                    // (winit's ModifiersChanged). Some
                                    // backends don't fire KeyboardInput
                                    // for the modifier keys themselves
                                    // when focus is on a non-text
                                    // element (e.g. an open overlay /
                                    // popover) — without this update
                                    // path the cache would latch at
                                    // `shift: true` after a Shift +
                                    // marquee gesture even after the
                                    // user lifted Shift.
                                    if ws.cached_modifiers != mods {
                                        tracing::trace!(
                                            target: "blinc_app::modifiers",
                                            shift = mods.shift,
                                            ctrl = mods.ctrl,
                                            alt = mods.alt,
                                            meta = mods.meta,
                                            "cached modifiers updated (ModifiersChanged)"
                                        );
                                        ws.cached_modifiers = mods;
                                    }
                                }
                            }

                            router.clear_event_callback();
                            (pending_events, keyboard_events, scroll_ended, gesture_ended, scroll_info, scroll_cancel_hit)
                        } else {
                            (Vec::new(), Vec::new(), false, false, None, None)
                        };

                        // Snapshot the cached modifier state before the
                        // dispatch loop's mutable borrow on `ws.render_tree`
                        // shadows access to `ws.cached_modifiers`. The
                        // pointer dispatch path reads from this snapshot to
                        // stamp Shift / Cmd / Ctrl / Alt onto the
                        // EventContext (the platform layer doesn't carry
                        // modifier state on mouse events).
                        let cached_modifiers_for_dispatch = ws.cached_modifiers;

                        // Second phase: dispatch events with mutable borrow
                        // This automatically marks the tree dirty when handlers fire
                        if let Some(ref mut tree) = ws.render_tree {
                            // "Grab-to-stop": if mouse-down landed on an
                            // animating scroll container, stop its
                            // momentum/rebound before any other handler
                            // runs. The target was captured in phase 1;
                            // we apply here where the tree is mutable.
                            if let Some((hit, ancestors)) = scroll_cancel_hit {
                                tree.cancel_scroll_animation_in_chain(hit, &ancestors);
                            }

                            // IMPORTANT: Process gesture_ended BEFORE scroll delta dispatch
                            // When gesture ends while overscrolling, we start bounce which
                            // sets state to Bouncing. Then apply_scroll_delta will early-return
                            // and ignore the momentum delta that came with this same event.
                            if gesture_ended {
                                tree.on_gesture_end();
                                // Request redraw to animate bounce-back
                                window.request_redraw();
                            }

                            // Handle scroll with nested scroll support
                            // Skip scroll delta entirely if gesture just ended - the delta
                            // from the same event as gesture_ended is the last finger movement,
                            // not momentum, but we still want to ignore it for instant snap-back
                            //
                            // Also skip scroll when an overlay with an actual backdrop is open to prevent
                            // background content from scrolling while dropdown/modal is visible.
                            // Note: We only check has_blocking_overlay(), not has_dismissable_overlay(),
                            // because overlays with dismiss_on_click_outside (like popovers) should allow
                            // scroll events to pass through to content behind them.
                            let has_overlay_backdrop = ws.ctx
                                .as_ref()
                                .map(|c| c.overlay_manager.has_blocking_overlay())
                                .unwrap_or(false);

                            if let Some((mouse_x, mouse_y, delta_x, delta_y)) = scroll_info {
                                // Skip if gesture ended in this same event - go straight to bounce
                                if gesture_ended {
                                    tracing::trace!("Skipping scroll delta - gesture ended, bouncing");
                                } else if delta_x == 0.0 && delta_y == 0.0 {
                                    // No-op scroll events fire from the platform layer with
                                    // zero deltas as gesture momentum decays past the floor.
                                    // canvas-kit's SCROLL branch still calls zoom_at with
                                    // factor=1.000, which is a no-op for zoom magnitude but
                                    // does write the viewport state through `update`,
                                    // counting as a state mutation and (on canvas-backed
                                    // hosts) feeding the dot-grid render. With the popover-
                                    // open continuous_redraw firing frames every vsync,
                                    // even noop scrolls compound visible cost. Drop them
                                    // at the dispatch boundary.
                                    tracing::trace!("Skipping zero-delta scroll");
                                } else if has_overlay_backdrop {
                                    // Skip scroll when overlay is visible to prevent background scrolling
                                    tracing::trace!("Skipping scroll delta - overlay with backdrop is visible");
                                } else {
                                    tracing::trace!(
                                        "Scroll dispatch: pos=({:.1}, {:.1}) delta=({:.1}, {:.1})",
                                        mouse_x, mouse_y, delta_x, delta_y
                                    );

                                    // Update overlay positions for overlays with follows_scroll enabled
                                    // Use the singleton overlay manager since components use get_overlay_manager()
                                    if OverlayContext::is_initialized() {
                                        let mgr = get_overlay_manager();
                                        if mgr.handle_scroll(delta_y) {
                                            // Apply scroll offsets to render tree for visual movement
                                            for (element_id, offset_y) in mgr.get_scroll_offsets() {
                                                if let Some(node_id) = tree.query_by_id(&element_id) {
                                                    tree.set_scroll_offset(node_id, 0.0, offset_y);
                                                }
                                            }
                                            window.request_redraw();
                                        }
                                    }

                                    // Re-do hit test with mutable borrow to get ancestor chain
                                    // Then use dispatch_scroll_chain for proper nested scroll handling
                                    if let Some(ref mut windowed_ctx) = ws.ctx {
                                        let router = &mut windowed_ctx.event_router;
                                        if let Some(hit) = router.hit_test(tree, mouse_x, mouse_y) {
                                            tree.dispatch_scroll_chain(
                                                hit.node,
                                                &hit.ancestors,
                                                mouse_x,
                                                mouse_y,
                                                delta_x,
                                                delta_y,
                                            );
                                        }
                                    }
                                }
                            }

                            // Per-hit cache-invalidation gate. Scan `pending_events`
                            // for any pointer event (POINTER_MOVE / DOWN / UP /
                            // DRAG / DRAG_END / FILE_DRAG_OVER) whose target node
                            // actually has a registered handler. The router's
                            // mouse-move path filters POINTER_MOVE emit to handler-
                            // bearing nodes; mouse-button / drag-fast paths emit
                            // unfiltered, so we re-check via `has_handler` here.
                            // Used below so the cache only invalidates when a
                            // real subscriber was under the cursor — bare cursor
                            // wiggle / click over empty space no longer pays the
                            // full re-render.
                            let pointer_event_to_handler = {
                                use blinc_core::events::event_types;
                                let registry = tree.handler_registry();
                                pending_events.iter().any(|e| {
                                    // DRAG / DRAG_END deliberately excluded.
                                    // Widgets that need a redraw on drag (cn
                                    // sliders / sortables, scroll-bar drag)
                                    // either go through Stateful (caught by
                                    // `state_changed` above) or call
                                    // `stateful::request_redraw()` themselves
                                    // when they actually move. Including DRAG
                                    // here invalidated the cache every frame
                                    // when the user dragged through an empty
                                    // area of a scroll container — the scroll
                                    // widget registers a permanent on_drag
                                    // handler that no-ops unless the scrollbar
                                    // itself is being dragged, so the gate saw
                                    // a handler and triggered the full slow
                                    // path for nothing.
                                    let is_pointer_event = matches!(
                                        e.event_type,
                                        event_types::POINTER_MOVE
                                            | event_types::POINTER_DOWN
                                            | event_types::POINTER_UP
                                            | event_types::POINTER_ENTER
                                            | event_types::POINTER_LEAVE
                                            | event_types::DOUBLE_TAP
                                            | event_types::FILE_DRAG_OVER
                                    );
                                    if !is_pointer_event {
                                        return false;
                                    }
                                    tree.stable_id(e.node_id)
                                        .map(|sid| registry.has_handler(sid, e.event_type))
                                        .unwrap_or(false)
                                })
                            };

                            // Dispatch mouse/touch events (scroll is handled above with nested support)
                            if let Some(ref mut windowed_ctx) = ws.ctx {
                                let router = &windowed_ctx.event_router;
                                for mut event in pending_events {
                                    // Skip scroll events - already handled with nested scroll support
                                    if event.event_type == blinc_core::events::event_types::SCROLL {
                                        continue;
                                    }
                                    // Gesture events (PINCH/ROTATE) need hit testing since
                                    // they were collected without a node target
                                    if (event.event_type == blinc_core::events::event_types::PINCH
                                        || event.event_type
                                            == blinc_core::events::event_types::ROTATE)
                                        && event.node_id == LayoutNodeId::default()
                                    {
                                        if let Some(hit) =
                                            router.hit_test(tree, event.mouse_x, event.mouse_y)
                                        {
                                            event.node_id = hit.node;
                                            event.local_x = hit.local_x;
                                            event.local_y = hit.local_y;
                                            event.bounds_x = hit.bounds_x;
                                            event.bounds_y = hit.bounds_y;
                                            event.bounds_width = hit.bounds_width;
                                            event.bounds_height = hit.bounds_height;
                                        } else {
                                            continue; // No element under cursor
                                        }
                                    }
                                    // Look up the correct bounds for this specific node.
                                    // When events bubble from a child to a parent handler,
                                    // we need the parent's bounds, not the original hit target's bounds.
                                    let (bounds_x, bounds_y, bounds_width, bounds_height) =
                                        router.get_node_bounds(event.node_id).unwrap_or((
                                            event.bounds_x,
                                            event.bounds_y,
                                            event.bounds_width,
                                            event.bounds_height,
                                        ));
                                    let local_x = event.mouse_x - bounds_x;
                                    let local_y = event.mouse_y - bounds_y;
                                    // Pointer events arrive without modifier
                                    // state attached — the platform layer
                                    // doesn't propagate it on mouse events.
                                    // Fall back to the per-event payload (set
                                    // by the keyboard arm above for actual
                                    // keyboard PendingEvents); if zero,
                                    // substitute the cached modifier snapshot
                                    // updated on every KeyboardInput.
                                    let mods_snapshot = cached_modifiers_for_dispatch;
                                    let shift = event.shift || mods_snapshot.shift;
                                    let ctrl = event.ctrl || mods_snapshot.ctrl;
                                    let alt = event.alt || mods_snapshot.alt;
                                    let meta = event.meta || mods_snapshot.meta;
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
                                        event.drag_delta_x,
                                        event.drag_delta_y,
                                        event.pinch_scale,
                                        shift,
                                        ctrl,
                                        alt,
                                        meta,
                                    );
                                }
                            }

                            // Note: Overlay events are now dispatched through the main tree
                            // since overlays are composed into the main tree via build_overlay_layer()

                            // Dispatch keyboard events
                            // Use broadcast instead of bubbling to handle focus correctly after tree rebuilds.
                            // Text inputs track their own focus state internally via `s.visual.is_focused()`,
                            // so broadcasting to all handlers is safe - only the focused one will process.
                            for event in keyboard_events {
                                if event.event_type == blinc_core::events::event_types::TEXT_INPUT {
                                    if let Some(c) = event.key_char {
                                        // Broadcast to all text input handlers
                                        // Each handler checks its own focus state internally
                                        tree.broadcast_text_input_event(
                                            c,
                                            event.shift,
                                            event.ctrl,
                                            event.alt,
                                            event.meta,
                                        );
                                    }
                                } else {
                                    // Broadcast KEY_DOWN to all key handlers
                                    tree.broadcast_key_event(
                                        event.event_type,
                                        event.key_code,
                                        event.shift,
                                        event.ctrl,
                                        event.alt,
                                        event.meta,
                                    );
                                }
                            }

                            // Fire the rebound on `TouchPhase::Ended`. macOS
                            // trackpads deliver Ended twice per gesture (once
                            // at finger-lift, once at OS-momentum end); the
                            // physics' `on_scroll_end` is idempotent — the
                            // second call is a no-op because `is_overscrolling`
                            // is false after the first spring has already
                            // clamped the content to the edge.
                            if scroll_ended {
                                tree.on_scroll_end();
                                window.request_redraw();
                            }

                            // After every input dispatch, check whether any
                            // handler set `NEEDS_REDRAW` (via
                            // `stateful::request_redraw()` from a `dispatch`
                            // / state-change path) or queued a subtree
                            // rebuild. On Linux's `ControlFlow::Wait` the
                            // event loop doesn't deliver `Event::Frame` on
                            // its own; we must explicitly request a
                            // redraw so the queued work actually runs.
                            // Sliders, sortable lists, splitter panes — any
                            // Stateful-driven drag — flow through here.
                            let state_changed = blinc_layout::peek_needs_redraw()
                                || blinc_layout::has_pending_subtree_rebuilds();
                            if state_changed {
                                frame_dirty.store(true, Ordering::Release);
                                window.request_redraw();
                                // Dispatch produced a real state change —
                                // drop the compositor cache so the next
                                // paint repopulates it with the new
                                // hover / focus / scroll / state.
                                if let Some(ref mut app) = ws.app {
                                    app.invalidate_render_cache_tagged("state_changed");
                                }
                            }
                            // Scroll events update `tree.scroll_offsets`
                            // through `dispatch_scroll`, which doesn't
                            // route through `stateful::request_redraw` —
                            // force invalidation any frame a scroll
                            // input landed.
                            let had_scroll = scroll_info.is_some() || scroll_ended;
                            if had_scroll {
                                if let Some(ref mut app) = ws.app {
                                    app.invalidate_render_cache_tagged("had_scroll");
                                }
                            }
                            // Interactive elements that don't route through
                            // `stateful::request_redraw` — canvas closures
                            // that read mouse position to draw hover state
                            // (`blinc_canvas_kit::kit.element(...)`), the
                            // `pointer_query` calc(env(...)) cursor
                            // subscribers, custom `on_pointer_*` handlers
                            // that update private state — all need a frame
                            // to paint their response to the event, but
                            // pre-fix the `state_changed` gate above
                            // missed them. The visible symptom was the
                            // user's "I have to scroll for the next frame
                            // to render" report: scroll was the only path
                            // that unconditionally invalidated.
                            //
                            // For mouse-button events (down / up) we
                            // unconditionally invalidate + redraw because
                            // a click is always a candidate state change.
                            // For mouse moves we only redraw when
                            // interactive subscribers exist on the tree —
                            // a static UI with no pointer-aware nodes
                            // shouldn't pay redraw cost for every cursor
                            // wiggle. Same gate the windowed redraw chain
                            // already uses: pointer_query elements or
                            // any node with an attached pointer handler.
                            if !state_changed && !had_scroll {
                                // Per-hit invalidation for ALL pointer events
                                // (moves, button events, drags). The previous
                                // gate invalidated unconditionally on button
                                // events and globally on moves — together that
                                // meant every click on empty space and every
                                // cursor wiggle near hoverable elements forced
                                // the slow path.
                                //
                                // `pointer_event_to_handler` (computed above
                                // before pending_events is consumed) is true
                                // only when an emitted pointer event actually
                                // targets a node with a registered handler for
                                // that event type. Stateful elements that
                                // mutate state on POINTER_DOWN still get
                                // caught by the `state_changed` gate above;
                                // this branch covers non-stateful handlers
                                // (custom on_click closures, drag handlers,
                                // etc.).
                                //
                                // Pointer-query elements (calc(env(--mouse...)) /
                                // canvas-kit cursor subscribers) read mouse
                                // position every frame regardless of hit chain
                                // and need the global invalidate; we OR them
                                // in for move events.
                                let has_pointer_query = ws
                                    .ctx
                                    .as_ref()
                                    .is_some_and(|c| !c.pointer_query.is_empty());
                                let need_invalidate = pointer_event_to_handler
                                    || (is_move_event && has_pointer_query);
                                if need_invalidate {
                                    frame_dirty.store(true, Ordering::Release);
                                    window.request_redraw();
                                    if let Some(ref mut app) = ws.app {
                                        app.invalidate_render_cache_tagged("pointer_event_with_subscribers");
                                    }
                                }
                            }
                        }
                    }

                    Event::Frame(_) => {
                        // Skip the frame entirely if nothing has changed since
                        // the last render. The OS sends `Event::Frame` at the
                        // display refresh rate to focused windows whether we
                        // asked for it or not; without this gate a static
                        // focused UI burns CPU re-rendering an identical scene
                        // every vsync interval. `frame_dirty` is flipped back
                        // to `true` by any input event (in the prelude above,
                        // bare mouse-moves excluded), by the scheduler wake
                        // callback (set during init), and by the end-of-frame
                        // redraw chain when any animation / cursor / transition
                        // / etc. signal indicates ongoing work.
                        //
                        // We also honour the layout-side stateful redraw
                        // signals here — a hover handler firing
                        // `stateful::request_redraw()` mid-mouse-move would
                        // otherwise be dropped now that bare moves don't
                        // flip `frame_dirty`. Peek-without-clear so the
                        // start-of-frame `take_needs_redraw()` still fires
                        // its normal prop-update / subtree-rebuild path.
                        let dirty = frame_dirty.swap(false, Ordering::AcqRel);
                        let stateful_dirty = blinc_layout::peek_needs_redraw()
                            || blinc_layout::has_pending_subtree_rebuilds();
                        // Wayland keep-alive: when the frame-gate is active we must
                        // keep presenting even when the scene is static. Once
                        // animations finish and the redraw chain goes quiet,
                        // skipping here stops all presents; Mutter then holds every
                        // swapchain buffer and the next get_current_texture()
                        // starves for the full ~1s acquire timeout (freeze). The
                        // gate throttles us to the compositor's Done cadence, so
                        // this stays vsync-paced, not a spin.
                        // Wayland keep-alive: keep presenting even when the
                        // scene is static (frame-gate or present-thread mode),
                        // else the swapchain goes idle and the compositor starves
                        // the next acquire.
                        #[allow(unused_mut)]
                        let mut keep_alive = false;
                        #[cfg(all(feature = "wayland-frame-gate", target_os = "linux"))]
                        {
                            keep_alive |= ws.wayland_gate.is_some();
                        }
                        #[cfg(target_os = "linux")]
                        {
                            keep_alive |= ws.wayland_keep_alive;
                        }
                        if !dirty && !stateful_dirty && !keep_alive {
                            return ControlFlow::Continue;
                        }

                        if let (
                            Some(blinc_app),
                            Some(config),
                            Some(windowed_ctx),
                            Some(rs),
                        ) = (&mut ws.app, &ws.surface_config, &mut ws.ctx, &mut ws.render_state)
                        {
                            // Per-phase frame timing. Cheap when the trace target
                            // is disabled (Instant::now is ~10 ns on macOS); gated
                            // behind a single check at frame end so format args
                            // aren't evaluated in production builds. Enable with
                            // `RUST_LOG=blinc_app::frame_timing=trace` to see
                            // where the frame budget is actually going.
                            let frame_start = std::time::Instant::now();
                            let mut t_phase1 = std::time::Duration::ZERO;
                            let mut t_phase2 = std::time::Duration::ZERO;
                            let mut t_phase3 = std::time::Duration::ZERO;
                            let mut t_phase4 = std::time::Duration::ZERO;
                            let mut t_phase5 = std::time::Duration::ZERO;
                            let mut did_rebuild = false;
                            let mut dirty_spring_count = 0usize;

                            // Bind the surface handle up front: the
                            // (feature-gated) frame-gate stall recovery below may
                            // reconfigure it, and the frame acquire further down
                            // uses it. Binding here keeps `surf` in scope for both
                            // regardless of which Wayland features are enabled.
                            let surf = match ws.surface.as_ref() {
                                Some(s) => s,
                                None => return ControlFlow::Continue,
                            };

                            // Experimental Wayland frame-callback gate: when
                            // active, drain our wl_callback Done events from
                            // the connection's per-queue inbox and bail out
                            // of this frame entirely if the compositor hasn't
                            // delivered Done for the last armed callback.
                            // Re-arming `request_redraw` keeps us re-checking
                            // without ever attempting a blocking acquire.
                            //
                            // The motivating bug: some Mesa + Wayland
                            // compositors don't deliver Done to winit's
                            // internal queue reliably; winit's own gating
                            // either fires too eagerly (and we hit the wgpu
                            // 1s acquire timeout) or stalls forever. Our
                            // parallel gate has its own queue independent
                            // of winit's, so it's not vulnerable to the
                            // same stale-source race.
                            #[cfg(all(feature = "wayland-frame-gate", target_os = "linux"))]
                            if let Some(gate) = ws.wayland_gate.as_ref() {
                                gate.dispatch_pending();
                                let ready = gate.is_frame_ready();
                                // 100 ms safety-valve so a stopped Done
                                // stream caps user-perceived freeze at one
                                // tenth of a second instead of forever.
                                let proceed = gate.is_frame_ready_or_timeout(
                                    std::time::Duration::from_millis(100),
                                );

                                // Periodic diagnostic: every 60 frames,
                                // print how many Done events have actually
                                // routed into our queue, and how many of
                                // those frames hit the safety valve instead
                                // of receiving Done in time. If
                                // `callbacks_received` stays at 0 the
                                // routing bridge between winit's connection
                                // read loop and our parallel queue is
                                // broken — every frame would be relying on
                                // the 100ms safety valve, which is exactly
                                // the "responsive then sluggish then
                                // freezes" pathology under continuous
                                // input.
                                use std::sync::atomic::{
                                    AtomicU64,
                                    Ordering as AtomicOrdering,
                                };
                                static FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
                                static SAFETY_VALVE_HITS: AtomicU64 = AtomicU64::new(0);
                                let fc =
                                    FRAME_COUNT.fetch_add(1, AtomicOrdering::Relaxed) + 1;
                                if !ready && proceed {
                                    SAFETY_VALVE_HITS
                                        .fetch_add(1, AtomicOrdering::Relaxed);
                                }
                                if fc % 60 == 0 {
                                    tracing::info!(
                                        target: "blinc_app::wayland_gate",
                                        frame = fc,
                                        done_events = gate.callbacks_received(),
                                        safety_valve_hits = SAFETY_VALVE_HITS
                                            .load(AtomicOrdering::Relaxed),
                                        "wayland-frame-gate heartbeat",
                                    );
                                }

                                if !proceed {
                                    // Not ready and the safety valve hasn't
                                    // elapsed: re-check next frame without
                                    // busy-spinning. request_redraw() re-dispatches
                                    // RedrawRequested immediately, so pace with a
                                    // sub-ms sleep (Done cadence is ~16ms; catching
                                    // it within ~0.5ms adds no perceptible latency).
                                    std::thread::sleep(std::time::Duration::from_micros(500));
                                    window.request_redraw();
                                    return ControlFlow::Continue;
                                }
                                // Proceeding WITHOUT Done means the safety valve
                                // fired — the compositor stalled its frame
                                // callbacks and the swapchain has starved (Mutter
                                // holds every buffer until the surface is
                                // reconfigured). Recreate it, exactly as a manual
                                // window resize does, so the acquire below doesn't
                                // block the full ~1s timeout.
                                if !ready {
                                    tracing::debug!(
                                        target: "blinc_app::wayland_gate",
                                        "Done stalled; recreating swapchain to un-starve acquire",
                                    );
                                    surf.configure(blinc_app.device(), config);
                                }
                            }

                            // Get current frame (surf bound above)
                            let frame = match surf.get_current_texture() {
                                Ok(f) => f,
                                Err(wgpu::SurfaceError::Lost) => {
                                    surf.configure(blinc_app.device(), config);
                                    return ControlFlow::Continue;
                                }
                                Err(wgpu::SurfaceError::OutOfMemory) => {
                                    tracing::error!("Out of GPU memory");
                                    return ControlFlow::Exit;
                                }
                                Err(wgpu::SurfaceError::Timeout) => {
                                    // The compositor stopped releasing swapchain
                                    // buffers, so the acquire starved. Reconfigure
                                    // to un-stick it — the same recovery a manual
                                    // window resize produces. Without this the UI
                                    // thread re-enters the acquire and parks on the
                                    // GPU fence again ("responsive for a while,
                                    // then froze"). The keep-alive self-drive keeps
                                    // presents flowing afterward so it stays live.
                                    tracing::debug!(
                                        "Surface acquire timeout — reconfiguring to un-stick"
                                    );
                                    surf.configure(blinc_app.device(), config);
                                    window.request_redraw();
                                    return ControlFlow::Continue;
                                }
                                Err(e) => {
                                    tracing::warn!("Surface error: {:?}", e);
                                    return ControlFlow::Continue;
                                }
                            };

                            let render_tex: &wgpu::Texture = &frame.texture;
                            let view = render_tex.create_view(&wgpu::TextureViewDescriptor::default());

                            // Update context from window
                            windowed_ctx.update_from_window(window);

                            // Update viewport for lazy loading visibility checks
                            // Uses logical pixels (width/height) as that's what layout uses
                            rs.set_viewport_size(windowed_ctx.width, windowed_ctx.height);

                            // Get current time for animation updates (used in multiple phases)
                            let current_time = elapsed_ms();

                            // Clear overlays from previous frame (cursor, selection, focus ring)
                            // These are re-added during rendering if still active
                            rs.clear_overlays();

                            // Tick scroll physics and sync ScrollRef state BEFORE any rebuilds
                            // This ensures ScrollRef has up-to-date values when stateful components
                            // query scroll position during rebuild
                            //
                            // `process_pending_scroll_refs` returns true when it just
                            // promoted a ScrollRef command (e.g. a click handler's
                            // `scroll_to_with_options`) into a live spring. Without
                            // OR-ing that into `scroll_animating`, the frame that
                            // started the spring sees `tick_scroll_physics`'s pre-
                            // process Idle result and the end-of-frame redraw chain
                            // closes — the spring then sits in `Bouncing` until the
                            // next stray input wakes the loop. Symptom: smooth-scroll
                            // commands in carousel_demo "jump" to the target only
                            // after the next mouse move.
                            let scroll_animating = if let Some(ref mut tree) = ws.render_tree {
                                let ticking = tree.tick_scroll_physics(current_time);
                                let just_started = tree.process_pending_scroll_refs();
                                ticking || just_started
                            } else {
                                false
                            };

                            // =========================================================
                            // PHASE 1: Check if tree structure needs rebuild
                            // Only structural changes require tree rebuild
                            // =========================================================
                            let phase1_start = std::time::Instant::now();

                            // Check if event handlers marked anything dirty (auto-rebuild).
                            //
                            // The `tracing::info!` calls below are diagnostic for the
                            // double-build-on-launch investigation: every rebuild source
                            // names itself so the source of a spurious rebuild is obvious
                            // from the default log output. Lower to `debug!` once the
                            // root cause is fixed and the diagnostic isn't needed.
                            if let Some(ref tree) = ws.render_tree {
                                if tree.needs_rebuild() {
                                    tracing::info!(
                                        target: "blinc_app::rebuild_source",
                                        "Rebuild triggered by: dirty_tracker"
                                    );
                                    ws.needs_rebuild = true;
                                }
                            }

                            // Check if element refs were modified (triggers rebuild)
                            if ref_dirty_flag.swap(false, Ordering::SeqCst) {
                                tracing::info!(
                                    target: "blinc_app::rebuild_source",
                                    "Rebuild triggered by: ref_dirty_flag (State::set or ctx.request_rebuild)"
                                );
                                ws.needs_rebuild = true;
                            }

                            // Check if text widgets requested a rebuild (focus/text changes)
                            if blinc_layout::widgets::take_needs_rebuild() {
                                tracing::info!(
                                    target: "blinc_app::rebuild_source",
                                    "Rebuild triggered by: blinc_layout::widgets::request_rebuild \
                                     (text-input focus change, request_full_rebuild from theme, etc.)"
                                );
                                ws.needs_rebuild = true;
                            }


                            // Check if a full relayout was requested (e.g., theme changes)
                            if blinc_layout::widgets::take_needs_relayout() {
                                tracing::info!(
                                    target: "blinc_app::rebuild_source",
                                    "Relayout triggered by: blinc_layout::widgets::request_relayout \
                                     (theme color scheme change, font reload, etc.)"
                                );
                                ws.needs_relayout = true;
                            }

                            // Check if CSS stylesheets need reparsing (e.g., theme color scheme changed)
                            // This must happen before tree rebuild so the new stylesheet is available
                            if blinc_layout::widgets::take_needs_css_reparse() {
                                tracing::debug!("Reparsing CSS stylesheets due to theme change");
                                windowed_ctx.reparse_css();
                            }

                            // Process pending motion exit starts BEFORE overlay update
                            // This is critical: when an overlay closes, it queues a motion exit via
                            // query_motion(key).exit(). The overlay's update() method then checks
                            // if the motion is done animating. If we don't process the exit queue
                            // first, the motion won't be in Exiting state yet, and update() will
                            // incorrectly think the exit animation is complete.
                            rs.process_global_motion_exit_starts();
                            rs.process_global_motion_exit_cancels();
                            // Process suspended motion starts queued via query_motion(key).start()
                            rs.process_global_motion_starts();

                            // Sync motion states to shared store so overlay can query them
                            // This must happen after processing exits but before overlay update
                            rs.sync_shared_motion_states();

                            // Update overlay manager viewport and state for subtree rebuilds
                            // This must happen BEFORE checking is_dirty() so build_overlay_layer() works correctly
                            windowed_ctx.overlay_manager.set_viewport_with_scale(
                                windowed_ctx.width,
                                windowed_ctx.height,
                                windowed_ctx.scale_factor as f32,
                            );
                            windowed_ctx.overlay_manager.update(current_time);
                            // Phase 3 transition: also tick the new OverlayStack +
                            // ToastTray. Each is the authoritative manager for the
                            // widgets that have already been migrated.
                            {
                                use blinc_layout::overlay_state::{overlay_stack, toast_tray};
                                if let Ok(mut s) = overlay_stack().lock() {
                                    s.set_viewport_with_scale(
                                        windowed_ctx.width,
                                        windowed_ctx.height,
                                        windowed_ctx.scale_factor as f32,
                                    );
                                    s.update(current_time);
                                }
                                if let Ok(mut t) = toast_tray().lock() {
                                    t.update(current_time);
                                }
                            }

                            // Check if overlay content changed (new overlay opened/closed)
                            // NOTE: We only rebuild on actual content changes, NOT during animations.
                            // Animation visual updates (backdrop opacity, motion transforms) are handled
                            // by the motion system and render-time interpolation, not content rebuilds.
                            // Rebuilding during animation breaks event handlers because node IDs change.
                            let overlay_content_dirty = windowed_ctx.overlay_manager.is_dirty();

                            if overlay_content_dirty {
                                tracing::debug!(
                                    "Overlay rebuild: dirty={}, has_visible={}",
                                    overlay_content_dirty,
                                    windowed_ctx.overlay_manager.has_visible_overlays()
                                );
                                // Look up the overlay layer node by its element ID
                                if let Some(overlay_node_id) = element_registry.get(
                                    blinc_layout::widgets::overlay::OVERLAY_LAYER_ID
                                ) {
                                    tracing::debug!("Overlay changed - queueing subtree rebuild for node {:?}", overlay_node_id);
                                    // Build the new overlay content and queue the subtree rebuild
                                    let overlay_content = windowed_ctx.overlay_manager.build_overlay_layer();
                                    blinc_layout::queue_subtree_rebuild(overlay_node_id, overlay_content);
                                } else {
                                    tracing::warn!("Overlay changed but node '{}' not found in registry - will rebuild on next frame",
                                        blinc_layout::widgets::overlay::OVERLAY_LAYER_ID);
                                }
                                // Consume the dirty flag
                                windowed_ctx.overlay_manager.take_dirty();
                            }

                            // Phase 3 transition: same incremental rebuild path
                            // for the new OverlayStack + ToastTray. Generic helper
                            // handles the dirty-check + registry-lookup +
                            // queue_subtree_rebuild dance per surface, so adding
                            // a new overlay layer in the future is a one-liner.
                            {
                                use blinc_layout::overlay_state::{
                                    overlay_stack, rebuild_overlay_subtree_if_dirty,
                                    toast_tray,
                                };
                                use blinc_layout::widgets::overlay_stack::OVERLAY_STACK_LAYER_ID;
                                use blinc_layout::widgets::toast_tray::TOAST_TRAY_LAYER_ID;

                                rebuild_overlay_subtree_if_dirty(
                                    &element_registry,
                                    OVERLAY_STACK_LAYER_ID,
                                    overlay_stack()
                                        .lock()
                                        .map(|s| s.take_dirty())
                                        .unwrap_or(false),
                                    || {
                                        overlay_stack()
                                            .lock()
                                            .ok()
                                            .map(|s| s.build_overlay_layer())
                                            .unwrap_or_else(div)
                                    },
                                );

                                let viewport = (windowed_ctx.width, windowed_ctx.height);
                                rebuild_overlay_subtree_if_dirty(
                                    &element_registry,
                                    TOAST_TRAY_LAYER_ID,
                                    toast_tray()
                                        .lock()
                                        .map(|t| t.take_dirty())
                                        .unwrap_or(false),
                                    || {
                                        toast_tray()
                                            .lock()
                                            .ok()
                                            .map(|t| t.build_tray_layer(viewport))
                                            .unwrap_or_else(div)
                                    },
                                );
                            }

                            // Check if stateful elements requested a redraw (hover/press changes)
                            // Apply incremental prop updates without full rebuild
                            let has_stateful_updates = blinc_layout::take_needs_redraw();
                            let has_pending_rebuilds = blinc_layout::has_pending_subtree_rebuilds();

                            if has_stateful_updates || has_pending_rebuilds {
                                if has_stateful_updates {
                                    tracing::debug!("Redraw requested by: stateful state change");
                                }

                                // Drain the unified property channel
                                // ([[project-reactive-architecture-v2]]). Accumulated
                                // side effects decide whether the drain forces a layout
                                // pass.
                                let prop_updates =
                                    blinc_layout::take_pending_partial_prop_updates();
                                let had_prop_updates = !prop_updates.is_empty();
                                let mut prop_effects = blinc_layout::SideEffects::default();
                                if let Some(ref mut tree) = ws.render_tree {
                                    for upd in prop_updates {
                                        prop_effects = prop_effects.or(upd.effects);
                                        // Tier-1 visual write into RenderProps, if any.
                                        if let Some(write) = upd.render_write {
                                            tree.update_render_props(upd.node_id, |p| write(p));
                                        }
                                        // Tier-2 layout write into the live taffy Style.
                                        // Read-modify-write because taffy stores styles
                                        // inside its own arena and exposes setter-only
                                        // API; the clone-and-replace round trip is the
                                        // cost of supporting `.w(&signal)`-style bindings.
                                        if let Some(write) = upd.layout_write {
                                            if let Some(mut style) =
                                                tree.layout_tree.get_style(upd.node_id)
                                            {
                                                write(&mut style);
                                                tree.layout_tree.set_style(upd.node_id, style);
                                            }
                                        }
                                    }
                                }

                                // Process subtree rebuilds (from stateful changes OR overlay changes).
                                // Pass the router so the per-rebuild base-style pass
                                // re-applies matching `:focus` / `:hover` / `:active`
                                // rules on top of the just-written base CSS — without
                                // this, animation-tick frames (a Stateful refresh while
                                // a spring is still running) reset `.cn-input` etc to
                                // their idle CSS and Phase 4's gated state pass skips,
                                // leaving the popup looking unfocused until a mouse-
                                // move bumps the router fingerprint.
                                let mut needs_layout = prop_effects.needs_layout;
                                let mut had_subtree_rebuild = false;
                                if let Some(ref mut tree) = ws.render_tree {
                                    let rebuilt = tree.process_pending_subtree_rebuilds_routed(
                                        Some(&windowed_ctx.event_router),
                                    );
                                    needs_layout |= rebuilt;
                                    had_subtree_rebuild = rebuilt;
                                }
                                // Subtree rebuild only mutates render
                                // nodes / base_styles / registry; it
                                // does NOT set ws.did_rebuild and does
                                // NOT touch the compositor cache. Next
                                // frame's fast path would blit stale
                                // bg + dispatch stale dyn batch until
                                // some other input invalidated the
                                // cache. Visible as a freshly-focused
                                // cn::input losing its focused bg /
                                // border for ~200ms post-open: the
                                // Stateful refreshed Idle → Focused
                                // but the cached primitives still hold
                                // the Idle visuals. Invalidate here so
                                // the post-rebuild paint runs the slow
                                // walker once and repopulates.
                                if had_subtree_rebuild {
                                    blinc_app.invalidate_render_cache_tagged(
                                        "subtree_rebuild_applied",
                                    );
                                }

                                // Drain deferred-focus queues NOW that the
                                // subtree rebuilds have actually applied
                                // (Stateful::build set the focused widget's
                                // node_id). Previously this ran BEFORE the
                                // rebuild processing, so the focus was
                                // applied to the data cell but
                                // focused_text_input_node_id() returned
                                // None (stateful_state.node_id still
                                // unset). The downstream EventRouter
                                // focus-sync at Phase 4 then read None,
                                // didn't bridge focus to the cn::input's
                                // node, and the CSS :focus selectors
                                // never matched — visible as the input
                                // appearing to lose its focus border /
                                // bg the first few frames after open
                                // (the cn::textarea case got lucky
                                // because the mouse landed on it,
                                // putting the FSM in FocusedHovered via
                                // POINTER_ENTER instead). Draining post-
                                // rebuild means node_id is set when
                                // focus_text_input runs, so the very
                                // next focus-sync sees the right
                                // LayoutNodeId and the input stays
                                // visually focused from frame 1.
                                blinc_layout::widgets::process_pending_input_focus();
                                blinc_layout::widgets::process_pending_area_focus();

                                // focus_text_input / focus_text_area
                                // drive the Stateful's `shared.state`
                                // to Focused and call `refresh_stateful`,
                                // which routes through `refresh_props_internal`
                                // and `queue_prop_update(node, focused_props)`.
                                // The drain at the top of this block
                                // already ran before focus, so the
                                // freshly-queued Focused props would sit
                                // in the partial-update channel until the
                                // NEXT frame — meaning this frame's slow
                                // walker (the one we just invalidated for
                                // above) still reads the Idle render
                                // props baked by the rebuild and the
                                // popup paints with no focus bg/border
                                // until either a mouse-move triggers
                                // another invalidation or the prop
                                // update lands. Drain again now so the
                                // Focused props flow into render_node.props
                                // before Phase 4 paints this frame.
                                let post_focus_updates =
                                    blinc_layout::take_pending_partial_prop_updates();
                                let had_post_focus_updates =
                                    !post_focus_updates.is_empty();
                                if had_post_focus_updates {
                                    if let Some(ref mut tree) = ws.render_tree {
                                        for upd in post_focus_updates {
                                            prop_effects = prop_effects.or(upd.effects);
                                            if let Some(write) = upd.render_write {
                                                tree.update_render_props(
                                                    upd.node_id,
                                                    |p| write(p),
                                                );
                                            }
                                            if let Some(write) = upd.layout_write {
                                                if let Some(mut style) =
                                                    tree.layout_tree.get_style(upd.node_id)
                                                {
                                                    write(&mut style);
                                                    tree.layout_tree
                                                        .set_style(upd.node_id, style);
                                                }
                                            }
                                        }
                                    }
                                    needs_layout |= prop_effects.needs_layout;
                                    blinc_app.invalidate_render_cache_tagged(
                                        "post_focus_prop_update",
                                    );
                                }

                                // `refresh_props_internal` (driven by
                                // focus_text_input / focus_text_area
                                // via refresh_stateful) queues BOTH a
                                // partial prop update AND a subtree
                                // rebuild — the rebuild lives in
                                // PENDING_SUBTREE_REBUILDS, NOT in the
                                // partial channel, so the drain above
                                // did NOT process it. cn::input /
                                // cn::textarea hit the topology
                                // rebuild path because their on_state
                                // callback adds the cursor canvas only
                                // when visual.is_focused(); Idle →
                                // Focused therefore queues a full
                                // structural rebuild whose post-step
                                // calls apply_stylesheet_base_styles_for_subtree
                                // and silently clobbers the focused
                                // bg/border with the `.cn-input` /
                                // `.cn-textarea` base CSS rule. Phase 4's
                                // apply_stylesheet_state_styles is gated
                                // on a router-state-fingerprint change;
                                // focus didn't change between the
                                // build frame and the next paint frame,
                                // so the gate fails and `:focus` never
                                // re-applies. The popup appears
                                // unfocused until a mouse-move bumps
                                // the fingerprint. Fix: unconditionally
                                // drain the queued subtree rebuilds and
                                // clear last_router_state_fp here so
                                // (1) the focus-triggered rebuild
                                // applies on the SAME frame as focus,
                                // and (2) Phase 4 re-runs the state-
                                // style pass and restores the :focus
                                // visuals on top of the just-clobbered
                                // base.
                                let pending_after_focus =
                                    blinc_layout::has_pending_subtree_rebuilds();
                                if had_post_focus_updates || pending_after_focus {
                                    // Bridge EventRouter.focus to the
                                    // freshly-focused text input BEFORE
                                    // the rebuild runs, so the routed
                                    // base-pass sees the correct focused
                                    // node when it evaluates `:focus`
                                    // selectors against the subtree.
                                    // (Phase 4's standard focus sync at
                                    // line ~6354 runs too late for the
                                    // state pass that fires inside
                                    // process_pending_subtree_rebuilds_routed.)
                                    let text_focus =
                                        blinc_layout::widgets::text_input::focused_text_input_node_id()
                                            .or_else(blinc_layout::widgets::text_input::focused_text_area_node_id);
                                    let current_focus = windowed_ctx.event_router.focused();
                                    if text_focus != current_focus {
                                        windowed_ctx.event_router.set_focus(text_focus);
                                    }
                                    if let Some(ref mut tree) = ws.render_tree {
                                        if tree.process_pending_subtree_rebuilds_routed(
                                            Some(&windowed_ctx.event_router),
                                        ) {
                                            needs_layout = true;
                                        }
                                    }
                                    blinc_app.invalidate_render_cache_tagged(
                                        "post_focus_subtree_rebuild",
                                    );
                                }

                                if needs_layout {
                                    if let Some(ref mut tree) = ws.render_tree {
                                        tracing::debug!("Subtree rebuilds processed, recomputing layout");
                                        tree.apply_stylesheet_layout_overrides();
                                        tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                                        // FLIP: detect position changes and start CSS transitions
                                        tree.apply_flip_transitions();
                                        // Update FLIP bounds cache for next rebuild
                                        tree.update_flip_bounds();
                                        // Begin/end motion frame to track which motions are still in tree
                                        rs.begin_stable_motion_frame();
                                        tree.initialize_motion_animations(rs);
                                        rs.end_stable_motion_frame();
                                        rs.process_global_motion_replays();
                                        // Start CSS animations for elements with animation properties
                                        tree.start_all_css_animations();
                                    }
                                    // Re-run hit-test at the current mouse position so newly
                                    // mounted subtrees (notably dropdown / context-menu items
                                    // that just appeared under the stationary cursor) get
                                    // POINTER_ENTER + CSS :hover immediately. Without this,
                                    // hover state only updates on the next mouse-move event —
                                    // so a freshly-opened dropdown shows no hover bg on the
                                    // item the cursor is already over.
                                    if let Some(ref mut tree) = ws.render_tree {
                                        let (mx, my) = windowed_ctx.event_router.mouse_position();
                                        if mx.is_finite() && my.is_finite() {
                                            let _ = windowed_ctx
                                                .event_router
                                                .on_mouse_move(tree, mx, my);
                                        }
                                    }
                                }
                                if had_prop_updates && !needs_layout {
                                    tracing::trace!("Visual-only prop updates, skipping layout");
                                }

                                // Stateful animations (springs, keyframes,
                                // timelines) queue prop updates here when their
                                // scheduler-driven `refresh_callback` re-runs
                                // the `on_state` callback. Without an explicit
                                // cache invalidate, the fast path's
                                // `cached_bg_batch` would keep the previous
                                // frame's primitives — and the walker (which is
                                // what reads `render_node.props` to emit fresh
                                // primitives) doesn't run on the fast path.
                                // Symptom: timeline_demo's bouncing ball,
                                // motion_demo's pull-to-refresh contents, and
                                // any other stateful-animation-driven visual
                                // stay frozen on the previous frame until some
                                // input (mouse move, scroll) trips a different
                                // invalidate. The state-driven path through
                                // `handle_event_internal` already covers
                                // event-fired transitions via `state_changed`
                                // above; this catches the scheduler-driven half.
                                if had_prop_updates {
                                    blinc_app.invalidate_render_cache_tagged(
                                        "stateful_prop_update",
                                    );
                                }

                                // Structural subtree rebuilds replace render
                                // nodes wholesale (new ids, fresh props from
                                // `collect_render_props_boxed`, CSS class base
                                // styles re-applied). The static-cache batch
                                // still holds the *previous* frame's
                                // primitives, so the first paint after a
                                // dropdown-open / overlay-push / stateful
                                // structural transition keeps showing stale
                                // visuals (search input renders without its
                                // configured bg, freshly-mounted `--selected`
                                // rows show no highlight, etc.) until the
                                // next user input trips `state_changed`. The
                                // gate above only fires on prop updates;
                                // mirror it here for the rebuild path. Same
                                // root cause class as the `motion_just_settled`
                                // cache-stamping fix — the walker has new data
                                // but the compositor's cache predicate hasn't
                                // been told to discard the old.
                                if has_pending_rebuilds {
                                    blinc_app.invalidate_render_cache_tagged(
                                        "structural_subtree_rebuild",
                                    );
                                }

                                // Visual-only updates (e.g. hover state flip)
                                // happened mid-frame — make sure the next
                                // frame renders rather than getting skipped
                                // by the start-of-frame dirty gate.
                                frame_dirty.store(true, Ordering::Release);
                                window.request_redraw();
                            }

                            t_phase1 = phase1_start.elapsed();

                            // =========================================================
                            // PHASE 2: Build/rebuild tree only for structural changes
                            // This must happen BEFORE tick() so motion animations are available
                            // =========================================================
                            let phase2_start = std::time::Instant::now();

                            // Begin stable motion frame tracking
                            // This clears the "used" set so we can detect which motions are no longer in the tree
                            rs.begin_stable_motion_frame();

                            // Hot-reload: a `subsecond::apply_patch` succeeded since
                            // the last frame, so the user's UI closure body has been
                            // swapped underneath us. Force a full tree rebuild so the
                            // closure (wrapped in `subsecond::call`) gets re-invoked
                            // — otherwise we'd keep painting the cached pre-patch tree.
                            //
                            // Also reset accumulated stylesheet state so `ctx.add_css`
                            // calls in the patched closure produce a fresh sheet. The
                            // common `rebuild_count == 0` guard around `add_css` would
                            // otherwise skip re-registration, and `add_css` itself
                            // cascades — deleted CSS rules would linger forever.
                            #[cfg(feature = "hot-reload")]
                            {
                                // Drain dx asset invalidations FIRST, so cached
                                // decoded copies are gone before the rebuild
                                // re-loads anything. Path-keyed: image cache,
                                // SVG cache, SVG atlas. Glyph caches and font
                                // faces aren't path-keyed and would need
                                // separate plumbing — out of scope for this
                                // pass.
                                let asset_paths = crate::hot_reload::take_invalidations();
                                for path in &asset_paths {
                                    blinc_app.context().invalidate_asset_path(path);
                                }

                                if crate::hot_reload::take_rebuild_pending() {
                                    tracing::info!("hot-reload: forcing tree rebuild");
                                    ws.needs_rebuild = true;
                                    // The builder runs again and the diff
                                    // carries the edit into the live tree.
                                    // Keeping the tree keeps scroll offsets,
                                    // focus and node identity, and costs one
                                    // incremental update instead of a full
                                    // build of the whole window.
                                    //
                                    // CSS is the part the diff can't see: an
                                    // edited stylesheet string is re-parsed
                                    // into `windowed_ctx.stylesheet`, but the
                                    // rules only reach nodes when a stylesheet
                                    // pass runs, and `NoChanges` / `VisualOnly`
                                    // don't run one. Flag it for the pass below.
                                    ws.hot_reload_restyle = true;
                                    windowed_ctx.reset_for_hot_reload();
                                    // The tree updates but the renderer's cached
                                    // scene doesn't, so a reloaded literal would
                                    // composite under the glyphs of the string
                                    // it replaced.
                                    blinc_app.invalidate_render_cache_tagged("hot-reload");
                                }
                            }

                            if ws.needs_rebuild || ws.render_tree.is_none() {
                                // Reset call counters for stable key generation
                                reset_call_counters();
                                // Clear stale Stateful base_render_props updaters
                                blinc_layout::clear_stateful_base_updaters();
                                blinc_layout::click_outside::clear_click_outside_handlers();

                                // Reset stable motions so they replay on full rebuild
                                // This ensures motion animations play when UI is reconstructed
                                rs.reset_stable_motions_for_rebuild();

                                // Note: Viewport and overlay state are already updated in PHASE 1
                                // so build_overlay_layer() has correct dimensions

                                // Build UI element tree
                                let user_ui = invoke_ui_builder(&mut ui_builder, windowed_ctx);

                                // Drain any CSS queued via
                                // `BlincContextState::queue_stylesheet`
                                // (e.g. by `BlincDsl::view_widget`'s
                                // first invocation). Same pattern
                                // as `drain_custom_passes` below —
                                // the queue exists because DSL /
                                // plugin code doesn't see
                                // `WindowedContext` directly.
                                {
                                    let queued =
                                        blinc_core::BlincContextState::get().drain_stylesheets();
                                    for css in queued {
                                        windowed_ctx.add_css(&css);
                                    }
                                }

                                // Compose user UI with overlay layer using a regular Div container
                                // We use position:relative with the overlay absolutely positioned on top.
                                let overlay_layer = windowed_ctx.overlay_manager.build_overlay_layer();
                                // Phase 3 transition: the new OverlayStack composites alongside
                                // the legacy manager. Each widget migration moves one widget
                                // from the legacy layer to this layer; both render until
                                // Phase 5 deletes the legacy module.
                                let stack_layer = {
                                    use blinc_layout::overlay_state::overlay_stack;
                                    let stack = overlay_stack();
                                    let guard = stack.lock().ok();
                                    match guard {
                                        Some(mut s) => {
                                            s.set_viewport_with_scale(
                                                windowed_ctx.width,
                                                windowed_ctx.height,
                                                windowed_ctx.scale_factor as f32,
                                            );
                                            s.build_overlay_layer()
                                        }
                                        None => div(),
                                    }
                                };
                                // Toast tray composites above the overlay stack
                                // (notification queue lives above modal stack).
                                let tray_layer = {
                                    use blinc_layout::overlay_state::toast_tray;
                                    let viewport = (windowed_ctx.width, windowed_ctx.height);
                                    toast_tray()
                                        .lock()
                                        .ok()
                                        .map(|t| t.build_tray_layer(viewport))
                                        .unwrap_or_else(div)
                                };
                                let ui = div()
                                    .w(windowed_ctx.width)
                                    .h(windowed_ctx.height)
                                    .relative() // positioning context for overlay
                                    .child(user_ui)
                                    .child(overlay_layer)
                                    .child(stack_layer)
                                    .child(tray_layer);

                                // Use incremental update if we have an existing tree
                                // BUT: Skip incremental update during resize - do full rebuild instead
                                // This ensures parent constraints properly propagate to all children
                                if let Some(ref mut existing_tree) = ws.render_tree {
                                    if ws.needs_relayout {
                                        // Window resize: bypass incremental update, do full rebuild
                                        // This ensures proper constraint propagation from parents to children
                                        tracing::debug!("Window resize: full tree rebuild (bypassing incremental update)");

                                        // Clear layout bounds storages before rebuild
                                        existing_tree.clear_layout_bounds_storages();

                                        // Full rebuild: create new tree from element with shared registry
                                        // Pass registry to from_element_with_registry so IDs are registered during build
                                        let mut tree = RenderTree::from_element_with_registry(
                                            &ui,
                                            Arc::clone(&element_registry),
                                        );

                                        // Set animation scheduler for scroll bounce springs
                                        tree.set_animations(&windowed_ctx.animations);

                                        // Share the CSS animation store (ticked by scheduler thread)
                                        tree.set_css_anim_store(Arc::clone(&css_anim_store));

                                        // Set DPI scale factor for HiDPI rendering
                                        tree.set_scale_factor(windowed_ctx.scale_factor as f32);

                                        // Set CSS stylesheet for automatic style application
                                        if let Some(ref stylesheet) = windowed_ctx.stylesheet {
                                            tree.set_stylesheet_arc(stylesheet.clone());
                                        }
                                        // Apply CSS visual + layout styles in a single optimized pass
                                        // (builds class index once, iterates rules once)
                                        tree.apply_all_stylesheet_styles();

                                        // Register pointer-space elements from stylesheet
                                        if let Some(ref stylesheet) = windowed_ctx.stylesheet {
                                            windowed_ctx.pointer_query.register_from_stylesheet(stylesheet);
                                        }

                                        // Compute layout with new viewport dimensions
                                        tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                                        tree.update_flip_bounds();

                                        // Initialize motion animations for any nodes wrapped in motion() containers
                                        tree.initialize_motion_animations(rs);
                                        // End motion frame to detect unmounted motions and trigger exit animations
                                        rs.end_stable_motion_frame();
                                        // Process any motion replay requests queued during tree building
                                        rs.process_global_motion_replays();
                                        // Start CSS animations for elements with animation properties
                                        tree.start_all_css_animations();

                                        // Replace existing tree with fresh one
                                        *existing_tree = tree;

                                        // Clear relayout flag after full rebuild
                                        ws.needs_relayout = false;
                                    } else {
                                        // Normal incremental update (no resize)
                                        use blinc_layout::UpdateResult;

                                        // Update stylesheet in case it changed between frames
                                        if let Some(ref stylesheet) = windowed_ctx.stylesheet {
                                            existing_tree.set_stylesheet_arc(stylesheet.clone());
                                        }

                                        let update_result = existing_tree.incremental_update(&ui);

                                        // An edited stylesheet leaves every
                                        // element hash untouched, so the diff
                                        // reports NoChanges and no pass runs.
                                        // Re-apply the rules the reload just
                                        // re-parsed, then let the match below
                                        // handle whatever else changed.
                                        let restyle = std::mem::take(&mut ws.hot_reload_restyle);
                                        if restyle {
                                            existing_tree.apply_all_stylesheet_styles();
                                            existing_tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                                            existing_tree.update_flip_bounds();
                                        }

                                        match update_result {
                                            UpdateResult::NoChanges => {
                                                tracing::debug!("Incremental update: NoChanges - skipping rebuild");
                                            }
                                            UpdateResult::VisualOnly => {
                                                tracing::debug!("Incremental update: VisualOnly - skipping layout");
                                                // Props already updated in-place by incremental_update
                                            }
                                            UpdateResult::LayoutChanged => {
                                                // Layout changed - recompute layout
                                                tracing::debug!("Incremental update: LayoutChanged - recomputing layout");
                                                existing_tree.apply_stylesheet_layout_overrides();
                                                existing_tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                                                existing_tree.update_flip_bounds();
                                            }
                                            UpdateResult::ChildrenChanged => {
                                                // Children changed - subtrees were rebuilt in place
                                                tracing::debug!("Incremental update: ChildrenChanged - subtrees rebuilt");

                                                // Apply CSS styles to new nodes from rebuilt subtrees
                                                // (collect_render_props only applies ID-based CSS;
                                                // class selectors need apply_stylesheet_base_styles)
                                                existing_tree.apply_stylesheet_base_styles();
                                                // Recompute layout since structure changed
                                                existing_tree.apply_stylesheet_layout_overrides();
                                                existing_tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                                                // FLIP: detect position changes and start CSS transitions
                                                existing_tree.apply_flip_transitions();
                                                existing_tree.update_flip_bounds();

                                                // Re-register pointer-space elements (new elements may have pointer-space)
                                                if let Some(ref stylesheet) = windowed_ctx.stylesheet {
                                                    windowed_ctx.pointer_query.register_from_stylesheet(stylesheet);
                                                }

                                                // Initialize motion animations for any new nodes wrapped in motion() containers
                                                existing_tree.initialize_motion_animations(rs);
                                                // End motion frame to detect unmounted motions and trigger exit animations
                                                rs.end_stable_motion_frame();

                                                // Process any global motion replays that were queued during tree building
                                                rs.process_global_motion_replays();
                                                // Start CSS animations for elements with animation properties
                                                existing_tree.start_all_css_animations();
                                            }
                                        }
                                    }
                                } else {
                                    // No existing tree - create new with shared registry
                                    let mut tree = RenderTree::from_element_with_registry(
                                        &ui,
                                        Arc::clone(&element_registry),
                                    );

                                    // Set animation scheduler for scroll bounce springs
                                    tree.set_animations(&windowed_ctx.animations);

                                    // Share the CSS animation store (ticked by scheduler thread)
                                    tree.set_css_anim_store(Arc::clone(&css_anim_store));

                                    // Set DPI scale factor for HiDPI rendering
                                    tree.set_scale_factor(windowed_ctx.scale_factor as f32);

                                    // Set CSS stylesheet for automatic style application
                                    if let Some(ref stylesheet) = windowed_ctx.stylesheet {
                                        tree.set_stylesheet_arc(stylesheet.clone());
                                    }
                                    // Apply CSS visual + layout styles in a single optimized pass
                                    tree.apply_all_stylesheet_styles();

                                    // Register pointer-space elements from stylesheet
                                    if let Some(ref stylesheet) = windowed_ctx.stylesheet {
                                        windowed_ctx.pointer_query.register_from_stylesheet(stylesheet);
                                    }

                                    // Compute layout in logical pixels
                                    tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                                    tree.update_flip_bounds();

                                    // Initialize motion animations for any nodes wrapped in motion() containers
                                    tree.initialize_motion_animations(rs);
                                    // End motion frame to detect unmounted motions and trigger exit animations
                                    rs.end_stable_motion_frame();

                                    // Process any global motion replays that were queued during tree building
                                    rs.process_global_motion_replays();
                                    // Start CSS animations for elements with animation properties
                                    tree.start_all_css_animations();

                                    ws.render_tree = Some(tree);
                                }

                                ws.needs_rebuild = false;
                                did_rebuild = true;
                                let was_first_rebuild = windowed_ctx.rebuild_count == 0;
                                windowed_ctx.rebuild_count = windowed_ctx.rebuild_count.saturating_add(1);

                                // Execute on_ready callbacks after first rebuild
                                if was_first_rebuild {
                                    if let Ok(mut callbacks) = ready_callbacks.lock() {
                                        for callback in callbacks.drain(..) {
                                            callback();
                                        }
                                    }
                                }
                            } else {
                                // No rebuild needed - still need to end the motion frame
                                // If an existing tree exists, initialize motions to mark them as used
                                if let Some(ref tree) = ws.render_tree {
                                    tree.initialize_motion_animations(rs);
                                }
                                rs.end_stable_motion_frame();
                            }

                            // Note: on_ready callbacks are only executed after the FIRST rebuild
                            // (in the was_first_rebuild block above). Callbacks registered
                            // after the first rebuild are executed immediately since the UI
                            // is already ready at that point.

                            t_phase2 = phase2_start.elapsed();

                            // =========================================================
                            // PHASE 3: Tick animations and dynamic render state
                            // This must happen AFTER tree rebuild so motions are initialized
                            // =========================================================
                            let phase3_start = std::time::Instant::now();

                            // Process any pending motion exit cancellations
                            // This must happen before tick() so cancelled motions don't continue exiting
                            rs.process_global_motion_exit_cancels();

                            // Process any pending motion exit starts (explicit exit triggers)
                            rs.process_global_motion_exit_starts();

                            // Process suspended motion starts queued via query_motion(key).start()
                            rs.process_global_motion_starts();

                            // Capture pre-tick motion liveness so the `should_render`
                            // gate below can recognise a "this frame's tick settled
                            // the motion" transition. Without this, the final tick
                            // (Entering → Visible) would leave `has_active_motions()`
                            // false at the should_render check, the cap gate could
                            // skip the paint, and the user would see the animation
                            // freeze at the penultimate state — the "animation is
                            // not allowed to settle" symptom. Capturing pre-tick
                            // ensures the paint of the settled state still ships.
                            let motion_was_active_pre_tick = rs.has_active_motions();
                            // Same snapshot, restricted to nodes the last paint
                            // actually walked. `motion_was_active_pre_tick` has to
                            // stay unfiltered for the settle detection below (it is
                            // compared against an equally unfiltered post-tick
                            // read), but the cap-bypass gate wants the visible form
                            // — an enter/exit motion on a node scrolled out of view
                            // has nothing to stair-step, so paying vsync for it is
                            // pure idle burn.
                            let motion_was_visible_pre_tick = ws
                                .render_tree
                                .as_ref()
                                .is_some_and(|t| {
                                    rs.has_active_motions_visible(&t.painted_node_ids())
                                });

                            // Tick render state (handles cursor blink, color animations, etc.)
                            // This updates dynamic properties without touching tree structure
                            let _animations_active = rs.tick(current_time);

                            // Detect "motion just settled this frame": pre-tick the
                            // FSM had at least one motion mid-flight, post-tick it
                            // has none. The current paint will display the settled
                            // state — but we still arm one more redraw via
                            // `stateful::request_redraw()` so the next Frame fires
                            // unconditionally. Belt-and-suspenders against the
                            // "final frame doesn't ship until mouse-move" symptom:
                            // even if the current frame somehow skipped its paint
                            // (cap-interval edge, surface contention, etc.), the
                            // follow-up frame still paints the final state without
                            // requiring external input to wake the loop.
                            let motion_just_settled =
                                motion_was_active_pre_tick && !rs.has_active_motions();
                            if motion_just_settled {
                                blinc_layout::stateful::request_redraw();
                                // Force the next paint to re-walk the tree from
                                // scratch. The compositor's static-cache
                                // invalidation gate (`other_animations_active`
                                // in `try_render_with_compositor`) reads
                                // `has_active_motions()` POST-tick — on the
                                // settle frame that's already `false`, so the
                                // gate keeps the cached primitives from the
                                // penultimate paint (which were rasterised with
                                // `motion.current` in its lerped state, not the
                                // settled `MotionKeyframe::default()`). User
                                // sees the animation freeze one frame short of
                                // the final state. Invalidating here forces
                                // the walker to repopulate the cache with
                                // post-tick `motion.current = default` values.
                                blinc_app.invalidate_render_cache_tagged(
                                    "motion_just_settled",
                                );
                            }

                            // Tick CSS animations/transitions synchronously on the main thread.
                            // The scheduler's bg thread drives 120fps redraws via wake_callback,
                            // but actual ticking is done here to stay in phase with rendering.
                            let dt_ms = if ws.last_frame_time_ms > 0 {
                                (current_time - ws.last_frame_time_ms) as f32
                            } else {
                                16.0
                            };
                            let (css_active, css_only_composite_promotable) =
                                if let Some(ref mut tree) = ws.render_tree {
                                    let store = tree.css_anim_store();
                                    let mut s = store.lock().unwrap();
                                    let (anim, trans) = s.tick(dt_ms);
                                    drop(s);
                                    let flip = tree.tick_flip_animations(dt_ms);
                                    let active =
                                        anim || trans || flip || tree.css_has_active();
                                    // Composite-promotable predicate ignores FLIP
                                    // (FLIP animations re-layout under the hood,
                                    // so they always need the slow path).
                                    let promotable = !flip
                                        && tree.css_active_all_composite_promotable();
                                    (active, promotable)
                                } else {
                                    (false, true)
                                };
                            ws.last_frame_time_ms = current_time;

                            // Sync motion states to shared store for query_motion API
                            rs.sync_shared_motion_states();

                            // Tick theme animation (handles color interpolation during theme transitions)
                            let theme_animating = blinc_theme::ThemeState::get().tick();

                            // Note: scroll physics tick moved to before PHASE 1 (before any rebuilds)
                            // so that ScrollRef has up-to-date values when stateful components rebuild

                            t_phase3 = phase3_start.elapsed();

                            // Animation-only frames (no rebuild, no relayout,
                            // no stateful redraw, no input-driven dirty)
                            // cost ~6 ms of CPU per rendered frame on
                            // cn_demo — overwhelmingly in GPU dispatch (the
                            // paint walker itself is only ~0.1 ms, see the
                            // `p4_breakdown` trace). When the app set an
                            // `animation_fps_cap`, throttle Phase 4 to that
                            // rate without losing the scheduler's Phase 3
                            // tick. The `wake_at` below schedules the next
                            // Frame for exactly the moment the cap interval
                            // elapses, so we don't get the spinning request
                            // _redraw / immediate-Frame loop that pinned
                            // CPU at 100 % (winit on macOS delivers
                            // `request_redraw`'d frames back-to-back at
                            // microsecond cadence, *not* vsync-locked, so
                            // the previous "re-arm at vsync" implementation
                            // burned 3000+ tick-only frames per second).
                            //
                            // Always render when: there's no cap, this is
                            // the very first paint, a rebuild ran this
                            // frame, or the cap interval has elapsed. Skip
                            // Phase 4 + present otherwise.
                            // Read the (possibly dynamic) cap from the atomic so
                            // the adaptive FPS path can change it at runtime
                            // without rewriting captured closure state. 0 ⇒ no cap.
                            let live_cap = animation_fps_cap_atomic.load(Ordering::Relaxed);
                            let cap_interval_ms = if live_cap == 0 {
                                None
                            } else {
                                Some(1000u64 / live_cap as u64)
                            };
                            let elapsed_since_paint =
                                current_time.saturating_sub(ws.last_paint_time_ms);

                            // Bypass the FPS cap whenever a vsync-class
                            // animation is mid-flight — transforms, layout
                            // sizing, clip-path geometry, and motion-FSM
                            // enter / exit animations all stair-step
                            // visibly when capped to 30 fps, and (more
                            // critically) the cap can swallow the final
                            // settle-to-Visible paint of a motion FSM
                            // animation, leaving the dialog / sheet stuck
                            // at the penultimate frame until a mouse-move
                            // wakes the loop. `has_visible_vsync_class`
                            // covers CSS animations / transitions; motion
                            // FSM (overlay enter / exit) is the
                            // `has_active_motions()` term.
                            //
                            // Use the PRE-tick `motion_was_active_pre_tick`
                            // snapshot — not the post-tick poll — so the
                            // frame where the final tick transitions the
                            // FSM from Entering → Visible still counts as
                            // vsync-class and the cap gate doesn't skip
                            // the paint that displays the settled state.
                            //
                            // Every term is painted-gated, matching
                            // `has_any_active_animation_visible`. The ungated form
                            // let a single off-screen looping motion hold
                            // `should_render` true forever: the cap never applied,
                            // the self-driving Frame loop rendered at vsync, and an
                            // idle window sat at 25-40% CPU with nothing moving on
                            // screen.
                            let needs_vsync = motion_was_visible_pre_tick
                                || ws
                                    .render_tree
                                    .as_ref()
                                    .is_some_and(|t| {
                                        rs.has_active_motions_visible(&t.painted_node_ids())
                                    })
                                || ws
                                    .render_tree
                                    .as_ref()
                                    .is_some_and(|t| {
                                        let store = t.css_anim_store();
                                        let guard = store.lock().unwrap();
                                        guard.has_visible_vsync_class(&t.painted_stable_ids())
                                    });
                            let should_render = match cap_interval_ms {
                                None => true,
                                Some(_) if did_rebuild => true,
                                Some(_) if ws.needs_relayout => true,
                                Some(_) if ws.last_paint_time_ms == 0 => true,
                                Some(_) if needs_vsync => true,
                                Some(interval) => elapsed_since_paint >= interval,
                            };
                            // Is layout being marked dirty every frame? Either
                            // `did_rebuild` or `needs_relayout` makes
                            // `should_render` true unconditionally, bypassing
                            // the FPS cap — an idle window that keeps setting
                            // one of them renders at vsync forever no matter
                            // what the animation-visibility gate says.
                            {
                                static T: std::sync::atomic::AtomicUsize =
                                    std::sync::atomic::AtomicUsize::new(0);
                                if T.fetch_add(1, Ordering::Relaxed) % 120 == 0 {
                                    tracing::debug!(
                                        did_rebuild,
                                        needs_relayout = ws.needs_relayout,
                                        needs_vsync,
                                        should_render,
                                        cap_interval_ms = ?cap_interval_ms,
                                        elapsed_since_paint,
                                        "frame gate: why are we rendering"
                                    );
                                }
                            }

                            if !should_render {
                                // Schedule the next Frame for exactly the
                                // moment the cap interval elapses. `wake_at`
                                // routes through the platform shim's timer
                                // thread (same path the cursor-blink pacing
                                // and the old `cap_applies` branch use), so
                                // we get one tick per cap interval instead
                                // of thousands per second.
                                if let Some(interval) = cap_interval_ms {
                                    let remaining =
                                        interval.saturating_sub(elapsed_since_paint).max(1);
                                    frame_dirty.store(true, Ordering::Release);
                                    wake_proxy_for_pacing
                                        .wake_at(std::time::Duration::from_millis(remaining));
                                }
                                let t_total = frame_start.elapsed();
                                tracing::trace!(
                                    target: "blinc_app::frame_timing",
                                    total_us = t_total.as_micros() as u64,
                                    p1_rebuild_check_us = t_phase1.as_micros() as u64,
                                    p2_rebuild_us = t_phase2.as_micros() as u64,
                                    p3_tick_us = t_phase3.as_micros() as u64,
                                    p4_render_us = 0u64,
                                    p5_chain_us = 0u64,
                                    did_rebuild,
                                    dirty_springs = dirty_spring_count,
                                    skipped = true,
                                    "frame"
                                );
                                return ControlFlow::Continue;
                            }
                            ws.last_paint_time_ms = current_time;

                            // =========================================================
                            // PHASE 4: Render
                            // Combines stable tree structure with dynamic render state
                            // =========================================================
                            let phase4_start = std::time::Instant::now();

                            // Sync text input/textarea focus to EventRouter so CSS :focus matching works
                            {
                                let text_focus = blinc_layout::widgets::text_input::focused_text_input_node_id()
                                    .or_else(blinc_layout::widgets::text_input::focused_text_area_node_id);
                                let current_focus = windowed_ctx.event_router.focused();
                                if text_focus != current_focus {
                                    windowed_ctx.event_router.set_focus(text_focus);
                                }
                            }

                            // Apply CSS state styles (:hover, :active, :focus) from stylesheet
                            // This also detects property changes and starts new transitions.
                            //
                            // Gate on event-router state fingerprint: only run the
                            // O(N) pass when the set of hovered / pressed / focused
                            // nodes actually differs from the previous frame.
                            // Animation-only ticks (the spinner-only steady state)
                            // share the previous frame's router state, so we skip
                            // the entire registered-IDs walk. First frame's
                            // `last_router_state_fp = None` forces the pass.
                            if let Some(ref mut tree) = ws.render_tree {
                                if tree.stylesheet().is_some() {
                                    let current_fp = windowed_ctx.event_router.state_fingerprint();
                                    let should_apply = ws.last_router_state_fp != Some(current_fp);
                                    if should_apply {
                                        let state_changed = tree.apply_stylesheet_state_styles(&windowed_ctx.event_router);
                                        ws.last_router_state_fp = Some(current_fp);
                                        // Recompute layout if state styles affected layout properties
                                        // (e.g. visibility: hidden → display: none, or height changes on hover)
                                        if state_changed {
                                            tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                                            tree.update_flip_bounds();
                                        }
                                    }
                                }
                            }

                            // Apply CSS animation/transition values AFTER state styles
                            // (state styles reset to base, animations must override)
                            if css_active || !ws.render_tree.as_ref().is_none_or(|t| t.css_transitions_empty()) {
                                if let Some(ref mut tree) = ws.render_tree {
                                    tree.apply_all_css_animation_props();
                                    tree.apply_all_css_transition_props();
                                    tree.apply_flip_animation_props();
                                    if tree.apply_animated_layout_props() {
                                        tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                                        tree.update_flip_bounds();
                                    }
                                }
                            }

                            // Update continuous pointer query state
                            if !windowed_ctx.pointer_query.is_empty() {
                                let (mx, my) = windowed_ctx.event_router.mouse_position();
                                let is_pressed = windowed_ctx.event_router.has_pressed_target();
                                let dt_sec = dt_ms / 1000.0;
                                let time_sec = current_time as f64 / 1000.0;
                                // Use event router's hit test results for hover detection.
                                // The router already handles scroll offsets, transforms, and occlusion
                                // correctly, so bounds from get_node_bounds match the rendering pipeline.
                                windowed_ctx.pointer_query.update(
                                    mx, my, is_pressed, dt_sec, time_sec,
                                    |id| {
                                        let node = element_registry.get(id)?;
                                        // is_hovered is stable-id-keyed
                                        // now (see event_router doc);
                                        // use the layout-id shim that
                                        // resolves internally.
                                        if let Some(ref tree) = ws.render_tree {
                                            if windowed_ctx
                                                .event_router
                                                .is_hovered_layout(tree, node)
                                            {
                                                return windowed_ctx
                                                    .event_router
                                                    .get_node_bounds(node);
                                            }
                                        }
                                        None
                                    },
                                );
                                // Evaluate dynamic calc(env(...)) properties with current pointer state
                                if let Some(ref mut tree) = ws.render_tree {
                                    tree.apply_pointer_styles(
                                        &windowed_ctx.pointer_query,
                                        &windowed_ctx.event_router,
                                    );
                                }
                            }

                            if let Some(ref tree) = ws.render_tree {
                                // Set blend target for mix-blend-mode support
                                blinc_app.set_blend_target(render_tex);

                                // Pass cursor position for @flow pointer input
                                let (mx, my) = windowed_ctx.event_router.mouse_position();
                                let sf = windowed_ctx.scale_factor as f32;
                                blinc_app.set_cursor_position(mx * sf, my * sf);

                                // Drain any custom passes queued via BlincContextState
                                // (e.g. SceneKit3D registering a GridPass from a closure)
                                {
                                    let ctx_state = blinc_core::BlincContextState::get();
                                    for pass in ctx_state.drain_custom_passes() {
                                        if let Ok(typed) = pass.downcast::<Box<dyn blinc_gpu::custom_pass::CustomRenderPass>>() {
                                            blinc_app.context().register_custom_pass(*typed);
                                        }
                                    }
                                }

                                // Clear alpha tracks per-window transparency so a
                                // mix of opaque and transparent windows can share
                                // the same BlincApp.
                                blinc_app.set_clear_alpha(if ws.transparent { 0.0 } else { 1.0 });

                                // Compositor fast-path eligibility.
                                //
                                // Cache invalidation is now wired through
                                // `Event::Input` (any input event clears
                                // the cache), so scroll / hover / focus /
                                // IME changes can't smuggle stale state
                                // into the fast path — the very next
                                // paint after any input takes the full
                                // walker route and repopulates the cache.
                                // The fast path engages between input
                                // events, which is exactly when idle
                                // animations (spinner / skeleton /
                                // progress fill) need it.
                                //
                                // Per-frame gate stays conservative — any
                                // frame-loop signal that the fast path's
                                // delta-apply can't reproduce trips the
                                // full path:
                                //  - structural change (`did_rebuild`,
                                //    `ws.needs_relayout`)
                                //  - CSS animation / transition tick
                                //    (`css_active`)
                                //  - scroll physics decay
                                //    (`scroll_animating`)
                                //  - cold start (no cache yet,
                                //    `last_paint_time_ms == 0`)
                                // Per-frame gate. The walker now records
                                // `composite_bindings` for every painted
                                // motion-bound node (animating or not),
                                // so a newly-active spring on a visible
                                // node finds its target entry already in
                                // the map on the very next fast-path
                                // frame. Off-screen nodes are still
                                // culled and unrecorded — when they
                                // scroll into view, the input event
                                // invalidates the cache and the next
                                // full paint records them.
                                // Canvas presence no longer bails the fast
                                // path — `BlincApp::redraw_canvases`
                                // re-invokes each recorded canvas's
                                // `render_fn` into a scratch context and
                                // splices the fresh primitives into the
                                // cached batch, so the surrounding tree
                                // stays cached and the walker doesn't run.
                                // See the split-paint flow in
                                // `render_tree_with_motion_opt`.
                                // CSS-only animation frames take the fast path
                                // when every playing animation / transition is
                                // composite-promotable (opacity / 2D translate /
                                // 2D scale): the walker doesn't need to run
                                // because the layer textures were rasterized on
                                // the last full paint and `composite_frame`
                                // already calls `composite_css_layers_overlay`
                                // to blit them with the current animated
                                // dest_pos / dest_size / opacity. Non-promotable
                                // CSS work (colour / layout / 3D / rotate-z)
                                // still trips the slow path through `css_active`.
                                let css_blocks_fast = css_active && !css_only_composite_promotable;
                                // Visual / FLIP / layout animations resize / reposition
                                // bounds each frame — the cached primitive batch
                                // can't reflect the new clip rect, so we must
                                // take the walker route while any of them are
                                // mid-flight. Pre-fix, the tree-view expand
                                // animation froze at whatever partial state the
                                // first slow-path frame painted into the cache;
                                // subsequent fast-path frames just blitted that
                                // stale partial state until an input event
                                // invalidated the cache. Same posture as
                                // `scroll_animating` above.
                                let bounds_anim_active = ws.render_tree.as_ref().is_some_and(|t| {
                                    t.has_active_visual_animations()
                                        || t.has_active_layout_animations()
                                        || t.has_active_flip_animations()
                                });
                                // Block the fast path while an overlay /
                                // toast is structurally changing or animating.
                                //
                                //  - `is_dirty()`: a new entry was just pushed
                                //    (or the layer was rebuilt). The cached
                                //    bg-batch from the previous frame doesn't
                                //    include the new layer content yet, so we
                                //    need a full slow-walker pass to repopulate
                                //    the static cache.
                                //  - `has_animating_overlays()` / `has_animating()`:
                                //    motion springs are still ticking. Animation
                                //    state is per-frame; the cached batch would
                                //    freeze at the moment the spring last
                                //    crossed a frame boundary, so we walk every
                                //    frame to re-emit primitives at the current
                                //    spring state.
                                //
                                // Settled, idle overlays do NOT block fast
                                // path. Pre-fix this branch also OR-d
                                // `has_visible_overlays()` / `!t.is_empty()`,
                                // which forced slow walker every frame for the
                                // entire duration any overlay was open. Combined
                                // with the cursor-blink continuous_redraw an
                                // auto-focused input emits, that's a slow-walker
                                // pass per vsync from the moment a popover with
                                // an input appears until it closes — visible as
                                // "canvas re-paints in a rapid zoom-flicker" on
                                // a canvas-backed host (node_editor_demo) because
                                // each pass re-bakes the static cache and re-
                                // collects the canvas overlay back-to-back.
                                // `is_dirty()` + `has_animating_overlays()` cover
                                // the real correctness needs (cache-stale-after-
                                // push, motion-needs-re-emit); the visible-only
                                // case is now allowed to fast-path.
                                let new_overlay_active = {
                                    use blinc_layout::overlay_state::{
                                        overlay_stack, toast_tray,
                                    };
                                    let s = overlay_stack()
                                        .lock()
                                        .map(|s| {
                                            s.is_dirty()
                                                || s.has_animating_overlays()
                                        })
                                        .unwrap_or(false);
                                    let t = toast_tray()
                                        .lock()
                                        .map(|t| t.is_dirty() || t.has_animating())
                                        .unwrap_or(false);
                                    s || t
                                };
                                let try_fast_paint = !did_rebuild
                                    && !ws.needs_relayout
                                    && !css_blocks_fast
                                    && !scroll_animating
                                    && !bounds_anim_active
                                    && !new_overlay_active
                                    && ws.last_paint_time_ms != 0
                                    && blinc_app.has_render_cache();

                                // Render with motion animations
                                // Use physical pixel dimensions for the render surface
                                let result = blinc_app.render_tree_with_motion_opt(
                                    tree,
                                    rs,
                                    &view,
                                    Some(render_tex),
                                    windowed_ctx.physical_width as u32,
                                    windowed_ctx.physical_height as u32,
                                    try_fast_paint,
                                );
                                if let Err(e) = result {
                                    tracing::error!("Render error: {}", e);
                                }

                                blinc_app.clear_blend_target();
                            }

                            // =========================================================
                            // PHASE 4b: Overlay state management (overlays now in main tree)
                            // Overlays are composed into the main tree via build_overlay_layer()
                            // so they share the same event routing and incremental update path.
                            // =========================================================

                            // Animation dirtiness is render-only and can be consumed after paint.
                            // Structural dirty flags must be preserved for the pre-render subtree
                            // rebuild phase: canvas-hosted immediate-mode widgets can call
                            // `.show()` while `render_tree_with_motion_opt` is painting, after the
                            // rebuild phase has already run for this frame. Clearing content dirty
                            // here would strand that new overlay in its manager without ever
                            // mounting it into the render tree.
                            let _animation_dirty = windowed_ctx.overlay_manager.take_animation_dirty();

                            // Track overlay visibility for triggering rebuilds
                            let has_visible_overlays = windowed_ctx.overlay_manager.has_visible_overlays();
                            windowed_ctx.had_visible_overlays = has_visible_overlays;

                            // Tell winit we're about to present. On Wayland this
                            // arms the `wl_surface::frame()` callback that gates
                            // the next `RedrawRequested` on the compositor's
                            // `wl_callback::Done`. Without this, winit emits the
                            // next redraw immediately and our `get_current_texture()`
                            // blocks for ~1 s per acquire while the compositor
                            // hasn't released a swapchain image — the documented
                            // pathology behind the Linux "frozen UI" reports.
                            // No-op on other platforms.
                            //
                            // When the experimental `wayland-frame-gate` feature
                            // is active AND the hand-rolled gate constructed
                            // successfully, WE own the frame-callback registration
                            // — skip winit's gating so the two queues don't both
                            // arm a `wl_surface::frame()` on each present.
                            #[cfg(all(feature = "wayland-frame-gate", target_os = "linux"))]
                            let _gate_active = ws.wayland_gate.is_some();
                            #[cfg(not(all(feature = "wayland-frame-gate", target_os = "linux")))]
                            let _gate_active = false;
                            if !_gate_active {
                                window.pre_present_notify();
                            }
                            // Arm BEFORE present. `frame.present()`
                            // calls wgpu's internal `wl_surface::commit()`;
                            // we need our `wl_surface::frame()` request to
                            // hit the wire first so the callback bundles
                            // with this commit rather than the next one.
                            // The "after a few seconds it freezes" symptom
                            // was caused by arming after present: callbacks
                            // were buffered for a next commit that never
                            // arrived when the render loop went quiet.
                            #[cfg(all(feature = "wayland-frame-gate", target_os = "linux"))]
                            if let Some(gate) = ws.wayland_gate.as_ref() {
                                gate.arm_before_present();
                            }
                            frame.present();
                            // Wayland keep-alive self-drive: request the next frame
                            // so the swapchain keeps cycling while the scene is
                            // static. `pre_present_notify` (called above) armed the
                            // compositor's frame callback, so winit paces this to
                            // vsync (Done) — steady ~7% CPU, not a busy spin.
                            // `keep_alive` is false off Wayland, so this is a no-op
                            // on X11 / macOS / Windows.
                            if keep_alive {
                                window.request_redraw();
                            }
                            // Frame-gate self-drive: keep presents flowing at the
                            // compositor's Done cadence so the swapchain never goes
                            // idle (see the keep-alive note above). The gate's Done
                            // wait throttles this to vsync, so it is not a spin.
                            #[cfg(all(feature = "wayland-frame-gate", target_os = "linux"))]
                            if _gate_active {
                                window.request_redraw();
                            }
                            t_phase4 = phase4_start.elapsed();

                            // =========================================================
                            // PHASE 5: Request next frame if animations are active
                            // This ensures smooth animation without waiting for events
                            // =========================================================
                            let phase5_start = std::time::Instant::now();

                            // Check if background animation thread signaled that redraw is needed
                            // The background thread runs at 120fps and sets this flag when
                            // there are active animations (springs, keyframes, timelines)
                            let scheduler = windowed_ctx.animations.lock().unwrap();
                            let needs_animation_redraw_raw = scheduler.take_needs_redraw();
                            dirty_spring_count = scheduler.dirty_spring_count();
                            drop(scheduler); // Release lock before request_redraw

                            // Check if stateful elements have active spring animations
                            // and re-run their callbacks to get updated animation values.
                            //
                            // CRUCIAL: drive this off the *raw* scheduler signal, not
                            // the visibility-gated one below. `check_stateful_animations`
                            // is what unregisters settled statefuls. If we skip it
                            // when the gate suppresses rendering, the registry never
                            // shrinks — which makes `has_animating_statefuls()` return
                            // a sticky `true`, which then keeps re-asserting the gate
                            // (because we OR it into `visible_anim`), and we never
                            // recover. The bookkeeping has to run on every animation
                            // tick regardless of whether we'll actually paint.
                            if needs_animation_redraw_raw && blinc_layout::has_animating_statefuls() {
                                blinc_layout::check_stateful_animations();
                            }

                            // Gate the animation signal on visibility. The scheduler
                            // ticks unconditionally for any active spring / keyframe /
                            // timeline — including ones tied to off-screen nodes. The
                            // paint walker sets `visible_anim_active` when it paints
                            // a node that drives a per-frame redraw (Canvas, motion
                            // bindings, active motion state).
                            //
                            // Stateful-driven animations (springs that mutate state and
                            // trigger rebuilds — e.g. cn_demo's spinner / accordion's
                            // `animated_progress`) bypass the per-node motion-binding
                            // check, so we additionally OR in the global "any animating
                            // stateful?" signal — but **filtered to those whose node
                            // was painted this frame**. The paint walker records every
                            // node it actually rendered into `painted_node_ids`;
                            // `has_visible_animating_statefuls` intersects that with
                            // the registry. Without this intersection a spinner
                            // scrolled off-screen pinned the redraw chain forever
                            // (cn_demo regression). Brand-new Statefuls whose node
                            // hasn't been bound yet are conservatively counted as
                            // visible by the predicate, so the very first frame still
                            // renders.
                            let visible_anim_paint = ws.render_tree
                                .as_ref()
                                .is_none_or(|t| t.visible_anim_active());
                            let visible_anim_stateful = ws.render_tree
                                .as_ref()
                                .is_some_and(|t| {
                                    blinc_layout::has_visible_animating_statefuls(
                                        &t.painted_node_ids(),
                                    )
                                });
                            let visible_anim = visible_anim_paint || visible_anim_stateful;
                            // Mirror the flag to the scheduler-side atomic so the
                            // wake callback (bg thread) skips waking the main
                            // thread when the only active animations are off-screen.
                            visible_anim_for_wake.store(visible_anim, Ordering::Release);
                            let needs_animation_redraw = needs_animation_redraw_raw && visible_anim;

                            // Cursor blink: a focused text input wants the
                            // cursor visibility flipped every ~400 ms. A
                            // sticky `has_focused_text_input()` read is the
                            // right signal — the previous consume-on-read
                            // flag forced a re-arm every frame which pinned
                            // CPU at vsync. The cursor-only redraw branch
                            // below paces this signal via `wake_at` so we
                            // only paint at blink interval, not vsync.
                            let needs_cursor_redraw = blinc_layout::widgets::has_focused_text_input();
                            let _ = blinc_layout::widgets::take_needs_continuous_redraw();

                            // Check if motion animations are active (enter/exit
                            // animations). Painted-gated for the same reason as
                            // `needs_vsync`: this term feeds `any_redraw_signal`,
                            // so an off-screen motion re-arms the redraw chain
                            // every frame and the loop never parks.
                            let needs_motion_redraw = match (&ws.render_state, &ws.render_tree) {
                                (Some(rs), Some(tree)) => {
                                    rs.has_active_motions_visible(&tree.painted_node_ids())
                                }
                                _ => false,
                            };

                            // Check if overlays changed (modal opened/closed, toast
                            // appeared, etc.) or are mid-animation. The presence of a
                            // *visible* overlay is NOT a redraw signal — a static
                            // popover should sit quiet between input events. Use
                            // `has_animating_overlays` (enter/exit motion) instead;
                            // any overlay-internal redraws (hover css, contained
                            // motion) flow through their own signals below.
                            let needs_overlay_redraw = {
                                let mgr = windowed_ctx.overlay_manager.lock().unwrap();
                                mgr.is_dirty()
                                    || mgr.take_animation_dirty()
                                    || mgr.has_animating_overlays()
                            };
                            // Phase 3 transition: same gate for the new stack +
                            // tray. Either dirtying source schedules the next paint.
                            let needs_overlay_stack_redraw = {
                                use blinc_layout::overlay_state::{overlay_stack, toast_tray};
                                let s_signal = overlay_stack()
                                    .lock()
                                    .map(|s| {
                                        s.is_dirty()
                                            || s.take_animation_dirty()
                                            || s.has_animating_overlays()
                                    })
                                    .unwrap_or(false);
                                let t_signal = toast_tray()
                                    .lock()
                                    .map(|t| t.is_dirty() || t.has_animating())
                                    .unwrap_or(false);
                                s_signal || t_signal
                            };
                            let needs_overlay_redraw = needs_overlay_redraw || needs_overlay_stack_redraw;

                            // Check if CSS animations/transitions/FLIP/visual-animations need
                            // continued redraws. Both `flip_animations` (older `animate_layout`)
                            // and `visual_animations` (newer `animate_bounds`, used by the cn
                            // accordion among others) drive bounds animation but live in
                            // separate maps. Missing the visual_animations check here was the
                            // cause of accordion jank: once the scheduler stopped waking the
                            // main thread on every tick, the only thing keeping the chain
                            // alive during an accordion expand was *no* signal at all, so the
                            // animation only progressed when some other event (scroll, hover)
                            // happened to fire `frame_dirty`.
                            // Visibility-gated CSS-redraw signal. Same shape as the
                            // four-way OR above used to be, but every term is now
                            // intersected with `painted_node_ids`. Off-screen
                            // `infinite` keyframes (the styling_demo had ~25 of
                            // them, pinning ~73 % CPU at idle even with the cursor
                            // parked) no longer keep the chain alive — they
                            // continue ticking so progress stays in sync, but the
                            // signal that drives request_redraw stops.
                            //
                            // The unfiltered `css_active`/`has_active_*` calls
                            // are still made above (we want to advance every
                            // animation regardless) — what changed is the GATE
                            // that triggers another frame.
                            let _ = css_active; // keep tick side-effects, drop signal
                            let css_needs_redraw = ws.render_tree.as_ref().is_some_and(|t| {
                                // CSS store keyed by `StableNodeId` (Phase 5);
                                // FLIP / visual-animation visibility checks
                                // still take `LayoutNodeId`, so we keep both.
                                let painted_stable = t.painted_stable_ids();
                                let painted = t.painted_node_ids();
                                let store = t.css_anim_store();
                                let store_guard = store.lock().unwrap();
                                let store_visible =
                                    store_guard.has_visible_active(&painted_stable);
                                drop(store_guard);
                                store_visible
                                    || t.css_has_visible_transitions(&painted)
                                    || t.has_active_visible_flip_animations(&painted)
                                    || t.has_active_visible_visual_animations(&painted)
                            });

                            // Check if pointer query elements need continuous redraws
                            let pointer_query_active = !windowed_ctx.pointer_query.is_empty();

                            // @flow shaders using time/animation builtins need continuous redraws
                            let flow_needs_redraw = blinc_app.has_active_flows();

                            // Image load-time fade-ins tick a per-image
                            // `fade_factor` based on elapsed wall-time;
                            // without firing redraw each frame the fade
                            // sits frozen until the user wiggles the
                            // mouse. Read the dedicated flag set in
                            // `RenderContext` so flows-flag overwrites
                            // can't clobber the signal mid-dispatch.
                            let image_fade_needs_redraw =
                                blinc_app.context().has_pending_image_fade();

                            // Log which signal(s) kept the redraw chain alive at trace
                            // level. Run with `RUST_LOG=blinc_app=trace` to see what's
                            // pinning a stuck-busy frame loop. Writes nothing in normal
                            // builds — the format args aren't even evaluated when the
                            // trace target is disabled.
                            tracing::trace!(
                                target: "blinc_app::redraw_signals",
                                animation = needs_animation_redraw,
                                cursor = needs_cursor_redraw,
                                motion = needs_motion_redraw,
                                scroll = scroll_animating,
                                overlay = needs_overlay_redraw,
                                theme = theme_animating,
                                css = css_needs_redraw,
                                pointer_query = pointer_query_active,
                                flow = flow_needs_redraw,
                                image_fade = image_fade_needs_redraw,
                                "redraw chain"
                            );

                            // External animation tick — set by code that
                            // drives its own per-frame work outside the
                            // scheduler / motion / stateful registries
                            // (canvas-closure animations like the node
                            // editor's edge-state shimmer). Take-and-clear
                            // so the source closure must re-request next
                            // frame; that way the chain stops cleanly the
                            // moment the source goes quiet.
                            let external_anim_tick = blinc_layout::take_animation_tick_request();

                            // cn dropdown/menu/select/popover fade in via CSS
                            // @keyframes, not the motion FSM, so
                            // `needs_overlay_redraw` (has_animating_overlays) never
                            // covers them; and the composite-promoted overlay
                            // container isn't reliably in `painted_node_ids`, so the
                            // painted-gated `css_needs_redraw` misses it too. Result:
                            // once the push frame's dirty flags clear, no term keeps
                            // the chain alive and the fade freezes until input.
                            // Mirror the image_fade pattern: a dedicated,
                            // non-painted-gated signal scoped to "an overlay is on
                            // screen AND a CSS anim/transition is playing". Bounded —
                            // `css_has_active()` clears when `is_playing` goes false
                            // and `has_visible_overlays()` clears when the overlay
                            // closes — so off-screen infinite keyframes elsewhere
                            // still die (the reason `css_needs_redraw` is painted-gated).
                            let overlay_css_needs_redraw = {
                                // Cover BOTH overlay systems: cn menubar/dropdown
                                // may live on the legacy `overlay_manager`, popover/
                                // select on the newer `overlay_stack`.
                                let mgr_present =
                                    windowed_ctx.overlay_manager.has_visible_overlays();
                                let stack_present =
                                    blinc_layout::overlay_state::overlay_stack()
                                        .lock()
                                        .map(|s| s.has_visible_overlays())
                                        .unwrap_or(false);
                                let overlay_present = mgr_present || stack_present;
                                let css_active = ws
                                    .render_tree
                                    .as_ref()
                                    .is_some_and(|t| t.css_has_active());
                                overlay_present && css_active
                            };

                            // CSS-activity edge repaints — one render-cache
                            // invalidation on EITHER transition of
                            // `css_has_active()`:
                            //
                            // * Rising edge (animations just started): the
                            //   overlay's content may have been collected on
                            //   a frame BEFORE `start_all_css_animations` ran
                            //   (the known first-open ordering race), so the
                            //   cached text pool has no composite-patch
                            //   records and would render at full alpha over
                            //   the fading panel. One slow walk re-collects
                            //   with `is_playing == true`: promotion, patch
                            //   records and baselines all captured.
                            //
                            // * Falling edge (animations settled): nothing on
                            //   the fast path ever demotes the promoted
                            //   region — the stale blit + base-alpha text
                            //   pool would persist until the next input
                            //   event. One slow walk demotes and re-emits the
                            //   subtree at its exact final state.
                            {
                                let css_active_now = ws
                                    .render_tree
                                    .as_ref()
                                    .is_some_and(|t| t.css_has_active());
                                if ws.prev_css_active != css_active_now {
                                    blinc_app.invalidate_render_cache_tagged(
                                        "css_activity_edge",
                                    );
                                    frame_dirty.store(true, Ordering::Release);
                                    window.request_redraw();
                                }
                                ws.prev_css_active = css_active_now;
                            }

                            let any_redraw_signal = needs_animation_redraw
                                || external_anim_tick
                                || needs_cursor_redraw
                                || needs_motion_redraw
                                || scroll_animating
                                || needs_overlay_redraw
                                || theme_animating
                                || css_needs_redraw
                                || overlay_css_needs_redraw
                                || pointer_query_active
                                || flow_needs_redraw
                                || image_fade_needs_redraw;
                            if any_redraw_signal {
                                // Cursor-only: a focused text input is the
                                // only redraw signal. Pace at the blink
                                // interval instead of vsync — the cursor
                                // toggles twice per second, painting every
                                // frame burns 15–25% CPU on a non-trivial
                                // UI tree for nothing visible.
                                let cursor_only = needs_cursor_redraw
                                    && !needs_animation_redraw
                                    && !needs_motion_redraw
                                    && !scroll_animating
                                    && !needs_overlay_redraw
                                    && !theme_animating
                                    && !css_needs_redraw
                                    && !overlay_css_needs_redraw
                                    && !pointer_query_active
                                    && !flow_needs_redraw;

                                // Which signal is actually keeping the frame
                                // pacer alive. Every one of these ORs into the
                                // decision to schedule another wake, so exactly
                                // one line here ends the guessing about why an
                                // idle window never parks.
                                {
                                    static T: std::sync::atomic::AtomicUsize =
                                        std::sync::atomic::AtomicUsize::new(0);
                                    if T.fetch_add(1, Ordering::Relaxed) % 120 == 0 {
                                        tracing::debug!(
                                            needs_animation_redraw,
                                            needs_motion_redraw,
                                            scroll_animating,
                                            needs_overlay_redraw,
                                            theme_animating,
                                            css_needs_redraw,
                                            overlay_css_needs_redraw,
                                            pointer_query_active,
                                            flow_needs_redraw,
                                            needs_cursor_redraw,
                                            "frame pacing: which signal is live"
                                        );
                                    }
                                }

                                if cursor_only {
                                    // Cursor blink toggles every ~400 ms
                                    // (see `RenderState::cursor_blink_interval`).
                                    // Schedule the next paint at the next
                                    // toggle, not at vsync.
                                    let delay = std::time::Duration::from_millis(400);
                                    frame_dirty.store(true, Ordering::Release);
                                    wake_proxy_for_pacing.wake_at(delay);
                                } else {
                                    // Read the live cap from the atomic (the
                                    // adaptive FPS adapter may have changed it
                                    // since startup). 0 ⇒ no cap → vsync pace.
                                    let live_cap_chain =
                                        animation_fps_cap_atomic.load(Ordering::Relaxed);
                                    if live_cap_chain > 0 && !keep_alive {
                                        // Capped, non-keep-alive (macOS / X11):
                                        // pace the next Frame at exactly the cap
                                        // interval. Otherwise (`request_redraw`)
                                        // winit on macOS would deliver Frames
                                        // back-to-back at microsecond cadence —
                                        // Phase 4 would skip them via the cap-
                                        // elapsed check but the loop still spun
                                        // 3000+ times per second, pinning CPU at
                                        // 100 % under continuous click + active
                                        // animation. `wake_at` routes through the
                                        // platform shim's timer thread so the next
                                        // Frame arrives at the exact moment Phase 4
                                        // is allowed to render.
                                        let delay = std::time::Duration::from_millis(
                                            1000 / live_cap_chain as u64,
                                        );
                                        frame_dirty.store(true, Ordering::Release);
                                        wake_proxy_for_pacing.wake_at(delay);
                                    } else {
                                        // Wayland keep-alive, or no cap → render at
                                        // vsync via `request_redraw`. Under keep-
                                        // alive this is ALREADY vsync-paced by the
                                        // compositor frame callback (same self-drive
                                        // as after present), so it never spins — and
                                        // it must NOT be a cap-interval `wake_at`
                                        // timer: that timer (e.g. 16 ms) beats
                                        // against the compositor Done cadence
                                        // (~16.67 ms), delivering scroll-decel and
                                        // spinner frames on an uneven 2:3 rhythm =
                                        // the "laggy on Wayland, smooth on Mac"
                                        // judder. `request_redraw` coalesces with
                                        // the keep-alive self-drive onto one vsync
                                        // cadence.
                                        frame_dirty.store(true, Ordering::Release);
                                        window.request_redraw();
                                    }
                                }
                            }
                            t_phase5 = phase5_start.elapsed();
                            let t_total = frame_start.elapsed();
                            // Feed the rendered-frame wall-clock into the
                            // adaptive FPS adapter. Only runs when an
                            // `Adaptive` policy was configured; `Fixed` /
                            // `Refresh` / legacy `animation_fps_cap`
                            // leave `fps_adapter == None`. The adapter
                            // returns the cap it wants going forward; if
                            // it differs from the atomic's current value
                            // we publish the new cap and notify the
                            // scheduler so spring / keyframe / timeline
                            // ticking matches the new paint cadence.
                            if let Some(adapter_arc) = fps_adapter.as_ref() {
                                if let Ok(mut adapter) = adapter_arc.lock() {
                                    let new_cap = adapter.record(t_total.as_micros() as u64);
                                    let prev_cap =
                                        animation_fps_cap_atomic.load(Ordering::Relaxed);
                                    if new_cap != prev_cap {
                                        animation_fps_cap_atomic
                                            .store(new_cap, Ordering::Relaxed);
                                        if let Ok(mut sched) = animations.lock() {
                                            sched.set_target_fps(new_cap);
                                        }
                                    }
                                }
                            }
                            // Per-phase breakdown. Disabled by default;
                            // RUST_LOG=blinc_app::frame_timing=trace surfaces it.
                            // The four phase totals will not exactly sum to
                            // `total` — the small gaps cover surface acquire +
                            // scheduling work that happens between the labelled
                            // sections (e.g. cursor sync, animation policy
                            // decisions). `did_rebuild` + `dirty_springs`
                            // contextualize the timings: if `did_rebuild=false`
                            // and `dirty_springs=1` then any time spent in
                            // phase 4 is paint walker overhead for what should
                            // be a one-binding update — a candidate for the
                            // upcoming fast Phase 4.
                            tracing::trace!(
                                target: "blinc_app::frame_timing",
                                total_us = t_total.as_micros() as u64,
                                p1_rebuild_check_us = t_phase1.as_micros() as u64,
                                p2_rebuild_us = t_phase2.as_micros() as u64,
                                p3_tick_us = t_phase3.as_micros() as u64,
                                p4_render_us = t_phase4.as_micros() as u64,
                                p5_chain_us = t_phase5.as_micros() as u64,
                                did_rebuild,
                                dirty_springs = dirty_spring_count,
                                "frame"
                            );
                        }
                    }

                    _ => {}
                }

                ControlFlow::Continue
            })
            .map_err(|e| BlincError::Platform(e.to_string()))?;

        Ok(())
    }

    /// Placeholder for non-windowed builds
    #[cfg(not(feature = "windowed"))]
    pub fn run<F, E>(_config: WindowConfig, _ui_builder: F) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder + 'static,
    {
        Err(BlincError::Platform(
            "Windowed feature not enabled. Add 'windowed' feature to blinc_app".to_string(),
        ))
    }
}

/// Convert platform mouse button to layout mouse button
#[cfg(all(feature = "windowed", not(target_os = "android")))]
fn convert_mouse_button(button: blinc_platform::MouseButton) -> MouseButton {
    match button {
        blinc_platform::MouseButton::Left => MouseButton::Left,
        blinc_platform::MouseButton::Right => MouseButton::Right,
        blinc_platform::MouseButton::Middle => MouseButton::Middle,
        blinc_platform::MouseButton::Back => MouseButton::Back,
        blinc_platform::MouseButton::Forward => MouseButton::Forward,
        blinc_platform::MouseButton::Other(n) => MouseButton::Other(n),
    }
}

/// Convert layout cursor style to platform cursor
#[cfg(all(feature = "windowed", not(target_os = "android")))]
fn convert_cursor_style(cursor: CursorStyle) -> blinc_platform::Cursor {
    match cursor {
        CursorStyle::Default => blinc_platform::Cursor::Default,
        CursorStyle::Pointer => blinc_platform::Cursor::Pointer,
        CursorStyle::Text => blinc_platform::Cursor::Text,
        CursorStyle::Crosshair => blinc_platform::Cursor::Crosshair,
        CursorStyle::Move => blinc_platform::Cursor::Move,
        CursorStyle::NotAllowed => blinc_platform::Cursor::NotAllowed,
        CursorStyle::ResizeNS => blinc_platform::Cursor::ResizeNS,
        CursorStyle::ResizeEW => blinc_platform::Cursor::ResizeEW,
        CursorStyle::ResizeNESW => blinc_platform::Cursor::ResizeNESW,
        CursorStyle::ResizeNWSE => blinc_platform::Cursor::ResizeNWSE,
        CursorStyle::Grab => blinc_platform::Cursor::Grab,
        CursorStyle::Grabbing => blinc_platform::Cursor::Grabbing,
        CursorStyle::Wait => blinc_platform::Cursor::Wait,
        CursorStyle::Progress => blinc_platform::Cursor::Progress,
        CursorStyle::None => blinc_platform::Cursor::None,
    }
}

/// Convenience function to run a windowed ws.app with default configuration
#[cfg(all(feature = "windowed", not(target_os = "android")))]
pub fn run_windowed<F, E>(ui_builder: F) -> Result<()>
where
    F: FnMut(&mut WindowedContext) -> E + 'static,
    E: ElementBuilder + 'static,
{
    WindowedApp::run(WindowConfig::default(), ui_builder)
}

/// Convenience function to run a windowed ws.app with a title
#[cfg(all(feature = "windowed", not(target_os = "android")))]
pub fn run_windowed_with_title<F, E>(title: &str, ui_builder: F) -> Result<()>
where
    F: FnMut(&mut WindowedContext) -> E + 'static,
    E: ElementBuilder + 'static,
{
    let config = WindowConfig {
        title: title.to_string(),
        ..Default::default()
    };
    WindowedApp::run(config, ui_builder)
}

/// Invoke the user's primary-window UI builder closure, optionally
/// routing the call through `subsecond::call` so changes to the
/// closure body can be hot-patched in development. Without the
/// `hot-reload` feature this is a direct call and gets inlined away.
///
/// Subsecond itself is gated on `debug_assertions` upstream, so even
/// with the `hot-reload` feature on a release build still pays no
/// runtime cost.
#[cfg(all(feature = "windowed", not(target_os = "android")))]
#[inline]
fn invoke_ui_builder<F, E>(ui_builder: &mut F, ctx: &mut WindowedContext) -> E
where
    F: FnMut(&mut WindowedContext) -> E,
    E: ElementBuilder,
{
    #[cfg(feature = "hot-reload")]
    {
        // `subsecond::call` takes a `FnOnce() -> O`. The captured
        // `&mut` borrows live only for this call, so the closure
        // satisfies `FnOnce` regardless of `F`'s `FnMut` bound.
        subsecond::call(move || ui_builder(ctx))
    }
    #[cfg(not(feature = "hot-reload"))]
    {
        ui_builder(ctx)
    }
}

/// Secondary-window variant of [`invoke_ui_builder`]. Secondary
/// windows store their builder as a boxed `WindowBuilder` returning
/// `Div` directly (rather than the generic `E: ElementBuilder` of
/// the primary), so they need a separate helper.
#[cfg(all(feature = "windowed", not(target_os = "android")))]
#[inline]
fn invoke_window_builder(builder: &mut WindowBuilder, ctx: &mut WindowedContext) -> Div {
    #[cfg(feature = "hot-reload")]
    {
        subsecond::call(move || builder(ctx))
    }
    #[cfg(not(feature = "hot-reload"))]
    {
        builder(ctx)
    }
}
