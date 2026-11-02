//! `ToastTray` — sibling type to `OverlayStack`.
//!
//! Notification queue, not a dismiss stack. Toasts are corner-stacked,
//! auto-dismiss after a duration, tap-to-dismiss, and don't participate in
//! input dispatch the way `OverlayStack` entries do. They render *above* the
//! overlay stack but in their own independent layer.
//!
//! The two types live in separate modules with separate singletons so they
//! can evolve independently. `blinc_app` composites the two layers when
//! building the final overlay layer.
//!
//! Animation, as with `OverlayStack`, is delegated to `motion()` / FLIP / CSS.
//! The tray observes `query_motion(toast.motion_key)` to know when an exiting
//! toast can be evicted; FLIP via `animate_bounds(::position().snappy())` on
//! the tray container handles smooth reorders when a toast in the middle
//! auto-dismisses.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::ElementAnimation;
use blinc_core::BlincContextState;
use blinc_core::context_state::MotionAnimationState;

use crate::div::Div;
use crate::widgets::overlay::Corner;

/// Stable id for the toast tray layer. See `OVERLAY_STACK_LAYER_ID` for the
/// subtree-rebuild rationale.
pub const TOAST_TRAY_LAYER_ID: &str = "__cn_toast_tray_layer__";

// =============================================================================
// ToastHandle
// =============================================================================

/// Stable id for a toast in the tray.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ToastHandle(u64);

impl ToastHandle {
    pub fn raw(&self) -> u64 {
        self.0
    }
}

// =============================================================================
// ToastEntry
// =============================================================================

pub struct ToastEntry {
    pub handle: ToastHandle,
    pub motion_key: String,
    pub spawned_at_ms: u64,
    /// Auto-dismiss after this many ms.
    pub auto_after_ms: u64,
    /// True once the dismiss has fired; rendered until motion exit completes.
    pub exiting: bool,
    /// Whether tapping the toast itself dismisses it.
    pub dismiss_on_click: bool,
    pub content_fn: Arc<dyn Fn() -> Div + Send + Sync>,
    pub on_close: Option<Arc<dyn Fn() + Send + Sync>>,
    pub motion_enter: Option<ElementAnimation>,
    pub motion_exit: Option<ElementAnimation>,
    /// Enter and exit run by CSS rules instead of `motion_enter` /
    /// `motion_exit`; see [`ToastBuilder::motion_class`].
    pub css_motion: Option<super::css_motion::CssMotion>,
}

impl ToastEntry {
    /// FSM stable key (with the "motion:" prefix that `motion_derived` adds
    /// internally, plus the `:child:0` suffix `collect_render_props_boxed`
    /// appends when propagating motion config to the Motion container's
    /// single child). See `OverlayEntry::motion_stable_key` for the full
    /// rationale.
    fn motion_stable_key(&self) -> String {
        format!("motion:{}:child:0", self.motion_key)
    }

    fn motion_done(&self) -> bool {
        let state = BlincContextState::try_get()
            .map(|ctx| ctx.query_motion(&self.motion_stable_key()))
            .unwrap_or(MotionAnimationState::NotFound);
        matches!(
            state,
            MotionAnimationState::Removed | MotionAnimationState::NotFound
        )
    }
}

// =============================================================================
// ToastTray
// =============================================================================

pub struct ToastTray {
    toasts: Vec<ToastEntry>,
    next_id: AtomicU64,
    corner: Corner,
    gap_px: f32,
    max_visible: usize,
    current_time_ms: u64,
    dirty: AtomicBool,
    /// Time-based force-redraw window — see `OverlayStack::redraw_until_ms`.
    redraw_until_ms: AtomicU64,
}

impl Default for ToastTray {
    fn default() -> Self {
        Self::new()
    }
}

impl ToastTray {
    pub fn new() -> Self {
        Self {
            toasts: Vec::new(),
            next_id: AtomicU64::new(1),
            corner: Corner::BottomRight,
            gap_px: 8.0,
            max_visible: 5,
            current_time_ms: 0,
            dirty: AtomicBool::new(false),
            redraw_until_ms: AtomicU64::new(0),
        }
    }

    fn extend_redraw_window(&self, duration_ms: u64) {
        let deadline = self.current_time_ms.saturating_add(duration_ms);
        let prev = self.redraw_until_ms.load(Ordering::Acquire);
        if deadline > prev {
            self.redraw_until_ms.store(deadline, Ordering::Release);
        }
    }

