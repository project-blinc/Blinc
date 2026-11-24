#![cfg(feature = "headless")]
//! `code_editor(..).minimap(true)` docks a minimap at the editor's right edge whose slider follows
//! the scroll, and a press on it scrolls the editor.

use blinc_layout::div::div;
use blinc_layout::prelude::*;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const W: f32 = 640.0;
const H: f32 = 300.0;

fn source(lines: usize) -> String {
    (0..lines)
        .map(|i| format!("let value_{i} = {i} * 2;{}", " // note".repeat(i % 5)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn session(state: &SharedCodeEditorState, minimap: bool) -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(W as u32, H as u32).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    let editor = state.clone();
    ui.mount(move |_| {
        div().w(W).h(H).child(
            code_editor(&editor)
                .w(W)
                .h(H)
                .font_size(13.0)
                .minimap(minimap),
        )
    });
    ui.settle();
    Some(ui)
}

/// The first and last rows of the minimap's right-edge column that are lighter than the editor's
/// background: the slider's, since no bar reaches that edge.
fn slider_rows(ui: &mut Headless, x: u32, top: u32, bottom: u32) -> (u32, u32) {
    let frame = ui.snapshot().unwrap();
    let bg = frame.get_pixel(x - 70, bottom - 2).unwrap();
    // The slider lightens the background; the editor's rounded edge darkens it.
    let lighter = |y: u32| {
        let p = frame.get_pixel(x, y).unwrap();
        (0..3).map(|c| u32::from(p[c])).sum::<u32>()
            > (0..3).map(|c| u32::from(bg[c])).sum::<u32>() + 12
    };
    let rows: Vec<u32> = (top..bottom).filter(|&y| lighter(y)).collect();
    (*rows.first().expect("a slider"), *rows.last().unwrap())
}

fn offset(state: &SharedCodeEditorState) -> (f32, f32) {
    let d = state.lock().unwrap();
    let p = d.scroll_physics.lock().unwrap();
    (-p.offset_y, p.content_height - p.viewport_height)
}

#[test]
fn a_docked_minimap_sits_at_the_right_edge_and_its_slider_follows_the_scroll() {
    let state = code_editor_state(source(200));
    let Some(mut ui) = session(&state, true) else {
        return;
    };
    let minimap = ui.find(".blinc-code-minimap").expect("a minimap");
    let editor = ui.find(".blinc-code").expect("the editor");
    let mb = ui.tree().get_absolute_bounds(minimap).unwrap();
    let eb = ui.tree().get_absolute_bounds(editor).unwrap();
    assert!(
        (mb.x + mb.width - (eb.x + eb.width)).abs() < 1.0 && (mb.width - 60.0).abs() < 1.0,
        "at the right edge: {mb:?} in {eb:?}"
    );

    let x = (mb.x + mb.width - 2.0) as u32;
    let (top, bottom) = (mb.y as u32, (mb.y + H) as u32);
    let (first, _) = slider_rows(&mut ui, x, top, bottom);
    assert!(
        first <= top + 6,
        "the slider starts at the top: {first} from {top}"
    );

    ui.move_to(W / 2.0, H / 2.0);
    for _ in 0..40 {
        ui.scroll(0.0, -400.0);
    }
    ui.settle_scroll();
    ui.settle();
    let (scrolled, max) = offset(&state);
    assert!(
        max > 0.0 && (scrolled - max).abs() < 1.0,
        "scrolled to the end: {scrolled} of {max}"
    );
    let (first_end, last_end) = slider_rows(&mut ui, x, top, bottom);
    assert!(
        first_end > first + 100,
        "the slider moved down: {first} -> {first_end}"
    );
    assert!(
        last_end >= bottom - 8,
        "the slider ends at the bottom: {last_end} of {bottom}"
    );
}

#[test]
fn a_press_on_the_minimap_scrolls_the_editor_there() {
    let state = code_editor_state(source(200));
    let Some(mut ui) = session(&state, true) else {
        return;
    };
    let minimap = ui.find(".blinc-code-minimap").unwrap();
    let mb = ui.tree().get_absolute_bounds(minimap).unwrap();
    assert_eq!(offset(&state).0, 0.0);
    ui.move_to(mb.x + mb.width / 2.0, mb.y + mb.height.min(H) * 0.75);
    ui.mouse_down();
    ui.mouse_up();
    ui.settle();
    let (scrolled, max) = offset(&state);
    assert!(
        scrolled > max * 0.5 && scrolled < max,
        "three quarters down: {scrolled} of {max}"
    );
    // The press scrolled; it did not move the caret.
    assert_eq!(state.lock().unwrap().cursor.line, 0);
}

#[test]
fn without_the_flag_there_is_no_minimap() {
    let state = code_editor_state(source(20));
    let Some(ui) = session(&state, false) else {
        return;
    };
    assert!(ui.find(".blinc-code-minimap").is_none());
}

#[test]
fn an_editor_without_line_numbers_scrolls() {
    let state = code_editor_state(source(200));
    let Some(mut ui) = session(&state, false) else {
        return;
    };
    ui.move_to(W / 2.0, H / 2.0);
    for _ in 0..40 {
        ui.scroll(0.0, -400.0);
    }
    ui.settle_scroll();
    ui.settle();
    let (scrolled, max) = offset(&state);
    assert!(
        max > 3000.0 && (scrolled - max).abs() < 1.0,
        "{scrolled} of {max}"
    );
}

#[test]
fn dragging_the_scrollbar_scrolls_without_moving_the_caret() {
    let state = code_editor_state(source(200));
    let Some(mut ui) = session(&state, false) else {
        return;
    };
    // Show the scrollbar, then press on its thumb at the editor's right edge and drag it down.
    ui.move_to(W / 2.0, H / 2.0);
    ui.advance(300);
    ui.move_to(W - 4.0, 12.0);
    ui.advance(300);
    ui.mouse_down();
    let mut offsets = Vec::new();
    for i in 1..=10 {
        ui.move_to(W - 4.0, 12.0 + i as f32 * 10.0);
        ui.advance(16);
        offsets.push(offset(&state).0);
    }
    ui.mouse_up();
    assert!(
        offsets.windows(2).all(|w| w[1] > w[0]),
        "each move scrolls further: {offsets:?}"
    );
    let d = state.lock().unwrap();
    assert_eq!(d.cursor.line, 0, "the drag did not place the caret");
    assert!(d.selection_start.is_none(), "nor select");
}
