//! Text-area resizing and pointer selection through the widget's event handlers.

use blinc_core::events::event_types;
use blinc_layout::div::ElementBuilder;
use blinc_layout::event_handler::EventContext;
use blinc_layout::renderer::RenderTree;
use blinc_layout::text_measure::TextFace;
use blinc_layout::widgets::text_area::{
    SharedTextAreaState, TextArea, TextAreaConfig, TextPosition, text_area, text_area_state,
};
use blinc_layout::widgets::text_input::{
    blur_all_text_inputs, cancel_long_press_timer, fire_long_press_timer_if_due,
    is_long_press_armed, set_touch_input,
};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

struct Scene {
    area: TextArea,
    tree: RenderTree,
    state: SharedTextAreaState,
}

impl Scene {
    fn new(text: &str, rows: usize, width: f32) -> Self {
        blinc_theme::ThemeState::init_default();
        blur_all_text_inputs();
        set_touch_input(false);
        let state = text_area_state();
        state.lock().unwrap().set_value(text);
        Self::build(state, rows, width)
    }

    fn build(state: SharedTextAreaState, rows: usize, width: f32) -> Self {
        let area = text_area(&state).w(width).rows(rows);
        let mut tree = RenderTree::from_element(&area);
        tree.compute_layout(400.0, 600.0);
        Self { area, tree, state }
    }

    fn dispatch(&mut self, mut event: EventContext) {
        let root = self.tree.root().unwrap();
        let bounds = self.tree.get_absolute_bounds(root).unwrap();
        event.bounds_width = bounds.width;
        event.bounds_height = bounds.height;
        self.area.event_handlers().unwrap().dispatch(&event);
        self.frame();
    }

    fn frame(&mut self) {
        let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(400.0, 600.0);
    }

    fn event(&self, kind: u32) -> EventContext {
        EventContext::new(kind, self.tree.root().unwrap())
    }

    fn point(&self, line: usize, column: usize) -> (f32, f32) {
        let cfg = TextAreaConfig::default();
        let d = self.state.lock().unwrap();
        let prefix: String = d.get_line(line).unwrap().chars().take(column).collect();
        (
            cfg.padding_x
                + cfg.border_width
                + TextFace::default().measure(&prefix, cfg.font_size).width,
            cfg.padding_y
                + cfg.border_width
                + line as f32 * cfg.font_size * cfg.line_height
                + cfg.font_size * 0.5
                - d.scroll_offset(),
        )
    }

    fn press(&mut self, line: usize, column: usize, shift: bool) {
        let (x, y) = self.point(line, column);
        self.dispatch(
            self.event(event_types::POINTER_DOWN)
                .with_local_pos(x, y)
                .with_modifiers(shift, false, false, false),
        );
    }

    fn release(&mut self) {
        self.dispatch(self.event(event_types::POINTER_UP));
    }

    fn click(&mut self, line: usize, column: usize) {
        self.press(line, column, false);
        self.release();
    }

    fn selected(&self) -> Option<String> {
        self.state.lock().unwrap().selected_text()
    }
}

struct InputGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl InputGuard {
    fn new() -> Self {
        Self {
            _lock: LOCK.lock().unwrap_or_else(|e| e.into_inner()),
        }
    }
}

impl Drop for InputGuard {
    fn drop(&mut self) {
        cancel_long_press_timer();
        set_touch_input(false);
        blur_all_text_inputs();
    }
}

#[test]
fn growing_rows_clear_scroll_without_another_edit() {
    let _guard = InputGuard::new();
    let mut s = Scene::new("", 1, 300.0);
    s.click(0, 0);
    for _ in 0..4 {
        s.dispatch(s.event(event_types::KEY_DOWN).with_key_code(13));
        assert!(s.state.lock().unwrap().scroll_offset() > 1.0);
        let rows = s.state.lock().unwrap().line_count();
        let next = Scene::build(s.state.clone(), rows, 300.0);
        // Keep the same focused state across the caller's rebuild.
        s.area = next.area;
        s.tree = next.tree;
        let offset = s.state.lock().unwrap().scroll_offset();
        assert!(
            offset.abs() < 0.01,
            "rows={rows}, offset={offset}, bounds={:?}",
            s.tree.get_absolute_bounds(s.tree.root().unwrap())
        );
    }
}

