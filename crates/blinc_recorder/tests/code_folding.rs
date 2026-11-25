#![cfg(feature = "headless")]
//! With a block folded, the caret, the current line and the selection are drawn on the rows the
//! lines are shown at.

use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::renderer::ElementType;
use blinc_layout::widgets::code::{code_editor, code_editor_state};
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const W: u32 = 480;
const H: u32 = 320;
const CODE: &str =
    "fn count() {\n    let a = 1;\n    for x in a {\n        seen(x);\n    }\n    a\n}";

/// The first node `found` accepts, in document order.
#[track_caller]
fn find(ui: &Headless, what: &str, found: impl Fn(LayoutNodeId) -> bool) -> LayoutNodeId {
    let tree = ui.tree();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(node) = stack.pop() {
        if found(node) {
            return node;
        }
        let mut children = tree.layout_tree.children(node);
        children.reverse();
        stack.extend(children);
    }
    panic!("no {what}");
}

/// The middle of the gutter's number `line`, counting from 1.
fn row(ui: &Headless, line: usize) -> f32 {
    let tree = ui.tree();
    let node = find(ui, &format!("number {line}"), |n| {
        tree.element_registry()
            .has_class(n, "blinc-code-line-number")
            && matches!(
                tree.get_render_node(n).map(|r| &r.element_type),
                Some(ElementType::Text(t)) if t.content == line.to_string()
            )
    });
    let b = tree.get_absolute_bounds(node).unwrap();
    b.y + b.height / 2.0
}

fn middle(ui: &Headless, class: &str) -> f32 {
    let node = find(ui, class, |n| {
        ui.tree().element_registry().has_class(n, class)
    });
    let b = ui.tree().get_absolute_bounds(node).unwrap();
    b.y + b.height / 2.0
}

#[test]
fn the_current_line_and_the_selection_follow_the_rows_of_a_folded_block() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let state = code_editor_state(CODE);
    let mut ui = Headless::new(W, H).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    let editor = state.clone();
    ui.mount(move |_| {
        div().w(W as f32).h(H as f32).child(
            code_editor(&editor)
                .line_numbers(true)
                .code_folding(true)
                .font_size(16.0)
                .w(440.0)
                .h(280.0),
        )
    });
    ui.settle();
    // Fold the inner block from its chevron in the gutter.
    let at = row(&ui, 3);
    ui.move_to(8.0, at).mouse_down().mouse_up();
    ui.settle();
    assert_eq!(state.lock().unwrap().folded_regions, [(2, 4)]);

    // Press on the row that shows line 6, below the fold.
    let at = row(&ui, 6);
    ui.move_to(200.0, at).mouse_down().mouse_up();
    ui.settle();
    assert_eq!(state.lock().unwrap().cursor.line, 5);
    let active = middle(&ui, "blinc-code-active-line");
    assert!(
        (active - at).abs() < 1.5,
        "current line at {active}, row at {at}"
    );

    // A drag along that row selects on it.
    ui.move_to(60.0, at).mouse_down();
    ui.frame(16);
    ui.move_to(200.0, at);
    ui.frame(16);
    ui.mouse_up().settle();
    {
        let d = state.lock().unwrap();
        assert_eq!(d.cursor.line, 5);
        assert!(d.selected_text().is_some_and(|t| !t.contains('\n')));
    }
    let selection = middle(&ui, "blinc-code-selection");
    assert!(
        (selection - at).abs() < 1.5,
        "selection at {selection}, row at {at}"
    );
    ui.assert_snapshot("code_folding_selection");
}
