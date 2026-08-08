//! `OverlayStack` — LIFO stack of dismissable overlays.
//!
//! Replaces the legacy `widgets::overlay::OverlayManagerInner`. Animation
//! lifecycle is delegated entirely to `motion()` / `animate_bounds()` / CSS
//! `@keyframes` — the stack owns ordering, dismissal rules, input dispatch,
//! and lifetime only.
//!
//! See `OVERLAY_STACK_DESIGN.md` at the repo root for the full design rationale.
//!
//! # Lifecycle
//!
//! ```text
//!  push() ────► entry in stack, motion key wired
//!                │
//!                │ user / programmatic / auto-timeout
//!                ▼
//!  pop()  ────► exiting = true, query_motion(key).exit()
//!                │
//!                │ next update() polls motion state
//!                ▼
//!  update() ──► motion reaches Removed/NotFound → evict from Vec
//! ```
//!
//! The "exiting" flag exists *only* so the stack can keep an entry rendered
//! while its motion plays the exit animation. It is NOT an animation FSM —
//! the motion system already has one and this stack just observes it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use blinc_animation::AnimationPreset;
use blinc_core::BlincContextState;
use blinc_core::context_state::MotionAnimationState;

use crate::div::Div;
use crate::motion::ElementAnimation;
use crate::widgets::overlay::{
    AnchorDirection, BackdropConfig, Corner, EdgeSide, OverlayKind, OverlayPosition,
};

/// Stable element id for the layer produced by `OverlayStack::build_overlay_layer()`.
/// The windowed runner registers this in the element registry on first mount so
/// `push()` / `close()` can queue a subtree rebuild of just this layer (not the
/// whole UI) when the stack content changes. Mirrors the legacy
/// `widgets::overlay::OVERLAY_LAYER_ID` pattern.
pub const OVERLAY_STACK_LAYER_ID: &str = "__cn_overlay_stack_layer__";

// =============================================================================
// OverlayHandle
// =============================================================================

/// Stable id for an overlay entry. Returned by `push()` / `OverlayBuilder::show()`.
///
/// The handle remains valid across rebuilds while the entry is alive (live or
/// exiting). Once the entry is evicted from the stack, the handle is dead —
/// `is_live()` / `is_exiting()` return `false` and `close()` is a no-op.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OverlayHandle(u64);

impl OverlayHandle {
    /// The raw u64 id. Cheap, stable identity for use as map key.
    pub fn raw(&self) -> u64 {
        self.0
    }

    /// Reconstruct from a raw id. Used when an id needs to round-trip through
    /// a `State<Option<u64>>` (the State system stores values by `Hash`, and
    /// `OverlayHandle` itself isn't a State-friendly type).
    pub fn from_raw(id: u64) -> Self {
        OverlayHandle(id)
    }
}

// =============================================================================
// DismissRules
// =============================================================================

/// Per-entry behavioural contract. Built from `OverlayKind`'s default + builder
/// overrides. Read by `handle_escape`, `handle_click_at`, `handle_mouse_leave`,
/// and `update` (for auto-dismiss timers).
#[derive(Clone, Debug)]
pub struct DismissRules {
    /// ESC key pops this entry.
    pub on_escape: bool,
    /// Click outside the entry's hit region pops it.
    pub on_click_outside: bool,
    /// Mouse leaving this entry / its anchor pops it after `mouse_leave_delay_ms`.
    pub on_mouse_leave: bool,
    /// Grace period after a `mouse_leave` event before the entry actually pops.
    /// `mouse_enter` during this window cancels the pending close.
    pub mouse_leave_delay_ms: u32,
    /// Auto-dismiss after this many milliseconds. `None` = sticky.
    pub auto_after_ms: Option<u64>,
    /// Blocks input from reaching layers below this entry.
    pub blocks_below: bool,
    /// Backdrop config; `None` means no backdrop element.
    pub backdrop: Option<BackdropConfig>,
}

impl DismissRules {
    /// Sensible defaults per overlay kind. Overrides via the builder.
    pub fn default_for(kind: OverlayKind) -> Self {
        match kind {
            OverlayKind::Modal | OverlayKind::Dialog => Self {
                on_escape: true,
                on_click_outside: true,
                on_mouse_leave: false,
                mouse_leave_delay_ms: 0,
                auto_after_ms: None,
                blocks_below: true,
                backdrop: Some(BackdropConfig::default()),
            },
            OverlayKind::Dropdown | OverlayKind::ContextMenu => Self {
                on_escape: true,
                on_click_outside: true,
                on_mouse_leave: false,
                mouse_leave_delay_ms: 0,
                auto_after_ms: None,
                blocks_below: false,
                backdrop: None,
            },
            OverlayKind::Toast => Self {
                on_escape: false,
                on_click_outside: false,
                on_mouse_leave: false,
                mouse_leave_delay_ms: 0,
                auto_after_ms: Some(4_000),
                blocks_below: false,
                backdrop: None,
            },
            OverlayKind::Tooltip => Self {
                on_escape: false,
                on_click_outside: false,
                on_mouse_leave: true,
                mouse_leave_delay_ms: 0,
                auto_after_ms: None,
                blocks_below: false,
                backdrop: None,
            },
        }
    }
}

// =============================================================================
// CloseReason
// =============================================================================

/// Reason an entry closed. Passed to `on_close` callbacks so user code can
/// branch on the cause (e.g. only persist form state on `Programmatic`, not on
/// accidental `ClickOutside`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseReason {
    /// Explicit `handle.close()` / `stack.close(handle)` call.
    Programmatic,
    /// ESC key.
    Escape,
    /// Click landed outside this entry's hit region.
    ClickOutside,
    /// Mouse left this entry + its trigger and the close-delay elapsed.
    MouseLeave,
    /// `auto_after_ms` timer elapsed (toasts).
    AutoTimeout,
    /// A lower entry was closed; this entry was stacked above and was closed
    /// atomically as part of the unwind.
    UnwindFromBelow,
}

// =============================================================================
// OverlayEntry
// =============================================================================

/// One overlay in the stack.
///
/// `motion_key` is the stable string handed to `motion()` when the renderer
/// wraps `content_fn()`. Whatever enter / exit animation the widget configured
/// runs through that key — the stack itself does no animation interpolation.
pub struct OverlayEntry {
    pub handle: OverlayHandle,
    pub kind: OverlayKind,
    /// Layout / positioning config — see `OverlayPosition` / `AnchorDirection`.
    pub position: OverlayPosition,
    pub anchor_direction: AnchorDirection,
    /// Explicit size override (None = content-sized).
    pub size: Option<(f32, f32)>,
    pub dismiss: DismissRules,
    pub content_fn: Arc<dyn Fn() -> Div + Send + Sync>,
    pub motion_key: String,
    /// Enter animation for the wrapping motion container. `None` lets the
    /// motion's default-no-animation behaviour apply (content snaps in).
    pub motion_enter: Option<ElementAnimation>,
    /// Exit animation. `None` means the exit transitions instantly — the
    /// stack reaps the entry on the next `update()` because
    /// `query_motion(key).is_exited()` is true immediately.
    pub motion_exit: Option<ElementAnimation>,
    pub spawned_at_ms: u64,
    /// True once `pop()` / `close()` / auto-dismiss fired. Renderer keeps
    /// painting the entry so its motion exit animation can play; the entry
    /// is excluded from input dispatch and evicted on the next `update()`
    /// after `query_motion(motion_key)` reports `Removed` / `NotFound`.
    pub exiting: bool,
    /// Mouse-leave countdown deadline (`spawned_at_ms`-style absolute time).
    /// `None` when not in the pending-close window.
    pub pending_close_deadline_ms: Option<u64>,
    pub on_close: Option<Arc<dyn Fn(CloseReason) + Send + Sync>>,
}

