#![cfg(feature = "headless")]
//! The cn code editor's floating find and replace bar.

use std::sync::{Arc, Mutex};

use blinc_core::Color;
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::renderer::ElementType;
use blinc_layout::widgets::code::{SharedCodeEditorState, code_editor_state};
use blinc_layout::widgets::text_area::TextPosition;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const W: u32 = 640;
const H: u32 = 360;
const F: u32 = 70;
const G: u32 = 71;
const C: u32 = 67;
const ENTER: u32 = 13;
const ESC: u32 = 27;

const CODE: &str = "let child = 1;\nlet other = child + 2;\nlet Child = child;";

/// A session over `state`, its editor `width` wide, with `css` after the cn sheet.
fn session(
    state: &SharedCodeEditorState,
    width: f32,
    css: &str,
    heard: Arc<Mutex<Vec<String>>>,
) -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(W, H).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    if !css.is_empty() {
        ui.css(css);
    }
    let editor = state.clone();
    ui.mount(move |_| {
        let heard = Arc::clone(&heard);
        div().w(W as f32).h(H as f32).p(3.0).child(
            div().w(width).child(
                blinc_cn::cn::code_editor(&editor)
                    .line_numbers(true)
                    .w_full()
                    .h(260.0)
                    .on_change(move |t| heard.lock().unwrap().push(t.to_string())),
            ),
        )
    });
    ui.settle();
    // The editor takes the keyboard from a press on its text.
    ui.move_to(200.0, 40.0).mouse_down().mouse_up();
    ui.settle();
    Some(ui)
}

/// A search goes forward from the cursor.
fn start_at_top(state: &SharedCodeEditorState) {
    let mut d = state.lock().unwrap();
    d.cursor = TextPosition::new(0, 0);
    d.selection_start = None;
}

fn command(ui: &mut Headless, key: u32, shift: bool, alt: bool) {
    ui.key_with_modifiers(key, shift, true, alt, true);
    ui.settle();
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
        let mut children = tree.layout_tree.children(n);
        children.reverse();
        stack.extend(children);
    }
    found
}

fn one(ui: &Headless, class: &str) -> LayoutNodeId {
    *nodes(ui, class)
        .first()
        .unwrap_or_else(|| panic!("no .{class}"))
}

/// The text under the first node of `class`.
fn text_in(ui: &Headless, class: &str) -> String {
    let tree = ui.tree();
    let mut stack = vec![one(ui, class)];
    let mut out = String::new();
    while let Some(n) = stack.pop() {
        if let Some(ElementType::Text(t)) = tree.get_render_node(n).map(|r| &r.element_type) {
            out.push_str(&t.content);
        }
        stack.extend(tree.layout_tree.children(n));
    }
    out
}

fn press(ui: &mut Headless, class: &str) {
    let b = ui.tree().get_absolute_bounds(one(ui, class)).unwrap();
    ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0);
    ui.mouse_down().mouse_up();
    ui.advance(300);
    ui.settle();
}

fn query(state: &SharedCodeEditorState) -> String {
    state
        .lock()
        .unwrap()
        .search_input_state
        .lock()
        .unwrap()
        .value
        .clone()
}

#[test]
fn command_f_opens_the_bar_on_the_selection_and_escape_returns_to_the_editor() {
    let state = code_editor_state(CODE);
    let Some(mut ui) = session(&state, 600.0, "", Default::default()) else {
        return;
    };
    assert!(nodes(&ui, "cn-code-editor-find").is_empty());
    {
        let mut d = state.lock().unwrap();
        d.selection_start = Some(TextPosition::new(0, 4));
        d.cursor = TextPosition::new(0, 9);
    }
    command(&mut ui, F, false, false);
    assert_eq!(nodes(&ui, "cn-code-editor-find").len(), 1, "the bar opened");
    assert_eq!(query(&state), "child");
    // With room for them, the field and its controls are one line.
    let middle = |class| {
        let b = ui.tree().get_absolute_bounds(one(&ui, class)).unwrap();
        b.y + b.height / 2.0
    };
    for class in ["cn-code-editor-find-option", "cn-code-editor-find-close"] {
        let (field, control) = (middle("cn-code-editor-find-query"), middle(class));
        assert!((field - control).abs() < 1.0, "{class}: {control} {field}");
    }
    // Matching is not case sensitive until the option says so.
    assert_eq!(text_in(&ui, "cn-code-editor-find-count"), "1 of 4");
    assert_eq!(nodes(&ui, "blinc-code-find-match").len(), 4);
    ui.advance(400);
    ui.settle();
    ui.assert_snapshot("cn_code_editor_find");
    // The keyboard is in the find field: typing replaces the selected query, not the code.
    ui.type_text("other");
    ui.settle();
    assert_eq!(query(&state), "other");
    assert_eq!(state.lock().unwrap().value(), CODE);
    assert_eq!(text_in(&ui, "cn-code-editor-find-count"), "1 of 1");
    // Escape closes the bar and the code takes the keyboard again.
    ui.key(ESC);
    ui.advance(300);
    ui.settle();
    assert!(
        nodes(&ui, "cn-code-editor-find").is_empty(),
        "the bar closed"
    );
    assert!(nodes(&ui, "blinc-code-find-match").is_empty());
    assert!(state.lock().unwrap().focused);
    ui.type_text("Z");
    ui.settle();
    assert!(state.lock().unwrap().value().contains('Z'));
}

