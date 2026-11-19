use super::*;

#[test]
fn right_click_preserves_selection_without_leaving_a_row_hit() {
    blinc_theme::ThemeState::init_default();
    let state = text_area_state();
    state.lock().unwrap().set_value("hello world\nsecond line");
    let area = text_area(&state);
    let tree = crate::renderer::RenderTree::from_element(&area);
    {
        let mut d = state.lock().unwrap();
        d.selection_start = Some(TextPosition::new(0, 6));
        d.cursor = TextPosition::new(0, 11);
        d.clicked_visual_line = Some(1);
    }
    let mut ctx = crate::event_handler::EventContext::new(
        blinc_core::events::event_types::POINTER_DOWN,
        tree.root().unwrap(),
    );
    ctx.mouse_button = 2;
    area.event_handlers().unwrap().dispatch(&ctx);
    let d = state.lock().unwrap();
    assert_eq!(d.selected_text().as_deref(), Some("world"));
    assert_eq!(d.clicked_visual_line, None);
}

#[test]
fn repeated_clicks_require_time_and_distance_proximity() {
    let mut clicks = super::selection::ClickSequence::default();
    let now = web_time::Instant::now();
    assert_eq!(clicks.register((10.0, 10.0), now), 1);
    assert_eq!(clicks.register((12.0, 10.0), now), 2);
    assert_eq!(clicks.register((12.0, 11.0), now), 3);
    assert_eq!(clicks.register((12.0, 11.0), now), 1);
    assert_eq!(clicks.register((40.0, 11.0), now), 1);
    assert_eq!(
        clicks.register((40.0, 11.0), now + std::time::Duration::from_millis(401)),
        1,
    );
}

#[test]
fn repainting_unchanged_lines_preserves_manual_scrolling() {
    blinc_theme::ThemeState::init_default();
    let cfg = TextAreaConfig {
        rows: Some(2),
        ..Default::default()
    };
    let mut state = TextAreaState::with_value("one\ntwo\nthree\nfour\nfive");
    state.visual = TextFieldState::Focused;
    layout_lines(&mut state, &cfg);
    state.scroll_physics.lock().unwrap().offset_y = -state.line_height;
    layout_lines(&mut state, &cfg);
    assert!((state.scroll_offset() - state.line_height).abs() < 0.01);
}

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

/// Wrapping, the caret and the click-to-caret mapping use the letter spacing the text is
/// drawn with, taken from the field's CSS like the font size is.
#[test]
fn lines_caret_and_hit_test_follow_css_letter_spacing() {
    use crate::text_measure::{TextLayoutOptions, measure_text_with_options};
    blinc_theme::ThemeState::init_default();
    let sheet = Stylesheet::parse(".tracked { letter-spacing: -3px; }").unwrap();
    let mut cfg = TextAreaConfig::default();
    apply_css_overrides_textarea(
        &mut cfg,
        &sheet,
        None,
        &[Arc::from("tracked")],
        &TextFieldState::Idle,
    );
    let drawn = |text: &str| {
        measure_text_with_options(
            text,
            cfg.font_size,
            &TextLayoutOptions::new().with_letter_spacing(-3.0),
        )
        .width
    };

    let mut state = TextAreaState::with_value("hello world");
    take_metrics(&mut state, &cfg);
    state.wrap_enabled = false;
    state.compute_visual_lines();
    state.cursor = TextPosition::new(0, 5);
    assert_eq!(state.cursor_x_in_visual_line(), drawn("hello"));
    assert_eq!(state.visual_lines[0].width, drawn("hello world"));

    // A click on the drawn edge after three characters lands after three.
    assert_eq!(
        state.char_position_in_visual_line(0, drawn("hel"), cfg.font_size),
        3
    );

    // Wrapping breaks where the drawn text runs out of room: the width that fits "hello world"
    // drawn tight breaks it when drawn without spacing.
    state.available_width = drawn("hello world") + 0.5;
    state.wrap_enabled = true;
    state.compute_visual_lines();
    assert_eq!(state.visual_lines.len(), 1);
    state.face = crate::text_measure::TextFace::default();
    state.compute_visual_lines();
    assert!(state.visual_lines.len() > 1);
}