#[test]
fn resizing_preserves_real_overflow_and_reflows_wrapped_lines() {
    let _guard = InputGuard::new();
    let mut s = Scene::new("first\nsecond\nthird\nfourth", 1, 300.0);
    s.click(0, 0);
    s.dispatch(s.event(event_types::KEY_DOWN).with_key_code(35));
    s.dispatch(s.event(event_types::KEY_DOWN).with_key_code(40));
    s.dispatch(s.event(event_types::KEY_DOWN).with_key_code(40));
    s.dispatch(s.event(event_types::KEY_DOWN).with_key_code(40));
    let next = Scene::build(s.state.clone(), 2, 300.0);
    s.area = next.area;
    s.tree = next.tree;
    let line_height = TextAreaConfig::default().font_size * TextAreaConfig::default().line_height;
    let offset = s.state.lock().unwrap().scroll_offset();
    assert!(
        (offset - 2.0 * line_height).abs() < 0.01,
        "offset={offset}, line_height={line_height}, bounds={:?}",
        s.tree.get_absolute_bounds(s.tree.root().unwrap())
    );

    s.state
        .lock()
        .unwrap()
        .set_value("one two three four five six seven eight");
    let next = Scene::build(s.state.clone(), 1, 75.0);
    s.area = next.area;
    s.tree = next.tree;
    assert!(s.state.lock().unwrap().scroll_offset() > 1.0);
    let next = Scene::build(s.state.clone(), 4, 350.0);
    s.area = next.area;
    s.tree = next.tree;
    assert!(s.state.lock().unwrap().scroll_offset().abs() < 0.01);
}

#[test]
fn double_click_selects_a_word_and_triple_click_selects_the_logical_line() {
    let _guard = InputGuard::new();
    let mut s = Scene::new("hello world\nsecond line", 4, 300.0);
    s.click(0, 7);
    assert_eq!(s.selected(), None);
    s.click(0, 7);
    assert_eq!(s.selected().as_deref(), Some("world"));
    s.click(0, 7);
    assert_eq!(s.selected().as_deref(), Some("hello world\n"));
}

#[test]
fn shift_click_extends_the_existing_anchor_in_both_directions() {
    let _guard = InputGuard::new();
    let mut s = Scene::new("hello world\nsecond line", 4, 300.0);
    s.click(0, 6);
    s.press(1, 6, true);
    s.release();
    assert_eq!(s.selected().as_deref(), Some("world\nsecond"));
    s.press(0, 0, true);
    s.release();
    assert_eq!(s.selected().as_deref(), Some("hello "));
    assert_eq!(
        s.state.lock().unwrap().selection_start,
        Some(TextPosition::new(0, 6))
    );
}

#[test]
fn fast_clicks_at_distant_positions_are_separate_clicks() {
    let _guard = InputGuard::new();
    let mut s = Scene::new("hello world\nsecond line", 4, 300.0);
    s.click(0, 1);
    s.click(1, 7);
    assert_eq!(s.selected(), None);
    assert_eq!(s.state.lock().unwrap().cursor, TextPosition::new(1, 7));
}

#[test]
fn double_click_keeps_unicode_words_and_graphemes_together() {
    let _guard = InputGuard::new();
    for (text, column, expected) in [
        ("cafe\u{301} noir", 3, "cafe\u{301}"),
        ("can't stop", 2, "can't"),
        ("привет мир", 2, "привет"),
        ("go 👩‍💻 now", 4, "👩‍💻"),
    ] {
        let mut s = Scene::new(text, 2, 300.0);
        s.click(0, column);
        s.click(0, column);
        assert_eq!(s.selected().as_deref(), Some(expected), "{text}");
    }
}

