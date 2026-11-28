//! A folded block takes its lines out of the editor's rows: the cursor moves over them, edits
//! elsewhere keep the block folded, and anything that lands inside it opens it.

use blinc_layout::renderer::RenderTree;
use blinc_layout::widgets::code::{SharedCodeEditorState, code, code_editor_state};
use blinc_layout::widgets::text_area::TextPosition;
use blinc_theme::{ColorScheme, HybridTheme, ThemeState};

/// Line 3 is the body of the block that opens on line 2.
const CODE: &str =
    "fn count() {\n    let a = 1;\n    for x in a {\n        seen(x);\n    }\n    a\n}";

/// The editor with the inner block folded, and the cursor at `(line, column)`.
fn folded(line: usize, column: usize) -> SharedCodeEditorState {
    ThemeState::init(HybridTheme::bundle(), ColorScheme::Dark);
    let state = code_editor_state(CODE);
    {
        let mut d = state.lock().unwrap();
        d.toggle_fold(2);
        d.cursor = TextPosition::new(line, column);
    }
    state
}

#[test]
fn a_press_finds_the_line_shown_on_its_row() {
    let state = folded(0, 0);
    let mut d = state.lock().unwrap();
    let row = d.config.line_height_px();
    assert_eq!(d.visible_lines(), [0, 1, 2, 4, 5, 6]);
    d.cursor_from_click(0.0, row * 4.5);
    assert_eq!(d.cursor.line, 5);
    assert_eq!(d.content_height(), row * 6.0 + d.config.padding * 2.0);
}

#[test]
fn the_cursor_moves_over_a_folded_block() {
    let end = "    for x in a {".len();
    let state = folded(2, end);
    let mut d = state.lock().unwrap();
    d.move_down(false);
    assert_eq!(d.cursor.line, 4);
    d.move_up(false);
    assert_eq!(d.cursor.line, 2);
    d.cursor.column = end;
    d.move_right(false);
    assert_eq!(d.cursor, TextPosition::new(4, 0));
    d.move_left(false);
    assert_eq!(d.cursor, TextPosition::new(2, end));
    d.move_word_right(false);
    assert_eq!(d.cursor, TextPosition::new(4, 0));
    d.move_word_left(false);
    assert_eq!(d.cursor, TextPosition::new(2, end));
    assert!(d.is_fold_start(2), "moving opened the block");
}

#[test]
fn a_fold_stays_on_its_block_as_lines_come_and_go_above_it() {
    let state = folded(0, "fn count() {".len());
    let mut d = state.lock().unwrap();
    d.insert("\n    let b = 2;");
    d.ensure_cursor_visible();
    assert_eq!(d.folded_regions, [(3, 5)]);
    assert!(d.is_line_folded(4));
    d.undo();
    d.ensure_cursor_visible();
    assert_eq!(d.folded_regions, [(2, 4)]);
}

#[test]
fn a_fold_opens_when_its_block_is_broken_or_the_cursor_lands_in_it() {
    // Losing the brace that opens the block.
    let state = folded(2, "    for x in a {".len());
    {
        let mut d = state.lock().unwrap();
        d.delete_backward();
        d.ensure_cursor_visible();
        assert!(d.folded_regions.is_empty());
    }
    // A new line at the end of the folded line goes inside the block.
    let state = folded(2, "    for x in a {".len());
    {
        let mut d = state.lock().unwrap();
        d.insert("\n");
        d.ensure_cursor_visible();
        assert!(d.folded_regions.is_empty());
    }
    // A search match inside the block.
    let state = folded(0, 0);
    let mut d = state.lock().unwrap();
    d.apply_search_query("seen".to_string());
    assert_eq!(d.cursor.line, 3);
    assert!(d.folded_regions.is_empty());
}

/// At a font size that makes the line height a half pixel, the rows are still
/// evenly spaced.
#[test]
fn rows_are_evenly_spaced_at_a_fractional_line_height() {
    ThemeState::init(HybridTheme::bundle(), ColorScheme::Dark);
    let block = code(CODE)
        .line_numbers(true)
        .font_size(13.0)
        .line_height(1.5)
        .w(400.0)
        .h(300.0);
    let mut tree = RenderTree::from_element(&block);
    tree.compute_layout(400.0, 300.0);

    let numbers = tree.element_registry().class_to_nodes_index();
    let mut tops: Vec<f32> = numbers["blinc-code-line-number"]
        .iter()
        .map(|&node| tree.get_absolute_bounds(node).unwrap().y)
        .collect();
    tops.sort_by(f32::total_cmp);
    assert_eq!(tops.len(), CODE.lines().count());
    let gaps: Vec<f32> = tops.windows(2).map(|pair| pair[1] - pair[0]).collect();
    assert!(gaps.iter().all(|gap| *gap == gaps[0]), "{gaps:?}");
}
