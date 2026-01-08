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
use slotmap::{new_key_type, SlotMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

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

/// Internal state of the animation scheduler
struct SchedulerInner {
    springs: SlotMap<SpringId, Spring>,
    keyframes: SlotMap<KeyframeId, KeyframeAnimation>,
    timelines: SlotMap<TimelineId, Timeline>,
    last_frame: Instant,
    target_fps: u32,
}

/// Callback type for waking up the main thread from the animation thread
///
/// This is called when there are active animations that need to be rendered.
/// The callback should wake up the event loop (e.g., via EventLoopProxy).
pub type WakeCallback = Arc<dyn Fn() + Send + Sync>;

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
    /// Background thread handle (if running)
    thread_handle: Option<JoinHandle<()>>,
    /// Optional callback to wake up the main thread
    wake_callback: Option<WakeCallback>,
}

impl AnimationScheduler {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(SchedulerInner {
                springs: SlotMap::with_key(),
                keyframes: SlotMap::with_key(),
                timelines: SlotMap::with_key(),
                last_frame: Instant::now(),
                target_fps: 120,
            })),
            stop_flag: Arc::new(AtomicBool::new(false)),
            needs_redraw: Arc::new(AtomicBool::new(false)),
            continuous_redraw: Arc::new(AtomicBool::new(false)),
            thread_handle: None,
            wake_callback: None,
        }
    }

    /// Set a wake callback that will be called when animations need a redraw
    ///
    /// This callback is invoked from the background animation thread when there
    /// are active animations. Use this to wake up an event loop from another thread.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let wake_proxy = event_loop.wake_proxy();
    /// scheduler.set_wake_callback(move || wake_proxy.wake());
    /// ```
    pub fn set_wake_callback<F>(&mut self, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.wake_callback = Some(Arc::new(callback));
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
    pub fn start_background(&mut self) {
        if self.thread_handle.is_some() {
            return; // Already running
        }

        let inner = Arc::clone(&self.inner);
        let stop_flag = Arc::clone(&self.stop_flag);
        let needs_redraw = Arc::clone(&self.needs_redraw);
        let continuous_redraw = Arc::clone(&self.continuous_redraw);
        let wake_callback = self.wake_callback.clone();

        self.thread_handle = Some(thread::spawn(move || {
            let frame_duration = Duration::from_micros(1_000_000 / 120); // 120fps

            while !stop_flag.load(Ordering::Relaxed) {
                let start = Instant::now();

                // Check if continuous redraw is requested (e.g., for cursor blink)
                let wants_continuous = continuous_redraw.load(Ordering::Relaxed);

                // Tick animations and check if any are active
                let has_active = {
                    let mut inner = inner.lock().unwrap();
                    let now = Instant::now();
                    let dt = (now - inner.last_frame).as_secs_f32();
                    let dt_ms = dt * 1000.0;
                    inner.last_frame = now;

                    // Update all springs
                    for (_, spring) in inner.springs.iter_mut() {
                        spring.step(dt);
                    }

                    // Update all keyframe animations
                    for (_, keyframe) in inner.keyframes.iter_mut() {
                        keyframe.tick(dt_ms);
                    }

                    // Update all timelines
                    for (_, timeline) in inner.timelines.iter_mut() {
                        timeline.tick(dt_ms);
                    }

                    // NOTE: We do NOT remove animations here!
                    // Springs, keyframes, and timelines are only removed when:
                    // 1. Their wrapper (AnimatedValue, AnimatedKeyframe, AnimatedTimeline) is dropped
                    // 2. set_immediate() is called on springs
                    // This ensures animations can be restarted after completing.

                    // Check if any animations are still active (playing, not just present)
                    inner.springs.iter().any(|(_, s)| !s.is_settled())
                        || inner.keyframes.iter().any(|(_, k)| k.is_playing())
                        || inner.timelines.iter().any(|(_, t)| t.is_playing())
                };

                // Signal main thread that it needs to redraw
                // Either from active animations OR continuous redraw request (cursor blink)
                if has_active || wants_continuous {
                    needs_redraw.store(true, Ordering::Release);

                    // Wake up the event loop if a callback is set
                    if let Some(ref callback) = wake_callback {
                        // Only log occasionally to avoid spam
                        static COUNTER: std::sync::atomic::AtomicU64 =
                            std::sync::atomic::AtomicU64::new(0);
                        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
                        if count % 120 == 0 {
                            // Log once per second at 120fps
                            tracing::debug!(
                                "Animation thread: waking event loop (continuous={}, active={})",
                                wants_continuous,
                                has_active
                            );
                        }
                        callback();
                    }
                }

                // Sleep for remaining frame time
                let elapsed = start.elapsed();
                if elapsed < frame_duration {
                    thread::sleep(frame_duration - elapsed);
                }
            }
        }));
    }

    /// Stop the background thread
    pub fn stop_background(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
        self.stop_flag.store(false, Ordering::Relaxed);
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
    }

    /// Check if continuous redraw mode is enabled
    pub fn is_continuous_redraw(&self) -> bool {
        self.continuous_redraw.load(Ordering::Relaxed)
    }

    /// Get a handle to this scheduler for passing to components
    pub fn handle(&self) -> SchedulerHandle {
        SchedulerHandle {
            inner: Arc::downgrade(&self.inner),
        }
    }

    pub fn set_target_fps(&mut self, fps: u32) {
        self.inner.lock().unwrap().target_fps = fps;
    }

    /// Tick all animations
    ///
    /// Returns true if any animations are still active (need another tick).
    pub fn tick(&self) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let now = Instant::now();
        let dt = (now - inner.last_frame).as_secs_f32();
        let dt_ms = dt * 1000.0;
        inner.last_frame = now;

        // Update all springs
        for (_, spring) in inner.springs.iter_mut() {
            spring.step(dt);
        }

        // Update all keyframe animations
        for (_, keyframe) in inner.keyframes.iter_mut() {
            keyframe.tick(dt_ms);
        }

        // Update all timelines
        for (_, timeline) in inner.timelines.iter_mut() {
            timeline.tick(dt_ms);
        }

        // NOTE: We do NOT remove animations here!
        // Springs, keyframes, and timelines are only removed when their wrappers drop.
        // This ensures animations can be restarted after completing.

        // Return true if there are still active (playing, not just present) animations
        inner.springs.iter().any(|(_, s)| !s.is_settled())
            || inner.keyframes.iter().any(|(_, k)| k.is_playing())
            || inner.timelines.iter().any(|(_, t)| t.is_playing())
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
        self.inner.lock().unwrap().springs.insert(spring)
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
        self.inner.lock().unwrap().keyframes.insert(keyframe)
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
        self.inner.lock().unwrap().timelines.insert(timeline)
    }

    pub fn start_timeline(&self, id: TimelineId) {
        if let Some(timeline) = self.inner.lock().unwrap().timelines.get_mut(id) {
            timeline.start();
        }
    }

    pub fn stop_timeline(&self, id: TimelineId) {
        if let Some(timeline) = self.inner.lock().unwrap().timelines.get_mut(id) {
            timeline.stop();
        }
    }

    pub fn remove_timeline(&self, id: TimelineId) -> Option<Timeline> {
        self.inner.lock().unwrap().timelines.remove(id)
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
            // Cloned scheduler doesn't own the background thread
            thread_handle: None,
            wake_callback: self.wake_callback.clone(),
        }
    }
}

