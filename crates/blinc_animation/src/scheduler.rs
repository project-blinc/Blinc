//! Animation scheduler
//!
//! Manages all active animations and updates them each frame.
//! Animations are implicitly registered when created through wrapper types:
//! - `AnimatedValue` - Spring-based physics animations
//! - `AnimatedKeyframe` - Keyframe-based timed animations
//! - `AnimatedTimeline` - Timeline orchestration of multiple animations

use crate::easing::Easing;
use crate::keyframe::{Keyframe, KeyframeAnimation};
use crate::spring::{Spring, SpringConfig};
use crate::timeline::Timeline;
use blinc_core::AnimationAccess;
use slotmap::{SlotMap, new_key_type};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};
use std::thread::JoinHandle;
// `web_time::Instant` is a drop-in replacement for `std::time::Instant`.
// On native targets it re-exports the std type with zero overhead. On
// `wasm32-unknown-unknown` it routes through `performance.now()`,
// which is the only way to get a monotonic clock in a browser — the
// std impl panics with "time not implemented on this platform" the
// moment `Instant::now()` is actually called.
use web_time::Instant;
// `Duration` and `thread` are only used by the desktop background-thread
// loop; `start_raf()` lets `requestAnimationFrame` pace itself to the
// display, and the `thread_handle` field on wasm32 is just an inert
// `Option<JoinHandle>` that's always `None`.
#[cfg(not(target_arch = "wasm32"))]
use std::thread;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

// ============================================================================
// Global Animation Scheduler State
// ============================================================================

/// Global scheduler handle for access from anywhere in the application
static GLOBAL_SCHEDULER: OnceLock<SchedulerHandle> = OnceLock::new();

/// Set the global animation scheduler handle
///
/// This should be called once at app startup after creating the AnimationScheduler.
/// Typically called from `WindowedApp::run()` after the scheduler is configured.
///
/// # Panics
///
/// Panics if called more than once.
pub fn set_global_scheduler(handle: SchedulerHandle) {
    if GLOBAL_SCHEDULER.set(handle).is_err() {
        panic!("set_global_scheduler() called more than once");
    }
}

/// Get the global animation scheduler handle
///
/// Returns the scheduler handle for creating animated values, keyframes, and timelines.
/// This enables components to create animations without needing explicit context passing.
///
/// # Panics
///
/// Panics if `set_global_scheduler()` has not been called.
///
/// # Example
///
/// ```ignore
/// use blinc_animation::{get_scheduler, AnimatedValue, SpringConfig};
///
/// let handle = get_scheduler();
/// let mut opacity = AnimatedValue::new(handle.clone(), 1.0, SpringConfig::stiff());
/// opacity.set_target(0.5);
/// ```
pub fn get_scheduler() -> SchedulerHandle {
    GLOBAL_SCHEDULER
        .get()
        .expect("Animation scheduler not initialized. Call set_global_scheduler() at app startup.")
        .clone()
}

/// Try to get the global scheduler (returns None if not initialized)
pub fn try_get_scheduler() -> Option<SchedulerHandle> {
    GLOBAL_SCHEDULER.get().cloned()
}

/// Check if the global scheduler has been initialized
pub fn is_scheduler_initialized() -> bool {
    GLOBAL_SCHEDULER.get().is_some()
}

new_key_type! {
    /// Handle to a registered spring animation
    pub struct SpringId;
    /// Handle to a registered keyframe animation
    pub struct KeyframeId;
    /// Handle to a registered timeline
    pub struct TimelineId;
}

impl SpringId {
    /// Convert to raw u64 for atomic storage
    ///
    /// Use with `from_raw()` for lock-free animation ID passing.
    pub fn to_raw(self) -> u64 {
        self.0.as_ffi()
    }

    /// Reconstruct from raw u64
    ///
    /// # Safety
    /// The raw value must have been created by `to_raw()` on a valid SpringId.
    pub fn from_raw(raw: u64) -> Self {
        SpringId::from(slotmap::KeyData::from_ffi(raw))
    }
}

/// Tick callback type - called each frame with delta time in seconds
pub type TickCallback = Arc<Mutex<dyn FnMut(f32) + Send + Sync>>;

new_key_type! {
    /// Handle to a registered tick callback
    pub struct TickCallbackId;
}

impl TickCallbackId {
    /// Convert to raw u64 for storage
    pub fn to_raw(self) -> u64 {
        self.0.as_ffi()
    }

    /// Reconstruct from raw u64
    pub fn from_raw(raw: u64) -> Self {
        TickCallbackId::from(slotmap::KeyData::from_ffi(raw))
    }
}

/// Internal state of the animation scheduler
struct SchedulerInner {
    springs: SlotMap<SpringId, Spring>,
    keyframes: SlotMap<KeyframeId, KeyframeAnimation>,
    timelines: SlotMap<TimelineId, Timeline>,
    tick_callbacks: SlotMap<TickCallbackId, TickCallback>,
    last_frame: Instant,
    target_fps: u32,
    /// EMA-smoothed `dt` used for spring / keyframe / timeline
    /// integration. `None` until the second tick.
    ///
    /// The bg thread targets `target_fps` (default = display refresh)
    /// via `wait_timeout`, but `wait_timeout` on Linux futex /
    /// Wayland / X11 has more wake-up jitter than macOS CFRunLoop —
    /// a steady "every 16.67 ms" target can land at 14 ms / 22 ms /
    /// 16 ms / 14 ms intervals under compositor load. Spring physics
    /// stays mathematically correct under raw dt (RK4 substepping at
    /// 8 ms keeps the integrator stable), but the *visual* sampling
    /// is uneven — motion-bound elements appear to vibrate even
    /// though the underlying trajectory is smooth. EMA blending
    /// damps single-tick outliers while staying within ~1 % of true
    /// elapsed wall time over the long run (verified in
    /// `dt_smoothing_tracks_wall_clock`).
    smoothed_dt: Option<f32>,
    /// Springs whose `value()` moved by ≥ `DIRTY_EPSILON` during the
    /// most recent tick, or whose target was just changed. Drained by
    /// the windowed runner each frame via
    /// [`SchedulerHandle::take_dirty_springs`] — the eventual consumer
    /// is the animation-only fast Phase 4 path, which patches the
    /// touched nodes' `GpuPrimitive` transform / opacity in place and
    /// skips the full paint walker.
    ///
    /// Insert sites:
    /// - `tick` / `tick_frame_inner` after `spring.step` if the value
    ///   actually moved.
    /// - `SchedulerHandle::set_spring_target` (so the very first
    ///   frame after `set_target` is treated as dirty even if the
    ///   subsequent step happens to be within epsilon).
    /// - `AnimatedValue::set_immediate` when the value really changed.
    /// - `register_spring` (a brand-new spring is implicitly dirty —
    ///   its initial value just appeared and downstream consumers
    ///   haven't seen it).
    ///
    /// Lifetime: drained whenever a consumer asks for it. Settled
    /// springs that aren't touched stay out of the set, so an
    /// off-screen spinner that's already at rest costs nothing.
    dirty_springs: std::collections::HashSet<SpringId>,
}

/// Minimum value delta that counts as "this spring moved this tick".
/// Below this we consider the spring effectively settled for the
/// purposes of triggering a fast-path repaint. Matches the
/// `EPSILON` used inside `Spring::is_settled` so the two stay in
/// sync — a spring that's already "settled enough to skip the next
/// integration step" shouldn't be dirtying GPU buffers either.
const DIRTY_EPSILON: f32 = 0.01;

/// Maximum dt (seconds) a single tick will advance animations by.
/// When the time between ticks exceeds this — first-frame shader
/// pipeline compile, GC pause, background-thread oversleep,
/// debugger stop — the excess is dropped. Without the cap, a slow
/// first paint substeps the spring through hundreds of iterations
/// in one tick and the animation teleports to its target; with
/// the cap, the spring pauses-and-resumes cleanly instead of
/// fast-forwarding through wall-clock time the user didn't see.
///
/// 32 ms ≈ 2 vsync intervals at 60 Hz. Tight enough that natural
/// frame jitter never hits it, loose enough that "skip a frame"
/// pacing (`animation_fps_cap = 30`) doesn't truncate normal
/// integration.
const MAX_TICK_DT: f32 = 0.032;

/// EMA blend factor for `dt` smoothing. `smoothed = α·raw + (1-α)·prev`.
///
/// `α = 0.6` damps a single 25 ms outlier at 60 Hz target (16.67 ms) to
/// ~21 ms on the spike frame and ~18 ms / ~17 ms on the two ticks after,
/// instead of letting the raw 25 ms reach the spring as-is. Total
/// integrated time tracks wall clock within ~1 % over the long run.
///
/// Lower α (more smoothing) under-shoots true elapsed time more
/// aggressively, which makes long animations appear to drift behind
/// wall clock; higher α (less smoothing) lets more compositor jitter
/// through. 0.6 is the empirical compromise.
const DT_SMOOTH_ALPHA: f32 = 0.6;

/// Apply EMA smoothing to a raw inter-tick `dt`, updating `prev` in
/// place. Returns the smoothed value to use for stepping.
///
/// First call after `prev = None` passes the raw value through (no
/// historical sample to blend with) and primes `prev`.
fn smooth_dt(raw_dt: f32, prev: &mut Option<f32>) -> f32 {
    let smoothed = match *prev {
        Some(p) => DT_SMOOTH_ALPHA * raw_dt + (1.0 - DT_SMOOTH_ALPHA) * p,
        None => raw_dt,
    };
    *prev = Some(smoothed);
    smoothed
}

/// Callback type for waking up the main thread from the animation thread.
///
/// This is called when there are active animations that need to be rendered.
/// The callback should wake up the event loop (e.g., via EventLoopProxy).
///
/// On native targets the callback is invoked from the scheduler's background
/// thread, so it must be `Send + Sync`. On `wasm32-unknown-unknown` the rAF
/// driver fires the callback synchronously on the main browser thread, so the
/// `Send + Sync` bound is dropped — the web runner needs to capture an
/// `Rc<RefCell<WebApp>>` (which is `!Send`) to render a frame.
#[cfg(not(target_arch = "wasm32"))]
pub type WakeCallback = Arc<dyn Fn() + Send + Sync>;
#[cfg(target_arch = "wasm32")]
pub type WakeCallback = Arc<dyn Fn()>;

/// The animation scheduler that ticks all active animations
///
/// This is typically held by the application context and shared via `SchedulerHandle`.
/// Animations register themselves implicitly when created.
///
/// # Background Thread Mode
///
/// The scheduler can run on its own background thread via `start_background()`.
/// This ensures animations continue even when the window loses focus.
///
/// ```ignore
/// let scheduler = AnimationScheduler::new();
/// scheduler.start_background(); // Runs at 120fps in background thread
/// ```
pub struct AnimationScheduler {
    inner: Arc<Mutex<SchedulerInner>>,
    /// Stop signal for background thread
    stop_flag: Arc<AtomicBool>,
    /// Flag set by background thread when animations need redraw
    /// The main thread should check and clear this to request window redraws
    needs_redraw: Arc<AtomicBool>,
    /// Flag to request continuous redraws (e.g., for cursor blink)
    /// When set, the background thread will keep signaling redraws
    continuous_redraw: Arc<AtomicBool>,
    /// External-animation signal — set by drivers that paint per
    /// frame via direct time reads (node-editor's running-edge
    /// pulses, canvas-kit's port pulses, anything that calls
    /// `blinc_layout::request_animation_tick`). Read by the bg
    /// thread's cadence decision: when true, treats the system as
    /// active so the `wants_continuous`-only half-rate downscale
    /// doesn't fire. Drivers `set_external_anim_active(true)` while
    /// they want frames and `(false)` when they settle. Decoupled
    /// from the per-frame request_animation_tick atomic in
    /// `blinc_layout::stateful` because that one is take-and-clear
    /// (single-frame scope) — drivers that want sustained cadence
    /// need a sticky flag the scheduler can sample without
    /// consuming it.
    external_anim_active: Arc<AtomicBool>,
    /// Background thread handle (if running)
    thread_handle: Option<JoinHandle<()>>,
    /// Optional callback to wake up the main thread
    wake_callback: Option<WakeCallback>,
    /// Condvar pair the bg thread parks on when idle. The bool is a
    /// "wake-pending" flag set by [`Self::wake`]; the bg thread checks
    /// it on every loop iteration so a wake that races with the start
    /// of a wait isn't lost.
    wakeup: Arc<(Mutex<bool>, Condvar)>,
    /// Tracks whether the bg thread was active on its previous tick.
    /// Used to edge-trigger `wake_callback` only on idle→active
    /// transitions: once the main thread has been kicked into rendering,
    /// the per-frame `request_redraw` chain takes over so we don't need
    /// to keep poking it from the bg thread on every tick. Reset to
    /// `false` whenever the bg thread ticks while inactive.
    last_active: Arc<AtomicBool>,
}

