use super::*;

#[test]
fn test_text_area_state_insert() {
    let mut state = TextAreaState::new();
    state.insert("hello");
    assert_eq!(state.value(), "hello");

    state.insert_newline();
    state.insert("world");
    assert_eq!(state.value(), "hello\nworld");
    assert_eq!(state.line_count(), 2);
}

#[test]
fn test_text_area_state_delete() {
    let mut state = TextAreaState::with_value("hello\nworld");
    state.cursor = TextPosition::new(1, 5);

    state.delete_backward();
    assert_eq!(state.value(), "hello\nworl");

    state.cursor = TextPosition::new(1, 0);
    state.delete_backward();
    assert_eq!(state.value(), "helloworl");
    assert_eq!(state.line_count(), 1);
}

#[test]
fn test_text_area_state_navigation() {
    let mut state = TextAreaState::with_value("line1\nline2\nline3");
    state.cursor = TextPosition::new(1, 3);

    state.move_up(false);
    assert_eq!(state.cursor, TextPosition::new(0, 3));

    state.move_down(false);
    assert_eq!(state.cursor, TextPosition::new(1, 3));

    state.move_to_line_start(false);
    assert_eq!(state.cursor, TextPosition::new(1, 0));

    state.move_to_line_end(false);
    assert_eq!(state.cursor, TextPosition::new(1, 5));
}

#[test]
fn test_text_area_state_selection() {
    let mut state = TextAreaState::with_value("hello\nworld");

    state.select_all();
    assert_eq!(state.selected_text(), Some("hello\nworld".to_string()));

    state.insert("new");
    assert_eq!(state.value(), "new");
    assert_eq!(state.line_count(), 1);
}