#[test]
fn enter_arrows_and_options_move_through_the_matches() {
    let state = code_editor_state(CODE);
    let Some(mut ui) = session(&state, 600.0, "", Default::default()) else {
        return;
    };
    start_at_top(&state);
    command(&mut ui, F, false, false);
    ui.type_text("child");
    ui.settle();
    let count = |ui: &Headless| text_in(ui, "cn-code-editor-find-count");
    assert_eq!(count(&ui), "1 of 4");
    ui.key(ENTER);
    ui.settle();
    assert_eq!(count(&ui), "2 of 4");
    ui.key_with_shift(ENTER, true);
    ui.settle();
    assert_eq!(count(&ui), "1 of 4");
    command(&mut ui, G, true, false);
    assert_eq!(count(&ui), "4 of 4", "Shift+Command+G goes back, around");
    command(&mut ui, G, false, false);
    assert_eq!(count(&ui), "1 of 4");
    press(&mut ui, "cn-code-editor-find-next");
    assert_eq!(count(&ui), "2 of 4");
    press(&mut ui, "cn-code-editor-find-previous");
    assert_eq!(count(&ui), "1 of 4");
    // A press on the bar leaves the keyboard in the find field.
    ui.key(ENTER);
    ui.settle();
    assert_eq!(count(&ui), "2 of 4");
    // Match case, from its toggle and from Alt+C.
    let option = one(&ui, "cn-code-editor-find-option");
    let b = ui.tree().get_absolute_bounds(option).unwrap();
    ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0);
    ui.mouse_down().mouse_up();
    ui.settle();
    assert!(state.lock().unwrap().search_case_sensitive);
    assert_eq!(count(&ui), "1 of 3");
    ui.key_with_modifiers(C, false, false, true, false);
    ui.settle();
    assert!(!state.lock().unwrap().search_case_sensitive);
    assert_eq!(count(&ui).split(' ').next_back(), Some("4"));
    assert_eq!(query(&state), "child", "Alt+C typed nothing");
    // A pattern that does not compile says why.
    state.lock().unwrap().search_regex = true;
    ui.type_text("(");
    ui.settle();
    assert_eq!(count(&ui), "No results");
    let why = text_in(&ui, "cn-code-editor-find-error");
    assert!(why.contains("unclosed"), "{why}");
    ui.assert_snapshot("cn_code_editor_error");
    ui.key(8);
    ui.settle();
    assert!(nodes(&ui, "cn-code-editor-find-error").is_empty());
}