// SAFETY: On wasm32 the wake callback is `Arc<dyn Fn()>` (no `Send +
// Sync`) because the rAF driver and the runner's render closure both
// run on the main browser thread. The wgpu surface, web_sys event
// handlers, and the rest of the dep tree assume single-threaded
// access. We manually opt the scheduler back into `Send + Sync` so
// downstream types — `Mutex<AnimationScheduler>`, `Weak<Mutex<…>>`,
// the `static FOCUSED_TEXT_AREA: Mutex<…>` slot — keep their existing
// auto-derived `Send + Sync` and don't ripple the relaxed bound
// throughout `blinc_layout`. This mirrors the same single-threaded-
// platform pattern used by `blinc_platform_harmony::HarmonyAssetLoader`
// and `blinc_platform_web::WebWindow`. There are no other threads on
// wasm32 to send to, so the `unsafe impl` cannot fire a real footgun.
#[cfg(target_arch = "wasm32")]
unsafe impl Send for AnimationScheduler {}
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for AnimationScheduler {}

impl AnimationScheduler {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(SchedulerInner {
                springs: SlotMap::with_key(),
                keyframes: SlotMap::with_key(),
                timelines: SlotMap::with_key(),
                tick_callbacks: SlotMap::with_key(),
                last_frame: Instant::now(),
                target_fps: 120,
                dirty_springs: std::collections::HashSet::new(),
                smoothed_dt: None,
            })),
            stop_flag: Arc::new(AtomicBool::new(false)),
            needs_redraw: Arc::new(AtomicBool::new(false)),
            continuous_redraw: Arc::new(AtomicBool::new(false)),
            external_anim_active: Arc::new(AtomicBool::new(false)),
            thread_handle: None,
            wake_callback: None,
            wakeup: Arc::new((Mutex::new(false), Condvar::new())),
            last_active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Wake the background thread if it's parked.
    ///
    /// Sets the wake-pending flag and notifies the Condvar. Cheap when
    /// no thread is parked (just a futex flip) — safe to call from any
    /// mutation that could change `has_active` from `false` to `true`.
    /// The bg thread re-evaluates activity on the next loop iteration.
    fn wake_inner(wakeup: &(Mutex<bool>, Condvar)) {
        let mut pending = wakeup.0.lock().unwrap();
        *pending = true;
        wakeup.1.notify_one();
    }

    /// Wake the bg thread (no-op on `wasm32` — there is no thread).
    pub fn wake(&self) {
        Self::wake_inner(&self.wakeup);
    }

    /// Notify the scheduler that an `add_spring` / `add_keyframe` /
    /// `add_timeline` / `add_tick_callback` (or analogous mutation)
    /// just enabled work that wasn't there before.
    ///
    /// Combines the bg-thread wake (`wake_inner`) with a direct
    /// `wake_callback` fire. The bg-thread wake is the only one that
    /// matters in `AnimationThreadMode::Background`; the
    /// `wake_callback` fire is the only one that matters in
    /// `AnimationThreadMode::Main` (it's how the main-thread runner
    /// learns it should re-render to tick the new animation, even
    /// when the registration is happening from a custom timer thread
    /// or background task).
    ///
    /// In `Background` the bg thread also fires `wake_callback` on
    /// its idle→active edge, so this method's direct fire becomes
    /// redundant — but the wake-proxy + `frame_dirty` flip on the
    /// receiving side are idempotent, so the duplication is
    /// harmless.
    fn notify_active(&self) {
        Self::wake_inner(&self.wakeup);
        if let Some(cb) = &self.wake_callback {
            cb();
        }
    }

    /// Set a wake callback that will be called when animations need a redraw.
    ///
    /// On native: invoked from the scheduler's background thread, so `F`
    /// must be `Send + Sync`. Use this to wake an event loop from
    /// another thread (e.g. via `EventLoopProxy::wake`).
    ///
    /// On wasm32: invoked synchronously from inside the rAF closure on
    /// the main browser thread, so the `Send + Sync` bound is dropped.
    /// The web runner uses this to install a closure that re-borrows
    /// its `Rc<RefCell<WebApp>>` and renders a frame.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let wake_proxy = event_loop.wake_proxy();
    /// scheduler.set_wake_callback(move || wake_proxy.wake());
    /// ```
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_wake_callback<F>(&mut self, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.wake_callback = Some(Arc::new(callback));
    }

    /// Wasm32 sibling of [`Self::set_wake_callback`] without the
    /// `Send + Sync` bound. See the native version's docs for the
    /// rationale; the only difference is the relaxed bound.
    #[cfg(target_arch = "wasm32")]
    pub fn set_wake_callback<F>(&mut self, callback: F)
    where
        F: Fn() + 'static,
    {
        self.wake_callback = Some(Arc::new(callback));
    }

    /// Tick everything once: springs, keyframes, timelines, and tick
    /// callbacks. Returns `(has_active, dt_secs)` so callers can decide
    /// whether to schedule another frame.
    ///
    /// This is the per-frame body shared by both the desktop background
    /// thread (see [`start_background`](Self::start_background)) and the
    /// browser `requestAnimationFrame` driver
    /// (see `start_raf` on wasm32). Extracted from the original
    /// `start_background` thread closure verbatim — no semantic change.
    fn tick_frame_inner(
        inner: &Arc<Mutex<SchedulerInner>>,
        needs_redraw: &Arc<AtomicBool>,
        wants_continuous: bool,
        wake_callback: Option<&WakeCallback>,
        last_active: &Arc<AtomicBool>,
    ) -> (bool, f32) {
        let (has_active, tick_callbacks_to_call, dt) = {
            let mut inner = inner.lock().unwrap();
            let now = Instant::now();
            // Cap the per-tick dt to avoid spring/keyframe/timeline
            // teleports when a frame stalls — first-frame shader
            // pipeline compile, GC pause, background-thread oversleep,
            // anything that produces a >100 ms gap. Without the cap,
            // a 280 ms first-paint causes the spring's RK4 substepping
            // to run ~35 iterations in one tick, settling a 600 ms
            // animation in a single frame (the user sees the bar jump
            // straight to its target). Capping to 32 ms (≈ 2 vsync
            // intervals) makes the animation pause-and-resume cleanly
            // instead of fast-forwarding through real time the user
            // didn't see.
            let raw_dt = (now - inner.last_frame).as_secs_f32().min(MAX_TICK_DT);
            inner.last_frame = now;
            // EMA-smooth dt before stepping. See [`smooth_dt`] for why.
            let dt = smooth_dt(raw_dt, &mut inner.smoothed_dt);
            let dt_ms = dt * 1000.0;

            // Update all springs. A spring whose `value()` actually
            // moves this tick is marked dirty for the fast-path
            // Phase 4 consumer; springs that didn't move (already
            // settled, or stepped within epsilon) stay out of the
            // set so an off-screen spinner pinging tick at 120 Hz
            // doesn't churn the GPU upload buffer.
            let collected_ids: Vec<SpringId> = inner.springs.iter().map(|(id, _)| id).collect();
            for id in &collected_ids {
                if let Some(spring) = inner.springs.get_mut(*id) {
                    let prev = spring.value();
                    spring.step(dt);
                    if (spring.value() - prev).abs() >= DIRTY_EPSILON {
                        inner.dirty_springs.insert(*id);
                    }
                }
            }

            // Update all keyframe animations
            for (_, keyframe) in inner.keyframes.iter_mut() {
                keyframe.tick(dt_ms);
            }

            // Update all timelines
            for (_, timeline) in inner.timelines.iter_mut() {
                timeline.tick(dt_ms);
            }

            // Collect tick callbacks to call (we'll call them after releasing the lock)
            let callbacks: Vec<_> = inner
                .tick_callbacks
                .iter()
                .map(|(_, cb)| Arc::clone(cb))
                .collect();

            // NOTE: We do NOT remove animations here!
            // Springs, keyframes, and timelines are only removed when:
            // 1. Their wrapper (AnimatedValue, AnimatedKeyframe, AnimatedTimeline) is dropped
            // 2. set_immediate() is called on springs
            // This ensures animations can be restarted after completing.

            // Check if any animations are still active (playing, not just present)
            // Tick callbacks count as active - they need continuous updates
            let has_active = inner.springs.iter().any(|(_, s)| !s.is_settled())
                || inner.keyframes.iter().any(|(_, k)| k.is_playing())
                || inner.timelines.iter().any(|(_, t)| t.is_playing())
                || !inner.tick_callbacks.is_empty();

            (has_active, callbacks, dt)
        };

        // Call tick callbacks outside the lock to avoid deadlocks
        for callback in tick_callbacks_to_call {
            if let Ok(mut cb) = callback.lock() {
                cb(dt);
            }
        }

        // Signal main thread that it needs to redraw
        // Either from active animations OR continuous redraw request (cursor blink)
        let now_active = has_active || wants_continuous;
        let was_active = last_active.swap(now_active, Ordering::AcqRel);
        if now_active {
            needs_redraw.store(true, Ordering::Release);

            // Wake the event loop only on idle→active transitions. Once
            // the main thread is rendering, its end-of-frame
            // `request_redraw` chain decides whether to keep going (and
            // gates that on visibility — off-screen-only animations
            // settle to a quiet bg-thread tick with no main-thread
            // wakes). Without the edge trigger, the bg thread would
            // call `wake_callback()` 60–120 times/sec for the entire
            // duration of any animation, which on cn_demo with three
            // infinite-loop spinners pinned the main thread at full
            // refresh rate even when the user wasn't looking at them.
            if !was_active {
                if let Some(callback) = wake_callback {
                    tracing::debug!(
                        "Animation tick: waking driver (transition idle→active, continuous={}, active={})",
                        wants_continuous,
                        has_active
                    );
                    callback();
                }
            }
        }

        (now_active, dt)
    }

    /// Start the scheduler on a background thread
    ///
    /// This ensures animations continue even when the window loses focus.
    /// The thread runs at the configured target FPS (default 120).
    ///
    /// The thread sets the `needs_redraw` flag whenever there are active
    /// animations. The main thread should call `take_needs_redraw()` to
    /// check and clear this flag, then request a window redraw.
    ///
    /// If a wake callback is set via `set_wake_callback()`, it will be called
    /// to wake up the main thread's event loop when animations are active.
    ///
    /// **Not available on `wasm32-unknown-unknown`** — the browser
    /// doesn't expose threads. Use `Self::start_raf` instead, which
    /// drives the same per-frame work from `requestAnimationFrame`.
    /// (Plain code reference rather than an intra-doc link because
    /// `start_raf` itself is `#[cfg(target_arch = "wasm32")]`-gated
    /// and isn't visible to rustdoc on host targets.)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_background(&mut self) {
        if self.thread_handle.is_some() {
            return; // Already running
        }

        let inner = Arc::clone(&self.inner);
        let stop_flag = Arc::clone(&self.stop_flag);
        let needs_redraw = Arc::clone(&self.needs_redraw);
        let continuous_redraw = Arc::clone(&self.continuous_redraw);
        let external_anim_active = Arc::clone(&self.external_anim_active);
        let wake_callback = self.wake_callback.clone();
        let wakeup = Arc::clone(&self.wakeup);
        let last_active = Arc::clone(&self.last_active);

        self.thread_handle = Some(thread::spawn(move || {
            // Adaptive FPS scheduler:
            //
            // * Active state — anything in `has_active` is true (springs,
            //   keyframes, timelines, tick callbacks) or `continuous_redraw`
            //   is set. We tick every `1 / target_fps` seconds and signal
            //   the main thread to redraw. `target_fps` is read fresh every
            //   iteration so `set_target_fps` takes effect on the next tick.
            //
            // * Idle state — nothing is active. We park on `wakeup.1` until
            //   a wake notification arrives (an animation registered, target
            //   set, continuous_redraw enabled, etc). Zero CPU on Linux —
            //   the futex blocks the thread.
            //
            // Replaces the prior unconditional `thread::sleep` at 120fps,
            // which combined with the perpetual keep-alive callback in
            // `windowed.rs` to pin a CPU core even on a static UI
            // (issue #28).
            while !stop_flag.load(Ordering::Relaxed) {
                let start = Instant::now();

                let wants_continuous = continuous_redraw.load(Ordering::Relaxed);
                let external_anim = external_anim_active.load(Ordering::Relaxed);
                let (has_active_inner, _) = Self::tick_frame_inner(
                    &inner,
                    &needs_redraw,
                    wants_continuous,
                    wake_callback.as_ref(),
                    &last_active,
                );
                // Treat external animation drivers (node-editor's
                // running-edge pulse, canvas-kit per-frame canvas
                // closures, anything that calls
                // `blinc_layout::set_external_anim_active(true)`) as
                // contributing to has_active. Without this, the
                // `!has_active && wants_continuous` half-rate
                // downscale below dropped every external-driver
                // animation to target_fps/2 whenever a text input
                // was focused. Scheduler-internal primitives
                // (springs/keyframes/timelines/tick_callbacks) drive
                // the inner check; the external flag is a sticky
                // bit drivers manage themselves.
                let has_active = has_active_inner || external_anim;
                let active = has_active || wants_continuous;

                // Read target_fps BEFORE taking the wakeup lock — `wake_inner`
                // callers (e.g. `add_spring`) always hold `inner.lock()`
                // before locking `wakeup`, so the bg thread must mirror that
                // order to avoid deadlock.
                //
                // When `wants_continuous` is the *only* reason we're awake
                // (no real animations are playing — has_active is false),
                // tick at half rate. The classic consumer of continuous
                // redraw is text-input cursor blink, which doesn't need
                // 60+ fps to look right. Cuts the CPU floor for any
                // focused text input in half.
                let frame_duration = if active {
                    let target_fps = inner.lock().unwrap().target_fps.max(1);
                    let effective_fps = if !has_active && wants_continuous {
                        (target_fps / 2).max(1)
                    } else {
                        target_fps
                    };
                    Duration::from_micros(1_000_000 / effective_fps as u64)
                } else {
                    Duration::ZERO // unused in idle branch
                };
                let elapsed = start.elapsed();

                // Reset the wake flag and wait. Holding the wakeup lock
                // across the reset + wait means a wake call racing with the
                // start of the wait either lands before (we observe
                // `*pending == true` on next iter) or after (`notify_one`
                // wakes us mid-wait) — never lost.
                let mut pending = wakeup.0.lock().unwrap();
                *pending = false;

                if active {
                    if let Some(remaining) = frame_duration.checked_sub(elapsed) {
                        if remaining > Duration::ZERO {
                            let (g, _) = wakeup.1.wait_timeout(pending, remaining).unwrap();
                            pending = g;
                        }
                    }
                    drop(pending);
                } else {
                    // Idle: park indefinitely until a wake arrives.
                    while !*pending && !stop_flag.load(Ordering::Relaxed) {
                        pending = wakeup.1.wait(pending).unwrap();
                    }
                    drop(pending);
                }
            }
        }));
    }

    /// Stop the background thread
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stop_background(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        // Kick the thread out of any park so it observes stop_flag.
        Self::wake_inner(&self.wakeup);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
        self.stop_flag.store(false, Ordering::Relaxed);
    }

    /// Start the scheduler on the browser's `requestAnimationFrame`.
    ///
    /// The wasm32 sibling of [`Self::start_background`]. Where the
    /// desktop version spawns an OS thread that ticks at 120 fps and
    /// signals the main thread via the wake callback, this version
    /// installs a `requestAnimationFrame` callback chain that ticks
    /// the same per-frame body once per browser frame and calls the
    /// wake callback synchronously inside the rAF closure.
    ///
    /// The frame budget is whatever the browser hands you (typically
    /// 60 fps, occasionally 120 on high-refresh displays). There is no
    /// `target_fps` cap — `requestAnimationFrame` already paces itself
    /// to the display.
    ///
    /// Set the wake callback **before** calling this. The wake
    /// callback is what the runner uses to actually render a frame —
    /// the scheduler doesn't know about wgpu surfaces, it just knows
    /// "tick everything, then call wake if anything moved".
    ///
    /// The rAF chain self-perpetuates: each closure invocation
    /// schedules the next via `window.requestAnimationFrame(self)`.
    /// Stopping it requires dropping the `Closure` that owns the
    /// chain — currently we leak it for the lifetime of the app
    /// (matching how `start_background()` runs forever on native).
    /// A future `stop_raf()` could swap out the captured closure
    /// reference if we ever need teardown.
    ///
    /// # Panics
    ///
    /// Panics if there is no global `window` object (e.g. running in
    /// a Web Worker). Use a try-construction site if you need to
    /// gracefully degrade.
    #[cfg(target_arch = "wasm32")]
    pub fn start_raf(&self) {
        use std::cell::RefCell;
        use std::rc::Rc;
        use wasm_bindgen::JsCast;
        use wasm_bindgen::closure::Closure;

        let window = web_sys::window().expect("AnimationScheduler::start_raf needs `window`");

        // Per-frame state — captured by the rAF closure. Cloning these
        // Arcs is cheap; the closure owns its own clones for the
        // lifetime of the app.
        let inner = Arc::clone(&self.inner);
        let needs_redraw = Arc::clone(&self.needs_redraw);
        let continuous_redraw = Arc::clone(&self.continuous_redraw);
        let wake_callback = self.wake_callback.clone();
        let last_active = Arc::clone(&self.last_active);

        // Self-referential closure cell. The outer `Rc` is what the
        // closure schedules itself with via
        // `window.request_animation_frame(closure.borrow().as_ref().unchecked_ref())`.
        // The borrow only happens *inside* the closure body, never on
        // the same call frame, so there's no aliasing issue.
        // (clippy::type_complexity is allowed locally because the type
        // *is* genuinely the rAF closure-cell shape — extracting a
        // type alias here would just rename the same complexity.)
        #[allow(clippy::type_complexity)]
        let closure_cell: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
        let closure_cell_for_init = Rc::clone(&closure_cell);

        // The window handle the closure needs to schedule the next frame.
        let window_for_closure = window.clone();

        *closure_cell_for_init.borrow_mut() = Some(Closure::wrap(Box::new(move || {
            let wants_continuous = continuous_redraw.load(Ordering::Relaxed);
            // Pass `None` for the wake callback — the edge-trigger
            // logic inside `tick_frame_inner` is for the native
            // `start_background` thread, where it stops the bg thread
            // from spamming the main thread once the chain is alive.
            // On wasm the wake callback IS the per-frame driver
            // (`WebApp::run_one_frame`), and RAF itself is browser-paced
            // — we want to fire it on every tick, not just idle→active
            // transitions. (Without this fix, only the very first
            // RAF tick called the callback; everything else was frozen
            // — scrolling, hover, mouse, animations, image rendering.)
            Self::tick_frame_inner(&inner, &needs_redraw, wants_continuous, None, &last_active);

            // Drive the per-frame work directly from the RAF closure.
            if let Some(ref cb) = wake_callback {
                cb();
            }

            // Schedule the next frame. Borrowing closure_cell here is
            // safe because the prior borrow_mut at install time has
            // already been released.
            if let Some(ref c) = *closure_cell.borrow() {
                let _ = window_for_closure.request_animation_frame(c.as_ref().unchecked_ref());
            }
        }) as Box<dyn FnMut()>));

        // Kick off the first frame.
        if let Some(ref c) = *closure_cell_for_init.borrow() {
            let _ = window.request_animation_frame(c.as_ref().unchecked_ref());
        }

        // Intentionally leak the closure cell — its only owner from
        // here on is the rAF callback chain itself, which keeps a
        // reference for as long as the chain runs (i.e. forever, like
        // `start_background()` on native). If a future `stop_raf()`
        // wants to break the chain, it can hold a Weak<RefCell<…>>
        // and clear the inner Option.
        std::mem::forget(closure_cell_for_init);
    }

    /// Check if the background thread is running
    pub fn is_background_running(&self) -> bool {
        self.thread_handle.is_some()
    }

    /// Check and clear the needs_redraw flag
    ///
    /// The background thread sets this flag when animations are active.
    /// Call this from the main thread's event loop to check if a redraw
    /// is needed, then request a window redraw if true.
    ///
    /// This is an atomic swap operation that returns the previous value
    /// and clears the flag in one operation.
    pub fn take_needs_redraw(&self) -> bool {
        self.needs_redraw.swap(false, Ordering::Acquire)
    }

    /// Manually request a redraw
    ///
    /// This sets the needs_redraw flag, which will be picked up by the
    /// main thread on its next event loop iteration.
    pub fn request_redraw(&self) {
        self.needs_redraw.store(true, Ordering::Release);
        Self::wake_inner(&self.wakeup);
    }

    /// Enable continuous redraw mode
    ///
    /// When enabled, the background thread will continuously signal redraws
    /// even without active animations. Use this for features like cursor blink
    /// that need regular redraws without registering full animations.
    ///
    /// Call `set_continuous_redraw(false)` when no longer needed.
    pub fn set_continuous_redraw(&self, enabled: bool) {
        tracing::debug!("AnimationScheduler: set_continuous_redraw({})", enabled);
        self.continuous_redraw.store(enabled, Ordering::Release);
        if enabled {
            Self::wake_inner(&self.wakeup);
        }
    }

    /// Check if continuous redraw mode is enabled
    pub fn is_continuous_redraw(&self) -> bool {
        self.continuous_redraw.load(Ordering::Relaxed)
    }

    /// Set the external-animation-active flag.
    ///
    /// Drivers that paint per frame via direct time reads
    /// (node-editor's running-edge pulses, canvas-kit's per-frame
    /// canvas closures with time-dependent visuals, anything that
    /// calls `blinc_layout::request_animation_tick`) flip this on
    /// while they have live animation and off when they settle.
    ///
    /// When `true`, the scheduler treats the system as active —
    /// the `!has_active && wants_continuous` half-rate downscale
    /// (intended for "cursor-blink-only on an otherwise idle page")
    /// does NOT fire, so external-driven animations keep their
    /// configured cadence regardless of whether a text input is
    /// focused.
    pub fn set_external_anim_active(&self, active: bool) {
        self.external_anim_active.store(active, Ordering::Release);
        if active {
            Self::wake_inner(&self.wakeup);
        }
    }

    /// Check whether the external-anim-active flag is currently set.
    pub fn is_external_anim_active(&self) -> bool {
        self.external_anim_active.load(Ordering::Relaxed)
    }

    /// Get a handle to this scheduler for passing to components
    pub fn handle(&self) -> SchedulerHandle {
        SchedulerHandle {
            inner: Arc::downgrade(&self.inner),
            needs_redraw: Arc::clone(&self.needs_redraw),
            wakeup: Arc::clone(&self.wakeup),
            wake_callback: self.wake_callback.clone(),
            external_anim_active: Arc::clone(&self.external_anim_active),
        }
    }

    pub fn set_target_fps(&mut self, fps: u32) {
        self.inner.lock().unwrap().target_fps = fps;
        // Wake the bg thread so a smaller frame_duration takes effect
        // immediately, rather than waiting out the previous (longer) one.
        Self::wake_inner(&self.wakeup);
    }

    /// Advance all springs / keyframes / timelines / tick_callbacks
    /// and return whether anything is still active.
    ///
    /// `dt` is computed internally from `last_frame` so the caller
    /// just needs a reference; no clock argument required.
    ///
    /// # Threading
    ///
    /// In `AnimationThreadMode::Main` (defined in `blinc_platform`,
    /// not linkable here; the default in `WindowConfig`) this is the
    /// sole tick path: the windowed runner calls it once per rendered
    /// frame in Phase 3, so animation values read at paint time are
    /// exactly in phase with the frame being drawn.
    ///
    /// In `AnimationThreadMode::Background` the dedicated bg thread
    /// owns ticking; calling `tick` from the main thread under that
    /// mode would race the bg thread on `inner.last_frame` and
    /// double-step every animation. To prevent that, this method
    /// returns immediately (without advancing state) when a bg
    /// thread is running, leaving the bg thread as the single
    /// source of truth. The `bool` it returns then reflects the
    /// activity state the bg thread last computed.
    ///
    /// `tick_callback`s registered via
    /// [`add_tick_callback`](Self::add_tick_callback) fire after the
    /// spring / keyframe / timeline pass, with `dt` in seconds. They
    /// fire on every main-thread `tick()` and on every bg-thread
    /// iteration — never both for a given frame, because of the
    /// no-op guard above.
    pub fn tick(&self) -> bool {
        // Background-thread mode: bg thread owns ticking. Skip here.
        // Returning the latest active state we know about lets the
        // caller still gate redraws correctly via `has_active`.
        #[cfg(not(target_arch = "wasm32"))]
        if self.thread_handle.is_some() {
            return self.has_active_animations();
        }

        let (has_active, tick_callbacks_to_call, dt) = {
            let mut inner = self.inner.lock().unwrap();
            let now = Instant::now();
            // Same per-tick dt cap as `tick_frame_inner` — prevents
            // spring/keyframe/timeline teleports on slow frames.
            // See `MAX_TICK_DT` doc above.
            let raw_dt = (now - inner.last_frame).as_secs_f32().min(MAX_TICK_DT);
            inner.last_frame = now;
            // EMA-smooth dt before stepping. See [`smooth_dt`] for why.
            let dt = smooth_dt(raw_dt, &mut inner.smoothed_dt);
            let dt_ms = dt * 1000.0;

            // Step springs and record those whose value actually
            // moved (≥ DIRTY_EPSILON) into `dirty_springs`. Mirrors
            // the bg-thread tick_frame_inner path so both modes feed
            // the same fast-path consumer.
            let collected_ids: Vec<SpringId> = inner.springs.iter().map(|(id, _)| id).collect();
            for id in &collected_ids {
                if let Some(spring) = inner.springs.get_mut(*id) {
                    let prev = spring.value();
                    spring.step(dt);
                    if (spring.value() - prev).abs() >= DIRTY_EPSILON {
                        inner.dirty_springs.insert(*id);
                    }
                }
            }
            for (_, keyframe) in inner.keyframes.iter_mut() {
                keyframe.tick(dt_ms);
            }
            for (_, timeline) in inner.timelines.iter_mut() {
                timeline.tick(dt_ms);
            }

            let callbacks: Vec<_> = inner
                .tick_callbacks
                .iter()
                .map(|(_, cb)| Arc::clone(cb))
                .collect();

            // Springs, keyframes, and timelines are only removed when
            // their wrappers drop — preserved across `is_settled` /
            // `is_playing` flips so animations can be restarted.
            let has_active = inner.springs.iter().any(|(_, s)| !s.is_settled())
                || inner.keyframes.iter().any(|(_, k)| k.is_playing())
                || inner.timelines.iter().any(|(_, t)| t.is_playing())
                || !inner.tick_callbacks.is_empty();

            (has_active, callbacks, dt)
        };

        // Run callbacks outside the lock to avoid deadlocks if a
        // callback re-enters the scheduler.
        for callback in tick_callbacks_to_call {
            if let Ok(mut cb) = callback.lock() {
                cb(dt);
            }
        }

        // Mirror the bg-thread path: when work is still active, flip
        // `needs_redraw` so the windowed runner's Phase 5
        // `take_needs_redraw()` keeps re-arming the redraw chain.
        // Without this, Main-mode loops sit in `ControlFlow::Wait`
        // after the first paint — timelines / spring values /
        // canvas-driven animations only advance on the next stray
        // input event.
        //
        // Deliberately NOT OR'd with `continuous_redraw`: the
        // bg-thread path includes it because the bg thread ticks
        // autonomously and needs to signal main "render now". In
        // Main mode the windowed runner reads
        // `widgets::has_focused_text_input()` directly and paces
        // cursor blinks via `wake_at(400ms)` (issue #28 follow-up);
        // mirroring `continuous_redraw` here would defeat that
        // pacing and pin focused-input idle at vsync (~30 % CPU).
        if has_active {
            self.needs_redraw.store(true, Ordering::Release);
        }

        has_active
    }

    /// Check if any animations are still active
    pub fn has_active_animations(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.springs.iter().any(|(_, s)| !s.is_settled())
            || inner.keyframes.iter().any(|(_, k)| k.is_playing())
            || inner.timelines.iter().any(|(_, t)| t.is_playing())
    }

    /// Get the number of active springs
    pub fn spring_count(&self) -> usize {
        self.inner.lock().unwrap().springs.len()
    }

    /// Drain springs that moved (or had their target set / were
    /// freshly registered) since the last drain. Counterpart to
    /// [`SchedulerHandle::take_dirty_springs`]; same semantics, just
    /// accessible from `&AnimationScheduler` without going through a
    /// handle. The windowed runner's frame-timing instrumentation
    /// reads this without first cloning the handle.
    pub fn take_dirty_springs(&self) -> Vec<SpringId> {
        self.inner.lock().unwrap().dirty_springs.drain().collect()
    }

    /// Peek at the dirty-spring count without draining.
    pub fn dirty_spring_count(&self) -> usize {
        self.inner.lock().unwrap().dirty_springs.len()
    }

    /// Get the number of active keyframe animations
    pub fn keyframe_count(&self) -> usize {
        self.inner.lock().unwrap().keyframes.len()
    }

    /// Get the number of active timelines
    pub fn timeline_count(&self) -> usize {
        self.inner.lock().unwrap().timelines.len()
    }

    // =========================================================================
    // Direct Spring Access (for advanced use cases)
    // =========================================================================

    pub fn add_spring(&self, spring: Spring) -> SpringId {
        let id = self.inner.lock().unwrap().springs.insert(spring);
        self.notify_active();
        id
    }

    pub fn get_spring(&self, id: SpringId) -> Option<Spring> {
        self.inner.lock().unwrap().springs.get(id).copied()
    }

    /// Apply a function to modify a spring if it exists
    pub fn with_spring_mut<F, R>(&self, id: SpringId, f: F) -> Option<R>
    where
        F: FnOnce(&mut Spring) -> R,
    {
        self.inner.lock().unwrap().springs.get_mut(id).map(f)
    }

    pub fn get_spring_value(&self, id: SpringId) -> Option<f32> {
        self.inner
            .lock()
            .unwrap()
            .springs
            .get(id)
            .map(|s| s.value())
    }

    pub fn set_spring_target(&self, id: SpringId, target: f32) {
        if let Some(spring) = self.inner.lock().unwrap().springs.get_mut(id) {
            spring.set_target(target);
        }
        self.notify_active();
    }

    pub fn remove_spring(&self, id: SpringId) -> Option<Spring> {
        self.inner.lock().unwrap().springs.remove(id)
    }

    /// Iterate over all springs mutably
    ///
    /// This is useful for manual animation loops where you want to step all springs.
    /// Returns an iterator adapter that holds the mutex lock.
    pub fn springs_iter_mut(&self) -> SpringsIterMut<'_> {
        SpringsIterMut {
            guard: self.inner.lock().unwrap(),
        }
    }

    // =========================================================================
    // Direct Keyframe Access (for advanced use cases)
    // =========================================================================

    pub fn add_keyframe(&self, keyframe: KeyframeAnimation) -> KeyframeId {
        let id = self.inner.lock().unwrap().keyframes.insert(keyframe);
        self.notify_active();
        id
    }

    pub fn get_keyframe_value(&self, id: KeyframeId) -> Option<f32> {
        self.inner
            .lock()
            .unwrap()
            .keyframes
            .get(id)
            .map(|k| k.value())
    }

    pub fn start_keyframe(&self, id: KeyframeId) {
        if let Some(keyframe) = self.inner.lock().unwrap().keyframes.get_mut(id) {
            keyframe.start();
        }
        self.notify_active();
    }

    pub fn stop_keyframe(&self, id: KeyframeId) {
        if let Some(keyframe) = self.inner.lock().unwrap().keyframes.get_mut(id) {
            keyframe.stop();
        }
    }

    pub fn remove_keyframe(&self, id: KeyframeId) -> Option<KeyframeAnimation> {
        self.inner.lock().unwrap().keyframes.remove(id)
    }

    // =========================================================================
    // Direct Timeline Access (for advanced use cases)
    // =========================================================================

    pub fn add_timeline(&self, timeline: Timeline) -> TimelineId {
        let id = self.inner.lock().unwrap().timelines.insert(timeline);
        self.notify_active();
        id
    }

    pub fn start_timeline(&self, id: TimelineId) {
        if let Some(timeline) = self.inner.lock().unwrap().timelines.get_mut(id) {
            timeline.start();
        }
        self.notify_active();
    }

    pub fn stop_timeline(&self, id: TimelineId) {
        if let Some(timeline) = self.inner.lock().unwrap().timelines.get_mut(id) {
            timeline.stop();
        }
    }

    pub fn remove_timeline(&self, id: TimelineId) -> Option<Timeline> {
        self.inner.lock().unwrap().timelines.remove(id)
    }

    // =========================================================================
    // Tick Callback API (for custom per-frame updates like ECS systems)
    // =========================================================================

    /// Register a tick callback that runs each frame
    ///
    /// The callback receives delta time in seconds. Use this to integrate
    /// custom systems (like ECS) with the animation scheduler's background thread.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let world = Arc::new(Mutex::new(World::new()));
    /// let world_clone = world.clone();
    /// let id = scheduler.add_tick_callback(move |dt| {
    ///     let mut world = world_clone.lock().unwrap();
    ///     // Run ECS systems with delta time
    ///     world.run_systems(dt);
    /// });
    /// ```
    pub fn add_tick_callback<F>(&self, callback: F) -> TickCallbackId
    where
        F: FnMut(f32) + Send + Sync + 'static,
    {
        let id = self
            .inner
            .lock()
            .unwrap()
            .tick_callbacks
            .insert(Arc::new(Mutex::new(callback)));
        self.notify_active();
        id
    }

    /// Remove a tick callback
    pub fn remove_tick_callback(&self, id: TickCallbackId) {
        self.inner.lock().unwrap().tick_callbacks.remove(id);
    }

    /// Get the number of registered tick callbacks
    pub fn tick_callback_count(&self) -> usize {
        self.inner.lock().unwrap().tick_callbacks.len()
    }
}

