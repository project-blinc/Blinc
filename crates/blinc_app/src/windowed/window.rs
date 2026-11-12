//! Per-window state and present-mode selection.

use crate::RenderTree;
use crate::app::BlincApp;
use crate::windowed::{WindowBuilder, WindowedContext};
use blinc_layout::prelude::*;
use std::sync::{Arc, Mutex};

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
    /// The scope each build runs in; a fresh tree disposes the builds before it.
    pub build_scope: blinc_core::owner::BuildScope,
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
    /// Keep presenting at vsync while the scene is static. Set at surface
    /// creation from the display backend; override with `BLINC_KEEP_ALIVE=1/0`.
    pub wayland_keep_alive: bool,
    /// Whether any CSS animation/transition was active last frame.
    /// Falling edge triggers a one-shot render-cache invalidation so
    /// composite-promoted subtrees demote and re-emit at their exact
    /// final state (the fast path never demotes on its own).
    pub prev_css_active: bool,
    /// When the last frame ended with no further frame asked for. Once the window has stayed
    /// parked for [`IDLE_RELEASE`], the app frees its between-frame GPU memory.
    pub parked_at: Option<std::time::Instant>,
}

/// How long a window stays parked before its app frees the GPU memory kept between frames.
pub const IDLE_RELEASE: std::time::Duration = std::time::Duration::from_secs(2);

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
            build_scope: blinc_core::owner::BuildScope::new(),
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
            parked_at: None,
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
pub(super) fn preferred_present_mode(
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
