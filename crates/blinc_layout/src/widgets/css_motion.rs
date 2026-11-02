//! Enter and exit run by CSS rules, for what an overlay stack or toast tray
//! keeps mounted while it leaves.
//!
//! The element that moves carries a class and `data-motion="enter"`, then
//! `"exit"` once it starts to leave, changed in place; rules name the
//! `@keyframes` each plays. The owner keeps it until its exit has played,
//! watched through [`crate::css_animation_playing`] by the element's id.

use std::sync::Arc;

use blinc_core::reactive::{ReactiveGraph, Signal, computed, signal};

use crate::div::Div;

/// How long an exit may run before it counts as done regardless; and how long
/// to wait for one that never starts, as when no rule names one.
const EXIT_MAX_MS: u64 = 1000;
const EXIT_START_MS: u64 = 100;

/// One element's CSS-run enter and exit.
pub struct CssMotion {
    class: Arc<str>,
    id: String,
    exiting: Signal<bool>,
    exit_started_ms: Option<u64>,
    /// The exit animation has been seen playing.
    exit_seen: bool,
}

impl CssMotion {
    /// Motion for an element of `class` carrying the id `id`.
    pub fn new(class: Arc<str>, id: String) -> Self {
        Self {
            class,
            id,
            exiting: signal(false),
            exit_started_ms: None,
            exit_seen: false,
        }
    }

    pub fn class(&self) -> &str {
        &self.class
    }

    /// Start leaving: `data-motion` becomes `"exit"`.
    pub fn exit(&self) {
        self.exiting.set(true);
    }

    /// `el` as the moving element: its class, id and `data-motion`.
    pub fn element(&self, el: Div) -> Div {
        self.state(el.class(self.class.as_ref()).id(&self.id))
    }

    /// `el` carrying this motion's `data-motion`, for a part that moves with
    /// it (a backdrop).
    pub fn state(&self, el: Div) -> Div {
        let exiting = self.exiting;
        let entering = computed(move |g: &ReactiveGraph| !g.get(exiting).unwrap_or(false));
        el.attr_when("data-motion", "enter", &entering)
            .attr_when("data-motion", "exit", exiting)
    }

    /// Whether the exit has played out, as of `now` (ms).
    pub fn exit_done(&mut self, now: u64) -> bool {
        let started = *self.exit_started_ms.get_or_insert(now);
        let playing = crate::css_animation_playing(&self.id);
        self.exit_seen |= playing;
        let elapsed = now.saturating_sub(started);
        (self.exit_seen && !playing)
            || (!self.exit_seen && elapsed >= EXIT_START_MS)
            || elapsed >= EXIT_MAX_MS
    }
}
