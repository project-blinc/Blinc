//! blinc_recorder - Recording and debugging infrastructure for Blinc applications.
//!
//! This crate provides:
//! - Event recording for user interactions
//! - Tree snapshot capture for debugging UI state
//! - Session management with start/pause/stop lifecycle
//!
//! # Quick Start
//!
//! ```ignore
//! use blinc_recorder::{install_recorder, RecordingConfig, SharedRecordingSession};
//! use std::sync::Arc;
//!
//! // Create and install a recorder
//! let session = Arc::new(SharedRecordingSession::new(RecordingConfig::debug()));
//! install_recorder(session.clone());
//!
//! // Start recording
//! session.start();
//!
//! // ... run your app ...
//!
//! // Stop and export
//! session.stop();
//! let export = session.export();
//! ```

pub mod capture;
pub mod server;
pub mod session;

pub use capture::{
    ChangeCategory, CustomEvent, ElementDiff, ElementSnapshot, FocusChangeEvent, HoverEvent, Key,
    Modifiers, MouseButton, MouseEvent, MouseMoveEvent, Point, PropertyChange, Rect,
    RecordedEvent, RecordingClock, ScrollEvent, TextInputEvent, Timestamp, TimestampedEvent,
    TreeDiff, TreeSnapshot, VisualProps, WindowResizeEvent,
};
pub use server::{
    start_local_server, start_local_server_named, DebugServer, DebugServerConfig, ServerHandle,
    ServerMessage,
};
pub use session::{
    RecordingConfig, RecordingExport, RecordingSession, SessionState, SessionStats,
    SharedRecordingSession,
};

use parking_lot::RwLock;
use std::sync::Arc;

/// Thread-local storage for the current recorder session.
std::thread_local! {
    static RECORDER: RwLock<Option<Arc<SharedRecordingSession>>> = const { RwLock::new(None) };
}

/// Install a recorder session for the current thread.
///
/// This makes the session available via `get_recorder()` and enables
/// automatic event/snapshot capture when hooks are wired up.
pub fn install_recorder(session: Arc<SharedRecordingSession>) {
    RECORDER.with(|r| {
        *r.write() = Some(session);
    });
}

/// Remove the recorder session from the current thread.
pub fn uninstall_recorder() {
    RECORDER.with(|r| {
        *r.write() = None;
    });
}

/// Get the current recorder session for this thread.
pub fn get_recorder() -> Option<Arc<SharedRecordingSession>> {
    RECORDER.with(|r| r.read().clone())
}

/// Check if a recorder is installed and recording.
pub fn is_recording() -> bool {
    get_recorder().map(|r| r.is_recording()).unwrap_or(false)
}

/// Record an event if a recorder is installed and recording.
pub fn record_event(event: RecordedEvent) {
    if let Some(recorder) = get_recorder() {
        recorder.record_event(event);
    }
}

/// Record a tree snapshot if a recorder is installed and recording.
pub fn record_snapshot(snapshot: TreeSnapshot) -> Option<TreeDiff> {
    get_recorder().and_then(|r| r.record_snapshot(snapshot))
}

/// Callback types for integration with BlincContextState.
pub mod callbacks {
    use super::*;

    /// Callback type for recording events.
    pub type RecorderEventCallback = Arc<dyn Fn(&RecordedEvent) + Send + Sync>;

    /// Callback type for capturing tree snapshots.
    pub type RecorderSnapshotCallback = Arc<dyn Fn(TreeSnapshot) + Send + Sync>;

    /// Update category for element changes.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum UpdateCategory {
        Visual,
        Layout,
        Structural,
    }

    /// Callback type for update notifications.
    pub type RecorderUpdateCallback = Arc<dyn Fn(&str, UpdateCategory) + Send + Sync>;

    /// Create an event callback that records to the thread-local recorder.
    pub fn make_event_callback() -> RecorderEventCallback {
        Arc::new(|event: &RecordedEvent| {
            record_event(event.clone());
        })
    }

    /// Create a snapshot callback that records to the thread-local recorder.
    pub fn make_snapshot_callback() -> RecorderSnapshotCallback {
        Arc::new(|snapshot: TreeSnapshot| {
            record_snapshot(snapshot);
        })
    }
}

/// Install recorder hooks into BlincContextState.
///
/// This sets up the event and snapshot callbacks to capture data
/// and send it to the thread-local recorder session.
///
/// Call this after installing a recorder and before starting recording.
///
/// # Example
///
/// ```ignore
/// use blinc_recorder::{install_recorder, install_hooks, RecordingConfig, SharedRecordingSession};
/// use std::sync::Arc;
///
/// let session = Arc::new(SharedRecordingSession::new(RecordingConfig::debug()));
/// install_recorder(session.clone());
/// install_hooks();
/// session.start();
/// ```
pub fn install_hooks() {
    use blinc_core::BlincContextState;

    // Only install hooks if BlincContextState is initialized
    if let Some(ctx) = BlincContextState::try_get() {
        // Event callback: convert type-erased event to RecordedEvent and record
        let event_callback: blinc_core::RecorderEventCallback =
            Arc::new(|event_any: blinc_core::RecordedEventAny| {
                if let Some(event) = event_any.downcast_ref::<RecordedEvent>() {
                    record_event(event.clone());
                }
            });
        ctx.set_recorder_event_callback(event_callback);

        // Snapshot callback: convert type-erased snapshot to TreeSnapshot and record
        let snapshot_callback: blinc_core::RecorderSnapshotCallback =
            Arc::new(|snapshot_any: blinc_core::TreeSnapshotAny| {
                if let Some(snapshot) = snapshot_any.downcast_ref::<TreeSnapshot>() {
                    record_snapshot(snapshot.clone());
                }
            });
        ctx.set_recorder_snapshot_callback(snapshot_callback);

        // Update callback: log element updates with their category
        let update_callback: blinc_core::RecorderUpdateCallback =
            Arc::new(|element_id: &str, category: blinc_core::UpdateCategory| {
                tracing::trace!(
                    target: "blinc_recorder::updates",
                    element_id = %element_id,
                    category = ?category,
                    "element update"
                );
            });
        ctx.set_recorder_update_callback(update_callback);
    }
}

