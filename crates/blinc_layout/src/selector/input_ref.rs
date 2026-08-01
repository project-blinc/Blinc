//! `InputRef` — a handle onto a text field.
//!
//! Composes a [`crate::selector::DivRef`] for the things every element
//! can do (focus, blur, scroll into view) with the field's own state for
//! the things only a text field can (read the value, replace it, select
//! it).
//!
//! Unlike the element half, the value half needs no round trip through
//! the renderer: a text input is built from a `SharedTextInputData` its
//! caller already holds, so binding can hand it over immediately.

use std::sync::{Arc, Mutex};

use crate::widgets::text_input::SharedTextInputData;

use super::DivRef;

/// A handle onto a text field.
///
/// ```rust,ignore
/// let email = InputRef::new();
///
/// text_input(&data).bind(&email)
///
/// // Later:
/// email.focus();
/// email.select_all();
/// let typed = email.value();
/// ```
#[derive(Clone, Default)]
pub struct InputRef {
    /// Everything an ordinary element can do.
    element: DivRef,
    /// The field's own state, once something binds it.
    data: Arc<Mutex<Option<SharedTextInputData>>>,
}

impl InputRef {
    /// A ref bound to nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The element half, for a builder that binds the node.
    pub fn element(&self) -> &DivRef {
        &self.element
    }

    /// Point this ref at a field's state. Called by the builder, which
    /// already holds it.
    pub fn bind_data(&self, data: &SharedTextInputData) {
        *self.data.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(data));
    }

    /// Whether anything has bound this ref.
    pub fn is_bound(&self) -> bool {
        self.data
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// Whether the bound element is still in the tree.
    ///
    /// Distinct from [`is_bound`](Self::is_bound): the state outlives
    /// the element, which is the point of holding it outside the tree.
    pub fn exists(&self) -> bool {
        self.element.exists()
    }

    /// What the field currently holds. `None` until something binds it.
    pub fn value(&self) -> Option<String> {
        self.with_data("value", |d| d.value.clone())
    }

    /// Replace the contents, leaving the cursor at the end.
    pub fn set_value(&self, value: impl Into<String>) {
        let value = value.into();
        self.with_data("set_value", |d| {
            d.cursor = value.chars().count();
            d.selection_start = None;
            d.value = value.clone();
        });
    }

    /// Empty the field.
    pub fn clear(&self) {
        self.set_value("");
    }

    /// Select everything in the field.
    pub fn select_all(&self) {
        self.with_data("select_all", |d| d.select_all());
    }

    /// Give the field keyboard focus.
    pub fn focus(&self) {
        self.element.focus();
    }

    /// Drop focus, if this field holds it.
    pub fn blur(&self) {
        self.element.blur();
    }

    /// Bring the field into view inside its scroll container.
    pub fn scroll_into_view(&self) {
        self.element.scroll_into_view();
    }

    fn with_data<T>(
        &self,
        action: &str,
        f: impl FnOnce(&mut crate::widgets::text_input::TextInputData) -> T,
    ) -> Option<T> {
        let guard = self.data.lock().unwrap_or_else(|e| e.into_inner());
        let Some(data) = guard.as_ref() else {
            tracing::warn!(action, "InputRef is not bound to a field");
            return None;
        };
        let mut data = data.lock().unwrap_or_else(|e| e.into_inner());
        Some(f(&mut data))
    }
}

/// A ref is captured by handlers, and a `Stateful` callback is
/// `Send + Sync`.
const _: fn() = || {
    fn assert<T: Send + Sync>() {}
    assert::<InputRef>();
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::text_input::text_input_data;

    #[test]
    fn an_unbound_ref_reads_nothing_and_does_nothing() {
        let r = InputRef::new();
        assert!(!r.is_bound());
        assert_eq!(r.value(), None);
        // Commands are no-ops rather than panics.
        r.set_value("ignored");
        r.clear();
        r.select_all();
        r.focus();
    }

    #[test]
    fn a_bound_ref_reads_and_writes_the_field() {
        let data = text_input_data();
        let r = InputRef::new();
        r.bind_data(&data);

        r.set_value("hello");
        assert_eq!(r.value().as_deref(), Some("hello"));
        // The field itself sees it, not just the ref.
        assert_eq!(data.lock().unwrap().value, "hello");

        r.clear();
        assert_eq!(r.value().as_deref(), Some(""));
    }

    /// Replacing the contents leaves the cursor somewhere valid — a
    /// cursor past the end indexes out of the string on the next edit.
    #[test]
    fn set_value_leaves_the_cursor_inside_the_new_text() {
        let data = text_input_data();
        let r = InputRef::new();
        r.bind_data(&data);

        r.set_value("a longer string");
        r.set_value("short");
        let d = data.lock().unwrap();
        assert!(
            d.cursor <= d.value.chars().count(),
            "cursor {} is inside {:?}",
            d.cursor,
            d.value
        );
    }

    /// The state outlives the element: a field scrolled out of the tree
    /// still knows what was typed into it.
    #[test]
    fn bound_state_survives_the_element_going_away() {
        let data = text_input_data();
        let r = InputRef::new();
        r.bind_data(&data);
        r.set_value("typed");

        assert!(!r.exists(), "nothing bound the element half");
        assert_eq!(r.value().as_deref(), Some("typed"), "the value stands");
    }
}
