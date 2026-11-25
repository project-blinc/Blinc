//! Driving a code editor from outside: read-only, tab size, setting the text, the placeholder and
//! the match marks a toolbar asks for.

use std::sync::{Arc, Mutex};

use blinc_layout::div::ElementBuilder;
use blinc_layout::event_handler::EventContext;
use blinc_layout::renderer::RenderTree;
use blinc_layout::widgets::code::{
    CodeEditor, SharedCodeEditorState, code_editor, code_editor_state, replace_all_code_matches,
    replace_code_match, set_code_value,
};
use blinc_layout::widgets::text_area::TextPosition;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init(
            blinc_theme::HybridTheme::bundle(),
            blinc_theme::ColorScheme::Dark,
        );
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

/// A focused editor with its tree, and what its `on_change` heard.
struct Editor {
    state: SharedCodeEditorState,
    element: CodeEditor,
    tree: RenderTree,
    heard: Arc<Mutex<Vec<String>>>,
}

impl Editor {
    fn new(text: &str, configure: impl FnOnce(CodeEditor) -> CodeEditor) -> Self {
        init();
        let state = code_editor_state(text);
        let heard = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&heard);
        let element = configure(
            code_editor(&state)
                .on_change(move |t| log.lock().unwrap().push(t.to_string()))
                .h(200.0),
        );
        state.lock().unwrap().focused = true;
        let mut tree = RenderTree::from_element(&element);
        tree.compute_layout(400.0, 200.0);
        Self {
            state,
            element,
            tree,
            heard,
        }
    }

    fn key(&self, code: u32, command: bool, shift: bool) {
        let mut event = EventContext::new(
            blinc_core::events::event_types::KEY_DOWN,
            self.tree.root().unwrap(),
        );
        event.key_code = code;
        event.meta = command;
        event.ctrl = command;
        event.shift = shift;
        self.element.event_handlers().unwrap().dispatch(&event);
    }

    fn type_text(&self, text: &str) {
        for ch in text.chars() {
            let event = EventContext::new(
                blinc_core::events::event_types::TEXT_INPUT,
                self.tree.root().unwrap(),
            )
            .with_key_char(ch);
            self.element.event_handlers().unwrap().dispatch(&event);
        }
    }

    fn value(&self) -> String {
        self.state.lock().unwrap().value()
    }
}

#[test]
fn a_read_only_editor_reads_selects_and_changes_nothing() {
    let editor = Editor::new("one\ntwo", |e| e.read_only(true));
    // Typing, Backspace, Delete, Enter, Tab, paste, undo and redo.
    editor.type_text("x");
    for key in [8, 127, 13, 9] {
        editor.key(key, false, false);
    }
    editor.key(86, true, false);
    editor.key(90, true, false);
    editor.key(90, true, true);
    // Select all and cut: it copies, and cuts nothing.
    editor.key(65, true, false);
    editor.key(88, true, false);
    assert_eq!(editor.value(), "one\ntwo");
    assert!(editor.heard.lock().unwrap().is_empty());
    // The code's own methods do nothing either.
    let mut d = editor.state.lock().unwrap();
    d.insert("x");
    d.delete_backward();
    d.indent_lines();
    d.dedent_lines();
    assert!(!d.delete_selection() && !d.undo() && !d.redo());
    d.apply_search_query("one".to_string());
    d.replace_text = "1".to_string();
    assert!(!d.replace_current());
    assert_eq!(d.replace_all(), 0);
    assert_eq!(d.value(), "one\ntwo");
    // Moving and selecting still work.
    d.move_to_line_end(true);
}

#[test]
fn tab_indents_by_the_tab_size() {
    let editor = Editor::new("a\nb", |e| e.tab_size(2));
    editor.key(9, false, false);
    assert_eq!(editor.value(), "  a\nb");
    editor.key(9, false, true);
    assert_eq!(editor.value(), "a\nb");
    // Over a selection, every line it touches.
    {
        let mut d = editor.state.lock().unwrap();
        d.selection_start = Some(TextPosition::new(0, 0));
        d.cursor = TextPosition::new(1, 1);
    }
    editor.key(9, false, false);
    assert_eq!(editor.value(), "  a\n  b");
}

#[test]
fn setting_the_text_is_the_applications_change() {
    let editor = Editor::new("old text\nsecond", |e| e);
    editor.type_text("x");
    {
        let mut d = editor.state.lock().unwrap();
        d.cursor = TextPosition::new(1, 5);
        d.apply_search_query("new".to_string());
    }
    set_code_value(&editor.state, "new");
    let d = editor.state.lock().unwrap();
    assert_eq!(d.value(), "new");
    // The cursor stays where it can, the search runs again, and nothing is undone.
    assert_eq!((d.cursor.line, d.cursor.column), (0, 3));
    assert_eq!(d.search_matches, [(0, 0, 3)]);
    drop(d);
    assert_eq!(editor.heard.lock().unwrap().len(), 1, "only the typing");
    assert!(!editor.state.lock().unwrap().undo());
}

#[test]
fn replacing_from_outside_reports_the_change() {
    let editor = Editor::new("a a", |e| e);
    {
        let mut d = editor.state.lock().unwrap();
        d.apply_search_query("a".to_string());
        d.replace_text = "b".to_string();
    }
    assert!(replace_code_match(&editor.state));
    assert_eq!(replace_all_code_matches(&editor.state), 1);
    assert_eq!(editor.heard.lock().unwrap().as_slice(), ["b a", "b b"]);
    // With nothing to replace, nothing is reported.
    assert!(!replace_code_match(&editor.state));
    assert_eq!(editor.heard.lock().unwrap().len(), 2);
}