/// Uninstall recorder hooks from BlincContextState.
pub fn uninstall_hooks() {
    use blinc_core::BlincContextState;

    if let Some(ctx) = BlincContextState::try_get() {
        ctx.clear_recorder_event_callback();
        ctx.clear_recorder_snapshot_callback();
        ctx.clear_recorder_update_callback();
    }
}

/// Convenience macro for enabling debug recording.
///
/// # Example
///
/// ```ignore
/// fn main() {
///     blinc_recorder::enable_debug_recording!();
///     // Your app code...
/// }
/// ```
#[macro_export]
macro_rules! enable_debug_recording {
    () => {{
        #[cfg(debug_assertions)]
        {
            let session = std::sync::Arc::new($crate::SharedRecordingSession::new(
                $crate::RecordingConfig::debug(),
            ));
            $crate::install_recorder(session.clone());
            session.start();
            session
        }
        #[cfg(not(debug_assertions))]
        {
            // No-op in release builds
            std::sync::Arc::new($crate::SharedRecordingSession::new(
                $crate::RecordingConfig::minimal(),
            ))
        }
    }};
    ($config:expr) => {{
        let session = std::sync::Arc::new($crate::SharedRecordingSession::new($config));
        $crate::install_recorder(session.clone());
        session.start();
        session
    }};
}

/// Convenience macro for enabling debug server with recording.
///
/// This macro:
/// 1. Creates a recording session with debug configuration
/// 2. Installs it as the thread-local recorder
/// 3. Installs hooks into BlincContextState (if available)
/// 4. Starts a local socket server for debugger connections
/// 5. Starts recording
///
/// In release builds (without debug_assertions), this is a no-op that returns
/// a minimal recording session without starting a server.
///
/// # Example
///
/// ```ignore
/// fn main() {
///     // Basic usage - uses default app name "blinc_app"
///     let (session, server) = blinc_recorder::enable_debug_server!();
///
///     // Your app code...
///
///     // The server will be shut down when `server` is dropped
/// }
/// ```
///
/// ```ignore
/// fn main() {
///     // With custom app name
///     let (session, server) = blinc_recorder::enable_debug_server!("my_app");
///
///     // Your app code...
/// }
/// ```
#[macro_export]
macro_rules! enable_debug_server {
    () => {{
        #[cfg(debug_assertions)]
        {
            let session = std::sync::Arc::new($crate::SharedRecordingSession::new(
                $crate::RecordingConfig::debug(),
            ));
            $crate::install_recorder(session.clone());
            $crate::install_hooks();

            let server_handle = $crate::start_local_server(session.clone())
                .expect("Failed to start debug server");

            session.start();
            (session, Some(server_handle))
        }
        #[cfg(not(debug_assertions))]
        {
            // No-op in release builds
            let session = std::sync::Arc::new($crate::SharedRecordingSession::new(
                $crate::RecordingConfig::minimal(),
            ));
            (session, None::<$crate::ServerHandle>)
        }
    }};
    ($app_name:expr) => {{
        #[cfg(debug_assertions)]
        {
            let session = std::sync::Arc::new($crate::SharedRecordingSession::new(
                $crate::RecordingConfig::debug(),
            ));
            $crate::install_recorder(session.clone());
            $crate::install_hooks();

            let server_handle = $crate::start_local_server_named($app_name, session.clone())
                .expect("Failed to start debug server");

            session.start();
            (session, Some(server_handle))
        }
        #[cfg(not(debug_assertions))]
        {
            let _ = $app_name; // Suppress unused warning
            let session = std::sync::Arc::new($crate::SharedRecordingSession::new(
                $crate::RecordingConfig::minimal(),
            ));
            (session, None::<$crate::ServerHandle>)
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thread_local_recorder() {
        let session = Arc::new(SharedRecordingSession::new(RecordingConfig::minimal()));
        install_recorder(session.clone());

        assert!(get_recorder().is_some());

        session.start();
        assert!(is_recording());

        record_event(RecordedEvent::Click(MouseEvent {
            position: Point::new(100.0, 100.0),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            target_element: Some("button".to_string()),
        }));

        assert_eq!(session.stats().total_events, 1);

        uninstall_recorder();
        assert!(get_recorder().is_none());
    }
}