    fn allocate_handle(&self) -> ToastHandle {
        ToastHandle(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Peek the id that `allocate_handle` will return on its next call.
    /// Mirrors `OverlayStack::peek_next_handle_id`.
    pub fn peek_next_handle_id(&self) -> u64 {
        self.next_id.load(Ordering::Relaxed)
    }

    /// Push a new toast onto the tray.
    pub fn push(&mut self, entry: ToastEntry) -> ToastHandle {
        let handle = entry.handle;
        self.toasts.push(entry);
        self.dirty.store(true, Ordering::Release);
        // Cover the typical enter-motion duration — same defence as
        // `OverlayStack::push`.
        self.extend_redraw_window(1_200);
        // Wake the runner — see `OverlayStack::push`.
        crate::stateful::request_redraw();
        handle
    }

    /// Begin dismissing a specific toast. Toast stays rendered until its
    /// motion exit completes; eviction happens in `update()`.
    pub fn dismiss(&mut self, handle: ToastHandle) {
        let mut did_dismiss = false;
        if let Some(entry) = self.toasts.iter_mut().find(|e| e.handle == handle) {
            if entry.exiting {
                return;
            }
            entry.exiting = true;
            match &entry.css_motion {
                Some(css) => css.exit(),
                None => crate::queue_global_motion_exit_start(entry.motion_stable_key()),
            }
            if let Some(cb) = &entry.on_close {
                cb();
            }
            self.dirty.store(true, Ordering::Release);
            did_dismiss = true;
        }
        if did_dismiss {
            // Cover the exit-motion duration; matches `OverlayStack::begin_exit`.
            self.extend_redraw_window(800);
            crate::stateful::request_redraw();
        }
    }

    /// Dismiss every toast.
    pub fn dismiss_all(&mut self) {
        let handles: Vec<_> = self
            .toasts
            .iter()
            .filter(|e| !e.exiting)
            .map(|e| e.handle)
            .collect();
        for h in handles {
            self.dismiss(h);
        }
    }

    /// Per-frame tick. Auto-dismisses expired toasts; reaps exited ones.
    pub fn update(&mut self, current_time_ms: u64) {
        self.current_time_ms = current_time_ms;

        // 1. Auto-dismiss timers.
        let due: Vec<_> = self
            .toasts
            .iter()
            .filter(|e| !e.exiting)
            .filter(|e| {
                let due_at = e.spawned_at_ms.saturating_add(e.auto_after_ms);
                current_time_ms >= due_at
            })
            .map(|e| e.handle)
            .collect();
        for handle in due {
            self.dismiss(handle);
        }

        // 2. Reap exited toasts.
        let before = self.toasts.len();
        self.toasts.retain_mut(|e| {
            if !e.exiting {
                return true;
            }
            let done = match e.css_motion.as_mut() {
                Some(css) => css.exit_done(current_time_ms),
                None => e.motion_done(),
            };
            !done
        });
        if self.toasts.len() != before {
            self.dirty.store(true, Ordering::Release);
        }
    }

    /// Click at viewport coords. Returns true if a toast absorbed the click.
    pub fn handle_click_at(
        &mut self,
        x: f32,
        y: f32,
        hit_test: &dyn Fn(&ToastEntry, f32, f32) -> bool,
    ) -> bool {
        let dismissables: Vec<_> = self
            .toasts
            .iter()
            .rev()
            .filter(|e| !e.exiting && e.dismiss_on_click && hit_test(e, x, y))
            .map(|e| e.handle)
            .collect();
        if dismissables.is_empty() {
            return false;
        }
        for h in dismissables {
            self.dismiss(h);
        }
        true
    }

    // ----- Inspect -----

    pub fn iter(&self) -> impl Iterator<Item = &ToastEntry> {
        self.toasts.iter()
    }

    pub fn contains(&self, handle: ToastHandle) -> bool {
        self.toasts.iter().any(|e| e.handle == handle)
    }

    pub fn is_empty(&self) -> bool {
        self.toasts.is_empty()
    }

    pub fn len(&self) -> usize {
        self.toasts.len()
    }

    pub fn corner(&self) -> Corner {
        self.corner
    }

    pub fn gap_px(&self) -> f32 {
        self.gap_px
    }

    pub fn max_visible(&self) -> usize {
        self.max_visible
    }

    pub fn current_time_ms(&self) -> u64 {
        self.current_time_ms
    }

    pub fn has_animating(&self) -> bool {
        // Time-window defence — see `OverlayStack::has_animating_overlays`.
        if self.current_time_ms < self.redraw_until_ms.load(Ordering::Acquire) {
            return true;
        }
        let Some(ctx) = BlincContextState::try_get() else {
            return false;
        };
        self.toasts.iter().any(|e| match e.css_motion {
            // A CSS-run exit is under way until the toast is reaped.
            Some(_) => e.exiting,
            None => ctx.query_motion(&e.motion_stable_key()).is_animating(),
        })
    }

    // ----- Render -----

    /// Build the toast layer. Each toast is wrapped in
    /// `motion_derived(motion_key)` so widgets can configure their own
    /// enter / exit. Toasts stack at the configured corner with `gap_px`
    /// between them.
    ///
    /// The tray container uses `animate_bounds(::position().snappy())`
    /// (FLIP) to smoothly reorder toasts when one in the middle
    /// auto-dismisses, instead of leaving a hole.
    pub fn build_tray_layer(&self, viewport: (f32, f32)) -> Div {
        use crate::motion::motion_derived;

        // Zero-sized when empty so the tray container never blanket-absorbs
        // events from the main UI underneath. Matches OverlayStack pattern.
        if self.toasts.is_empty() || viewport.0 <= 0.0 || viewport.1 <= 0.0 {
            return Div::new()
                .id(TOAST_TRAY_LAYER_ID)
                .absolute()
                .top(0.0)
                .left(0.0)
                .w(0.0)
                .h(0.0)
                .stack_layer()
                .overlay_root()
                .pointer_events_none();
        }

        // Outer layer is full-viewport (matches OverlayStack pattern) and
        // serves as the positioned ancestor for per-toast wrappers. It
        // doesn't lay its own children out — each toast wrapper inside
        // carries explicit `top()` / `left()` insets computed from the
        // viewport and the stack index.
        //
        // Pre-fix we tried two flex-based approaches: (a) `bottom() +
        // right()` insets with `flex_col_reverse()` and auto width, and
        // (b) full-viewport tray with `flex_col() + justify_end() +
        // items_end()`. Both rendered the toasts at top-left in cn_demo —
        // taffy's resolution of corner-anchored absolute boxes via either
        // bare bottom/right insets or in-line flex alignment of in-flow
        // children inside an absolute parent didn't behave the way CSS
        // would suggest. Explicit `top()` / `left()` on each toast wrapper
        // sidesteps the issue: the position is deterministic in viewport
        // pixels.
        const INSET: f32 = 16.0;
        const TOAST_WIDTH: f32 = 360.0; // matches `cn::toast`'s `.w(360.0)`
        // Height is approximate (we don't know the real per-toast height
        // until layout completes). For first-cut stacking we assume a
        // typical 2-line title+description card. Toasts with action
        // buttons or `body()` content may overlap slightly; revisit when
        // a measured-height feedback pass exists.
        const ESTIMATED_TOAST_HEIGHT: f32 = 90.0;

        let mut layer = Div::new()
            .id(TOAST_TRAY_LAYER_ID)
            .absolute()
            .top(0.0)
            .left(0.0)
            .w(viewport.0)
            .h(viewport.1)
            // Render above the main UI via the renderer's z-layer
            // increment. Without this, toasts overlapping main-UI content
            // sometimes render *under* it (notably text glyphs / SVGs).
            .stack_layer()
            // Route the entire toast-tray subtree's SDF + text + SVG
            // into the dynamic batch so it paints in composite_frame's
            // overlay pass — after the static cache + static-SVG
            // dispatch is blitted, so toasts always sit on top.
            .overlay_root()
            // Layer is transparent / event-pass-through; each toast card
            // re-enables input as needed.
            .pointer_events_none();

        let toast_left = match self.corner {
            Corner::TopLeft | Corner::BottomLeft => INSET,
            Corner::TopRight | Corner::BottomRight => viewport.0 - INSET - TOAST_WIDTH,
        };
        let is_bottom_anchored = matches!(self.corner, Corner::BottomLeft | Corner::BottomRight);

        // For bottom-anchored corners the newest toast sits at the corner
        // (closest to the anchor), and older toasts pile away from it. We
        // reverse the slice so iteration order matches stack index 0 =
        // closest-to-corner.
        let visible: Vec<&ToastEntry> = self.toasts.iter().take(self.max_visible).collect();
        let stacked: Vec<&ToastEntry> = if is_bottom_anchored {
            visible.into_iter().rev().collect()
        } else {
            visible
        };

        for (idx, toast) in stacked.iter().enumerate() {
            let offset = INSET + idx as f32 * (ESTIMATED_TOAST_HEIGHT + self.gap_px);
            let toast_top = if is_bottom_anchored {
                viewport.1 - offset - ESTIMATED_TOAST_HEIGHT
            } else {
                offset
            };

            let content = (toast.content_fn)();
            if let Some(css) = &toast.css_motion {
                let positioned = Div::new()
                    .absolute()
                    .top(toast_top)
                    .left(toast_left)
                    .w(TOAST_WIDTH)
                    .child(css.element(Div::new().flex_col()).child(content));
                layer = layer.child(positioned);
                continue;
            }
            let mut motion_wrapper = motion_derived(&toast.motion_key).fit_content();
            if let Some(ref enter) = toast.motion_enter {
                motion_wrapper = motion_wrapper.enter_animation(enter.clone());
            }
            if let Some(ref exit) = toast.motion_exit {
                motion_wrapper = motion_wrapper.exit_animation(exit.clone());
            }

            let positioned = Div::new()
                .absolute()
                .top(toast_top)
                .left(toast_left)
                .w(TOAST_WIDTH)
                .child(motion_wrapper.child(content));

            layer = layer.child(positioned);
        }

        layer
    }

    // ----- Config -----

    pub fn set_corner(&mut self, c: Corner) {
        self.corner = c;
        self.dirty.store(true, Ordering::Release);
    }

    pub fn set_gap_px(&mut self, gap: f32) {
        self.gap_px = gap;
    }

    pub fn set_max_visible(&mut self, max: usize) {
        self.max_visible = max;
    }

    // ----- Dirty flags -----

    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Acquire)
    }
}

