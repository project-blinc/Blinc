//! Fuchsia event loop implementation
//!
//! Uses fuchsia-async executor for async event handling with FIDL services.
//!
//! # Architecture
//!
//! The event loop integrates with:
//!
//! - **fuchsia-async**: Async executor for futures
//! - **Flatland OnNextFrameBegin**: Frame scheduling
//! - **TouchSource/MouseSource**: Input events
//! - **Keyboard**: Keyboard input
//!
//! # Event Flow
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    fuchsia-async executor                    │
//! │  ┌─────────────────────────────────────────────────────────┐│
//! │  │                   Main Event Loop                       ││
//! │  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐ ││
//! │  │  │   Flatland  │  │   Touch/    │  │    Keyboard     │ ││
//! │  │  │   Events    │  │   Mouse     │  │    Events       │ ││
//! │  │  └──────┬──────┘  └──────┬──────┘  └────────┬────────┘ ││
//! │  │         │                │                   │          ││
//! │  │         └────────────────┴───────────────────┘          ││
//! │  │                          │                              ││
//! │  │                    Event Handler                        ││
//! │  └──────────────────────────┼──────────────────────────────┘│
//! └─────────────────────────────┼───────────────────────────────┘
//!                               ▼
//!                      Blinc Application
//! ```

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use blinc_platform::{ControlFlow, Event, EventLoop, PlatformError};

#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
use fidl_fuchsia_ui_composition::{FlatlandEvent, FlatlandEventStream, ParentViewportWatcherProxy};
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
use fidl_fuchsia_ui_pointer::{TouchSourceProxy, MouseSourceProxy, TouchResponse as FidlTouchResponse, TouchResponseType as FidlTouchResponseType};
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
use fidl_fuchsia_ui_views::ViewRefFocusedProxy;
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
use futures::{FutureExt, StreamExt};

use crate::window::FuchsiaWindow;

/// Wake proxy for Fuchsia event loop
///
/// Use this to request a redraw from a background animation thread.
/// The wake signal is processed on the next executor iteration.
#[derive(Clone)]
pub struct FuchsiaWakeProxy {
    /// Wake request flag
    wake_requested: Arc<AtomicBool>,
    /// Wake counter for debugging
    wake_count: Arc<AtomicU64>,
}

/// Events that can wake the event loop
#[derive(Clone, Debug)]
pub enum WakeEvent {
    /// Animation frame requested
    AnimationFrame,
    /// State changed, needs rebuild
    StateChanged,
    /// Custom user event
    Custom(u32),
}