impl Default for AnimationScheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Iterator adapter for mutable access to springs
///
/// Holds the mutex lock for the duration of iteration.
/// Use in a `for` loop to step all springs.
pub struct SpringsIterMut<'a> {
    guard: std::sync::MutexGuard<'a, SchedulerInner>,
}

impl SpringsIterMut<'_> {
    /// Get an iterator over springs mutably
    ///
    /// Use this with `for (id, spring) in iter.iter_mut() { ... }`
    pub fn for_each<F>(&mut self, mut f: F)
    where
        F: FnMut(SpringId, &mut Spring),
    {
        for (id, spring) in self.guard.springs.iter_mut() {
            f(id, spring);
        }
    }
}

impl<'a> IntoIterator for &'a mut SpringsIterMut<'_> {
    type Item = (SpringId, &'a mut Spring);
    type IntoIter = slotmap::basic::IterMut<'a, SpringId, Spring>;

    fn into_iter(self) -> Self::IntoIter {
        self.guard.springs.iter_mut()
    }
}

impl Clone for AnimationScheduler {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            stop_flag: Arc::clone(&self.stop_flag),
            needs_redraw: Arc::clone(&self.needs_redraw),
            continuous_redraw: Arc::clone(&self.continuous_redraw),
            external_anim_active: Arc::clone(&self.external_anim_active),
            // Cloned scheduler doesn't own the background thread
            thread_handle: None,
            wake_callback: self.wake_callback.clone(),
            wakeup: Arc::clone(&self.wakeup),
            last_active: Arc::clone(&self.last_active),
        }
    }
}

