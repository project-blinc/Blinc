//! Drag and drop support
//!
//! Handle files and content dragged onto the application window.
//!
//! # Example
//!
//! ```ignore
//! use blinc_app::dnd::{on_file_drop, DropEvent};
//!
//! on_file_drop(|event| {
//!     match event {
//!         DropEvent::Hovered(paths) => println!("Hovering: {:?}", paths),
//!         DropEvent::Dropped(paths) => println!("Dropped: {:?}", paths),
//!         DropEvent::Cancelled => println!("Cancelled"),
//!     }
//! });
//! ```

use std::path::PathBuf;
use std::sync::Mutex;

/// Events for file drag-and-drop operations
#[derive(Clone, Debug)]
pub enum DropEvent {
    /// Files are being dragged over the window
    Hovered(Vec<PathBuf>),
    /// Files were dropped onto the window
    Dropped(Vec<PathBuf>),
    /// The drag operation was cancelled (dragged away from window)
    Cancelled,
}

type DropCallback = Box<dyn Fn(DropEvent) + Send + Sync>;

static DROP_HANDLER: Mutex<Option<DropCallback>> = Mutex::new(None);

/// Register a callback for file drop events.
///
/// Only one handler can be active at a time. Calling this replaces the previous handler.
pub fn on_file_drop<F>(callback: F)
where
    F: Fn(DropEvent) + Send + Sync + 'static,
{
    *DROP_HANDLER.lock().unwrap() = Some(Box::new(callback));
}

/// Clear the file drop handler
pub fn clear_file_drop_handler() {
    *DROP_HANDLER.lock().unwrap() = None;
}

/// Internal: dispatch a drop event to the registered handler
pub(crate) fn dispatch_drop_event(event: DropEvent) {
    if let Ok(guard) = DROP_HANDLER.lock()
        && let Some(ref handler) = *guard
    {
        handler(event);
    }
}