#[test]
fn dragging_a_double_click_extends_by_whole_words() {
    let _guard = InputGuard::new();
    let mut s = Scene::new("first middle last", 2, 300.0);
    s.click(0, 8);
    s.press(0, 8, false);
    let (x, y) = s.point(0, 15);
    s.dispatch(s.event(event_types::DRAG).with_local_pos(x, y));
    assert_eq!(s.selected().as_deref(), Some("middle last"));
    let (x, y) = s.point(0, 2);
    s.dispatch(s.event(event_types::DRAG).with_local_pos(x, y));
    assert_eq!(s.selected().as_deref(), Some("first middle"));
    s.release();
}

#[test]
fn triple_click_on_a_wrapped_row_selects_the_whole_logical_line() {
    let _guard = InputGuard::new();
    let text = "one two three four five six seven eight";
    let mut s = Scene::new(text, 12, 90.0);
    let cfg = TextAreaConfig::default();
    let x = cfg.padding_x + cfg.border_width + 3.0;
    let y = cfg.padding_y + cfg.border_width + 2.5 * cfg.font_size * cfg.line_height;
    for _ in 0..3 {
        s.dispatch(s.event(event_types::POINTER_DOWN).with_local_pos(x, y));
        s.release();
    }
    assert_eq!(s.selected().as_deref(), Some(text));
}

#[test]
fn touch_release_and_movement_cancel_long_press() {
    let _guard = InputGuard::new();
    let mut s = Scene::new("hello world", 2, 300.0);
    set_touch_input(true);
    s.press(0, 7, false);
    assert!(is_long_press_armed());
    s.release();
    assert!(!is_long_press_armed());
    s.press(0, 1, false);
    assert!(is_long_press_armed());
    let (x, y) = s.point(0, 10);
    s.dispatch(s.event(event_types::DRAG).with_local_pos(x, y));
    assert!(!is_long_press_armed());
    assert_eq!(s.selected(), None);
}

#[test]
fn layout_measured_height_changes_update_scrolling() {
    let _guard = InputGuard::new();
    blinc_theme::ThemeState::init_default();
    let state = text_area_state();
    state.lock().unwrap().set_value("one\ntwo\nthree\nfour");
    let area = text_area(&state).w(300.0).h_full();
    let handlers = area.event_handlers().unwrap().clone();
    let mut tree =
        RenderTree::from_element(&blinc_layout::div::div().size(300.0, 65.0).child(area));
    tree.compute_layout(400.0, 600.0);
    let root = tree.root().unwrap();
    let field = tree.layout_tree.children(root)[0];
    handlers
        .dispatch(&EventContext::new(event_types::POINTER_DOWN, field).with_local_pos(14.0, 14.0));
    for _ in 0..4 {
        handlers.dispatch(&EventContext::new(event_types::KEY_DOWN, field).with_key_code(40));
    }
    assert!(state.lock().unwrap().scroll_offset() > 1.0);
    let mut style = tree.layout_tree.get_style(root).unwrap();
    style.size.height = taffy::prelude::Dimension::length(180.0);
    tree.layout_tree.set_style(root, style);
    tree.compute_layout(400.0, 600.0);
    assert!(state.lock().unwrap().scroll_offset().abs() < 0.01);
}

#[test]
fn touch_double_tap_and_long_press_select_the_word() {
    let _guard = InputGuard::new();
    let mut s = Scene::new("hello world", 2, 300.0);
    set_touch_input(true);
    s.click(0, 7);
    s.click(0, 7);
    assert_eq!(s.selected().as_deref(), Some("world"));
    assert!(!is_long_press_armed());
    drop(s);

    let mut s = Scene::new("hello world", 2, 300.0);
    set_touch_input(true);
    s.press(0, 7, false);
    assert!(is_long_press_armed());
    std::thread::sleep(std::time::Duration::from_millis(510));
    assert!(fire_long_press_timer_if_due());
    s.frame();
    assert_eq!(s.selected().as_deref(), Some("world"));
    s.release();
}
