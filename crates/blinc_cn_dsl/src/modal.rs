//! Signal-driven overlays: dialogs, sheets, drawers.
//!
//! The Rust widgets are imperative — `show()` pushes onto the overlay
//! stack and hands back a handle to close later. A DSL source has
//! nowhere to keep that handle, so the signal takes its place: set it
//! and the overlay appears, clear it and it goes.
//!
//! Three things have to hold, and each of them failed on the way here:
//!
//! * The element has to be **subscribed**. Nothing else in a view
//!   depends on an overlay's `open` signal, so a plain element is built
//!   once and a write raises nothing until something unrelated drives a
//!   frame.
//! * `show` has to be **idempotent**. The watcher re-renders on every
//!   frame it is dirty, and showing an already-open overlay stacks a
//!   second copy behind the first.
//! * The live handle has to live **outside the widget**. The widget
//!   struct is rebuilt every frame, so a handle stored on it is empty
//!   next time round and the overlay goes up again, once per frame.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use blinc_core::reactive::State;
use blinc_layout::div::{Div, div};
use blinc_layout::stateful::{Stateful, stateful_with_key};
use blinc_layout::widgets::overlay_stack::OverlayHandle;

/// Overlays currently up, by the signal that opened them.
fn open_overlays() -> &'static Mutex<HashMap<u64, OverlayHandle>> {
    static OVERLAYS: OnceLock<Mutex<HashMap<u64, OverlayHandle>>> = OnceLock::new();
    OVERLAYS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// An element that draws nothing and keeps an overlay in step with a
/// signal.
///
/// `show` is called when the signal turns true and nothing is up for
/// it; the handle it returns is closed when the signal turns false.
///
/// Keyed by the signal rather than by call site. A call id is not
/// always available here — it reads as zero in this path, which put
/// every overlay in a program on one key, so a second one silently
/// never opened. The signal is always there, and two overlays bound to
/// one signal are one overlay by construction.
pub fn watcher<F>(open: State<bool>, show: F) -> Stateful<()>
where
    F: Fn() -> OverlayHandle + Send + Sync + 'static,
{
    let signal_id = open.signal_id();
    let key = signal_id.to_raw();
    stateful_with_key::<()>(format!("cn_modal_{key}"))
        .deps([signal_id])
        .on_state(move |_ctx| {
            sync(key, open.try_get().unwrap_or(false), &show);
            div()
        })
}

fn sync(key: u64, should_be_open: bool, show: &impl Fn() -> OverlayHandle) {
    // Nothing that can re-enter runs under the guard. `show` builds an
    // overlay and `close` tears one down, and both reach back into the
    // overlay stack — and, once an overlay reports its own dismissal,
    // into whatever the caller bound to it, which arrives back here.
    // `std::sync::Mutex` is not re-entrant, so a callback invoked under
    // the guard hangs the thread rather than failing. See
    // `gotcha_overlay_handle_lock_reentrancy`.
    let live = {
        let overlays = open_overlays().lock().expect("open overlays");
        overlays.get(&key).is_some_and(|h| h.is_live())
    };

    match (should_be_open, live) {
        (true, false) => {
            let handle = show();
            open_overlays()
                .lock()
                .expect("open overlays")
                .insert(key, handle);
        }
        (false, true) => {
            // Removed BEFORE closing: `close` reaches back into the
            // overlay stack, and a handle left behind would read as live
            // on the next frame. Removing first also terminates the
            // cycle a self-reporting overlay creates — its dismissal
            // clears the bound signal, which lands here again and finds
            // nothing live to close.
            let handle = open_overlays().lock().expect("open overlays").remove(&key);
            if let Some(h) = handle {
                h.close();
            }
        }
        _ => {}
    }
}

/// Clear `open` when the overlay closes itself.
///
/// A backdrop click, an Escape, a close button — all of them dismiss
/// the overlay without the signal knowing. Left set, it would raise the
/// overlay again on the next frame.
pub fn closing_handler(open: State<bool>, user: i64) -> impl Fn() + Send + Sync + 'static {
    move || {
        open.set(false);
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