impl Drop for AnimationScheduler {
    fn drop(&mut self) {
        // Stop background thread when scheduler is dropped. The web
        // path doesn't have a thread to stop — its `requestAnimationFrame`
        // chain is intentionally leaked for the lifetime of the app
        // (matching native's "thread runs forever" semantics) and the
        // browser tears it down on page unload.
        #[cfg(not(target_arch = "wasm32"))]
        self.stop_background();
    }
}

/// Implement AnimationAccess for AnimationScheduler
///
/// This allows the scheduler to be used directly with ValueContext for
/// resolving dynamic animation values at render time.
impl AnimationAccess for AnimationScheduler {
    fn get_spring_value(&self, id: u64, generation: u32) -> Option<f32> {
        // Reconstruct SpringId from raw parts
        // slotmap keys are 64-bit with version in upper bits
        let key_data = slotmap::KeyData::from_ffi((id as u32 as u64) | ((generation as u64) << 32));
        let spring_id = SpringId::from(key_data);
        self.inner
            .lock()
            .unwrap()
            .springs
            .get(spring_id)
            .map(|s| s.value())
    }

    fn get_keyframe_value(&self, id: u64) -> Option<f32> {
        // For keyframes, we use the full id (version is in upper 32 bits)
        let key_data = slotmap::KeyData::from_ffi(id);
        let keyframe_id = KeyframeId::from(key_data);
        self.inner
            .lock()
            .unwrap()
            .keyframes
            .get(keyframe_id)
            .map(|k| k.value())
    }