impl OverlayEntry {
    /// The FSM stable key for this entry's motion. `motion_derived(parent_key)`
    /// internally prefixes the key with `"motion:"`, so EVERY query / queue
    /// against the motion store must use this prefixed form. Forgetting the
    /// prefix means `query_motion` returns `NotFound` (motion is opaque), and
    /// `queue_global_motion_exit_start` is dropped (motion can't be exited).
    ///
    /// `:child:0` suffix: `collect_render_props_boxed` appends `":child:{idx}"`
    /// to the Motion container's stable id when propagating motion config to
    /// its children (the actual content the FSM animates). We wrap the
    /// overlay content with `motion_derived(...).child(content)` — exactly one
    /// child, index 0 — so the registered motion ends up at this child key.
    /// Without the suffix, `query_motion` would look up the parent (empty)
    /// key, get `NotFound`, and `motion_done()` would report `true` on the
    /// first `update()` after close — reaping the entry before the exit
    /// animation has a chance to play.
    fn motion_stable_key(&self) -> String {
        format!("motion:{}:child:0", self.motion_key)
    }

    /// Backdrop motion's FSM stable key. Only meaningful when the entry
    /// renders a backdrop (`dismiss.backdrop.is_some()`); otherwise no such
    /// motion is registered. Same `:child:0` suffix logic as
    /// `motion_stable_key` — `build_overlay_layer` creates the backdrop via
    /// `motion_derived("{motion_key}:backdrop").child(backdrop_div)`, one
    /// child at index 0.
    fn backdrop_motion_stable_key(&self) -> Option<String> {
        self.dismiss
            .backdrop
            .as_ref()
            .map(|_| format!("motion:{}:backdrop:child:0", self.motion_key))
    }

    /// Returns true if the motion FSM has finished its exit (or was never
    /// registered, i.e. the widget skipped the motion wrapper).
    fn motion_done(&self) -> bool {
        let state = BlincContextState::try_get()
            .map(|ctx| ctx.query_motion(&self.motion_stable_key()))
            .unwrap_or(MotionAnimationState::NotFound);
        matches!(
            state,
            MotionAnimationState::Removed | MotionAnimationState::NotFound
        )
    }

    /// Trigger the motion's exit animation via the queued global mechanism.
    /// Also queues the backdrop's exit so it fades out in lockstep with the
    /// content; without this the dim layer would stay at full opacity until
    /// the entry is reaped, then snap to gone.
    fn queue_motion_exit(&self) {
        crate::queue_global_motion_exit_start(self.motion_stable_key());
        if let Some(backdrop_key) = self.backdrop_motion_stable_key() {
            crate::queue_global_motion_exit_start(backdrop_key);
        }
    }

    /// Interrupt the motion's exit animation (mouse re-entry during the
    /// close-delay window). Cancels both the content's and backdrop's exit
    /// so they revive in sync.
    fn queue_motion_exit_cancel(&self) {
        crate::queue_global_motion_exit_cancel(self.motion_stable_key());
        if let Some(backdrop_key) = self.backdrop_motion_stable_key() {
            crate::queue_global_motion_exit_cancel(backdrop_key);
        }
    }

    /// Invoke `on_close` with the given reason if set. Idempotent — caller
    /// must guard against double-fire (this method always fires).
    fn fire_on_close(&self, reason: CloseReason) {
        if let Some(cb) = &self.on_close {
            cb(reason);
        }
    }
}

// =============================================================================
// OverlayStack
// =============================================================================

/// LIFO stack of dismissable overlays.
///
/// `entries.last()` is the top. Push appends; pop removes from the end (after
/// the exit animation completes). Mid-stack removals (via `close(handle)`)
/// truncate everything from the target handle upward atomically.
pub struct OverlayStack {
    entries: Vec<OverlayEntry>,
    /// Close callbacks waiting to run outside this stack's mutex.
    ///
    /// Firing one under the guard hands control to user code that may
    /// come straight back — clearing a bound signal re-enters whatever
    /// drives the overlay, which locks the stack again. Drained by
    /// [`drain_close_callbacks`].
    pending_close_callbacks: Vec<(Arc<dyn Fn(CloseReason) + Send + Sync>, CloseReason)>,
    next_id: AtomicU64,
    viewport: (f32, f32),
    scale_factor: f32,
    current_time_ms: u64,
    /// Set when entries are added / removed (structural change). Consumed by
    /// the windowed runner to trigger a content rebuild.
    dirty: AtomicBool,
    /// Set when only animation state advanced (no structural delta). Consumed
    /// by the windowed runner to request a redraw without rebuilding.
    animation_dirty: AtomicBool,
    /// Wall-time deadline (in `current_time_ms` space) until which
    /// `has_animating_overlays` should report `true` regardless of the
    /// FSM-poll result. Asserted on push / close so the runner keeps
    /// scheduling frames for the entry's enter / exit motion to play out,
    /// even if the FSM hasn't registered the motion yet (first frame race)
    /// or has transitioned briefly through a Visible state between phases.
    /// Time-based defense against the FSM-poll race; no functional cost
    /// once past the deadline.
    redraw_until_ms: AtomicU64,
}

impl Default for OverlayStack {
    fn default() -> Self {
        Self::new()
    }
}