impl FuchsiaWakeProxy {
    /// Create a new wake proxy
    pub fn new() -> Self {
        Self {
            wake_requested: Arc::new(AtomicBool::new(false)),
            wake_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Wake up the event loop
    ///
    /// This is safe to call from any thread.
    pub fn wake(&self) {
        self.wake_requested.store(true, Ordering::SeqCst);
        self.wake_count.fetch_add(1, Ordering::Relaxed);
        // On Fuchsia, would also signal the async executor
    }

    /// Check if a wake was requested and clear the flag
    pub fn take_wake_request(&self) -> bool {
        self.wake_requested.swap(false, Ordering::SeqCst)
    }

    /// Get the total wake count (for debugging)
    pub fn wake_count(&self) -> u64 {
        self.wake_count.load(Ordering::Relaxed)
    }
}

impl Default for FuchsiaWakeProxy {
    fn default() -> Self {
        Self::new()
    }
}

/// Frame scheduling state from Flatland
#[derive(Clone, Debug, Default)]
pub struct FrameSchedulingState {
    /// Next expected presentation time (nanoseconds)
    pub next_presentation_time: i64,
    /// Time available to render before latch point
    pub latch_point: i64,
    /// Number of presents allowed
    pub presents_remaining: u32,
}

impl FrameSchedulingState {
    /// Check if we should render a new frame
    pub fn should_render(&self) -> bool {
        self.presents_remaining > 0
    }

    /// Get time until next present in milliseconds
    pub fn time_until_present_ms(&self) -> f64 {
        // On Fuchsia, would calculate from zx::Time
        16.67 // ~60fps
    }
}

/// Input sources available on Fuchsia
pub struct InputSources {
    /// Touch input is available
    pub touch_available: bool,
    /// Mouse input is available
    pub mouse_available: bool,
    /// Keyboard input is available
    pub keyboard_available: bool,
}

impl Default for InputSources {
    fn default() -> Self {
        Self {
            touch_available: true,
            mouse_available: false, // Touch-first platform
            keyboard_available: true,
        }
    }
}

/// Event loop configuration
#[derive(Clone, Debug)]
pub struct EventLoopConfig {
    /// Whether to use frame scheduling
    pub use_frame_scheduling: bool,
    /// Timeout for waiting on events (if no frame scheduling)
    pub poll_timeout: Duration,
    /// Maximum frames to queue
    pub max_pending_frames: u32,
}

impl Default for EventLoopConfig {
    fn default() -> Self {
        Self {
            use_frame_scheduling: true,
            poll_timeout: Duration::from_millis(16),
            max_pending_frames: 2,
        }
    }
}

/// Fuchsia event loop using fuchsia-async
///
/// This event loop integrates with Fuchsia's async runtime and handles:
/// - Flatland frame scheduling (OnNextFrameBegin)
/// - Input events from TouchSource/MouseSource
/// - Keyboard events from input3.Keyboard
pub struct FuchsiaEventLoop {
    /// Wake proxy for animation thread
    wake_proxy: FuchsiaWakeProxy,
    /// Event loop configuration
    config: EventLoopConfig,
    /// Frame scheduling state
    frame_state: FrameSchedulingState,
    /// Available input sources
    input_sources: InputSources,
}

impl FuchsiaEventLoop {
    /// Create a new Fuchsia event loop
    pub fn new() -> Self {
        Self {
            wake_proxy: FuchsiaWakeProxy::new(),
            config: EventLoopConfig::default(),
            frame_state: FrameSchedulingState::default(),
            input_sources: InputSources::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: EventLoopConfig) -> Self {
        Self {
            wake_proxy: FuchsiaWakeProxy::new(),
            config,
            frame_state: FrameSchedulingState::default(),
            input_sources: InputSources::default(),
        }
    }

    /// Get a wake proxy for animation threads
    pub fn wake_proxy(&self) -> FuchsiaWakeProxy {
        self.wake_proxy.clone()
    }

    /// Get the current frame scheduling state
    pub fn frame_state(&self) -> &FrameSchedulingState {
        &self.frame_state
    }

    /// Check if input source is available
    pub fn has_touch(&self) -> bool {
        self.input_sources.touch_available
    }

    /// Check if mouse is available
    pub fn has_mouse(&self) -> bool {
        self.input_sources.mouse_available
    }

    /// Check if keyboard is available
    pub fn has_keyboard(&self) -> bool {
        self.input_sources.keyboard_available
    }
}

impl Default for FuchsiaEventLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl EventLoop for FuchsiaEventLoop {
    type Window = FuchsiaWindow;

    fn run<F>(self, mut handler: F) -> Result<(), PlatformError>
    where
        F: FnMut(Event, &Self::Window) -> ControlFlow + 'static,
    {
        #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
        {
            // On Fuchsia, we run a synchronous event loop that polls
            // for wake requests. The full implementation would use
            // fuchsia-async with select! on multiple event sources.
            //
            // Key integration points:
            // 1. Flatland.OnNextFrameBegin - frame scheduling signal
            // 2. TouchSource.Watch - touch input events
            // 3. MouseSource.Watch - mouse input events
            // 4. Keyboard.SetListener - keyboard events
            // 5. ViewRefFocused.Watch - focus changes

            tracing::info!("Fuchsia event loop started");

            let window = FuchsiaWindow::new(1.0);
            let mut running = true;

            // Send initial events
            let _ = handler(Event::Resumed, &window);
            let _ = handler(Event::RedrawRequested, &window);

            while running {
                // Check for wake requests from animation thread
                if self.wake_proxy.take_wake_request() {
                    let flow = handler(Event::RedrawRequested, &window);
                    if flow == ControlFlow::Exit {
                        running = false;
                    }
                }

                // Sleep to avoid busy-looping
                // Real implementation would await on FIDL events
                std::thread::sleep(self.config.poll_timeout);
            }

            tracing::info!("Fuchsia event loop exiting");
            Ok(())
        }

        #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
        {
            Err(PlatformError::Unsupported(
                "Fuchsia event loop only available on Fuchsia OS".to_string(),
            ))
        }
    }
}

/// Helper trait for running event loops with frame scheduling
pub trait FrameScheduledEventLoop {
    /// Called when Flatland signals OnNextFrameBegin
    fn on_next_frame_begin(&mut self, info: &FrameSchedulingState);

    /// Called to request a frame
    fn request_frame(&mut self);
}

// ============================================================================
// Async Event Loop Support
// ============================================================================

/// Unified event from any Fuchsia FIDL source
#[derive(Clone, Debug)]
pub enum FuchsiaEvent {
    /// Frame ready to render (from Flatland.OnNextFrameBegin)
    FrameReady {
        /// Frame scheduling info
        scheduling: FrameSchedulingState,
    },
    /// View layout changed (from ParentViewportWatcher.GetLayout)
    LayoutChanged {
        /// Logical width in DIP
        width: f32,
        /// Logical height in DIP
        height: f32,
        /// Device pixel ratio
        scale_factor: f64,
        /// Insets (top, right, bottom, left)
        insets: (f32, f32, f32, f32),
    },
    /// Touch event (from TouchSource.Watch)
    Touch {
        /// Touch interaction data
        interaction: crate::input::TouchInteraction,
    },
    /// Mouse event (from MouseSource.Watch)
    Mouse {
        /// Mouse interaction data
        interaction: crate::input::MouseInteraction,
    },
    /// Keyboard event (from input3.Keyboard listener)
    Keyboard {
        /// Key event data
        event: crate::input::KeyEvent,
    },
    /// Focus changed (from ViewRefFocused.Watch)
    FocusChanged(bool),
    /// View is detached
    ViewDetached,
    /// View is being destroyed
    ViewDestroyed,
    /// Wake requested by animation thread
    WakeRequested,
}

/// Event sources holder for async event loop
///
/// On Fuchsia, this holds the FIDL proxy connections that we poll.
/// Each async method (Watch, GetLayout, etc.) is a hanging get that
/// returns when new data is available.
/// Stub event sources for builds without full Fuchsia SDK
#[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
#[derive(Default)]
pub struct FuchsiaEventSources {
    /// Has touch source connection
    pub has_touch: bool,
    /// Has mouse source connection
    pub has_mouse: bool,
    /// Has keyboard listener
    pub has_keyboard: bool,
}

/// Event sources holder for async event loop (Fuchsia)
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
pub struct FuchsiaEventSources {
    /// Flatland event stream
    pub flatland_events: Option<FlatlandEventStream>,
    /// ParentViewportWatcher proxy
    pub parent_viewport_watcher: Option<ParentViewportWatcherProxy>,
    /// Touch source proxy
    pub touch_source: Option<TouchSourceProxy>,
    /// Mouse source proxy
    pub mouse_source: Option<MouseSourceProxy>,
    /// Focus watcher proxy
    pub view_ref_focused: Option<ViewRefFocusedProxy>,
}

#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
impl Default for FuchsiaEventSources {
    fn default() -> Self {
        Self {
            flatland_events: None,
            parent_viewport_watcher: None,
            touch_source: None,
            mouse_source: None,
            view_ref_focused: None,
        }
    }
}

#[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
impl FuchsiaEventSources {
    /// Create new event sources
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark touch source as connected
    pub fn with_touch(mut self) -> Self {
        self.has_touch = true;
        self
    }

    /// Mark mouse source as connected
    pub fn with_mouse(mut self) -> Self {
        self.has_mouse = true;
        self
    }

    /// Mark keyboard as connected
    pub fn with_keyboard(mut self) -> Self {
        self.has_keyboard = true;
        self
    }
}

#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
impl FuchsiaEventSources {
    /// Create new event sources
    pub fn new() -> Self {
        Self::default()
    }

    /// Set Flatland event stream
    pub fn with_flatland_events(mut self, stream: FlatlandEventStream) -> Self {
        self.flatland_events = Some(stream);
        self
    }

    /// Set ParentViewportWatcher proxy
    pub fn with_parent_viewport_watcher(mut self, watcher: ParentViewportWatcherProxy) -> Self {
        self.parent_viewport_watcher = Some(watcher);
        self
    }

    /// Set touch source proxy
    pub fn with_touch_source(mut self, source: TouchSourceProxy) -> Self {
        self.touch_source = Some(source);
        self
    }

    /// Set mouse source proxy
    pub fn with_mouse_source(mut self, source: MouseSourceProxy) -> Self {
        self.mouse_source = Some(source);
        self
    }

    /// Set focus watcher proxy
    pub fn with_view_ref_focused(mut self, focused: ViewRefFocusedProxy) -> Self {
        self.view_ref_focused = Some(focused);
        self
    }

    /// Check if touch source is connected
    pub fn has_touch(&self) -> bool {
        self.touch_source.is_some()
    }

    /// Check if mouse source is connected
    pub fn has_mouse(&self) -> bool {
        self.mouse_source.is_some()
    }
}

/// Async runner for Fuchsia event loop
///
/// This structure demonstrates the pattern used on Fuchsia for handling
/// multiple async FIDL sources. The actual implementation would use
/// `futures::select!` or `fuchsia_async::select!` to wait on all sources.
///
/// # Example Pattern (on Fuchsia)
///
/// ```ignore
/// async fn run_event_loop(
///     flatland: FlatlandProxy,
///     touch_source: TouchSourceProxy,
///     parent_viewport: ParentViewportWatcherProxy,
///     mut handler: impl FnMut(FuchsiaEvent),
/// ) {
///     let mut frame_fut = flatland.on_next_frame_begin().fuse();
///     let mut touch_fut = touch_source.watch(&[]).fuse();
///     let mut layout_fut = parent_viewport.get_layout().fuse();
///
///     loop {
///         futures::select! {
///             frame = frame_fut => {
///                 handler(FuchsiaEvent::FrameReady { ... });
///                 frame_fut = flatland.on_next_frame_begin().fuse();
///             }
///             touches = touch_fut => {
///                 for touch in touches {
///                     handler(FuchsiaEvent::Touch { interaction: touch.into() });
///                 }
///                 touch_fut = touch_source.watch(&responses).fuse();
///             }
///             layout = layout_fut => {
///                 handler(FuchsiaEvent::LayoutChanged { ... });
///                 layout_fut = parent_viewport.get_layout().fuse();
///             }
///         }
///     }
/// }
/// ```
pub struct AsyncEventLoop {
    /// Wake proxy for animation wakeups
    wake_proxy: FuchsiaWakeProxy,
    /// Frame scheduling state
    frame_state: FrameSchedulingState,
    /// Event sources info
    sources: FuchsiaEventSources,
}

impl AsyncEventLoop {
    /// Create a new async event loop
    pub fn new(wake_proxy: FuchsiaWakeProxy) -> Self {
        Self {
            wake_proxy,
            frame_state: FrameSchedulingState::default(),
            sources: FuchsiaEventSources::new(),
        }
    }

    /// Set up event sources
    pub fn with_sources(mut self, sources: FuchsiaEventSources) -> Self {
        self.sources = sources;
        self
    }

    /// Get wake proxy
    pub fn wake_proxy(&self) -> FuchsiaWakeProxy {
        self.wake_proxy.clone()
    }

    /// Update frame scheduling state
    pub fn update_frame_state(&mut self, state: FrameSchedulingState) {
        self.frame_state = state;
    }

    /// Check if we should render
    pub fn should_render(&self) -> bool {
        self.frame_state.should_render()
    }

    /// Run the async event loop (placeholder for non-Fuchsia)
    ///
    /// On actual Fuchsia, this would be an async function using
    /// fuchsia-async executor.
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn run_sync<F>(&mut self, mut handler: F, poll_timeout: Duration)
    where
        F: FnMut(FuchsiaEvent) -> bool,
    {
        let mut running = true;
        while running {
            // Check wake proxy
            if self.wake_proxy.take_wake_request() {
                if !handler(FuchsiaEvent::WakeRequested) {
                    running = false;
                }
            }

            // Simulate frame ready for testing
            self.frame_state.presents_remaining = 2;
            if !handler(FuchsiaEvent::FrameReady {
                scheduling: self.frame_state.clone(),
            }) {
                running = false;
            }

            std::thread::sleep(poll_timeout);
        }
    }

    /// Run the async event loop (Fuchsia implementation)
    ///
    /// This is the real async implementation using fuchsia-async.
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub async fn run_async<F>(&mut self, mut handler: F)
    where
        F: FnMut(FuchsiaEvent) -> bool,
    {
        use crate::input::{TouchInteraction, MouseInteraction, TouchPhase, MousePhase};

        // Track touch responses for gesture disambiguation
        let mut pending_touch_responses: Vec<FidlTouchResponse> = vec![];

        tracing::info!("Fuchsia async event loop starting");

        loop {
            // Check for wake requests first
            if self.wake_proxy.take_wake_request() {
                if !handler(FuchsiaEvent::WakeRequested) {
                    tracing::info!("Event loop exit requested via wake");
                    break;
                }
            }

            // Use futures::select! to wait on multiple FIDL sources
            futures::select! {
                // Flatland events (OnNextFrameBegin, OnError)
                flatland_event = async {
                    if let Some(ref mut stream) = self.sources.flatland_events {
                        stream.next().await
                    } else {
                        futures::future::pending::<Option<Result<FlatlandEvent, fidl::Error>>>().await
                    }
                }.fuse() => {
                    if let Some(Ok(event)) = flatland_event {
                        match event {
                            FlatlandEvent::OnNextFrameBegin { values } => {
                                let scheduling = FrameSchedulingState {
                                    next_presentation_time: values.future_presentation_infos
                                        .as_ref()
                                        .and_then(|v| v.first())
                                        .and_then(|info| info.presentation_time)
                                        .unwrap_or(0),
                                    latch_point: values.future_presentation_infos
                                        .as_ref()
                                        .and_then(|v| v.first())
                                        .and_then(|info| info.latch_point)
                                        .unwrap_or(0),
                                    presents_remaining: values.additional_present_credits.unwrap_or(0) as u32,
                                };
                                self.frame_state = scheduling.clone();

                                if !handler(FuchsiaEvent::FrameReady { scheduling }) {
                                    break;
                                }
                            }
                            FlatlandEvent::OnFramePresented { .. } => {
                                // Frame was presented - could track latency here
                            }
                            FlatlandEvent::OnError { error } => {
                                tracing::error!("Flatland error: {:?}", error);
                                break;
                            }
                        }
                    }
                }

                // Layout changes from ParentViewportWatcher
                layout = async {
                    if let Some(ref watcher) = self.sources.parent_viewport_watcher {
                        watcher.get_layout().await.ok()
                    } else {
                        futures::future::pending::<Option<fidl_fuchsia_ui_composition::LayoutInfo>>().await
                    }
                }.fuse() => {
                    if let Some(layout) = layout {
                        let (width, height) = layout.logical_size
                            .map(|s| (s.width as f32, s.height as f32))
                            .unwrap_or((1920.0, 1080.0));
                        let scale_factor = layout.device_pixel_ratio
                            .map(|r| r.x as f64)
                            .unwrap_or(1.0);
                        let insets = layout.inset
                            .map(|i| (i.top as f32, i.right as f32, i.bottom as f32, i.left as f32))
                            .unwrap_or((0.0, 0.0, 0.0, 0.0));

                        if !handler(FuchsiaEvent::LayoutChanged { width, height, scale_factor, insets }) {
                            break;
                        }
                    }
                }

                // Touch events from TouchSource
                touches = async {
                    if let Some(ref source) = self.sources.touch_source {
                        source.watch(&pending_touch_responses).await.ok()
                    } else {
                        futures::future::pending::<Option<Vec<fidl_fuchsia_ui_pointer::TouchEvent>>>().await
                    }
                }.fuse() => {
                    if let Some(touch_events) = touches {
                        // Clear previous responses
                        pending_touch_responses.clear();

                        for touch in touch_events {
                            // Convert FIDL TouchEvent to our TouchInteraction
                            let pointer_sample = match touch.pointer_sample {
                                Some(sample) => sample,
                                None => continue,
                            };

                            let interaction_id = touch.interaction_id.map(|id| {
                                crate::input::InteractionId {
                                    device_id: id.device_id,
                                    pointer_id: id.pointer_id,
                                    interaction_id: id.interaction_id,
                                }
                            }).unwrap_or_default();

                            let phase = match pointer_sample.phase {
                                Some(fidl_fuchsia_ui_pointer::EventPhase::Add) => TouchPhase::Started,
                                Some(fidl_fuchsia_ui_pointer::EventPhase::Change) => TouchPhase::Moved,
                                Some(fidl_fuchsia_ui_pointer::EventPhase::Remove) => TouchPhase::Ended,
                                Some(fidl_fuchsia_ui_pointer::EventPhase::Cancel) => TouchPhase::Cancelled,
                                None => continue,
                            };

                            let position = pointer_sample.position_in_viewport
                                .map(|p| (p[0], p[1]))
                                .unwrap_or((0.0, 0.0));

                            let interaction = TouchInteraction {
                                id: interaction_id.clone(),
                                phase,
                                x: position.0,
                                y: position.1,
                                force: None,
                            };

                            if !handler(FuchsiaEvent::Touch { interaction }) {
                                break;
                            }

                            // Respond to accept the touch (for gesture disambiguation)
                            if let Some(id) = touch.interaction_id {
                                pending_touch_responses.push(FidlTouchResponse {
                                    response_type: Some(FidlTouchResponseType::Yes),
                                    trace_flow_id: None,
                                    ..Default::default()
                                });
                            }
                        }
                    }
                }

                // Mouse events from MouseSource
                mouse_events = async {
                    if let Some(ref source) = self.sources.mouse_source {
                        source.watch().await.ok()
                    } else {
                        futures::future::pending::<Option<Vec<fidl_fuchsia_ui_pointer::MouseEvent>>>().await
                    }
                }.fuse() => {
                    if let Some(events) = mouse_events {
                        for mouse in events {
                            let pointer_sample = match mouse.pointer_sample {
                                Some(sample) => sample,
                                None => continue,
                            };

                            let position = pointer_sample.position_in_viewport
                                .map(|p| (p[0], p[1]))
                                .unwrap_or((0.0, 0.0));

                            let buttons = pointer_sample.pressed_buttons.unwrap_or_default();
                            let scroll = pointer_sample.scroll_v.map(|v| (0.0, v as f32));

                            let phase = if buttons.is_empty() {
                                MousePhase::Move
                            } else {
                                MousePhase::Down
                            };

                            let interaction = MouseInteraction {
                                phase,
                                x: position.0,
                                y: position.1,
                                buttons: buttons.into_iter().map(|b| crate::input::FuchsiaMouseButton(b)).collect(),
                                scroll_delta: scroll,
                            };

                            if !handler(FuchsiaEvent::Mouse { interaction }) {
                                break;
                            }
                        }
                    }
                }

                // Focus changes from ViewRefFocused
                focus = async {
                    if let Some(ref focused) = self.sources.view_ref_focused {
                        focused.watch().await.ok()
                    } else {
                        futures::future::pending::<Option<fidl_fuchsia_ui_views::FocusState>>().await
                    }
                }.fuse() => {
                    if let Some(state) = focus {
                        let is_focused = state.focused.unwrap_or(false);
                        if !handler(FuchsiaEvent::FocusChanged(is_focused)) {
                            break;
                        }
                    }
                }
            }
        }

        tracing::info!("Fuchsia async event loop ended");
    }
}

/// Touch response for TouchSource.Watch
///
/// After receiving touch events, we respond to tell the system
/// whether we want to continue receiving this interaction.
#[derive(Clone, Debug)]
pub struct TouchResponse {
    /// The interaction this response is for
    pub interaction_id: crate::input::InteractionId,
    /// Our response
    pub response: crate::input::TouchResponseType,
}

impl TouchResponse {
    /// Accept the touch interaction
    pub fn accept(id: crate::input::InteractionId) -> Self {
        Self {
            interaction_id: id,
            response: crate::input::TouchResponseType::Yes,
        }
    }

    /// Reject the touch interaction
    pub fn reject(id: crate::input::InteractionId) -> Self {
        Self {
            interaction_id: id,
            response: crate::input::TouchResponseType::No,
        }
    }

    /// Hold decision for gesture disambiguation
    pub fn hold(id: crate::input::InteractionId) -> Self {
        Self {
            interaction_id: id,
            response: crate::input::TouchResponseType::Hold,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wake_proxy() {
        let proxy = FuchsiaWakeProxy::new();
        assert!(!proxy.take_wake_request());

        proxy.wake();
        assert!(proxy.take_wake_request());
        assert!(!proxy.take_wake_request()); // Should be cleared
    }

    #[test]
    fn test_wake_count() {
        let proxy = FuchsiaWakeProxy::new();
        assert_eq!(proxy.wake_count(), 0);

        proxy.wake();
        proxy.wake();
        proxy.wake();

        assert_eq!(proxy.wake_count(), 3);
    }

    #[test]
    fn test_frame_scheduling_state() {
        let state = FrameSchedulingState {
            presents_remaining: 2,
            ..Default::default()
        };
        assert!(state.should_render());

        let state_no_presents = FrameSchedulingState::default();
        assert!(!state_no_presents.should_render());
    }
}