    fn get_timeline_value(&self, timeline_id: u64, _property: &str) -> Option<f32> {
        // Timeline values are accessed through entry IDs, not property names
        // This is a placeholder - timeline access is more complex
        let key_data = slotmap::KeyData::from_ffi(timeline_id);
        let tid = TimelineId::from(key_data);
        // For now, return None as timeline access requires entry IDs
        // Future: parse property as "entry_{id}" and look up
        self.inner.lock().unwrap().timelines.get(tid).map(|_t| 0.0) // Placeholder
    }
}

/// A weak handle to the animation scheduler
///
/// This is passed to components that need to register animations.
/// It won't prevent the scheduler from being dropped.
#[derive(Clone)]
pub struct SchedulerHandle {
    inner: Weak<Mutex<SchedulerInner>>,
    needs_redraw: Arc<AtomicBool>,
    wakeup: Arc<(Mutex<bool>, Condvar)>,
    /// Optional wake callback shared with the parent `AnimationScheduler`.
    /// In `AnimationThreadMode::Main`, mutations from a background
    /// thread (worker, timer, `on_ready` after the 200ms delay) need
    /// to wake the main event loop or the next tick never fires.
    /// Without this, calling `set_spring_target` from off-main does
    /// nothing visible until some unrelated event happens to
    /// schedule a redraw.
    wake_callback: Option<WakeCallback>,
    /// Shared `external_anim_active` flag with the parent scheduler.
    /// Drivers that paint per frame via direct time reads flip this
    /// through the handle so the scheduler's half-rate-on-cursor-blink
    /// downscale doesn't suppress them. See
    /// `AnimationScheduler::set_external_anim_active`.
    external_anim_active: Arc<AtomicBool>,
}

// SAFETY: Mirrors the wasm-only safety boundary on `AnimationScheduler`.
// `SchedulerHandle` carries the same wake callback so web can capture
// `Rc<RefCell<_>>`, but on `wasm32-unknown-unknown` all scheduler access is
// driven on the browser main thread. Marking the handle keeps global caches
// of scheduler-backed animation values usable without requiring native
// `Send + Sync` bounds for web callbacks.
#[cfg(target_arch = "wasm32")]
unsafe impl Send for SchedulerHandle {}
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for SchedulerHandle {}

impl SchedulerHandle {
    /// Wake the scheduler's bg thread if it's parked AND fire the
    /// main-thread wake callback. Called whenever a mutation could
    /// transition `has_active` from false to true.
    ///
    /// In `AnimationThreadMode::Background` the bg-thread wake is
    /// what matters; in `Main` mode the wake_callback is the only
    /// thing that gets the main thread out of `ControlFlow::Wait`.
    /// Doing both makes the handle work correctly under either
    /// mode without callers having to care which is active.
    fn wake(&self) {
        let mut pending = self.wakeup.0.lock().unwrap();
        *pending = true;
        self.wakeup.1.notify_one();
        drop(pending);
        if let Some(cb) = &self.wake_callback {
            cb();
        }
    }

    /// Request a redraw from anywhere — fires the scheduler's
    /// `needs_redraw` flag which the main thread's event loop picks up.
    /// Use this from background threads (e.g. video decode) that need
    /// the UI to repaint without registering a full animation.
    pub fn request_redraw(&self) {
        self.needs_redraw.store(true, Ordering::Release);
        self.wake();
    }

    // =========================================================================
    // Spring Operations
    // =========================================================================

    /// Register a spring and return its ID
    pub fn register_spring(&self, spring: Spring) -> Option<SpringId> {
        let id = self.inner.upgrade().map(|inner| {
            let mut guard = inner.lock().unwrap();
            // Reset last_frame to now to prevent huge dt on first tick
            // This ensures new springs start animating smoothly from their current frame
            guard.last_frame = Instant::now();
            let id = guard.springs.insert(spring);
            // A freshly registered spring is implicitly dirty — its
            // initial value just materialized and downstream
            // consumers haven't had a chance to observe it yet.
            // Without this, the very first paint after a new motion
            // binding mounts could miss the value for one frame.
            guard.dirty_springs.insert(id);
            id
        });
        if id.is_some() {
            self.wake();
        }
        id
    }

    /// Update a spring's target
    pub fn set_spring_target(&self, id: SpringId, target: f32) {
        if let Some(inner) = self.inner.upgrade() {
            let mut guard = inner.lock().unwrap();
            if let Some(spring) = guard.springs.get_mut(id) {
                spring.set_target(target);
            }
            // Even if the next tick happens to settle within
            // DIRTY_EPSILON (very short throw, tight spring), the
            // intent here is "the user requested a new state" — mark
            // dirty so the fast path repaints at least one frame.
            guard.dirty_springs.insert(id);
            // Reset the tick clock so the first post-target step uses
            // a fresh, small dt. Without this, a spring that settled
            // (so `last_frame` stopped advancing) and is re-armed
            // seconds later sees `dt = idle time` on the very next
            // tick — RK4 substepping then runs ~`idle / 8 ms`
            // iterations and the value snaps almost the whole way
            // to target in one frame, defeating the spring's
            // perceived duration. Mirrors what `register_spring`
            // already does for the same reason on fresh registration.
            guard.last_frame = Instant::now();
        }
        self.wake();
    }

    /// Get current spring value
    pub fn get_spring_value(&self, id: SpringId) -> Option<f32> {
        self.inner
            .upgrade()
            .and_then(|inner| inner.lock().unwrap().springs.get(id).map(|s| s.value()))
    }

    /// Check if a spring has settled (at rest at target)
    ///
    /// Returns `true` if the spring exists and has settled, or if the spring
    /// doesn't exist (considered settled since there's nothing animating).
    pub fn is_spring_settled(&self, id: SpringId) -> bool {
        self.inner
            .upgrade()
            .and_then(|inner| {
                inner
                    .lock()
                    .unwrap()
                    .springs
                    .get(id)
                    .map(|s| s.is_settled())
            })
            .unwrap_or(true) // If spring gone, consider settled
    }

    /// Remove a spring
    pub fn remove_spring(&self, id: SpringId) {
        if let Some(inner) = self.inner.upgrade() {
            let mut guard = inner.lock().unwrap();
            guard.springs.remove(id);
            // Removed springs can't dirty anything later — drop the
            // entry so a stale id never makes it into a fast-path
            // drain (slotmap reuses the slot; without this you could
            // chase a phantom binding).
            guard.dirty_springs.remove(&id);
        }
    }

    /// Drain springs that moved (or had their target set / were
    /// freshly registered) since the last drain. The returned ids
    /// are intended for the windowed runner's fast Phase 4 path —
    /// each id maps to a `(LayoutNodeId, target_property)` via a
    /// bindings index, and the consumer patches the corresponding
    /// `GpuPrimitive` field in place instead of running the full
    /// paint walker.
    ///
    /// Calling this clears the set. Empty return means "no
    /// scheduler-tracked animation changed this frame" — at which
    /// point the fast path can return immediately without walking
    /// the tree.
    pub fn take_dirty_springs(&self) -> Vec<SpringId> {
        self.inner
            .upgrade()
            .map(|inner| {
                let mut guard = inner.lock().unwrap();
                guard.dirty_springs.drain().collect()
            })
            .unwrap_or_default()
    }

    /// Peek at the count of dirty springs without draining. Useful
    /// for the windowed runner's "should we even bother taking the
    /// fast path this frame" gate — at zero we know upfront there
    /// is no scheduler-side change to consume.
    pub fn dirty_spring_count(&self) -> usize {
        self.inner
            .upgrade()
            .map(|inner| inner.lock().unwrap().dirty_springs.len())
            .unwrap_or(0)
    }

    /// Pause a spring — freezes at current position
    pub fn pause_spring(&self, id: SpringId) {
        if let Some(inner) = self.inner.upgrade() {
            if let Some(spring) = inner.lock().unwrap().springs.get_mut(id) {
                spring.pause();
            }
        }
    }

    /// Resume a paused spring
    pub fn resume_spring(&self, id: SpringId) {
        if let Some(inner) = self.inner.upgrade() {
            if let Some(spring) = inner.lock().unwrap().springs.get_mut(id) {
                spring.resume();
            }
        }
        self.wake();
    }

    // =========================================================================
    // Keyframe Operations
    // =========================================================================

    /// Register a keyframe animation and return its ID
    pub fn register_keyframe(&self, keyframe: KeyframeAnimation) -> Option<KeyframeId> {
        let id = self
            .inner
            .upgrade()
            .map(|inner| inner.lock().unwrap().keyframes.insert(keyframe));
        if id.is_some() {
            self.wake();
        }
        id
    }

    /// Get current keyframe animation value
    pub fn get_keyframe_value(&self, id: KeyframeId) -> Option<f32> {
        self.inner
            .upgrade()
            .and_then(|inner| inner.lock().unwrap().keyframes.get(id).map(|k| k.value()))
    }

    /// Get keyframe animation progress (0.0 to 1.0)
    pub fn get_keyframe_progress(&self, id: KeyframeId) -> Option<f32> {
        self.inner.upgrade().and_then(|inner| {
            inner
                .lock()
                .unwrap()
                .keyframes
                .get(id)
                .map(|k| k.progress())
        })
    }

    /// Check if keyframe animation is playing
    pub fn is_keyframe_playing(&self, id: KeyframeId) -> bool {
        self.inner
            .upgrade()
            .and_then(|inner| {
                inner
                    .lock()
                    .unwrap()
                    .keyframes
                    .get(id)
                    .map(|k| k.is_playing())
            })
            .unwrap_or(false)
    }

    /// Start a keyframe animation
    pub fn start_keyframe(&self, id: KeyframeId) {
        if let Some(inner) = self.inner.upgrade() {
            if let Some(keyframe) = inner.lock().unwrap().keyframes.get_mut(id) {
                keyframe.start();
            }
        }
        self.wake();
    }

    /// Stop a keyframe animation
    pub fn stop_keyframe(&self, id: KeyframeId) {
        if let Some(inner) = self.inner.upgrade() {
            if let Some(keyframe) = inner.lock().unwrap().keyframes.get_mut(id) {
                keyframe.stop();
            }
        }
    }

    /// Remove a keyframe animation
    pub fn remove_keyframe(&self, id: KeyframeId) {
        if let Some(inner) = self.inner.upgrade() {
            inner.lock().unwrap().keyframes.remove(id);
        }
    }