impl OverlayStack {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            pending_close_callbacks: Vec::new(),
            next_id: AtomicU64::new(1),
            viewport: (0.0, 0.0),
            scale_factor: 1.0,
            current_time_ms: 0,
            dirty: AtomicBool::new(false),
            animation_dirty: AtomicBool::new(false),
            redraw_until_ms: AtomicU64::new(0),
        }
    }

    /// Extend the "force redraw" deadline by `duration_ms` from the current
    /// stack time. Capped at `now + duration_ms`; never shortens.
    fn extend_redraw_window(&self, duration_ms: u64) {
        let deadline = self.current_time_ms.saturating_add(duration_ms);
        // Monotonic: never roll the deadline backwards.
        let prev = self.redraw_until_ms.load(Ordering::Acquire);
        if deadline > prev {
            self.redraw_until_ms.store(deadline, Ordering::Release);
        }
    }

    // ----- Builder entry point -----

    /// Allocate a fresh handle. Used by the builder when constructing a new
    /// entry before `push()`.
    fn allocate_handle(&self) -> OverlayHandle {
        OverlayHandle(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Peek the id that `allocate_handle` will return on its next call. Used
    /// by widgets that need to compute a derived id (e.g. the popover's
    /// click-outside element id) BEFORE pushing the entry. The widget must
    /// `debug_assert_eq!(returned_handle.raw(), peeked_id)` after push to
    /// detect a stale peek caused by concurrent pushes from another thread.
    pub fn peek_next_handle_id(&self) -> u64 {
        self.next_id.load(Ordering::Relaxed)
    }

    /// Push an entry onto the top of the stack. Marks the stack dirty and
    /// asserts `animation_dirty` so the windowed runner's redraw chain stays
    /// alive long enough for the entry's enter motion to play out — without
    /// this, a static FSM-poll race could leave the loop sleeping mid-flight.
    pub fn push(&mut self, entry: OverlayEntry) -> OverlayHandle {
        let handle = entry.handle;
        let has_enter_motion = entry.motion_enter.is_some();
        tracing::trace!(
            target: "blinc_layout::overlay_stack",
            "push: handle={} kind={:?} motion_key={} stack_len={}->{}",
            handle.raw(),
            entry.kind,
            entry.motion_key,
            self.entries.len(),
            self.entries.len() + 1,
        );
        self.entries.push(entry);
        self.dirty.store(true, Ordering::Release);
        self.animation_dirty.store(true, Ordering::Release);
        // Cover the typical enter-motion duration so the redraw chain stays
        // alive even if the FSM-poll race briefly reports the motion as
        // settled (or NotFound) mid-flight. Only extended when this overlay
        // ACTUALLY carries an enter motion — for plain popovers (no
        // motion_enter), keeping the window open burned ~1.2 s of slow-
        // walker frames per open while `has_animating_overlays()` reported
        // animating purely off the time defence, even though nothing was
        // animating. The compositor fast path could then never re-engage
        // for the first second of every popover, and a focused cn::input
        // riding continuous_redraw saw a slow walker every vsync — visible
        // as a 1-second rapid "zoom" flicker on canvas-backed hosts.
        if has_enter_motion {
            self.extend_redraw_window(1_200);
        }
        // Wake the windowed runner. The Frame-event gate checks
        // `stateful::peek_needs_redraw()`, so without this flip a push that
        // happens between Frame events (typical from an `on_click`) would
        // wait until the next external input to actually paint.
        crate::stateful::request_redraw();
        handle
    }

    // ----- Inspect -----

    pub fn top(&self) -> Option<&OverlayEntry> {
        self.entries.last()
    }

    pub fn top_handle(&self) -> Option<OverlayHandle> {
        self.entries.last().map(|e| e.handle)
    }

    pub fn topmost_dismissable_by_escape(&self) -> Option<&OverlayEntry> {
        self.entries
            .iter()
            .rev()
            .find(|e| !e.exiting && e.dismiss.on_escape)
    }

    pub fn topmost_blocking(&self) -> Option<&OverlayEntry> {
        self.entries
            .iter()
            .rev()
            .find(|e| !e.exiting && e.dismiss.blocks_below)
    }

    /// Iterate entries top → bottom (last pushed first).
    pub fn iter_top_down(&self) -> impl Iterator<Item = &OverlayEntry> {
        self.entries.iter().rev()
    }

    /// Iterate entries bottom → top (insertion order).
    pub fn iter_bottom_up(&self) -> impl Iterator<Item = &OverlayEntry> {
        self.entries.iter()
    }

    pub fn contains(&self, handle: OverlayHandle) -> bool {
        self.entries.iter().any(|e| e.handle == handle)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    // ----- Closing -----

    /// Begin closing the entry with `handle` + everything stacked above it.
    /// Each affected entry gets `exiting = true` and its motion exit kicked.
    /// Lower entries get `UnwindFromBelow` on their `on_close`; the targeted
    /// entry gets `reason`.
    pub fn close_with_reason(&mut self, handle: OverlayHandle, reason: CloseReason) {
        let Some(idx) = self.entries.iter().position(|e| e.handle == handle) else {
            return;
        };
        // Everything ABOVE the target gets UnwindFromBelow.
        for i in (idx + 1)..self.entries.len() {
            self.begin_exit(i, CloseReason::UnwindFromBelow);
        }
        // The target gets the actual reason.
        self.begin_exit(idx, reason);
        self.dirty.store(true, Ordering::Release);
    }

    /// Begin closing the entry with `handle` + everything stacked above it.
    /// `reason` defaults to `Programmatic`.
    pub fn close(&mut self, handle: OverlayHandle) {
        self.close_with_reason(handle, CloseReason::Programmatic);
    }

    /// Pop the top entry. Returns the handle that was popped (or `None` if
    /// the stack was empty / only contained already-exiting entries).
    pub fn pop(&mut self) -> Option<OverlayHandle> {
        // Find the topmost not-yet-exiting entry.
        let idx = self.entries.iter().rposition(|e| !e.exiting)?;
        let handle = self.entries[idx].handle;
        self.close_with_reason(handle, CloseReason::Programmatic);
        Some(handle)
    }

    /// Close every entry in the stack. Each fires `UnwindFromBelow` (since we
    /// don't know which one the caller "meant"; programmatic mass-close is
    /// always an unwind from the user's perspective).
    pub fn close_all(&mut self) {
        for i in 0..self.entries.len() {
            self.begin_exit(i, CloseReason::UnwindFromBelow);
        }
        if !self.entries.is_empty() {
            self.dirty.store(true, Ordering::Release);
        }
    }

    /// Close every entry of the given kind. Used by widgets that enforce
    /// single-instance semantics (only one tooltip, only one toast in
    /// progress, etc.) — they call this before pushing a fresh entry to
    /// reap stragglers that escaped per-handle lifecycle tracking.
    pub fn close_all_of_kind(&mut self, kind: OverlayKind) {
        let mut closed_any = false;
        for i in 0..self.entries.len() {
            if self.entries[i].kind == kind && !self.entries[i].exiting {
                self.begin_exit(i, CloseReason::Programmatic);
                closed_any = true;
            }
        }
        if closed_any {
            self.dirty.store(true, Ordering::Release);
        }
    }

    /// Interrupt an in-flight exit. Used when a hover-driven close needs to be
    /// cancelled because the user re-entered the trigger / content. Clears the
    /// `exiting` flag, dismisses any pending motion exit, and clears any
    /// pending mouse-leave countdown. No-op if the entry is already alive or
    /// missing.
    ///
    /// Note: this does NOT un-fire `on_close` — if the close was driven by a
    /// timer that already invoked the callback, side effects (e.g. user-side
    /// `State<bool>` flips) have already happened. Widget authors who want to
    /// support clean revival should keep `on_close` side-effect-free and rely
    /// on `is_live()` / `is_exiting()` queries to coordinate state.
    pub fn revive(&mut self, handle: OverlayHandle) {
        let Some(entry) = self.entries.iter_mut().find(|e| e.handle == handle) else {
            return;
        };
        if !entry.exiting && entry.pending_close_deadline_ms.is_none() {
            return;
        }
        entry.exiting = false;
        entry.pending_close_deadline_ms = None;
        entry.queue_motion_exit_cancel();
        self.animation_dirty.store(true, Ordering::Release);
    }

    /// Internal helper. Idempotent — calling twice on the same idx is a no-op
    /// the second time (the entry is already `exiting`, exit motion already
    /// queued, on_close already fired).
    /// Take the close callbacks queued since the last take.
    ///
    /// The caller runs them once it has released this stack's guard.
    /// [`drain_close_callbacks`] does that for the global stack; a
    /// caller holding its own stack drains it directly.
    pub fn take_pending_close_callbacks(
        &mut self,
    ) -> Vec<(Arc<dyn Fn(CloseReason) + Send + Sync>, CloseReason)> {
        std::mem::take(&mut self.pending_close_callbacks)
    }

    fn begin_exit(&mut self, idx: usize, reason: CloseReason) {
        let entry = &mut self.entries[idx];
        if entry.exiting {
            return;
        }
        let has_exit_motion = entry.motion_exit.is_some();
        entry.exiting = true;
        entry.pending_close_deadline_ms = None;
        // Trigger the motion exit via the FSM-keyed queue. If the entry has
        // no motion wrapper (motion FSM was never registered for the key),
        // the queue is a no-op and the entry will be reaped on the next
        // `update()` tick via `motion_done() → true`.
        entry.queue_motion_exit();
        // QUEUED, not fired. `begin_exit` runs under the stack's own
        // mutex, and a callback is user code: a caller that clears a
        // bound signal from it lands back in whatever drives this
        // overlay, which locks the stack again and hangs the thread.
        // `std::sync::Mutex` is not re-entrant. Drained by
        // `drain_close_callbacks` once the guard is gone.
        if let Some(cb) = entry.on_close.clone() {
            self.pending_close_callbacks.push((cb, reason));
        }
        // Assert animation_dirty so the runner's redraw chain wakes up to
        // drive the exit motion — closing via ESC / programmatic close
        // without input would otherwise leave the loop sleeping and the
        // motion would only complete on the next mouse-move event.
        self.animation_dirty.store(true, Ordering::Release);
        // Same time-window defence as `push`: cover the typical exit motion.
        // Gated on the presence of an actual exit motion — see `push` for
        // why a blanket extension burns slow-walker frames on no-motion
        // overlays.
        if has_exit_motion {
            self.extend_redraw_window(800);
        }
        // Wake the runner — see `push` for the rationale.
        crate::stateful::request_redraw();
    }

    // ----- Input handlers -----

    /// ESC key: close the topmost entry whose `dismiss.on_escape` is true.
    /// Returns true if a close was initiated.
    pub fn handle_escape(&mut self) -> bool {
        let target = self
            .entries
            .iter()
            .rev()
            .find(|e| !e.exiting && e.dismiss.on_escape)
            .map(|e| e.handle);
        if let Some(h) = target {
            self.close_with_reason(h, CloseReason::Escape);
            true
        } else {
            false
        }
    }

    /// Click at `(x, y)` in viewport coordinates. Walks the stack top → bottom.
    /// - If the click is inside an entry's hit region, the entry "captures" the
    ///   click and the walk stops (returning true so the caller knows not to
    ///   propagate the event to widgets below).
    /// - If the click is outside an entry AND that entry's `on_click_outside`
    ///   is true, the entry closes and the walk continues (so a click in empty
    ///   space cascades through stacked menus).
    /// - If the click is outside an entry whose `on_click_outside` is false,
    ///   the walk stops (the entry "absorbs" the click without closing — e.g.
    ///   a sticky modal whose backdrop doesn't dismiss).
    /// - Returns true if any entry consumed the click.
    pub fn handle_click_at(
        &mut self,
        x: f32,
        y: f32,
        hit_test: &dyn Fn(&OverlayEntry, f32, f32) -> bool,
    ) -> bool {
        // Walk top-down without holding a borrow — collect handles first.
        let snapshot: Vec<(OverlayHandle, bool, bool)> = self
            .entries
            .iter()
            .rev()
            .filter(|e| !e.exiting)
            .map(|e| (e.handle, hit_test(e, x, y), e.dismiss.on_click_outside))
            .collect();

        let mut consumed = false;
        for (handle, hit, dismiss_on_outside) in snapshot {
            if hit {
                // Inside this entry — it absorbs the click. Stop walking.
                consumed = true;
                break;
            }
            if dismiss_on_outside {
                self.close_with_reason(handle, CloseReason::ClickOutside);
                consumed = true;
                // Continue: lower entries may also dismiss on the same click.
                continue;
            }
            // Outside this entry but it doesn't dismiss-on-outside — it
            // absorbs the click without closing (modal-style "guard").
            // If the entry blocks below, stop. Otherwise keep walking to let
            // it propagate down.
            if self
                .entries
                .iter()
                .any(|e| e.handle == handle && e.dismiss.blocks_below)
            {
                consumed = true;
                break;
            }
        }
        if consumed {
            // Even if no entry was structurally removed, the click was consumed;
            // animation_dirty so the redraw chain ticks.
            self.animation_dirty.store(true, Ordering::Release);
        }
        consumed
    }

    /// Mouse left the entry's hit region (or its trigger). Starts the
    /// close-delay countdown if `dismiss.on_mouse_leave`.
    pub fn handle_mouse_leave(&mut self, handle: OverlayHandle) {
        let now = self.current_time_ms;
        if let Some(entry) = self.entries.iter_mut().find(|e| e.handle == handle) {
            if entry.exiting || !entry.dismiss.on_mouse_leave {
                return;
            }
            let delay = entry.dismiss.mouse_leave_delay_ms as u64;
            entry.pending_close_deadline_ms = Some(now + delay);
            self.animation_dirty.store(true, Ordering::Release);
        }
    }

    /// Mouse re-entered the entry. Cancels any pending close.
    pub fn handle_mouse_enter(&mut self, handle: OverlayHandle) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.handle == handle) {
            entry.pending_close_deadline_ms = None;
        }
    }

    // ----- Per-frame tick -----

    /// Advance time. Two responsibilities (animation is NOT one of them):
    /// - Fire auto-dismiss timers (toasts).
    /// - Fire pending mouse-leave timers.
    /// - Reap exiting entries whose motion has finished.
    pub fn update(&mut self, current_time_ms: u64) {
        self.current_time_ms = current_time_ms;

        // 1. Auto-dismiss timers.
        let auto_close: Vec<OverlayHandle> = self
            .entries
            .iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| {
                let after = e.dismiss.auto_after_ms?;
                let due = e.spawned_at_ms.saturating_add(after);
                if current_time_ms >= due {
                    Some(e.handle)
                } else {
                    None
                }
            })
            .collect();
        for handle in auto_close {
            self.close_with_reason(handle, CloseReason::AutoTimeout);
        }

        // 2. Pending mouse-leave timers.
        let leave_close: Vec<OverlayHandle> = self
            .entries
            .iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| {
                let due = e.pending_close_deadline_ms?;
                if current_time_ms >= due {
                    Some(e.handle)
                } else {
                    None
                }
            })
            .collect();
        for handle in leave_close {
            self.close_with_reason(handle, CloseReason::MouseLeave);
        }

        // 3. Reap exiting entries whose motion has finished. We retain in
        // place so the index-stability of `entries` isn't disturbed mid-frame.
        let before_len = self.entries.len();
        self.entries.retain(|e| !(e.exiting && e.motion_done()));
        if self.entries.len() != before_len {
            self.dirty.store(true, Ordering::Release);
        }
    }

    // ----- Queries used by windowed runner -----

    /// Any live (non-exiting) blocking modal anywhere in the stack? Used to
    /// gate event routing to underlying widgets.
    pub fn has_blocking_overlay(&self) -> bool {
        self.entries
            .iter()
            .any(|e| !e.exiting && e.dismiss.blocks_below)
    }

    /// Any entry has a motion / animation currently mid-flight? Used by the
    /// windowed runner's redraw chain to keep frames coming during transitions.
    pub fn has_animating_overlays(&self) -> bool {
        // Time-window defence: a recent push / close extends `redraw_until_ms`
        // so the runner keeps scheduling frames even if the FSM-poll below
        // briefly returns NotFound / Visible mid-transition (race between the
        // shared-state sync, the renderer's motion tick, and the post-frame
        // signal check).
        if self.current_time_ms < self.redraw_until_ms.load(Ordering::Acquire) {
            return true;
        }
        let Some(ctx) = BlincContextState::try_get() else {
            return false;
        };
        self.entries.iter().any(|e| {
            // MUST query via the FSM-prefixed key — see `motion_stable_key`.
            let state = ctx.query_motion(&e.motion_stable_key());
            state.is_animating()
        })
    }

    pub fn has_visible_overlays(&self) -> bool {
        !self.entries.is_empty()
    }

    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Acquire)
    }

    pub fn take_animation_dirty(&self) -> bool {
        self.animation_dirty.swap(false, Ordering::AcqRel)
    }

    // ----- Viewport / scale -----

    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport = (width, height);
    }

    pub fn set_viewport_with_scale(&mut self, width: f32, height: f32, scale_factor: f32) {
        self.viewport = (width, height);
        self.scale_factor = scale_factor;
    }

    pub fn viewport(&self) -> (f32, f32) {
        self.viewport
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    pub fn current_time_ms(&self) -> u64 {
        self.current_time_ms
    }

    // ----- Render -----

    /// Build the overlay render layer. Returns a single `Div` containing every
    /// entry's content (and backdrop, if any) wrapped in `motion_derived(motion_key)`.
    ///
    /// Entries render in bottom-up stack order; each entry's backdrop emits as
    /// a sibling immediately before its content so a fade-out on the backdrop
    /// doesn't unmount the content above.
    ///
    /// Positioning per `OverlayPosition`:
    /// - `Centered` → centered in viewport via flex centering.
    /// - `AtPoint { x, y }` → absolute(x, y).
    /// - `Corner(_)` / `Edge(_)` → handled minimally for Phase 2; widget
    ///   migration in Phase 3 refines with proper anchoring.
    /// - `RelativeToAnchor { … }` → consults `LayoutTree` for the anchor's
    ///   resolved bounds; for Phase 2 this is a coarse pass-through (widget
    ///   layer measures + supplies absolute coords).
    ///
    /// Toasts are NOT included here — `ToastTray::build_tray_layer()` owns
    /// that surface and `blinc_app` composites the two.
    pub fn build_overlay_layer(&self) -> Div {
        use crate::motion::motion_derived;

        tracing::trace!(
            target: "blinc_layout::overlay_stack",
            "build_overlay_layer: entries={} viewport={:?}",
            self.entries.len(),
            self.viewport,
        );

        // Match the legacy `OverlayManagerInner::build_overlay_layer` shape:
        // - Zero-sized container when no entries → does NOT blanket the
        //   viewport and steal scroll / hover events from the main UI.
        // - `stack_layer()` so the overlay renders above the main UI via
        //   z-layer increment (interleaved rendering picks this up).
        // - `pointer_events_none()` so the container itself never absorbs
        //   events; backdrops + content children re-enable as needed.
        let has_visible = !self.entries.is_empty();
        let (layer_w, layer_h) = if has_visible && self.viewport.0 > 0.0 && self.viewport.1 > 0.0 {
            self.viewport
        } else {
            (0.0, 0.0)
        };

        let mut layer = Div::new()
            .id(OVERLAY_STACK_LAYER_ID)
            .absolute()
            .top(0.0)
            .left(0.0)
            .w(layer_w)
            .h(layer_h)
            .stack_layer()
            // Route the entire overlay-stack subtree's SDF + text + SVG
            // into the dynamic batch so it paints in composite_frame's
            // overlay pass — after the static cache + static-SVG
            // dispatch is blitted. Otherwise a sibling button's static
            // chevron-down SVG at z=0 paints on top of the overlay
            // panel's z=1 bg primitive (the SVG dispatch is one batched
            // draw at the end of the cache pass).
            .overlay_root()
            .pointer_events_none();

        if !has_visible {
            return layer;
        }

        tracing::trace!(
            target: "blinc_layout::overlay_stack",
            "build_overlay_layer: rendering {} entries at viewport {:?}",
            self.entries.len(),
            self.viewport,
        );

        for entry in self.entries.iter() {
            // Backdrop — emit before content so the click-target is below.
            // Visual fades / opacity are owned by the backdrop's own motion
            // wrapper (keyed `{motion_key}:backdrop`).
            if let Some(ref backdrop) = entry.dismiss.backdrop {
                let backdrop_key = format!("{}:backdrop", entry.motion_key);
                let mut backdrop_div = Div::new()
                    .absolute()
                    .top(0.0)
                    .left(0.0)
                    .w(self.viewport.0)
                    .h(self.viewport.1)
                    .bg(backdrop.color);
                // Backdrop click-to-dismiss. Fires OUTSIDE any held stack lock
                // (the input pipeline dispatches events after release), so
                // `handle.close()` (which re-acquires the lock) is safe here.
                if backdrop.dismiss_on_click {
                    let handle = entry.handle;
                    backdrop_div = backdrop_div.on_click(move |_| {
                        use crate::overlay_state::overlay_stack;
                        if let Ok(mut stack) = overlay_stack().lock() {
                            stack.close_with_reason(handle, CloseReason::ClickOutside);
                        }
                    });
                }
                // Match the backdrop's fade timing to the entry's own enter /
                // exit motion so the chrome (dim layer) and content (dialog /
                // sheet / drawer panel) start and finish their animations
                // together. Without these, the backdrop pops in and out
                // instantly while the content animates — visually jarring.
                let backdrop_enter_ms = entry
                    .motion_enter
                    .as_ref()
                    .map(|e| e.animation.duration_ms())
                    .unwrap_or(200);
                let backdrop_exit_ms = entry
                    .motion_exit
                    .as_ref()
                    .map(|e| e.animation.duration_ms())
                    .unwrap_or(150);
                let backdrop_motion = motion_derived(&backdrop_key)
                    .enter_animation(AnimationPreset::fade_in(backdrop_enter_ms))
                    .exit_animation(AnimationPreset::fade_out(backdrop_exit_ms));
                layer = layer.child(backdrop_motion.child(backdrop_div));
            }

            // Structure: positioned wrapper > motion > content_fn().
            //
            // The OUTER div carries absolute positioning so the overlay lands
            // at the viewport coords the widget chose. The motion lives
            // INSIDE that wrapper so its enter / exit animations operate on
            // the in-flow content. If motion were the outer node, its child
            // would be `position: absolute` (out of flow) and motion couldn't
            // measure / transform it — opacity might still apply, but
            // translate / scale animations would silently no-op. This
            // matches the legacy `mgr.dropdown().content(...)` shape: a
            // positioned container with an inner motion wrapper.
            let content = (entry.content_fn)();
            let mut motion_wrapper = motion_derived(&entry.motion_key);
            // Position kinds where the wrapper must be content-sized (so flex
            // centering or corner anchoring positions the panel by its own
            // dimensions). Edge positions intentionally fill the perpendicular
            // axis, so they keep the default w:100% / flex_grow:1.
            match entry.position {
                OverlayPosition::Centered
                | OverlayPosition::AtPoint { .. }
                | OverlayPosition::Corner(_)
                | OverlayPosition::RelativeToAnchor { .. } => {
                    motion_wrapper = motion_wrapper.fit_content();
                }
                OverlayPosition::Edge(_) => {}
            }
            if let Some(ref enter) = entry.motion_enter {
                motion_wrapper = motion_wrapper.enter_animation(enter.clone());
            }
            if let Some(ref exit) = entry.motion_exit {
                motion_wrapper = motion_wrapper.exit_animation(exit.clone());
            }
            let positioned = position_wrapper(
                motion_wrapper.child(content),
                &entry.position,
                entry.size,
                self.viewport,
            );
            layer = layer.child(positioned);
        }

        layer
    }
}