#[test]
fn replacing_from_the_bar_changes_the_code_and_tells_the_application() {
    let state = code_editor_state(CODE);
    let heard = Arc::new(Mutex::new(Vec::new()));
    let Some(mut ui) = session(&state, 600.0, "", Arc::clone(&heard)) else {
        return;
    };
    start_at_top(&state);
    command(&mut ui, F, false, true);
    assert_eq!(nodes(&ui, "cn-code-editor-find-replace").len(), 1);
    ui.advance(400);
    ui.settle();
    ui.assert_snapshot("cn_code_editor_replace");
    // The two fields are one width.
    let width = |class: &str| {
        let tree = ui.tree();
        let node = tree.layout_tree.children(one(&ui, class))[1];
        tree.get_absolute_bounds(node).unwrap().width
    };
    let (find, replace) = (
        width("cn-code-editor-find-row"),
        width("cn-code-editor-find-replace"),
    );
    assert!((find - replace).abs() < 0.5, "{find} {replace}");
    ui.type_text("child");
    ui.settle();
    // The replace field takes what to put instead.
    let field = one(&ui, "cn-code-editor-find-replace");
    let b = ui.tree().get_absolute_bounds(field).unwrap();
    ui.move_to(b.x + b.width / 3.0, b.y + b.height / 2.0);
    // The window blurs every field before it routes a press, which headless does not.
    blinc_layout::widgets::blur_all_text_inputs();
    ui.mouse_down().mouse_up();
    ui.settle();
    ui.type_text("kid");
    ui.settle();
    ui.key(ENTER);
    ui.settle();
    assert_eq!(
        heard.lock().unwrap().last().unwrap(),
        "let kid = 1;\nlet other = child + 2;\nlet Child = child;"
    );
    press(&mut ui, "cn-code-editor-find-replace-all");
    assert_eq!(
        state.lock().unwrap().value(),
        "let kid = 1;\nlet other = kid + 2;\nlet kid = kid;"
    );
    assert_eq!(heard.lock().unwrap().len(), 2);
}

#[test]
fn a_narrow_editor_wraps_the_bars_controls_inside_it() {
    for width in [300.0, 220.0] {
        narrow_editor(width);
    }
}

fn narrow_editor(width: f32) {
    let state = code_editor_state(CODE);
    let Some(mut ui) = session(&state, width, "", Default::default()) else {
        return;
    };
    command(&mut ui, F, false, false);
    ui.advance(400);
    ui.settle();
    let tree = ui.tree();
    let editor = tree
        .get_absolute_bounds(one(&ui, "cn-code-editor"))
        .unwrap();
    let bar = tree
        .get_absolute_bounds(one(&ui, "cn-code-editor-find"))
        .unwrap();
    assert!(
        bar.x >= editor.x - 0.5 && bar.x + bar.width <= editor.x + editor.width + 0.5,
        "the bar leaves the editor: {bar:?} in {editor:?}"
    );
    let tools = tree
        .get_absolute_bounds(one(&ui, "cn-code-editor-find-tools"))
        .unwrap();
    let query_field = tree
        .get_absolute_bounds(one(&ui, "cn-code-editor-find-query"))
        .unwrap();
    assert!(
        tools.y >= query_field.y + query_field.height - 0.5,
        "the controls did not wrap below the field: {tools:?} {query_field:?}"
    );
    // No control leaves the bar.
    for class in [
        "cn-code-editor-find-previous",
        "cn-code-editor-find-next",
        "cn-code-editor-find-close",
    ] {
        let b = tree.get_absolute_bounds(one(&ui, class)).unwrap();
        assert!(
            b.x >= bar.x - 0.5 && b.x + b.width <= bar.x + bar.width + 0.5,
            "{width}: {class} {b:?} outside {bar:?}"
        );
    }
    if width == 300.0 {
        ui.assert_snapshot("cn_code_editor_narrow");
    }
}

#[test]
fn a_stylesheet_restyles_the_bar() {
    let state = code_editor_state(CODE);
    let css = ".cn-code-editor-find { background: #ff0000; border: 3px solid #0000ff;
               border-radius: 0px; box-shadow: none; }";
    let Some(mut ui) = session(&state, 600.0, css, Default::default()) else {
        return;
    };
    command(&mut ui, F, false, false);
    ui.advance(400);
    ui.settle();
    let b = ui
        .tree()
        .get_absolute_bounds(one(&ui, "cn-code-editor-find"))
        .unwrap();
    let frame = ui.snapshot().unwrap();
    let at = |x: f32, y: f32| {
        let p = frame.get_pixel(x as u32, y as u32).unwrap();
        Color::rgb(
            p[0] as f32 / 255.0,
            p[1] as f32 / 255.0,
            p[2] as f32 / 255.0,
        )
    };
    let (edge, fill) = (
        at(b.x + 1.0, b.y + b.height - 2.0),
        at(b.x + 4.0, b.y + b.height - 5.0),
    );
    assert!(edge.b > 0.9 && edge.r < 0.1, "border {edge:?}");
    assert!(fill.r > 0.9 && fill.b < 0.1, "fill {fill:?}");
}