    // =========================================================================
    // Timeline Operations
    // =========================================================================

    /// Register a timeline and return its ID
    pub fn register_timeline(&self, timeline: Timeline) -> Option<TimelineId> {
        let id = self
            .inner
            .upgrade()
            .map(|inner| inner.lock().unwrap().timelines.insert(timeline));
        if id.is_some() {
            self.wake();
        }
        id
    }

    /// Check if timeline is playing
    pub fn is_timeline_playing(&self, id: TimelineId) -> bool {
        self.inner
            .upgrade()
            .and_then(|inner| {
                inner
                    .lock()
                    .unwrap()
                    .timelines
                    .get(id)
                    .map(|t| t.is_playing())
            })
            .unwrap_or(false)
    }

    /// Start a timeline
    pub fn start_timeline(&self, id: TimelineId) {
        if let Some(inner) = self.inner.upgrade() {
            if let Some(timeline) = inner.lock().unwrap().timelines.get_mut(id) {
                timeline.start();
            }
        }
        self.wake();
    }

    /// Stop a timeline
    pub fn stop_timeline(&self, id: TimelineId) {
        if let Some(inner) = self.inner.upgrade() {
            if let Some(timeline) = inner.lock().unwrap().timelines.get_mut(id) {
                timeline.stop();
            }
        }
    }

    /// Remove a timeline
    pub fn remove_timeline(&self, id: TimelineId) {
        if let Some(inner) = self.inner.upgrade() {
            inner.lock().unwrap().timelines.remove(id);
        }
    }

    /// Access a timeline to add entries or get values
    ///
    /// The closure receives a mutable reference to the timeline.
    /// Returns None if the scheduler is dropped or timeline doesn't exist.
    pub fn with_timeline<F, R>(&self, id: TimelineId, f: F) -> Option<R>
    where
        F: FnOnce(&mut Timeline) -> R,
    {
        let result = self
            .inner
            .upgrade()
            .and_then(|inner| inner.lock().unwrap().timelines.get_mut(id).map(f));
        // The closure may have re-armed/started the timeline; wake the
        // scheduler unconditionally rather than try to peek inside it.
        if result.is_some() {
            self.wake();
        }
        result
    }

    /// Read a timeline without waking the scheduler.
    ///
    /// The `&mut` sibling has to assume the closure re-armed the
    /// timeline, so it wakes on every successful access. Routing a pure
    /// read through it makes the read itself a redraw signal: the paint
    /// walk samples a spinner's rotation, the wake callback sets
    /// `frame_dirty`, and the next frame is guaranteed — a loop that no
    /// visibility gate or FPS cap can break, because it is re-armed by
    /// the act of drawing. Queries that only observe state take this
    /// path; anything that seeks, starts, or mutates keeps the waking
    /// one.
    pub fn with_timeline_read<F, R>(&self, id: TimelineId, f: F) -> Option<R>
    where
        F: FnOnce(&Timeline) -> R,
    {
        self.inner
            .upgrade()
            .and_then(|inner| inner.lock().unwrap().timelines.get(id).map(f))
    }

    /// Check if the scheduler is still alive
    pub fn is_alive(&self) -> bool {
        self.inner.strong_count() > 0
    }

    /// Set the external-animation-active flag. See
    /// [`AnimationScheduler::set_external_anim_active`] for the
    /// rationale. Wakes the bg thread when transitioning to true so
    /// it picks up the new cadence on the next iteration instead of
    /// waiting out the prior (possibly half-rate) sleep window.
    pub fn set_external_anim_active(&self, active: bool) {
        self.external_anim_active.store(active, Ordering::Release);
        if active {
            self.wake();
        }
    }

    // =========================================================================
    // Tick Callback Operations
    // =========================================================================

    /// Register a tick callback that runs each frame
    ///
    /// The callback receives delta time in seconds. Use this to integrate
    /// custom systems (like ECS) with the animation scheduler's background thread.
    ///
    /// Returns None if the scheduler has been dropped.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let world = Arc::new(Mutex::new(World::new()));
    /// let world_clone = world.clone();
    /// let id = handle.register_tick_callback(move |dt| {
    ///     let mut world = world_clone.lock().unwrap();
    ///     // Run ECS systems with delta time
    ///     world.run_systems(dt);
    /// });
    /// ```
    pub fn register_tick_callback<F>(&self, callback: F) -> Option<TickCallbackId>
    where
        F: FnMut(f32) + Send + Sync + 'static,
    {
        let id = self.inner.upgrade().map(|inner| {
            inner
                .lock()
                .unwrap()
                .tick_callbacks
                .insert(Arc::new(Mutex::new(callback)))
        });
        if id.is_some() {
            self.wake();
        }
        id
    }

    /// Remove a tick callback
    pub fn remove_tick_callback(&self, id: TickCallbackId) {
        if let Some(inner) = self.inner.upgrade() {
            inner.lock().unwrap().tick_callbacks.remove(id);
        }
    }
}

/// Implement AnimationAccess for SchedulerHandle
///
/// This allows the handle to be used with ValueContext for resolving
/// dynamic animation values at render time.
impl AnimationAccess for SchedulerHandle {
    fn get_spring_value(&self, id: u64, generation: u32) -> Option<f32> {
        self.inner.upgrade().and_then(|inner| {
            let key_data =
                slotmap::KeyData::from_ffi((id as u32 as u64) | ((generation as u64) << 32));
            let spring_id = SpringId::from(key_data);
            inner
                .lock()
                .unwrap()
                .springs
                .get(spring_id)
                .map(|s| s.value())
        })
    }

    fn get_keyframe_value(&self, id: u64) -> Option<f32> {
        self.inner.upgrade().and_then(|inner| {
            let key_data = slotmap::KeyData::from_ffi(id);
            let keyframe_id = KeyframeId::from(key_data);
            inner
                .lock()
                .unwrap()
                .keyframes
                .get(keyframe_id)
                .map(|k| k.value())
        })
    }

    fn get_timeline_value(&self, _timeline_id: u64, _property: &str) -> Option<f32> {
        // Placeholder - timeline access is more complex
        None
    }
}

// ============================================================================
// Animated Value (Spring-based)
// ============================================================================

/// An animated value that automatically registers with the scheduler
///
/// When the target changes, the value smoothly animates to it using spring physics.
/// The animation is automatically registered with the scheduler and ticked each frame.
///
/// # Example
///
/// ```ignore
/// // Create an animated value (auto-registers with scheduler)
/// let opacity = AnimatedValue::new(ctx.animation_handle(), 1.0, SpringConfig::stiff());
///
/// // Change target - automatically animates
/// opacity.set_target(0.5);
///
/// // Get current animated value (interpolated)
/// let current = opacity.get();
/// ```
#[derive(Clone)]
pub struct AnimatedValue {
    handle: SchedulerHandle,
    spring_id: Option<SpringId>,
    config: SpringConfig,
    /// The last known value (updated when spring settles)
    current: f32,
    /// The target value we're animating towards
    target: f32,
}

impl AnimatedValue {
    /// Create a new animated value with the given initial value
    pub fn new(handle: SchedulerHandle, initial: f32, config: SpringConfig) -> Self {
        // Don't register immediately - only when we have a target change
        Self {
            handle,
            spring_id: None,
            config,
            current: initial,
            target: initial,
        }
    }

    /// Create with default spring config (stiff)
    pub fn with_default(handle: SchedulerHandle, initial: f32) -> Self {
        Self::new(handle, initial, SpringConfig::stiff())
    }

    /// Set the target value - starts animation if different from current
    pub fn set_target(&mut self, target: f32) {
        self.target = target;

        // If we have a spring, just update its target (spring persists until dropped)
        if let Some(id) = self.spring_id {
            self.handle.set_spring_target(id, target);
        } else {
            // No spring yet - create one if target differs from current
            if (target - self.current).abs() > 0.001 {
                let spring = Spring::new(self.config, self.current);
                if let Some(id) = self.handle.register_spring(spring) {
                    self.spring_id = Some(id);
                    self.handle.set_spring_target(id, target);
                    // Auto-register to current suspension scope
                    crate::suspension::register_spring(id);
                }
            }
        }
    }

    /// Get the current animated value
    pub fn get(&self) -> f32 {
        if let Some(id) = self.spring_id {
            // Try to get spring value; if spring was removed (settled), use target
            self.handle.get_spring_value(id).unwrap_or(self.target)
        } else {
            self.current
        }
    }

    /// Set value immediately without animation
    pub fn set_immediate(&mut self, value: f32) {
        // Remove any active spring
        if let Some(id) = self.spring_id.take() {
            self.handle.remove_spring(id);
        }
        // Symmetric with `set_target`: a value mutation that is going
        // to drive a visual change needs to nudge the redraw chain.
        // Without this nudge, drag-style flows like
        // `cn::slider`/`motion_demo`'s pull-to-refresh only repaint
        // when some other event coincidentally flips `frame_dirty`,
        // because (a) `set_immediate` removes the spring so the
        // binding-vsync paint-walker signal can't carry the change,
        // and (b) bare mouse-moves on the input fast path skip
        // `frame_dirty=true` to keep idle CPU down. Gated on a real
        // value change so the previously-reverted unconditional
        // version (which redraws on no-op writes, e.g.
        // `set_immediate(self.current)` from idle ticks of
        // `visual_animation`) stays gone. `wake` is what `set_target`
        // already calls via `set_spring_target`; same code path,
        // same idempotent dirty flip + wake-proxy ping.
        let changed = (self.current - value).abs() > f32::EPSILON
            || (self.target - value).abs() > f32::EPSILON;
        self.current = value;
        self.target = value;
        if changed {
            self.handle.wake();
        }
    }

    /// Pause the spring — freezes at current position, step() is no-op
    pub fn pause(&mut self) {
        if let Some(id) = self.spring_id {
            self.handle.pause_spring(id);
        }
    }

    /// Resume from paused state
    pub fn resume(&mut self) {
        if let Some(id) = self.spring_id {
            self.handle.resume_spring(id);
        }
    }

    /// Check if currently animating
    pub fn is_animating(&self) -> bool {
        if let Some(id) = self.spring_id {
            // Check actual settled state, not just existence
            !self.handle.is_spring_settled(id)
        } else {
            false
        }
    }

    /// Snap immediately to the target value, stopping any active animation
    ///
    /// This removes the spring entirely and sets the current value to the target.
    /// Useful for immediately completing an animation.
    pub fn snap_to_target(&mut self) {
        self.set_immediate(self.target);
    }

    /// Get the current target value
    pub fn target(&self) -> f32 {
        self.target
    }
}

impl Drop for AnimatedValue {
    fn drop(&mut self) {
        if let Some(id) = self.spring_id {
            crate::suspension::unregister_spring(id);
            self.handle.remove_spring(id);
        }
    }
}

// ============================================================================
// Animated Keyframe
// ============================================================================

/// A keyframe animation that automatically registers with the scheduler
///
/// Provides timed animations with easing functions between keyframes.
/// The animation is automatically registered and ticked by the scheduler.
///
/// # Example
///
/// ```ignore
/// use blinc_animation::{AnimatedKeyframe, Keyframe, Easing};
///
/// // Create a keyframe animation
/// let mut anim = AnimatedKeyframe::new(ctx.animation_handle(), 1000); // 1 second
///
/// // Add keyframes
/// anim.keyframe(0.0, 0.0, Easing::Linear);      // Start at 0
/// anim.keyframe(0.5, 100.0, Easing::EaseOut);   // Middle at 100
/// anim.keyframe(1.0, 50.0, Easing::EaseInOut);  // End at 50
///
/// // Start the animation
/// anim.start();
///
/// // Get current value (updated by scheduler)
/// let value = anim.get();
/// ```
#[derive(Clone)]
pub struct AnimatedKeyframe {
    handle: SchedulerHandle,
    keyframe_id: Option<KeyframeId>,
    duration_ms: u32,
    keyframes: Vec<Keyframe>,
    auto_start: bool,
    /// Number of iterations (-1 for infinite, 0 for none, 1 for once, etc.)
    iterations: i32,
    /// Whether to reverse direction on each iteration (ping-pong)
    ping_pong: bool,
    /// Current iteration count
    current_iteration: i32,
    /// Whether currently playing in reverse
    reversed: bool,
    /// Delay before animation starts (ms)
    delay_ms: u32,
    /// Time when animation started (for delay tracking)
    start_time: Option<Instant>,
}