/// Wrap a child element in a positioned outer Div according to the entry's
/// `OverlayPosition`. The child (typically `motion_derived(...).child(content_fn())`)
/// renders in normal flow inside the wrapper so motion animations work
/// correctly; the wrapper handles the absolute placement at viewport coords.
fn position_wrapper(
    child: impl crate::div::ElementBuilder + 'static,
    position: &OverlayPosition,
    size: Option<(f32, f32)>,
    viewport: (f32, f32),
) -> Div {
    let outer = Div::new().absolute();
    match position {
        OverlayPosition::Centered => outer
            .top(0.0)
            .left(0.0)
            .w(viewport.0)
            .h(viewport.1)
            .items_center()
            .justify_center()
            .child(child),
        OverlayPosition::AtPoint { x, y } => {
            // Viewport-aware placement: when the caller supplied a
            // `.size(w, h)` hint (or the OverlayKind has a sensible
            // default — see `default_size_estimate_for`) we know the
            // overlay's bounding box and can keep it inside the
            // window. Flip vertically if anchoring at the desired y
            // would overflow the bottom; shift left if it would
            // overflow the right. Without a size hint we fall back
            // to the raw `(x, y)` placement (legacy behaviour) — the
            // caller has opted out of clamping by omitting the size.
            //
            // Symptom motivating this: context_menu / popover / any
            // .at(x, y) overlay near the window edge would clip its
            // Done / OK / menu items off-screen, making the action
            // unreachable. The user-flagged "useless dialog" case.
            let margin: f32 = 8.0;
            match size {
                Some((w, _h)) => {
                    // Right edge — shift left so the panel right side
                    // sits at most `viewport.0 - margin`. The width hint
                    // tracks real width closely, so horizontal overflow is
                    // a genuine clip worth correcting.
                    let mut cx = *x;
                    if cx + w + margin > viewport.0 {
                        cx = (viewport.0 - w - margin).max(margin);
                    }
                    if cx < margin {
                        cx = margin;
                    }
                    // Vertical: the `.size` hint is a worst-case CAP, NOT
                    // the panel's real content-driven height. The old code
                    // flipped by that hint (`cy - h`), which hurled a small
                    // panel far above the trigger once the anchor scrolled
                    // into the lower viewport — the "dropdown teleports to
                    // the top" bug. Decide the flip on the anchor's ACTUAL
                    // remaining space below, and when we do flip, anchor the
                    // panel by its BOTTOM at the trigger row (grows upward,
                    // height-independent) so it stays adjacent whatever its
                    // real height is. Trade-off: a genuinely tall menu with
                    // only moderate space below may clip a little at the
                    // bottom rather than flip — far preferable to the
                    // teleport, and a menu truly at the bottom edge still
                    // flips so its top rows stay visible.
                    const MIN_BELOW: f32 = 96.0;
                    let open_upward = viewport.1 - *y < MIN_BELOW && *y - margin > MIN_BELOW;
                    if open_upward {
                        outer
                            .left(cx)
                            .bottom((viewport.1 - *y + margin).max(margin))
                            .child(child)
                    } else {
                        outer.left(cx).top((*y).max(margin)).child(child)
                    }
                }
                // No size hint → caller opted out of clamping; raw
                // screen-space placement.
                None => outer.left(*x).top(*y).child(child),
            }
        }
        OverlayPosition::Corner(c) => {
            // Anchor to a corner with a default 16px inset. Widgets can override.
            const INSET: f32 = 16.0;
            let outer = match c {
                Corner::TopLeft => outer.top(INSET).left(INSET),
                Corner::TopRight => outer.top(INSET).left(viewport.0 - INSET),
                Corner::BottomLeft => outer.top(viewport.1 - INSET).left(INSET),
                Corner::BottomRight => outer.top(viewport.1 - INSET).left(viewport.0 - INSET),
            };
            outer.child(child)
        }
        OverlayPosition::Edge(side) => {
            // Drawer / sheet placement. When the widget supplied `entry.size`,
            // pin the panel flush to that edge with the override size; the
            // perpendicular axis stretches to the viewport (full-height panel
            // sliding from L/R, full-width panel sliding from T/B). Without a
            // size override we fall back to viewport-sized which is rarely what
            // a widget wants — preferred call shape is `.size(w, h).edge(...)`.
            let (panel_w, panel_h) = size.unwrap_or(viewport);
            let outer = match side {
                EdgeSide::Left => outer.top(0.0).left(0.0).w(panel_w).h(viewport.1),
                EdgeSide::Right => outer
                    .top(0.0)
                    .left(viewport.0 - panel_w)
                    .w(panel_w)
                    .h(viewport.1),
                EdgeSide::Top => outer.top(0.0).left(0.0).w(viewport.0).h(panel_h),
                EdgeSide::Bottom => outer
                    .top(viewport.1 - panel_h)
                    .left(0.0)
                    .w(viewport.0)
                    .h(panel_h),
            };
            outer.child(child)
        }
        OverlayPosition::RelativeToAnchor {
            offset_x, offset_y, ..
        } => {
            // Phase 2: pass through as a relative offset. The cn widget layer
            // resolves the anchor's absolute bounds before pushing the entry,
            // so by the time we render the offset is already absolute.
            outer.left(*offset_x).top(*offset_y).child(child)
        }
    }
}

