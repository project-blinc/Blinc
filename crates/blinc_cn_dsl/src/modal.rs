//! Signal-driven overlays: dialogs, sheets, drawers.
//!
//! The Rust widgets are imperative — `show()` pushes onto the overlay
//! stack and hands back a handle to close later. A DSL source has
//! nowhere to keep that handle, so the signal takes its place: set it
//! and the overlay appears, clear it and it goes.
//!
//! The element is a region that holds while the signal does. Building it
//! raises the overlay, once; tearing it down closes the overlay, through
//! the handle its scope kept. Nothing is built or kept while the signal is
//! false, so there is no handle to lose across rebuilds and no second copy
//! to stack behind the first.

use blinc_core::reactive::State;
use blinc_layout::div::{Div, div};
use blinc_layout::widgets::overlay_stack::OverlayHandle;

/// An element that draws nothing and keeps an overlay in step with a
/// signal.
///
/// `show` is called when the signal turns true; the handle it returns is
/// closed when the signal turns false.
pub fn watcher<F>(open: State<bool>, show: F) -> Div
where
    F: Fn() -> OverlayHandle + 'static,
{
    div().show(&open, move || {
        let handle = show();
        blinc_core::owner::Owner::on_cleanup(move || {
            // Outside the overlay stack's lock: closing reaches back into
            // it, and into whatever the caller bound to the overlay.
            handle.close();
        });
        div()
    })
}

/// Clear `open` when the overlay closes itself.
///
/// A backdrop click, an Escape, a close button — all of them dismiss
/// the overlay without the signal knowing. Left set, it would raise the
/// overlay again on the next frame.
pub fn closing_handler(open: State<bool>, user: i64) -> impl Fn() + Send + Sync + 'static {
    move || {
        // Only a change: writing the value it already holds wakes whatever
        // follows it, and this runs while the overlay it bound is closing.
        if open.get() {
            open.set(false);
        }
        if user != 0 {
            type ClosureFn = extern "C" fn();
            // SAFETY: Zyntax mints a zero-arg `extern "C" fn()` for a
            // DSL closure and hands the pointer across as `i64`.
            let func: ClosureFn = unsafe { std::mem::transmute(user) };
            func();
        }
    }
}

/// A body block as something a `content` slot can take, or `None` when
/// the block was empty.
pub fn content_recipe(
    children: Vec<Box<dyn blinc_layout::div::ElementBuilder>>,
) -> Option<std::sync::Arc<dyn Fn() -> Div + Send + Sync>> {
    (!children.is_empty()).then(|| -> std::sync::Arc<dyn Fn() -> Div + Send + Sync> {
        std::sync::Arc::new(crate::shared_child::body_recipe(children))
    })
}