// =============================================================================
// Tests
// =============================================================================

// =============================================================================
// ToastBuilder
// =============================================================================

/// Fluent builder for pushing toasts to the global `toast_tray()`.
pub struct ToastBuilder {
    auto_after_ms: u64,
    dismiss_on_click: bool,
    corner: Option<Corner>,
    content_fn: Option<Arc<dyn Fn() -> Div + Send + Sync>>,
    on_close: Option<Arc<dyn Fn() + Send + Sync>>,
    motion_key: Option<String>,
    motion_enter: Option<ElementAnimation>,
    motion_exit: Option<ElementAnimation>,
    motion_class: Option<Arc<str>>,
}

impl Default for ToastBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ToastBuilder {
    pub fn new() -> Self {
        Self {
            auto_after_ms: 4_000,
            dismiss_on_click: true,
            corner: None,
            content_fn: None,
            on_close: None,
            motion_key: None,
            motion_enter: None,
            motion_exit: None,
            motion_class: None,
        }
    }

    /// Run enter and exit by CSS rules: the toast's wrapper carries `class`
    /// and `data-motion="enter"`, then `"exit"` while it leaves; rules name
    /// the `@keyframes`, and the toast stays until its exit has played.
    /// Takes the place of `motion_enter` / `motion_exit`.
    pub fn motion_class(mut self, class: impl AsRef<str>) -> Self {
        self.motion_class = Some(blinc_core::intern::intern(class.as_ref()));
        self
    }

