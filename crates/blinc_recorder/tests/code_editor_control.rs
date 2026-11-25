#![cfg(feature = "headless")]
//! A code editor driven from outside: the placeholder, the match marks a toolbar asks for, a new
//! text and the focus.

use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::prelude::*;
use blinc_layout::renderer::ElementType;
use blinc_layout::widgets::code::{
    SharedCodeEditorState, focus_code_editor, refresh_code_editor, set_code_value,
};
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

/// A session with `state`'s editor, configured by `configure`.
fn session(
    state: &SharedCodeEditorState,
    configure: impl Fn(CodeEditor) -> CodeEditor + Send + Sync + 'static,
) -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(400, 160).ok()?;
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    let editor = state.clone();
    ui.mount(move |_| {
        div()
            .w(400.0)
            .h(160.0)
            .child(configure(code_editor(&editor).w(400.0).h(160.0)))
    });
    ui.settle();
    Some(ui)
}

fn nodes(ui: &Headless, class: &str) -> Vec<LayoutNodeId> {
    let tree = ui.tree();
    let registry = tree.element_registry();
    let mut stack = vec![tree.root().unwrap()];
    let mut found = Vec::new();
    while let Some(n) = stack.pop() {
        if registry.has_class(n, class) {
            found.push(n);
        }
        stack.extend(tree.layout_tree.children(n));
    }
    found
}

fn shows(ui: &Headless, text: &str) -> bool {
    let tree = ui.tree();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(n) = stack.pop() {
        let drawn = match tree.get_render_node(n).map(|r| &r.element_type) {
            Some(ElementType::Text(t)) => Some(t.content.as_str()),
            Some(ElementType::StyledText(t)) => Some(t.content.as_str()),
            _ => None,
        };
        if drawn == Some(text) {
            return true;
        }
        stack.extend(tree.layout_tree.children(n));
    }
    false
}

#[test]
fn the_placeholder_shows_only_while_there_is_no_text() {
    let state = code_editor_state("");
    let Some(mut ui) = session(&state, |e| e.placeholder("Write some code")) else {
        return;
    };
    assert!(shows(&ui, "Write some code"));
    assert_eq!(nodes(&ui, "blinc-code-placeholder").len(), 1);
    ui.move_to(100.0, 24.0).mouse_down().mouse_up();
    ui.settle();
    ui.type_text("x");
    ui.settle();
    assert_eq!(nodes(&ui, "blinc-code-placeholder").len(), 0);
    set_code_value(&state, "");
    ui.settle();
    assert_eq!(nodes(&ui, "blinc-code-placeholder").len(), 1);
}

#[test]
fn matches_are_marked_for_an_outside_find_bar() {
    let state = code_editor_state("a b a b a");
    let Some(mut ui) = session(&state, |e| e) else {
        return;
    };
    {
        let mut d = state.lock().unwrap();
        d.apply_search_query("a".to_string());
        d.find_open = true;
    }
    refresh_code_editor(&state);
    ui.settle();
    assert_eq!(nodes(&ui, "blinc-code-find-match").len(), 3);
    state.lock().unwrap().find_open = false;
    refresh_code_editor(&state);
    ui.settle();
    assert_eq!(nodes(&ui, "blinc-code-find-match").len(), 0);
}

#[test]
fn a_new_text_is_drawn_and_focus_returns_the_keyboard() {
    let state = code_editor_state("first");
    let Some(mut ui) = session(&state, |e| e) else {
        return;
    };
    ui.move_to(100.0, 24.0).mouse_down().mouse_up();
    ui.settle();
    assert!(state.lock().unwrap().focused);
    blinc_layout::widgets::text_input::blur_all_text_inputs();
    assert!(!state.lock().unwrap().focused);
    focus_code_editor(&state);
    ui.settle();
    assert!(state.lock().unwrap().focused);
    ui.type_text("!");
    ui.settle();
    assert!(
        state.lock().unwrap().value().contains('!'),
        "typing reaches the editor"
    );
    set_code_value(&state, "second line\nand third");
    ui.settle();
    assert!(shows(&ui, "and third"), "the new text is drawn");
}

#[test]
fn a_read_only_editor_takes_no_typing() {
    let state = code_editor_state("read me");
    let Some(mut ui) = session(&state, |e| e.read_only(true)) else {
        return;
    };
    ui.move_to(100.0, 24.0).mouse_down().mouse_up();
    ui.settle();
    ui.type_text("x");
    ui.key(8);
    ui.settle();
    assert_eq!(state.lock().unwrap().value(), "read me");
}
