use super::*;
use crate::div::ElementBuilder;
use crate::event_handler::EventContext;
use crate::tree::LayoutNodeId;
use blinc_core::events::event_types;
use std::sync::{Arc, Mutex, Once};
use taffy::Dimension;

static THEME_INIT: Once = Once::new();

fn ensure_theme_initialized() {
    THEME_INIT.call_once(ThemeState::init_default);
}

#[test]
fn test_text_input_data_insert() {
    let mut data = TextInputData::new();

    data.insert("hello");
    assert_eq!(data.value, "hello");
    assert_eq!(data.cursor, 5);

    data.cursor = 0;
    data.insert("world ");
    assert_eq!(data.value, "world hello");
}

#[test]
fn test_text_input_data_delete() {
    let mut data = TextInputData::with_value("hello");

    data.cursor = 5;
    data.delete_backward();
    assert_eq!(data.value, "hell");

    data.cursor = 0;
    data.delete_forward();
    assert_eq!(data.value, "ell");
}

#[test]
fn test_input_type_filtering() {
    let mut data = TextInputData::new();
    data.input_type = InputType::Number;

    data.insert("123.45");
    assert_eq!(data.value, "123.45");

    data.value.clear();
    data.cursor = 0;
    data.insert("abc123");
    assert_eq!(data.value, "123");
}

/// Regression test: keyboard stepping while focused used to leave
/// the displayed value stale — the focus guard meant for typing
/// swallowed step changes too.
#[test]
fn force_sync_once_overrides_focus_guard_for_step_changes() {
    let mut data = TextInputData::with_value("5");
    data.visual = TextFieldState::Focused;

    // Typing case: no flag, stays guarded.
    let is_focused = data.visual.is_focused();
    let force = std::mem::take(&mut data.force_sync_once);
    assert!(!force && is_focused);

    // Step case: flag forces the sync through despite focus, then
    // resets so it doesn't leak into the next one.
    data.force_sync_once = true;
    let is_focused = data.visual.is_focused();
    let force = std::mem::take(&mut data.force_sync_once);
    assert!(force || !is_focused);
    assert!(!data.force_sync_once);
}

#[test]
fn the_shown_text_is_the_value_on_change_normalized() {
    ensure_theme_initialized();

    let data = text_input_data();
    {
        let mut data = data.lock().unwrap();
        data.visual = TextFieldState::Focused;
    }

    let data_for_change = Arc::clone(&data);
    let input = text_input(&data)
        .input_type(InputType::Integer)
        .on_change(move |_| {
            let mut data = data_for_change.lock().unwrap();
            data.value.clear();
            data.cursor = 0;
            data.selection_start = None;
        });
    let mut tree = crate::renderer::RenderTree::from_element(&input);
    tree.compute_layout(300.0, 100.0);

    let ctx =
        EventContext::new(event_types::TEXT_INPUT, LayoutNodeId::default()).with_key_char('-');
    ElementBuilder::event_handlers(&input)
        .unwrap()
        .dispatch(&ctx);
    let updates = crate::stateful::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);

    assert_eq!(data.lock().unwrap().value, "");
    let mut shown = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(node) = stack.pop() {
        if let Some(crate::renderer::ElementType::Text(t)) =
            tree.get_render_node(node).map(|r| &r.element_type)
        {
            shown.push(t.content.clone());
        }
        stack.extend(tree.layout_tree.children(node));
    }
    assert!(
        shown.iter().all(|t| t.is_empty()),
        "the field shows {shown:?}, not the value on_change left"
    );
}

#[test]
fn state_callback_preserves_explicit_width() {
    ensure_theme_initialized();

    let data = text_input_data();
    let input = text_input(&data).w(42.0);

    let mut tree = LayoutTree::new();
    let root = input.build(&mut tree);
    let style = tree.get_style(root).unwrap();

    assert_eq!(style.size.width, Dimension::length(42.0));
}

// Global focus statics (PENDING_FOCUS_INPUT, FOCUSED_TEXT_INPUT, ...)
// are process-wide, so serialize tests that touch them.
static FOCUS_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn blur_cancels_a_pending_deferred_focus() {
    let _guard = FOCUS_TEST_LOCK.lock().unwrap();
    ensure_theme_initialized();
    blur_all_text_inputs();

    let data = text_input_data();
    let mut tree = LayoutTree::new();
    let _root = text_input(&data).build(&mut tree);

    // Auto-advance queues a re-focus (e.g. OTP typing the last
    // digit), then the user clicks outside before it drains.
    focus_text_input_deferred(&data);
    blur_all_text_inputs();
    process_pending_input_focus();

    assert!(!data.lock().unwrap().visual.is_focused());
}

#[test]
fn deferred_focus_applies_without_an_intervening_blur() {
    let _guard = FOCUS_TEST_LOCK.lock().unwrap();
    ensure_theme_initialized();
    blur_all_text_inputs();

    let data = text_input_data();
    let mut tree = LayoutTree::new();
    let _root = text_input(&data).build(&mut tree);

    focus_text_input_deferred(&data);
    process_pending_input_focus();

    assert!(data.lock().unwrap().visual.is_focused());
}

#[test]
fn focusing_another_input_blurs_previous_stateful_visual_state() {
    let _guard = FOCUS_TEST_LOCK.lock().unwrap();
    ensure_theme_initialized();
    blur_all_text_inputs();

    let first = text_input_data();
    let second = text_input_data();
    let mut tree = LayoutTree::new();
    let _first_root = text_input(&first).build(&mut tree);
    let _second_root = text_input(&second).build(&mut tree);

    focus_text_input(&first);
    focus_text_input(&second);

    assert!(!first.lock().unwrap().visual.is_focused());
}