impl AnimatedKeyframe {
    /// Create a new keyframe animation with the given duration
    pub fn new(handle: SchedulerHandle, duration_ms: u32) -> Self {
        Self {
            handle,
            keyframe_id: None,
            duration_ms,
            keyframes: Vec::new(),
            auto_start: false,
            iterations: 1, // Play once by default
            ping_pong: false,
            current_iteration: 0,
            reversed: false,
            delay_ms: 0,
            start_time: None,
        }
    }

    /// Add a keyframe at the given time position (0.0 to 1.0)
    pub fn keyframe(mut self, time: f32, value: f32, easing: Easing) -> Self {
        self.keyframes.push(Keyframe {
            time,
            value,
            easing,
        });
        self
    }

    /// Set whether to auto-start when registered
    pub fn auto_start(mut self, auto: bool) -> Self {
        self.auto_start = auto;
        self
    }

    /// Set number of iterations (-1 for infinite)
    pub fn iterations(mut self, count: i32) -> Self {
        self.iterations = count;
        self
    }

    /// Enable infinite looping
    pub fn loop_infinite(mut self) -> Self {
        self.iterations = -1;
        self
    }

    /// Enable ping-pong mode (reverse direction on each iteration)
    pub fn ping_pong(mut self, enabled: bool) -> Self {
        self.ping_pong = enabled;
        self
    }