    /// Configure the enter animation. Defaults to slide-in from the corner
    /// the tray is anchored to.
    pub fn motion_enter(mut self, anim: impl Into<ElementAnimation>) -> Self {
        self.motion_enter = Some(anim.into());
        self
    }

    /// Configure the exit animation.
    pub fn motion_exit(mut self, anim: impl Into<ElementAnimation>) -> Self {
        self.motion_exit = Some(anim.into());
        self
    }

    /// Auto-dismiss after this many ms. 0 = persistent (never auto-dismiss).
    pub fn auto_after_ms(mut self, ms: u64) -> Self {
        self.auto_after_ms = ms;
        self
    }

    /// Whether clicking the toast dismisses it. Default true.
    pub fn dismiss_on_click(mut self, b: bool) -> Self {
        self.dismiss_on_click = b;
        self
    }

    /// Override the tray's corner for THIS toast. Note: corner is a tray-level
    /// config, so the last `corner()` call wins for the next render frame.
    /// In practice all toasts in an app should use the same corner.
    pub fn corner(mut self, c: Corner) -> Self {
        self.corner = Some(c);
        self
    }

    pub fn content<F: Fn() -> Div + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.content_fn = Some(Arc::new(f));
        self
    }

    pub fn on_close(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_close = Some(Arc::new(f));
        self
    }

    /// Override the motion key. Defaults to `cn-toast:{handle.raw()}`.
    pub fn motion_key(mut self, key: impl Into<String>) -> Self {
        self.motion_key = Some(key.into());
        self
    }

    /// Push to the global toast tray. Returns the new handle.
    pub fn show(self) -> ToastHandle {
        use crate::overlay_state::toast_tray;

        let tray_arc = toast_tray();
        let mut tray = tray_arc.lock().unwrap();
        let handle = tray.allocate_handle();
        let motion_key = self
            .motion_key
            .unwrap_or_else(|| format!("cn-toast:{}", handle.raw()));
        let spawned_at_ms = tray.current_time_ms;

        if let Some(c) = self.corner {
            tray.set_corner(c);
        }

        // Default enter/exit motion. Translate-only (no opacity keyframes)
        // so the toast slides in/out fully opaque from the start.
        //
        // The off-the-shelf `slide_in_*` / `slide_out_*` presets layer
        // `opacity: 0 → 1` on top of translate. For card-style toasts
        // with `shadow_lg()`, that opacity fade causes a visible
        // darkening artifact mid-animation:
        // 1. The motion wrapper has a single child (the toast div), so the
        //    paint walker takes the `can_flatten_opacity` fast path and
        //    multiplies the wrapper's opacity into each descendant
        //    primitive's alpha individually instead of pushing a
        //    layer.
        // 2. At 50 % opacity, the toast's solid bg primitive paints with
        //    `α = 0.5` on top of (a) the shadow primitive's blurred
        //    falloff outside the toast bounds and (b) the page bg
        //    inside. The bg no longer fully occludes the visible part
        //    of the shadow on either side of the toast's edge, so the
        //    blurred shadow blends through the bg and reads as a
        //    grey/dark tint over the toast itself.
        // 3. A layer-push path (which would rasterise the toast +
        //    shadow at full opacity and composite the texture once)
        //    would avoid the artifact, but is gated behind
        //    `safe_to_flatten = children > 1` — a structural change
        //    just for toasts would be invasive.
        //
        // Dropping the opacity keyframes is the lightest fix: toasts
        // still slide in/out (the spatial motion gives plenty of
        // affordance), and there's no fractional-α frame where the
        // shadow can bleed through.
        use blinc_animation::{Easing, KeyframeProperties, MultiKeyframeAnimation};
        let corner_for_motion = self.corner.unwrap_or_else(|| tray.corner());
        let slide_distance = 320.0_f32;
        let enter_translate = match corner_for_motion {
            Corner::TopLeft | Corner::BottomLeft => (-slide_distance, 0.0),
            Corner::TopRight | Corner::BottomRight => (slide_distance, 0.0),
        };
        // Theme-driven curves: enter uses `ease_spring` for the
        // attention-grabbing pop; exit uses `ease_state` because the
        // toast going away is interaction feedback, not navigation.
        // Fall back to EaseOutCubic / EaseInCubic if ThemeState
        // hasn't been initialised yet (rare — only in tests / cold
        // boot before the runner installs the bundle).
        let (enter_easing, exit_easing) = blinc_theme::ThemeState::try_get()
            .map(|s| {
                let a = s.animations();
                (
                    a.ease_spring.to_animation_easing(),
                    a.ease_state.to_animation_easing(),
                )
            })
            .unwrap_or((Easing::EaseOutCubic, Easing::EaseInCubic));
        let motion_enter = self.motion_enter.unwrap_or_else(|| {
            MultiKeyframeAnimation::new(300)
                .keyframe(
                    0.0,
                    KeyframeProperties::default()
                        .with_translate(enter_translate.0, enter_translate.1),
                    Easing::Linear,
                )
                .keyframe(
                    1.0,
                    KeyframeProperties::default().with_translate(0.0, 0.0),
                    enter_easing,
                )
                .into()
        });
        let motion_exit = self.motion_exit.unwrap_or_else(|| {
            MultiKeyframeAnimation::new(225)
                .keyframe(
                    0.0,
                    KeyframeProperties::default().with_translate(0.0, 0.0),
                    Easing::Linear,
                )
                .keyframe(
                    1.0,
                    KeyframeProperties::default()
                        .with_translate(enter_translate.0, enter_translate.1),
                    exit_easing,
                )
                .into()
        });

        let css_motion = self.motion_class.map(|class| {
            super::css_motion::CssMotion::new(class, format!("blinc-toast-motion-{}", handle.raw()))
        });
        let (motion_enter, motion_exit) = match css_motion {
            Some(_) => (None, None),
            None => (Some(motion_enter), Some(motion_exit)),
        };
        let entry = ToastEntry {
            handle,
            motion_key,
            spawned_at_ms,
            auto_after_ms: self.auto_after_ms,
            exiting: false,
            dismiss_on_click: self.dismiss_on_click,
            content_fn: self.content_fn.unwrap_or_else(|| Arc::new(Div::new)),
            on_close: self.on_close,
            motion_enter,
            motion_exit,
            css_motion,
        };
        tray.push(entry)
    }
}

