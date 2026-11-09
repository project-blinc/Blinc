#![cfg(feature = "headless")]
//! A code editor shows diagnostics inline: an underline under the span, a tint across its line,
//! a mark in the gutter, and the line's message after its text.

use blinc_layout::div::div;
use blinc_layout::prelude::*;
use blinc_recorder::Headless;
use blinc_recorder::testing::ScreenshotExporter;
use blinc_theme::{ColorScheme, HybridTheme};

const SOURCE: &str = "fn main() {\n    let x = ;\n    let y = 2;\n    x + y\n}\n";

#[test]
fn diagnostics_are_drawn_inline() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(640, 220).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    let state = code_editor_state(SOURCE);
    set_code_diagnostics(
        &state,
        vec![
            CodeDiagnostic::new(
                1,
                12,
                13,
                DiagnosticSeverity::Error,
                "expected an expression, found `;`",
            ),
            CodeDiagnostic::new(2, 8, 9, DiagnosticSeverity::Warning, "unused variable: `y`"),
        ],
    );
    let editor = state.clone();
    ui.mount(move |_| {
        div().w(640.0).h(220.0).child(
            code_editor(&editor)
                .w(640.0)
                .h(220.0)
                .font_size(14.0)
                .line_numbers(true),
        )
    });
    ui.settle();
    let frame = ui.snapshot().unwrap();
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("code_editor_diagnostics.png");
    ScreenshotExporter::save_png(&frame, &out).unwrap();
    eprintln!("saved {}", out.display());

    for class in [
        ".blinc-code-underline-error",
        ".blinc-code-underline-warning",
        ".blinc-code-line-error",
        ".blinc-code-mark-error",
        ".blinc-code-lens-error",
        ".blinc-code-lens-warning",
    ] {
        assert!(ui.find(class).is_some(), "{class} is drawn");
    }
    // The error's underline sits under `;`, the thirteenth character of its line, at the line's
    // foot.
    let tree = ui.tree();
    let underline = ui.find(".blinc-code-underline-error").unwrap();
    let ub = tree.get_absolute_bounds(underline).unwrap();
    let line = ui.find(".blinc-code-line-error").unwrap();
    let lb = tree.get_absolute_bounds(line).unwrap();
    assert!(
        ub.y > lb.y + lb.height * 0.6 && ub.y < lb.y + lb.height,
        "underline at the line's foot: {ub:?} in {lb:?}"
    );
    assert!(
        ub.width > 2.0 && ub.width < 16.0,
        "one character wide: {ub:?}"
    );
}

#[test]
fn clearing_diagnostics_removes_them() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(640, 220).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    let state = code_editor_state(SOURCE);
    set_code_diagnostics(
        &state,
        vec![CodeDiagnostic::new(
            1,
            12,
            13,
            DiagnosticSeverity::Error,
            "bad",
        )],
    );
    let editor = state.clone();
    ui.mount(move |_| {
        div()
            .w(640.0)
            .h(220.0)
            .child(code_editor(&editor).w(640.0).h(220.0))
    });
    ui.settle();
    assert!(ui.find(".blinc-code-lens-error").is_some());
    set_code_diagnostics(&state, Vec::new());
    ui.settle();
    assert!(
        ui.find(".blinc-code-lens-error").is_none(),
        "the editor redraws without them"
    );
}

/// Code is set in the face `--blinc-code-font-family` names, and diagnostics are measured in it.
#[test]
fn code_takes_the_font_the_stylesheet_names() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(640, 220).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    ui.load_font_data(
        include_bytes!("../../../examples/blinc_component_demos/assets/JetBrainsMono-Regular.ttf")
            .to_vec(),
    );
    ui.css(":root { --blinc-code-font-family: \"JetBrains Mono\", monospace; }");
    let state = code_editor_state(SOURCE);
    set_code_diagnostics(
        &state,
        vec![CodeDiagnostic::new(
            1,
            12,
            13,
            DiagnosticSeverity::Error,
            "expected an expression",
        )],
    );
    let editor = state.clone();
    ui.mount(move |_| {
        div().w(640.0).h(220.0).child(
            code_editor(&editor)
                .w(640.0)
                .h(220.0)
                .font_size(14.0)
                .line_numbers(true),
        )
    });
    ui.settle();
    let frame = ui.snapshot().unwrap();
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("code_editor_jetbrains.png");
    ScreenshotExporter::save_png(&frame, &out).unwrap();
    eprintln!("saved {}", out.display());

    let tree = ui.tree();
    let mut faces = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let blinc_layout::renderer::ElementType::Text(t) = &node.element_type
            && t.content.contains("let x")
        {
            faces.push(format!("{:?}", t.font_family));
        }
        stack.extend(tree.layout_tree.children(id));
    }
    assert!(!faces.is_empty(), "the line is drawn");
    assert!(
        faces.iter().all(|f| f.contains("JetBrains Mono")),
        "{faces:?}"
    );
    // Measured in the same face: the underline covers one JetBrains Mono character (0.6 em).
    let underline = ui.find(".blinc-code-underline-error").unwrap();
    let w = tree.get_absolute_bounds(underline).unwrap().width;
    assert!(
        (w - 14.0 * 0.6).abs() < 0.6,
        "one character of JetBrains Mono at 14px: {w}"
    );
}