// =============================================================================
// OverlayBuilder
// =============================================================================

/// Fluent builder for pushing entries onto the global `overlay_stack()`.
///
/// One builder type replaces six in the legacy `widgets::overlay`
/// (`ModalBuilder` / `DialogBuilder` / `ContextMenuBuilder` / `ToastBuilder` /
/// `DropdownBuilder` / `hover_card`). Per-kind defaults come from
/// `OverlayKind::default_*` constructors; user code overrides any field via the
/// builder methods below.
pub struct OverlayBuilder {
    kind: OverlayKind,
    position: OverlayPosition,
    anchor_direction: AnchorDirection,
    size: Option<(f32, f32)>,
    dismiss: DismissRules,
    content_fn: Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    on_open: Option<Arc<dyn Fn() + Send + Sync>>,
    on_close: Option<Arc<dyn Fn(CloseReason) + Send + Sync>>,
    motion_key: Option<String>,
    motion_enter: Option<ElementAnimation>,
    motion_exit: Option<ElementAnimation>,
}

impl OverlayBuilder {
    /// Start a builder with kind-appropriate defaults. Prefer the kind-specific
    /// helpers (`OverlayBuilder::dialog()`, `popover()`, …) for terser call sites.
    pub fn new(kind: OverlayKind) -> Self {
        let position = match kind {
            OverlayKind::Modal | OverlayKind::Dialog => OverlayPosition::Centered,
            OverlayKind::Toast => OverlayPosition::Corner(Corner::TopRight),
            _ => OverlayPosition::AtPoint { x: 0.0, y: 0.0 },
        };
        Self {
            kind,
            position,
            anchor_direction: AnchorDirection::default(),
            size: None,
            dismiss: DismissRules::default_for(kind),
            content_fn: None,
            on_open: None,
            on_close: None,
            motion_key: None,
            motion_enter: None,
            motion_exit: None,
        }
    }