    /// Set delay before animation starts (in milliseconds)
    pub fn delay(mut self, delay_ms: u32) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    /// Build and register the animation, returning self for chaining
    pub fn build(mut self) -> Self {
        // Sort keyframes by time
        self.keyframes.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Create the underlying animation (don't start it yet - we handle that)
        let animation = KeyframeAnimation::new(self.duration_ms, self.keyframes.clone());

        if let Some(id) = self.handle.register_keyframe(animation) {
            self.keyframe_id = Some(id);
        }

        // If auto_start, call our start() method which handles delay properly
        if self.auto_start {
            self.start();
        }

        self
    }

    /// Start the animation
    pub fn start(&mut self) {
        self.current_iteration = 0;
        self.reversed = false;

        // Track start time for delay
        if self.delay_ms > 0 {
            self.start_time = Some(Instant::now());
        } else {
            self.start_time = None;
        }

        if let Some(id) = self.keyframe_id {
            if self.delay_ms == 0 {
                self.handle.start_keyframe(id);
            }
            // If there's a delay, don't start yet - check_and_update will handle it
        } else {
            // Not yet registered - register now
            self.keyframes.sort_by(|a, b| {
                a.time
                    .partial_cmp(&b.time)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let mut animation = KeyframeAnimation::new(self.duration_ms, self.keyframes.clone());
            if self.delay_ms == 0 {
                animation.start();
            }

            if let Some(id) = self.handle.register_keyframe(animation) {
                self.keyframe_id = Some(id);
            }
        }
    }

    /// Stop the animation
    pub fn stop(&mut self) {
        self.start_time = None;
        if let Some(id) = self.keyframe_id {
            self.handle.stop_keyframe(id);
        }
    }

    /// Restart the animation from the beginning
    pub fn restart(&mut self) {
        self.stop();
        self.start();
    }

    /// Check and handle iteration completion, delay, etc.
    /// Returns true if animation should continue running.
    fn check_and_update(&mut self) -> bool {
        // Handle delay
        if let Some(start_time) = self.start_time {
            let elapsed = start_time.elapsed().as_millis() as u32;
            if elapsed < self.delay_ms {
                return true; // Still in delay period
            }
            // Delay complete - start the actual animation
            self.start_time = None;
            if let Some(id) = self.keyframe_id {
                self.handle.start_keyframe(id);
            }
        }

        // Check if current iteration completed
        if let Some(id) = self.keyframe_id {
            if !self.handle.is_keyframe_playing(id) {
                // Animation finished this iteration
                self.current_iteration += 1;

                // Check if we should continue
                let should_continue =
                    self.iterations < 0 || self.current_iteration < self.iterations;

                if should_continue {
                    // Handle ping-pong
                    if self.ping_pong {
                        self.reversed = !self.reversed;
                    }
                    // Restart the animation for next iteration
                    self.handle.start_keyframe(id);
                    return true;
                }
            } else {
                return true; // Still playing
            }
        }

        false
    }

    /// Get the current animated value
    pub fn get(&mut self) -> f32 {
        // Check for iteration completion and handle looping
        self.check_and_update();

        // If in delay period, return initial value
        if self.start_time.is_some() {
            return self.get_initial_value();
        }

        if let Some(id) = self.keyframe_id {
            let raw_value = self.handle.get_keyframe_value(id).unwrap_or(0.0);

            // Apply reverse if in ping-pong and on reverse phase
            if self.reversed && !self.keyframes.is_empty() {
                // Map value from [start, end] to [end, start]
                let first = self.keyframes.first().map(|k| k.value).unwrap_or(0.0);
                let last = self.keyframes.last().map(|k| k.value).unwrap_or(0.0);
                // Reverse: if raw is at 'first' position, return 'last', and vice versa
                first + last - raw_value
            } else {
                raw_value
            }
        } else {
            self.get_initial_value()
        }
    }

    /// Get immutable value (doesn't check iteration)
    fn get_initial_value(&self) -> f32 {
        if !self.keyframes.is_empty() {
            self.keyframes[0].value
        } else {
            0.0
        }
    }

    /// Get the current progress (0.0 to 1.0)
    pub fn progress(&self) -> f32 {
        if let Some(id) = self.keyframe_id {
            let raw_progress = self.handle.get_keyframe_progress(id).unwrap_or(0.0);
            if self.reversed {
                1.0 - raw_progress
            } else {
                raw_progress
            }
        } else {
            0.0
        }
    }

    /// Check if the animation is playing (including during delay and looping)
    pub fn is_playing(&mut self) -> bool {
        // In delay period counts as playing
        if self.start_time.is_some() {
            return true;
        }

        // Check and update iteration state
        self.check_and_update();

        // Check if underlying animation is playing
        if let Some(id) = self.keyframe_id {
            if self.handle.is_keyframe_playing(id) {
                return true;
            }
            // If not playing, check if we should continue looping
            self.iterations < 0 || self.current_iteration < self.iterations
        } else {
            false
        }
    }
}

impl Drop for AnimatedKeyframe {
    fn drop(&mut self) {
        if let Some(id) = self.keyframe_id {
            self.handle.remove_keyframe(id);
        }
    }
}

// ============================================================================
// Animated Timeline
// ============================================================================

/// Trait for types that can be returned from `AnimatedTimeline::configure()`
///
/// Implemented for single `TimelineEntryId` and tuples of entry IDs.
/// This allows `configure()` to reconstruct the return value from stored entry IDs
/// when the timeline is already configured.
pub trait ConfigureResult {
    /// Reconstruct the result from a list of entry IDs
    fn from_entry_ids(ids: &[crate::timeline::TimelineEntryId]) -> Self;
}

impl ConfigureResult for crate::timeline::TimelineEntryId {
    fn from_entry_ids(ids: &[crate::timeline::TimelineEntryId]) -> Self {
        ids[0]
    }
}

impl ConfigureResult
    for (
        crate::timeline::TimelineEntryId,
        crate::timeline::TimelineEntryId,
    )
{
    fn from_entry_ids(ids: &[crate::timeline::TimelineEntryId]) -> Self {
        (ids[0], ids[1])
    }
}

impl ConfigureResult
    for (
        crate::timeline::TimelineEntryId,
        crate::timeline::TimelineEntryId,
        crate::timeline::TimelineEntryId,
    )
{
    fn from_entry_ids(ids: &[crate::timeline::TimelineEntryId]) -> Self {
        (ids[0], ids[1], ids[2])
    }
}

impl ConfigureResult for Vec<crate::timeline::TimelineEntryId> {
    fn from_entry_ids(ids: &[crate::timeline::TimelineEntryId]) -> Self {
        ids.to_vec()
    }
}

/// A timeline animation that automatically registers with the scheduler
///
/// Orchestrates multiple animations with offsets and looping support.
/// The timeline is automatically registered and ticked by the scheduler.
///
/// # Example
///
/// ```ignore
/// use blinc_animation::AnimatedTimeline;
///
/// // Create a timeline
/// let mut timeline = AnimatedTimeline::new(ctx.animation_handle());
///
/// // Add animations at different offsets
/// let opacity_id = timeline.add(0, 500, 0.0, 1.0);      // Fade in from 0-500ms
/// let scale_id = timeline.add(250, 500, 0.8, 1.0);      // Scale up from 250-750ms
/// let slide_id = timeline.add(0, 750, -100.0, 0.0);     // Slide in from 0-750ms
///
/// // Configure looping
/// timeline.set_loop(-1); // Infinite loop
///
/// // Start the timeline
/// timeline.start();
///
/// // Get values for each animation
/// let opacity = timeline.get(opacity_id);
/// let scale = timeline.get(scale_id);
/// let slide = timeline.get(slide_id);
/// ```
pub struct AnimatedTimeline {
    handle: SchedulerHandle,
    timeline_id: Option<TimelineId>,
}

impl AnimatedTimeline {
    /// Create a new timeline animation
    pub fn new(handle: SchedulerHandle) -> Self {
        // Register an empty timeline immediately
        let timeline = Timeline::new();
        let timeline_id = handle.register_timeline(timeline);

        Self {
            handle,
            timeline_id,
        }
    }

    /// Configure the timeline if not already configured, returning entry IDs
    ///
    /// The closure is only called on the first invocation (when the timeline has no entries).
    /// On subsequent calls, it returns the existing entry IDs.
    ///
    /// This is the recommended way to set up persisted timelines, as it handles
    /// both initial configuration and retrieval of existing entries in one call.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let timeline = ctx.use_animated_timeline();
    /// let entry_id = timeline.lock().unwrap().configure(|t| {
    ///     let id = t.add(0, 1000, 0.0, 1.0);
    ///     t.set_loop(-1);
    ///     t.start();
    ///     id  // Return entry ID(s) for later use
    /// });
    /// ```
    pub fn configure<T, F>(&mut self, f: F) -> T
    where
        F: FnOnce(&mut Self) -> T,
        T: ConfigureResult,
    {
        if self.has_entries() {
            // Already configured - return existing entry IDs
            T::from_entry_ids(&self.entry_ids())
        } else {
            // First time - run configuration
            f(self)
        }
    }

    /// Add an animation to the timeline
    ///
    /// Returns an entry ID that can be used to get the current value.
    pub fn add(
        &mut self,
        offset_ms: i32,
        duration_ms: u32,
        start_value: f32,
        end_value: f32,
    ) -> crate::timeline::TimelineEntryId {
        if let Some(id) = self.timeline_id {
            self.handle
                .with_timeline(id, |timeline| {
                    timeline.add(offset_ms, duration_ms, start_value, end_value)
                })
                .expect("Timeline should exist")
        } else {
            panic!("Timeline not registered - scheduler may have been dropped")
        }
    }

    /// Add an animation with a specific easing function
    pub fn add_with_easing(
        &mut self,
        offset_ms: i32,
        duration_ms: u32,
        start_value: f32,
        end_value: f32,
        easing: Easing,
    ) -> crate::timeline::TimelineEntryId {
        if let Some(id) = self.timeline_id {
            self.handle
                .with_timeline(id, |timeline| {
                    timeline.add_with_easing(offset_ms, duration_ms, start_value, end_value, easing)
                })
                .expect("Timeline should exist")
        } else {
            panic!("Timeline not registered - scheduler may have been dropped")
        }
    }

    /// Set loop count (-1 for infinite)
    pub fn set_loop(&mut self, count: i32) {
        if let Some(id) = self.timeline_id {
            self.handle.with_timeline(id, |timeline| {
                timeline.set_loop(count);
            });
        }
    }

    /// Enable/disable alternate (ping-pong) mode
    ///
    /// When enabled, the timeline reverses direction each loop instead of
    /// jumping back to the start.
    pub fn set_alternate(&mut self, enabled: bool) {
        if let Some(id) = self.timeline_id {
            self.handle.with_timeline(id, |timeline| {
                timeline.set_alternate(enabled);
            });
        }
    }

    /// Set playback rate (1.0 = normal speed, 2.0 = 2x speed)
    pub fn set_playback_rate(&mut self, rate: f32) {
        if let Some(id) = self.timeline_id {
            self.handle.with_timeline(id, |timeline| {
                timeline.set_playback_rate(rate);
            });
        }
    }

    /// Start the timeline
    ///
    /// If the timeline has finished and been removed from the scheduler,
    /// use `restart()` instead to re-register it.
    pub fn start(&self) {
        if let Some(id) = self.timeline_id {
            self.handle.start_timeline(id);
        }
    }

    /// Restart the timeline from the beginning
    ///
    /// Resets the timeline to time 0 and starts playing.
    /// This works even after the timeline has completed.
    pub fn restart(&self) {
        if let Some(id) = self.timeline_id {
            self.handle.with_timeline(id, |timeline| {
                timeline.start(); // start() already resets time to 0
            });
        }
    }

    /// Stop the timeline
    pub fn stop(&self) {
        if let Some(id) = self.timeline_id {
            self.handle.stop_timeline(id);
        }
    }

    /// Pause the timeline (can be resumed)
    pub fn pause(&self) {
        if let Some(id) = self.timeline_id {
            self.handle.with_timeline(id, |timeline| {
                timeline.pause();
            });
        }
    }

    /// Resume a paused timeline
    pub fn resume(&self) {
        if let Some(id) = self.timeline_id {
            self.handle.with_timeline(id, |timeline| {
                timeline.resume();
            });
        }
    }

    /// Reverse the playback direction
    pub fn reverse(&self) {
        if let Some(id) = self.timeline_id {
            self.handle.with_timeline(id, |timeline| {
                timeline.reverse();
            });
        }
    }

    /// Seek to a specific time position (in milliseconds)
    pub fn seek(&self, time_ms: f32) {
        if let Some(id) = self.timeline_id {
            self.handle.with_timeline(id, |timeline| {
                timeline.seek(time_ms);
            });
        }
    }

    /// Get the current value for a timeline entry
    pub fn get(&self, entry_id: crate::timeline::TimelineEntryId) -> Option<f32> {
        if let Some(id) = self.timeline_id {
            self.handle
                .with_timeline_read(id, |timeline| timeline.value(entry_id))
                .flatten()
        } else {
            None
        }
    }

    /// Check if the timeline is playing
    pub fn is_playing(&self) -> bool {
        if let Some(id) = self.timeline_id {
            self.handle.is_timeline_playing(id)
        } else {
            false
        }
    }

    /// Get the overall timeline progress (0.0 to 1.0)
    pub fn progress(&self) -> f32 {
        if let Some(id) = self.timeline_id {
            self.handle
                .with_timeline_read(id, |timeline| timeline.progress())
                .unwrap_or(0.0)
        } else {
            0.0
        }
    }

    /// Get progress of a specific entry (0.0 to 1.0)
    pub fn entry_progress(&self, entry_id: crate::timeline::TimelineEntryId) -> Option<f32> {
        if let Some(id) = self.timeline_id {
            self.handle
                .with_timeline_read(id, |timeline| timeline.entry_progress(entry_id))
                .flatten()
        } else {
            None
        }
    }

    /// Check if the timeline has any entries
    ///
    /// Returns true if at least one animation has been added to the timeline.
    /// Useful for checking if a persisted timeline needs configuration.
    pub fn has_entries(&self) -> bool {
        if let Some(id) = self.timeline_id {
            self.handle
                .with_timeline_read(id, |timeline| timeline.entry_count() > 0)
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// Get all entry IDs in this timeline
    ///
    /// Useful for retrieving persisted entry IDs after a timeline has been restored.
    pub fn entry_ids(&self) -> Vec<crate::timeline::TimelineEntryId> {
        if let Some(id) = self.timeline_id {
            self.handle
                .with_timeline_read(id, |timeline| timeline.entry_ids())
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    }
}

impl Drop for AnimatedTimeline {
    fn drop(&mut self) {
        if let Some(id) = self.timeline_id {
            self.handle.remove_timeline(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_tick() {
        let scheduler = AnimationScheduler::new();

        // Add a spring
        let spring = Spring::new(SpringConfig::stiff(), 0.0);
        let id = scheduler.add_spring(spring);
        scheduler.set_spring_target(id, 100.0);

        // Tick
        assert!(scheduler.tick());

        // Value should have moved
        let value = scheduler.get_spring_value(id).unwrap();
        assert!(value > 0.0);
    }

    #[test]
    fn test_dirty_springs_tracked_through_lifecycle() {
        // The fast-path Phase 4 consumer drains this set every frame
        // to decide which `GpuPrimitive`s need their transform / opacity
        // patched in place. Lifecycle: register → set_target → tick
        // (each step adds the id) → drain clears → next tick after
        // settle stops adding.
        let scheduler = AnimationScheduler::new();
        let handle = scheduler.handle();

        // Fresh registration is implicitly dirty (initial value just
        // appeared; downstream hasn't seen it yet).
        let spring = Spring::new(SpringConfig::stiff(), 0.0);
        let id = handle.register_spring(spring).unwrap();
        assert!(
            handle.take_dirty_springs().contains(&id),
            "register_spring should mark the new spring dirty"
        );
        assert_eq!(
            handle.dirty_spring_count(),
            0,
            "take_dirty_springs should have drained"
        );

        // set_spring_target marks dirty even before any tick — the
        // user requested a state change, fast path needs to repaint.
        handle.set_spring_target(id, 100.0);
        assert!(handle.take_dirty_springs().contains(&id));

        // Ticks that move the spring mark it dirty. We may need a few
        // iterations: `register_spring` resets `last_frame=now`, so
        // the very first post-register tick has dt≈0 and may not
        // cross `DIRTY_EPSILON`. Drive a few µs of real time between
        // ticks and accumulate.
        let mut saw_dirty = false;
        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(2));
            scheduler.tick();
            if handle.take_dirty_springs().contains(&id) {
                saw_dirty = true;
                break;
            }
        }
        assert!(saw_dirty, "a moving tick should mark the spring dirty");

        // Removing the spring drops the dirty entry — a stale id
        // must never make it into a downstream drain (slotmap
        // reuses the slot). We rely on the next `set_spring_target`
        // to re-arm; that flow is the one the runtime exercises.
        let _ = handle.take_dirty_springs();
        handle.set_spring_target(id, 200.0);
        assert!(handle.dirty_spring_count() > 0);
        handle.remove_spring(id);
        assert_eq!(
            handle.dirty_spring_count(),
            0,
            "remove_spring should drop the dirty entry"
        );
    }

    #[test]
    fn test_animated_value() {
        let scheduler = AnimationScheduler::new();
        let handle = scheduler.handle();

        let mut value = AnimatedValue::new(handle, 0.0, SpringConfig::stiff());

        assert_eq!(value.get(), 0.0);
        assert!(!value.is_animating());

        // Set target
        value.set_target(100.0);
        assert!(value.is_animating());

        // Tick scheduler
        scheduler.tick();

        // Value should have moved
        assert!(value.get() > 0.0);
    }

    #[test]
    fn test_animated_keyframe() {
        let scheduler = AnimationScheduler::new();
        let handle = scheduler.handle();

        let mut anim = AnimatedKeyframe::new(handle, 1000)
            .keyframe(0.0, 0.0, Easing::Linear)
            .keyframe(1.0, 100.0, Easing::Linear);

        // Start animation
        anim.start();
        assert!(anim.is_playing());

        // Initial value should be 0
        assert_eq!(anim.get(), 0.0);

        // Tick scheduler (simulates time passing)
        scheduler.tick();

        // Animation should still be playing
        assert!(anim.is_playing());
    }

    #[test]
    fn test_animated_timeline() {
        let scheduler = AnimationScheduler::new();
        let handle = scheduler.handle();

        let mut timeline = AnimatedTimeline::new(handle);

        // Add an animation
        let entry = timeline.add(0, 1000, 0.0, 100.0);

        // Start timeline
        timeline.start();
        assert!(timeline.is_playing());

        // Initial value should be 0
        assert_eq!(timeline.get(entry), Some(0.0));

        // Tick scheduler
        scheduler.tick();

        // Timeline should still be playing
        assert!(timeline.is_playing());
    }

    #[test]
    fn test_handle_weak_reference() {
        let handle = {
            let scheduler = AnimationScheduler::new();
            scheduler.handle()
        };

        // Scheduler is dropped, handle should not be alive
        assert!(!handle.is_alive());

        // Operations should safely no-op
        assert!(
            handle
                .register_spring(Spring::new(SpringConfig::stiff(), 0.0))
                .is_none()
        );
    }

    #[test]
    fn test_scheduler_counts() {
        let scheduler = AnimationScheduler::new();

        assert_eq!(scheduler.spring_count(), 0);
        assert_eq!(scheduler.keyframe_count(), 0);
        assert_eq!(scheduler.timeline_count(), 0);

        // Add animations
        let spring = Spring::new(SpringConfig::stiff(), 0.0);
        scheduler.add_spring(spring);

        let mut keyframe = KeyframeAnimation::new(
            1000,
            vec![Keyframe {
                time: 0.0,
                value: 0.0,
                easing: Easing::Linear,
            }],
        );
        keyframe.start();
        scheduler.add_keyframe(keyframe);

        let mut timeline = Timeline::new();
        timeline.add(0, 1000, 0.0, 100.0);
        timeline.start();
        scheduler.add_timeline(timeline);

        assert_eq!(scheduler.spring_count(), 1);
        assert_eq!(scheduler.keyframe_count(), 1);
        assert_eq!(scheduler.timeline_count(), 1);
    }

    #[test]
    fn dt_smoothing_damps_outliers() {
        // A 25 ms spike at 16.67 ms steady state must be partially
        // smoothed (not passed through raw) but not over-smoothed
        // either.
        let mut prev = Some(0.01667);
        let smoothed = smooth_dt(0.025, &mut prev);
        assert!(
            smoothed < 0.025 && smoothed > 0.01667,
            "outlier should be damped toward the running average, got {smoothed}"
        );
        // EMA weight: 0.6·0.025 + 0.4·0.01667 = 0.0217 ≈ 21.7 ms
        assert!((smoothed - 0.02167).abs() < 1e-4);
    }

    #[test]
    fn dt_smoothing_first_tick_passes_through() {
        // No history → no smoothing, just record.
        let mut prev = None;
        let smoothed = smooth_dt(0.0167, &mut prev);
        assert_eq!(smoothed, 0.0167);
        assert_eq!(prev, Some(0.0167));
    }

    #[test]
    fn dt_smoothing_tracks_wall_clock() {
        // EMA must not drift more than ~1 % from true elapsed time
        // over a long run, otherwise spring animations would
        // visibly fall behind wall clock. Simulate 600 ticks (10 s
        // at 60 Hz) with a noisy stream and check the integrated
        // smoothed dt is within 1 % of the raw sum.
        let mut prev: Option<f32> = None;
        let raw_dts: Vec<f32> = (0..600)
            .map(|i| {
                // Jittery 16.67 ms target: spike every 7 ticks to 24
                // ms, otherwise normal jitter ±1 ms.
                if i % 7 == 0 {
                    0.024
                } else if i % 2 == 0 {
                    0.0157
                } else {
                    0.0176
                }
            })
            .collect();
        let raw_sum: f32 = raw_dts.iter().sum();
        let smoothed_sum: f32 = raw_dts.iter().map(|&raw| smooth_dt(raw, &mut prev)).sum();
        let drift = (smoothed_sum - raw_sum).abs() / raw_sum;
        assert!(
            drift < 0.01,
            "smoothed integration drifted from wall clock by {:.3}% (raw={raw_sum}, smoothed={smoothed_sum})",
            drift * 100.0
        );
    }
}