impl Drop for AnimationScheduler {
    fn drop(&mut self) {
        // Stop background thread when scheduler is dropped
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
}

impl SchedulerHandle {
    // =========================================================================
    // Spring Operations
    // =========================================================================

    /// Register a spring and return its ID
    pub fn register_spring(&self, spring: Spring) -> Option<SpringId> {
        self.inner.upgrade().map(|inner| {
            let mut guard = inner.lock().unwrap();
            // Reset last_frame to now to prevent huge dt on first tick
            // This ensures new springs start animating smoothly from their current frame
            guard.last_frame = std::time::Instant::now();
            guard.springs.insert(spring)
        })
    }

    /// Update a spring's target
    pub fn set_spring_target(&self, id: SpringId, target: f32) {
        if let Some(inner) = self.inner.upgrade() {
            if let Some(spring) = inner.lock().unwrap().springs.get_mut(id) {
                spring.set_target(target);
            }
        }
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
            .and_then(|inner| inner.lock().unwrap().springs.get(id).map(|s| s.is_settled()))
            .unwrap_or(true) // If spring gone, consider settled
    }

    /// Remove a spring
    pub fn remove_spring(&self, id: SpringId) {
        if let Some(inner) = self.inner.upgrade() {
            inner.lock().unwrap().springs.remove(id);
        }
    }

    // =========================================================================
    // Keyframe Operations
    // =========================================================================

    /// Register a keyframe animation and return its ID
    pub fn register_keyframe(&self, keyframe: KeyframeAnimation) -> Option<KeyframeId> {
        self.inner
            .upgrade()
            .map(|inner| inner.lock().unwrap().keyframes.insert(keyframe))
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
        self.inner
            .upgrade()
            .map(|inner| inner.lock().unwrap().timelines.insert(timeline))
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
        self.inner.upgrade().and_then(|inner| {
            inner
                .lock()
                .unwrap()
                .timelines
                .get_mut(id)
                .map(|timeline| f(timeline))
        })
    }

    /// Check if the scheduler is still alive
    pub fn is_alive(&self) -> bool {
        self.inner.strong_count() > 0
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
        self.current = value;
        self.target = value;
    }

    /// Check if currently animating
    ///
    /// Returns `true` only while the spring is actively moving toward its target.
    /// Once the spring has settled (reached target with near-zero velocity), this
    /// returns `false`.
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
        // Clean up spring when value is dropped
        if let Some(id) = self.spring_id {
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
    start_time: Option<std::time::Instant>,
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
            self.start_time = Some(std::time::Instant::now());
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
                .with_timeline(id, |timeline| timeline.value(entry_id))
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
                .with_timeline(id, |timeline| timeline.progress())
                .unwrap_or(0.0)
        } else {
            0.0
        }
    }

    /// Get progress of a specific entry (0.0 to 1.0)
    pub fn entry_progress(&self, entry_id: crate::timeline::TimelineEntryId) -> Option<f32> {
        if let Some(id) = self.timeline_id {
            self.handle
                .with_timeline(id, |timeline| timeline.entry_progress(entry_id))
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
                .with_timeline(id, |timeline| timeline.entry_count() > 0)
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
                .with_timeline(id, |timeline| timeline.entry_ids())
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
        assert!(handle
            .register_spring(Spring::new(SpringConfig::stiff(), 0.0))
            .is_none());
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
}
