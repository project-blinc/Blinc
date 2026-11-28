#![cfg(feature = "headless")]
//! A code editor scrolls when lines are out of view, not when its box cuts only the room under
//! the last one.

use blinc_layout::div::div;
use blinc_layout::widgets::code::{SharedCodeEditorState, code_editor, code_editor_state};
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const DOWN: u32 = 40;
const PAGE_DOWN: u32 = 34;

/// An editor of `lines` lines, 320 px tall, with the keyboard.
fn editor(lines: usize) -> Option<(Headless, SharedCodeEditorState)> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let text: Vec<String> = (1..=lines).map(|n| format!("line {n}")).collect();
    let state = code_editor_state(text.join("\n"));
    let mut ui = Headless::new(480, 360).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    ui.scroll_physics();
    let shown = state.clone();
    ui.mount(move |_| {
        div()
            .w(480.0)
            .h(360.0)
            .child(code_editor(&shown).line_numbers(true).w(440.0).h(320.0))
    });
    ui.settle();
    ui.move_to(200.0, 24.0).mouse_down().mouse_up();
    ui.settle();
    Some((ui, state))
}

/// How far the editor is scrolled, whether it can scroll and whether its bar shows.
fn scrolled(state: &SharedCodeEditorState) -> (f32, bool, bool) {
    let physics = state.lock().unwrap().scroll_physics.clone();
    let p = physics.lock().unwrap();
    (
        -p.offset_y,
        p.can_scroll_y(),
        p.scrollbar_render_info().show_vertical,
    )
}

fn frames(ui: &mut Headless, count: usize) {
    for _ in 0..count {
        ui.frame(16);
    }
}

#[test]
fn a_box_that_cuts_only_the_padding_under_the_text_does_not_scroll() {
    // Fifteen rows and the padding above them fit 320 px; the padding below does not.
    let Some((mut ui, state)) = editor(15) else {
        return;
    };
    let content = state.lock().unwrap().content_height();
    assert!(content > 320.0 && content - 320.0 < 16.0, "{content}");
    assert_eq!(scrolled(&state), (0.0, false, false));
    ui.scroll(0.0, -40.0);
    frames(&mut ui, 60);
    assert_eq!(scrolled(&state), (0.0, false, false));
    for _ in 0..14 {
        ui.key(DOWN);
    }
    ui.settle();
    assert_eq!(state.lock().unwrap().cursor.line, 14);
    assert_eq!(scrolled(&state).0, 0.0);
}

#[test]
fn a_longer_text_follows_the_cursor_to_its_last_line() {
    let Some((mut ui, state)) = editor(40) else {
        return;
    };
    assert_eq!(scrolled(&state), (0.0, true, true));
    // A page is the rows the box holds.
    ui.key(PAGE_DOWN).settle();
    assert_eq!(state.lock().unwrap().cursor.line, 16);
    for _ in 0..3 {
        ui.key(PAGE_DOWN);
    }
    ui.settle();
    let (row, pad) = {
        let d = state.lock().unwrap();
        assert_eq!(d.cursor.line, 39);
        (d.config.line_height_px(), d.config.padding)
    };
    // The last row's foot is at the foot of the box.
    let (offset, ..) = scrolled(&state);
    assert!(
        (offset - (pad + 40.0 * row - 320.0)).abs() < 1.0,
        "{offset}"
    );
}