    pub fn modal() -> Self {
        Self::new(OverlayKind::Modal)
    }
    pub fn dialog() -> Self {
        Self::new(OverlayKind::Dialog)
    }
    pub fn dropdown() -> Self {
        Self::new(OverlayKind::Dropdown)
    }
    pub fn context_menu() -> Self {
        Self::new(OverlayKind::ContextMenu)
    }
    pub fn toast() -> Self {
        Self::new(OverlayKind::Toast)
    }
    pub fn tooltip() -> Self {
        Self::new(OverlayKind::Tooltip)
    }
    pub fn popover() -> Self {
        // Popover shares Dropdown's dismiss rules — anchored, click-outside
        // dismisses, no backdrop, ESC closes. The `OverlayKind::Dropdown`
        // tag is reused; widget CSS classes distinguish at the render layer.
        Self::new(OverlayKind::Dropdown)
    }

    // ----- Dismiss-rules overrides -----

    pub fn dismissable_by_escape(mut self, b: bool) -> Self {
        self.dismiss.on_escape = b;
        self
    }
    pub fn dismissable_by_click_outside(mut self, b: bool) -> Self {
        self.dismiss.on_click_outside = b;
        self
    }
    pub fn dismissable_by_mouse_leave(mut self, b: bool, delay_ms: u32) -> Self {
        self.dismiss.on_mouse_leave = b;
        self.dismiss.mouse_leave_delay_ms = delay_ms;
        self
    }
    pub fn auto_dismiss_after_ms(mut self, ms: u64) -> Self {
        self.dismiss.auto_after_ms = Some(ms);
        self
    }
    pub fn blocks_below(mut self, b: bool) -> Self {
        self.dismiss.blocks_below = b;
        self
    }
    pub fn with_backdrop(mut self, cfg: BackdropConfig) -> Self {
        self.dismiss.backdrop = Some(cfg);
        self
    }
    pub fn without_backdrop(mut self) -> Self {
        self.dismiss.backdrop = None;
        self
    }
    /// Sugar — sets both `dismissable_by_escape` and `dismissable_by_click_outside`.
    pub fn dismissable(self, b: bool) -> Self {
        self.dismissable_by_escape(b)
            .dismissable_by_click_outside(b)
    }

    // ----- Position / sizing -----

    pub fn at(mut self, x: f32, y: f32) -> Self {
        self.position = OverlayPosition::AtPoint { x, y };
        self
    }
    pub fn centered(mut self) -> Self {
        self.position = OverlayPosition::Centered;
        self
    }
    pub fn corner(mut self, c: Corner) -> Self {
        self.position = OverlayPosition::Corner(c);
        self
    }
    pub fn edge(mut self, side: EdgeSide) -> Self {
        self.position = OverlayPosition::Edge(side);
        self
    }
    pub fn anchor_direction(mut self, d: AnchorDirection) -> Self {
        self.anchor_direction = d;
        self
    }
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.size = Some((w, h));
        self
    }

    // ----- Content -----

    pub fn content<F: Fn() -> Div + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.content_fn = Some(Arc::new(f));
        self
    }

    // ----- Lifecycle -----

    pub fn on_open(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_open = Some(Arc::new(f));
        self
    }
    pub fn on_close(mut self, f: impl Fn(CloseReason) + Send + Sync + 'static) -> Self {
        self.on_close = Some(Arc::new(f));
        self
    }

    /// Override the motion key. Defaults to `cn-overlay:{handle.raw()}`.
    /// Override when you need to share a motion key across multiple entries
    /// (e.g. menubar's hover-switch wants one shared key so the new menu
    /// animates from the old menu's position).
    pub fn motion_key(mut self, key: impl Into<String>) -> Self {
        self.motion_key = Some(key.into());
        self
    }

    /// Configure the enter animation for the wrapping motion container.
    /// Typically `AnimationPreset::fade_in(ms)` or `scale_in(ms)`. `None` (the
    /// default) snaps in without animation.
    pub fn motion_enter(mut self, animation: impl Into<ElementAnimation>) -> Self {
        self.motion_enter = Some(animation.into());
        self
    }

    /// Configure the exit animation. The stack waits for this to complete
    /// before reaping the entry (observed via `query_motion(key).is_exited()`).
    /// `None` means instant eviction.
    pub fn motion_exit(mut self, animation: impl Into<ElementAnimation>) -> Self {
        self.motion_exit = Some(animation.into());
        self
    }

    /// Push onto the global stack. Returns the new handle. Fires `on_open`
    /// once if configured.
    ///
    /// Empty `content_fn` → no-op (returns a fresh handle but doesn't enqueue
    /// anything renderable). Most useful API path is to always call `.content()`.
    pub fn show(self) -> OverlayHandle {
        use crate::overlay_state::overlay_stack;

        let stack_arc = overlay_stack();
        let mut stack = stack_arc.lock().unwrap();
        let handle = stack.allocate_handle();
        let motion_key = self
            .motion_key
            .unwrap_or_else(|| format!("cn-overlay:{}", handle.raw()));
        let spawned_at_ms = stack.current_time_ms;

        let entry = OverlayEntry {
            handle,
            kind: self.kind,
            position: self.position,
            anchor_direction: self.anchor_direction,
            size: self.size,
            dismiss: self.dismiss,
            content_fn: self.content_fn.unwrap_or_else(|| Arc::new(Div::new)),
            motion_key,
            motion_enter: self.motion_enter,
            motion_exit: self.motion_exit,
            spawned_at_ms,
            exiting: false,
            pending_close_deadline_ms: None,
            on_close: self.on_close,
        };
        let h = stack.push(entry);
        drop(stack);
        if let Some(on_open) = self.on_open {
            on_open();
        }
        h
    }
}

