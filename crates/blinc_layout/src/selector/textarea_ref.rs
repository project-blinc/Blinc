//! `TextareaRef` — a handle onto a multi-line field.
//!
//! The same two halves as [`crate::selector::InputRef`]: a
//! [`crate::selector::DivRef`] for what every element can do, and the
//! field's own state for reading and replacing what it holds.
//!
//! Separate from `InputRef` because the widget underneath is: a text
//! area keeps `Vec<String>` lines and a `(line, column)` cursor, where
//! a text input keeps one string and a byte offset. Sharing a ref type
//! would mean a value method that means something different depending
//! on what happened to be bound.

use std::sync::{Arc, Mutex};

use crate::widgets::text_area::SharedTextAreaState;

use super::DivRef;

/// A handle onto a multi-line field.
///
/// ```rust,ignore
/// let bio = TextareaRef::new();
///
/// text_area(&state).bind(&bio)
///
/// // Later:
/// bio.focus();
/// bio.set_value("first line\nsecond line");
/// ```
#[derive(Clone)]
pub struct TextareaRef {
    element: DivRef,
    state: Arc<Mutex<Option<SharedTextAreaState>>>,
}

impl Default for TextareaRef {
    fn default() -> Self {
        Self::new()
    }
}

impl TextareaRef {
    /// A ref bound to nothing yet.
    pub fn new() -> Self {
        Self {
            // The field routes its own focus, so the element half never
            // reads an id — see `InputRef` for why assigning one to a
            // field that already handles its own input is a change it
            // did not ask for.
            element: DivRef::without_id_assignment(),
            state: Arc::default(),
        }
    }

    /// The element half, for a builder that binds the node.
    pub fn element(&self) -> &DivRef {
        &self.element
    }

    /// Point this ref at a field's state. Called by the builder, which
    /// already holds it.
    pub fn bind_state(&self, state: &SharedTextAreaState) {
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(state));
    }

    /// Whether anything has bound this ref.
    pub fn is_bound(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// Whether the bound element is still in the tree.
    pub fn exists(&self) -> bool {
        self.element.exists()
    }

    /// The full text, lines joined. `None` until something binds it.
    pub fn value(&self) -> Option<String> {
        self.with_state("value", |s| s.value())
    }

    /// Replace the contents. Newlines split into lines, as typing them
    /// would.
    pub fn set_value(&self, value: impl AsRef<str>) {
        self.with_state("set_value", |s| s.set_value(value.as_ref()));
        self.refresh();
    }

    /// Empty the field.
    pub fn clear(&self) {
        self.set_value("");
    }

    /// Select everything in the field.
    pub fn select_all(&self) {
        self.with_state("select_all", |s| s.select_all());
        self.refresh();
    }

    /// Give the field keyboard focus.
    pub fn focus(&self) {
        match self.state_handle() {
            Some(state) => crate::widgets::text_area::focus_text_area(&state),
            None => self.element.focus(),
        }
    }

    /// Drop focus, if this field holds it.
    pub fn blur(&self) {
        self.element.blur();
    }

    /// Bring the field into view inside its scroll container.
    pub fn scroll_into_view(&self) {
        self.element.scroll_into_view();
    }

    fn state_handle(&self) -> Option<SharedTextAreaState> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(Arc::clone)
    }

    fn with_state<T>(
        &self,
        action: &str,
        f: impl FnOnce(&mut crate::widgets::text_area::TextAreaState) -> T,
    ) -> Option<T> {
        let Some(state) = self.state_handle() else {
            tracing::warn!(action, "TextareaRef is not bound to a field");
            return None;
        };
        let mut guard = state.lock().unwrap_or_else(|e| e.into_inner());
        Some(f(&mut guard))
    }

    /// Writing the state changes nothing on screen by itself — the
    /// callback that builds the visible text has to run again. Same
    /// reason `InputRef` refreshes rather than asking for a redraw.
    fn refresh(&self) {
        let Some(state) = self.state_handle() else {
            return;
        };
        let stateful = state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stateful_state
            .clone();
        if let Some(stateful) = stateful {
            crate::stateful::refresh_stateful(&stateful);
        }
        crate::stateful::request_redraw();
    }
}

/// A ref is captured by handlers, and a `Stateful` callback is
/// `Send + Sync`.
const _: fn() = || {
    fn assert<T: Send + Sync>() {}
    assert::<TextareaRef>();
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::text_area::text_area_state;

    #[test]
    fn an_unbound_ref_reads_nothing_and_does_nothing() {
        let r = TextareaRef::new();
        assert!(!r.is_bound());
        assert_eq!(r.value(), None);
        r.set_value("ignored");
        r.clear();
        r.select_all();
        r.focus();
    }

    #[test]
    fn a_bound_ref_reads_and_writes_the_field() {
        let state = text_area_state();
        let r = TextareaRef::new();
        r.bind_state(&state);

        r.set_value("hello");
        assert_eq!(r.value().as_deref(), Some("hello"));
        assert_eq!(state.lock().unwrap().value(), "hello");
    }

    /// Newlines become lines, which is the whole difference from a text
    /// input: a value written as one string has to come back as one.
    #[test]
    fn newlines_round_trip_as_lines() {
        let state = text_area_state();
        let r = TextareaRef::new();
        r.bind_state(&state);

        r.set_value("first\nsecond\nthird");
        assert_eq!(state.lock().unwrap().lines.len(), 3, "stored as lines");
        assert_eq!(r.value().as_deref(), Some("first\nsecond\nthird"));
    }

    /// An empty field is one empty line, not zero lines — nothing that
    /// indexes `lines[cursor.line]` survives an empty vector.
    #[test]
    fn clearing_leaves_one_empty_line() {
        let state = text_area_state();
        let r = TextareaRef::new();
        r.bind_state(&state);

        r.set_value("something");
        r.clear();
        assert_eq!(r.value().as_deref(), Some(""));
        assert_eq!(state.lock().unwrap().lines.len(), 1);
    }
}