impl ToastHandle {
    /// Reconstruct from a raw id. Mirror of `OverlayHandle::from_raw`.
    pub fn from_raw(id: u64) -> Self {
        ToastHandle(id)
    }

    /// Sugar — calls `toast_tray().dismiss(self)`.
    pub fn dismiss(&self) {
        use crate::overlay_state::toast_tray;
        if let Ok(mut t) = toast_tray().lock() {
            t.dismiss(*self);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_toast(tray: &ToastTray, spawn_at: u64, auto_after: u64) -> ToastEntry {
        ToastEntry {
            handle: tray.allocate_handle(),
            motion_key: format!("toast-test:{}", tray.next_id.load(Ordering::Relaxed)),
            spawned_at_ms: spawn_at,
            auto_after_ms: auto_after,
            exiting: false,
            dismiss_on_click: true,
            content_fn: Arc::new(Div::new),
            on_close: None,
            motion_enter: None,
            motion_exit: None,
            css_motion: None,
        }
    }

    #[test]
    fn push_dismiss_basic() {
        let mut tray = ToastTray::new();
        let h = tray.push(dummy_toast(&tray, 0, 4_000));
        assert_eq!(tray.len(), 1);
        assert!(tray.contains(h));

        tray.dismiss(h);
        let entry = tray.iter().find(|e| e.handle == h).unwrap();
        assert!(entry.exiting);
    }

    #[test]
    fn auto_dismiss_via_update() {
        let mut tray = ToastTray::new();
        let h = tray.push(dummy_toast(&tray, 1_000, 4_000));

        tray.update(4_999);
        let e = tray.iter().find(|e| e.handle == h).unwrap();
        assert!(!e.exiting);

        // Tick past due-time. In tests there's no motion system, so
        // `motion_done()` returns true immediately — the toast is reaped on
        // the same update() call that fires auto-dismiss.
        tray.update(5_000);
        assert!(
            !tray.contains(h),
            "toast should be reaped on the same update() tick (instant eviction \
             when no motion system is wired)"
        );
    }

    #[test]
    fn dismiss_all_marks_every_toast() {
        let mut tray = ToastTray::new();
        let a = tray.push(dummy_toast(&tray, 0, 4_000));
        let b = tray.push(dummy_toast(&tray, 0, 4_000));
        let c = tray.push(dummy_toast(&tray, 0, 4_000));

        tray.dismiss_all();
        for h in [a, b, c] {
            let e = tray.iter().find(|e| e.handle == h).unwrap();
            assert!(e.exiting);
        }
    }

    #[test]
    fn click_dismisses_hit_toast_only() {
        let mut tray = ToastTray::new();
        let a = tray.push(dummy_toast(&tray, 0, 4_000));
        let b = tray.push(dummy_toast(&tray, 0, 4_000));

        let hit_b = move |t: &ToastEntry, _: f32, _: f32| -> bool { t.handle == b };
        let consumed = tray.handle_click_at(0.0, 0.0, &hit_b);
        assert!(consumed);

        let a_e = tray.iter().find(|e| e.handle == a).unwrap();
        let b_e = tray.iter().find(|e| e.handle == b).unwrap();
        assert!(!a_e.exiting);
        assert!(b_e.exiting);
    }

    #[test]
    fn click_without_hit_returns_false() {
        let mut tray = ToastTray::new();
        let _h = tray.push(dummy_toast(&tray, 0, 4_000));
        let no_hit = |_: &ToastEntry, _: f32, _: f32| -> bool { false };
        let consumed = tray.handle_click_at(0.0, 0.0, &no_hit);
        assert!(!consumed);
    }

    #[test]
    fn double_dismiss_is_idempotent() {
        let mut tray = ToastTray::new();
        let h = tray.push(dummy_toast(&tray, 0, 4_000));
        tray.dismiss(h);
        tray.dismiss(h);
        let e = tray.iter().find(|e| e.handle == h).unwrap();
        assert!(e.exiting);
    }
}