// =============================================================================
// OverlayHandle convenience
// =============================================================================

impl OverlayHandle {
    /// Returns true if this handle's entry is in the stack and not exiting.
    pub fn is_live(&self) -> bool {
        use crate::overlay_state::overlay_stack;
        overlay_stack()
            .lock()
            .map(|s| s.entries.iter().any(|e| e.handle == *self && !e.exiting))
            .unwrap_or(false)
    }

    /// Returns true if this handle's entry is in the stack and exiting (motion
    /// playing out before eviction).
    pub fn is_exiting(&self) -> bool {
        use crate::overlay_state::overlay_stack;
        overlay_stack()
            .lock()
            .map(|s| s.entries.iter().any(|e| e.handle == *self && e.exiting))
            .unwrap_or(false)
    }

    /// Sugar — calls `overlay_stack().lock().close(self)`.
    pub fn close(&self) {
        use crate::overlay_state::overlay_stack;
        if let Ok(mut s) = overlay_stack().lock() {
            s.close(*self);
        }
        // Outside the guard: a close callback is user code and may come
        // straight back into the stack.
        drain_close_callbacks();
    }
}

/// Run the close callbacks queued since the last drain.
///
/// Separate from closing because `begin_exit` runs under the stack's
/// mutex and a callback is user code: clearing a bound signal from one
/// re-enters whatever drives the overlay, which locks the stack again
/// and hangs the thread. Called after every programmatic close and once
/// per frame by the runner, so an input-driven dismissal (backdrop,
/// Escape) reports at most a frame later.
pub fn drain_close_callbacks() {
    use crate::overlay_state::overlay_stack;
    let pending = match overlay_stack().lock() {
        Ok(mut s) => std::mem::take(&mut s.pending_close_callbacks),
        Err(_) => return,
    };
    for (cb, reason) in pending {
        cb(reason);
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_entry(stack: &OverlayStack, kind: OverlayKind) -> OverlayEntry {
        OverlayEntry {
            handle: stack.allocate_handle(),
            kind,
            position: OverlayPosition::default(),
            anchor_direction: AnchorDirection::default(),
            size: None,
            dismiss: DismissRules::default_for(kind),
            content_fn: Arc::new(Div::new),
            motion_key: format!(
                "test:{}:{}",
                kind as u8,
                stack.next_id.load(Ordering::Relaxed)
            ),
            motion_enter: None,
            motion_exit: None,
            spawned_at_ms: 0,
            exiting: false,
            pending_close_deadline_ms: None,
            on_close: None,
        }
    }

    #[test]
    fn push_pop_preserves_lifo_order() {
        let mut stack = OverlayStack::new();
        let a = stack.push(dummy_entry(&stack, OverlayKind::Modal));
        let b = stack.push(dummy_entry(&stack, OverlayKind::Dropdown));
        let c = stack.push(dummy_entry(&stack, OverlayKind::ContextMenu));

        assert_eq!(stack.len(), 3);
        assert_eq!(stack.top_handle(), Some(c));

        // pop() marks top as exiting (but doesn't remove until update)
        let popped = stack.pop().unwrap();
        assert_eq!(popped, c);
        // C is exiting, B is now the top non-exiting
        assert_eq!(
            stack.iter_top_down().find(|e| !e.exiting).map(|e| e.handle),
            Some(b)
        );

        // A is still the bottom-most (alive).
        assert!(stack.contains(a));
    }

    #[test]
    fn close_handle_unwinds_above() {
        let mut stack = OverlayStack::new();
        let a = stack.push(dummy_entry(&stack, OverlayKind::Modal));
        let _b = stack.push(dummy_entry(&stack, OverlayKind::Dropdown));
        let _c = stack.push(dummy_entry(&stack, OverlayKind::ContextMenu));

        stack.close(a);

        // All three should now be exiting.
        for entry in stack.iter_bottom_up() {
            assert!(
                entry.exiting,
                "expected entry {:?} to be exiting after close(a)",
                entry.handle
            );
        }
    }

    #[test]
    fn escape_walks_past_non_dismissable() {
        let mut stack = OverlayStack::new();
        // Toast at bottom: on_escape = false
        let toast = stack.push(dummy_entry(&stack, OverlayKind::Toast));
        // Modal in middle: on_escape = true — this is the target ESC should find
        let modal = stack.push(dummy_entry(&stack, OverlayKind::Modal));
        // Tooltip on top: on_escape = false — ESC walks past it
        let tooltip = stack.push(dummy_entry(&stack, OverlayKind::Tooltip));

        let handled = stack.handle_escape();
        assert!(handled);

        // ESC found the modal as the topmost dismissable. `close()` semantics
        // require that everything stacked above the target also unwinds, so
        // the tooltip closes too (as `UnwindFromBelow`). The toast — which is
        // BELOW the target — stays alive.
        let modal_entry = stack.iter_bottom_up().find(|e| e.handle == modal).unwrap();
        assert!(
            modal_entry.exiting,
            "modal should be exiting (target of ESC)"
        );

        let tooltip_entry = stack
            .iter_bottom_up()
            .find(|e| e.handle == tooltip)
            .unwrap();
        assert!(
            tooltip_entry.exiting,
            "tooltip should unwind because it was stacked above the modal"
        );

        let toast_entry = stack.iter_bottom_up().find(|e| e.handle == toast).unwrap();
        assert!(
            !toast_entry.exiting,
            "toast (below ESC target) must remain alive"
        );
    }

    #[test]
    fn escape_on_empty_returns_false() {
        let mut stack = OverlayStack::new();
        assert!(!stack.handle_escape());
    }

    #[test]
    fn escape_with_only_non_dismissable_returns_false() {
        let mut stack = OverlayStack::new();
        // Toast and tooltip both have on_escape = false.
        let _t = stack.push(dummy_entry(&stack, OverlayKind::Toast));
        let _tt = stack.push(dummy_entry(&stack, OverlayKind::Tooltip));

        assert!(!stack.handle_escape());
        // Nothing closed.
        for entry in stack.iter_bottom_up() {
            assert!(!entry.exiting);
        }
    }

    #[test]
    fn click_outside_cascades_through_dropdowns() {
        let mut stack = OverlayStack::new();
        let dropdown_a = stack.push(dummy_entry(&stack, OverlayKind::Dropdown));
        let dropdown_b = stack.push(dummy_entry(&stack, OverlayKind::Dropdown));

        // Hit test that says: every entry is outside the click.
        let hit_outside = |_: &OverlayEntry, _: f32, _: f32| -> bool { false };

        let consumed = stack.handle_click_at(0.0, 0.0, &hit_outside);
        assert!(consumed);

        // Both dropdowns should be exiting.
        let a = stack
            .iter_bottom_up()
            .find(|e| e.handle == dropdown_a)
            .unwrap();
        let b = stack
            .iter_bottom_up()
            .find(|e| e.handle == dropdown_b)
            .unwrap();
        assert!(a.exiting);
        assert!(b.exiting);
    }

    #[test]
    fn click_inside_top_absorbs_without_closing_lower() {
        let mut stack = OverlayStack::new();
        let dropdown_a = stack.push(dummy_entry(&stack, OverlayKind::Dropdown));
        let dropdown_b = stack.push(dummy_entry(&stack, OverlayKind::Dropdown));

        // Hit test that says: top entry contains the click, lower doesn't.
        let hit_top_only =
            move |entry: &OverlayEntry, _: f32, _: f32| -> bool { entry.handle == dropdown_b };

        let consumed = stack.handle_click_at(0.0, 0.0, &hit_top_only);
        assert!(consumed);

        // Neither entry should be exiting.
        let a = stack
            .iter_bottom_up()
            .find(|e| e.handle == dropdown_a)
            .unwrap();
        let b = stack
            .iter_bottom_up()
            .find(|e| e.handle == dropdown_b)
            .unwrap();
        assert!(!a.exiting);
        assert!(!b.exiting);
    }

    #[test]
    fn has_blocking_overlay_reflects_modal_presence() {
        let mut stack = OverlayStack::new();
        assert!(!stack.has_blocking_overlay());

        let _drop = stack.push(dummy_entry(&stack, OverlayKind::Dropdown));
        assert!(!stack.has_blocking_overlay()); // dropdown doesn't block

        let modal = stack.push(dummy_entry(&stack, OverlayKind::Modal));
        assert!(stack.has_blocking_overlay()); // modal blocks

        stack.close(modal);
        // Still exiting — has_blocking should now be false (exiting entries don't count).
        assert!(!stack.has_blocking_overlay());
    }

    #[test]
    fn auto_dismiss_fires_via_update() {
        let mut stack = OverlayStack::new();
        let mut entry = dummy_entry(&stack, OverlayKind::Toast);
        entry.spawned_at_ms = 1_000;
        // Default toast auto_after_ms = 4000, so due at 5000.
        let toast_handle = stack.push(entry);

        // Tick to 4999 — should not fire yet.
        stack.update(4_999);
        let t = stack
            .iter_bottom_up()
            .find(|e| e.handle == toast_handle)
            .unwrap();
        assert!(!t.exiting);

        // Tick to 5000 — auto-dismiss fires. In a test context there's no
        // motion system, so `motion_done()` returns true immediately (the
        // entry's motion_key is `NotFound`). The entry is therefore reaped
        // on this same `update()` call — eviction is the success criterion,
        // not the `exiting` flag.
        stack.update(5_000);
        assert!(
            !stack.contains(toast_handle),
            "toast should have been reaped on the same update() tick \
             that fired auto-dismiss (no motion system in tests = instant eviction)"
        );
    }

    #[test]
    fn close_fires_on_close_callback_with_right_reason() {
        use std::sync::Mutex;
        let received: Arc<Mutex<Vec<(OverlayHandle, CloseReason)>>> =
            Arc::new(Mutex::new(Vec::new()));

        let mut stack = OverlayStack::new();
        let r = received.clone();

        let mut entry = dummy_entry(&stack, OverlayKind::Modal);
        let handle = entry.handle;
        entry.on_close = Some(Arc::new(move |reason| {
            r.lock().unwrap().push((handle, reason));
        }));
        stack.push(entry);

        stack.handle_escape();
        // Queued under the guard, run outside it — see `begin_exit`.
        for (cb, reason) in stack.take_pending_close_callbacks() {
            cb(reason);
        }

        let log = received.lock().unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].1, CloseReason::Escape);
    }

    #[test]
    fn close_unwinds_with_distinct_reasons_per_entry() {
        use std::sync::Mutex;
        let received: Arc<Mutex<Vec<(OverlayHandle, CloseReason)>>> =
            Arc::new(Mutex::new(Vec::new()));

        let mut stack = OverlayStack::new();

        // Three entries; close the bottom programmatically. Bottom gets
        // Programmatic, the two above get UnwindFromBelow.
        let mut a = dummy_entry(&stack, OverlayKind::Modal);
        let a_handle = a.handle;
        let r = received.clone();
        a.on_close = Some(Arc::new(move |reason| {
            r.lock().unwrap().push((a_handle, reason));
        }));

        let mut b = dummy_entry(&stack, OverlayKind::Dropdown);
        let b_handle = b.handle;
        let r = received.clone();
        b.on_close = Some(Arc::new(move |reason| {
            r.lock().unwrap().push((b_handle, reason));
        }));

        let mut c = dummy_entry(&stack, OverlayKind::ContextMenu);
        let c_handle = c.handle;
        let r = received.clone();
        c.on_close = Some(Arc::new(move |reason| {
            r.lock().unwrap().push((c_handle, reason));
        }));

        stack.push(a);
        stack.push(b);
        stack.push(c);

        stack.close(a_handle);
        // Queued under the guard, run outside it — see `begin_exit`.
        for (cb, reason) in stack.take_pending_close_callbacks() {
            cb(reason);
        }

        let log = received.lock().unwrap();
        let by_handle: std::collections::HashMap<OverlayHandle, CloseReason> =
            log.iter().copied().collect();
        assert_eq!(by_handle.get(&a_handle), Some(&CloseReason::Programmatic));
        assert_eq!(
            by_handle.get(&b_handle),
            Some(&CloseReason::UnwindFromBelow)
        );
        assert_eq!(
            by_handle.get(&c_handle),
            Some(&CloseReason::UnwindFromBelow)
        );
    }

    #[test]
    fn double_close_is_idempotent() {
        let mut stack = OverlayStack::new();
        let h = stack.push(dummy_entry(&stack, OverlayKind::Modal));
        stack.close(h);
        // Second close should not re-fire callbacks (begin_exit guards on exiting).
        stack.close(h);
        let entry = stack.iter_bottom_up().find(|e| e.handle == h).unwrap();
        assert!(entry.exiting);
    }

    #[test]
    fn close_all_of_kind_targets_only_matching_kind() {
        let mut stack = OverlayStack::new();
        let dialog_a = stack.push(dummy_entry(&stack, OverlayKind::Dialog));
        let tooltip_b = stack.push(dummy_entry(&stack, OverlayKind::Tooltip));
        let tooltip_c = stack.push(dummy_entry(&stack, OverlayKind::Tooltip));
        let dialog_d = stack.push(dummy_entry(&stack, OverlayKind::Dialog));

        stack.close_all_of_kind(OverlayKind::Tooltip);

        let b = stack
            .iter_bottom_up()
            .find(|e| e.handle == tooltip_b)
            .unwrap();
        let c = stack
            .iter_bottom_up()
            .find(|e| e.handle == tooltip_c)
            .unwrap();
        assert!(b.exiting && c.exiting, "tooltips should be exiting");

        let a = stack
            .iter_bottom_up()
            .find(|e| e.handle == dialog_a)
            .unwrap();
        let d = stack
            .iter_bottom_up()
            .find(|e| e.handle == dialog_d)
            .unwrap();
        assert!(!a.exiting && !d.exiting, "dialogs should remain alive");
    }

    #[test]
    fn revive_clears_pending_close_and_exiting() {
        let mut stack = OverlayStack::new();
        // Use HoverCard-like settings: tooltip kind has on_mouse_leave = true.
        let h = stack.push(dummy_entry(&stack, OverlayKind::Tooltip));

        // 1. Simulate hover-leave: pending_close_deadline_ms gets set.
        stack.handle_mouse_leave(h);
        assert!(
            stack
                .iter_bottom_up()
                .find(|e| e.handle == h)
                .unwrap()
                .pending_close_deadline_ms
                .is_some()
        );

        // Revive should clear the pending close.
        stack.revive(h);
        assert!(
            stack
                .iter_bottom_up()
                .find(|e| e.handle == h)
                .unwrap()
                .pending_close_deadline_ms
                .is_none()
        );

        // 2. Now actually close it.
        stack.close(h);
        assert!(
            stack
                .iter_bottom_up()
                .find(|e| e.handle == h)
                .unwrap()
                .exiting
        );

        // Revive should clear the exiting flag.
        stack.revive(h);
        assert!(
            !stack
                .iter_bottom_up()
                .find(|e| e.handle == h)
                .unwrap()
                .exiting
        );
    }

    #[test]
    fn iter_orders_match_insertion() {
        let mut stack = OverlayStack::new();
        let a = stack.push(dummy_entry(&stack, OverlayKind::Modal));
        let b = stack.push(dummy_entry(&stack, OverlayKind::Dropdown));
        let c = stack.push(dummy_entry(&stack, OverlayKind::ContextMenu));

        let bu: Vec<_> = stack.iter_bottom_up().map(|e| e.handle).collect();
        assert_eq!(bu, vec![a, b, c]);

        let td: Vec<_> = stack.iter_top_down().map(|e| e.handle).collect();
        assert_eq!(td, vec![c, b, a]);
    }
}
