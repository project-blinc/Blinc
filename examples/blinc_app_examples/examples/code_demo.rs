//! Code Element Demo
//!
//! Demonstrates both read-only code display and editable code editor.
//!
//! Features demonstrated:
//! - Syntax highlighting with built-in Rust and JSON highlighters
//! - Line numbers in the gutter
//! - Editable code editor with Stateful incremental updates
//! - Cursor, selection, clipboard (Cmd+C/X/V), undo/redo (Cmd+Z)
//! - Word navigation (Cmd+Left/Right), select all (Cmd+A)
//! - Vertical scrolling in the editor
//!
//! Run with: cargo run -p blinc_app_examples --example code_demo

use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;
use blinc_core::Color;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Code Element Demo".to_string(),
        width: 1000,
        height: 800,
        resizable: true,
        ..Default::default()
    };

    blinc_app::windowed::WindowedApp::run(config, build_ui)
}

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    // Create editable code state outside the UI builder so it persists
    let editor_state = code_editor_state(
        r#"use blinc_layout::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let ui = div()
        .flex_col()
        .gap(16.0)
        .child(text("Hello, World!").size(24.0))
        .child(button("Click me").on_click(|_| {
            println!("Button clicked!");
        }));

    // Try editing this code!
    // Cmd+C/X/V for clipboard, Cmd+Z for undo
    // Cmd+Left/Right for word navigation
    // Cmd+A to select all
    render(ui);
}"#,
    );

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.06, 0.06, 0.08, 1.0))
        .flex_col()
        .gap(5.0)
        .p(10.0)
        .items_center()
        // Title
        .child(text("Code Element Demo").size(28.0).color(Color::WHITE))
        .child(
            text("Read-only display + editable code editor")
                .size(18.0)
                .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
        )
        // Demo sections in a scrollable area
        .child(
            scroll()
                .rounded(8.0)
                .w_full()
                .h_full()
                .direction(ScrollDirection::Vertical)
                .bg(Color::from_hex(0x222222))
                .child(
                    div()
                        .w_full()
                        .h_full()
                        .p(5.0)
                        .flex_col()
                        .gap(16.0)
                        .flex_grow()
                        .child(editable_code_section(&editor_state))
                        .child(rust_code_section())
                        .child(json_code_section())
                        .child(plain_code_section())
                        .child(line_numbers_section()),
                ),
        )
}

/// Editable code editor with syntax highlighting
fn editable_code_section(state: &SharedCodeEditorState) -> Div {
    div()
        .flex_col()
        .gap(2.0)
        .child(
            text("Editable Code Editor")
                .size(18.0)
                .color(Color::WHITE)
                .bold(),
        )
        .child(
            text("Click to focus, then type. Supports selection, clipboard, undo/redo, word nav.")
                .size(12.0)
                .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
        )
        .child(
            div()
                .flex_row()
                .w_full()
                .h(300.0)
                .overflow_clip()
                .rounded(8.0)
                .child(
                    code_editor(state)
                        .syntax(SyntaxConfig::new(RustHighlighter::new()))
                        .line_numbers(true)
                        .indent_guides(true)
                        .code_folding(true)
                        // Demo hosts the editor in the main viewport
                        // so Cmd+F / Cmd+G / Cmd+H / Cmd+R are safe
                        // to enable. Hosts embedding inside a
                        // cn::popover / overlay should leave this
                        // off (default) — see search_enabled doc.
                        .search_enabled(true)
                        .font_size(13.0)
                        .w_full()
                        .h(300.0)
                        .on_change(|new_content| {
                            tracing::info!("Content changed ({} chars)", new_content.len());
                        }),
                )
                .child(code_minimap(state)),
        )
}

/// Read-only Rust code with syntax highlighting
fn rust_code_section() -> Div {
    let rust_code = r#"use blinc_layout::prelude::*;

fn fibonacci(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}"#;

    div()
        .flex_col()
        .gap(2.0)
        .child(
            text("Rust Syntax Highlighting (read-only)")
                .size(18.0)
                .color(Color::WHITE)
                .bold(),
        )
        .child(
            text("Using RustHighlighter for keyword, string, and comment highlighting")
                .size(12.0)
                .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
        )
        .child(
            code(rust_code)
                .syntax(SyntaxConfig::new(RustHighlighter::new()))
                .line_numbers(true)
                .font_size(13.0)
                .w_full(),
        )
}

/// JSON code with syntax highlighting
fn json_code_section() -> Div {
    let json_code = r#"{
    "name": "blinc",
    "version": "0.1.0",
    "description": "A GPU-accelerated UI framework",
    "features": {
        "syntax_highlighting": true,
        "line_numbers": true,
        "editable": false
    },
    "dependencies": ["wgpu", "taffy", "regex"]
}"#;

    div()
        .flex_col()
        .gap(2.0)
        .child(
            text("JSON Syntax Highlighting")
                .size(18.0)
                .color(Color::WHITE)
                .bold(),
        )
        .child(
            text("Using JsonHighlighter for keys, strings, numbers, and booleans")
                .size(12.0)
                .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
        )
        .child(
            code(json_code)
                .syntax(SyntaxConfig::new(JsonHighlighter::new()))
                .line_numbers(true)
                .font_size(13.0)
                .w_full(),
        )
}

/// Plain text without highlighting
fn plain_code_section() -> Div {
    let plain_text = r#"This is plain preformatted text.
No syntax highlighting is applied.
Useful for logs, output, or raw text display.

    Indentation is preserved.
    Whitespace matters here."#;

    div()
        .flex_col()
        .gap(2.0)
        .child(
            text("Plain Text (pre)")
                .size(18.0)
                .color(Color::WHITE)
                .bold(),
        )
        .child(
            text("Using PlainHighlighter or no highlighter at all")
                .size(12.0)
                .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
        )
        .child(
            pre(plain_text)
                .syntax(SyntaxConfig::new(
                    PlainHighlighter::new()
                        .text_color(Color::rgba(0.8, 0.9, 0.8, 1.0))
                        .background(Color::rgba(0.1, 0.12, 0.1, 1.0)),
                ))
                .font_size(13.0)
                .w_full(),
        )
}

/// Demonstrating line numbers toggle
fn line_numbers_section() -> Div {
    let sample_code = r#"fn fibonacci(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}"#;

    div()
        .w_full()
        .flex_col()
        .gap(2.0)
        .child(
            text("Line Numbers Comparison")
                .size(18.0)
                .color(Color::WHITE)
                .bold(),
        )
        .child(
            text("Same code with and without line numbers")
                .size(12.0)
                .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
        )
        .child(
            scroll()
                .h_fit()
                .w_full()
                .direction(ScrollDirection::Horizontal)
                .child(
                    div()
                        .flex_row()
                        .gap(5.0)
                        .justify_between()
                        .child(
                            div()
                                .flex_col()
                                .gap(4.0)
                                .flex_grow()
                                .child(text("With line numbers").size(12.0).color(Color::WHITE))
                                .child(
                                    code(sample_code)
                                        .syntax(SyntaxConfig::new(RustHighlighter::new()))
                                        .line_numbers(true)
                                        .font_size(12.0)
                                        .w_full(),
                                ),
                        )
                        .child(div().h_full().w(1.0).bg(Color::WHITE.with_alpha(0.5)))
                        .child(
                            div()
                                .flex_col()
                                .gap(4.0)
                                .flex_grow()
                                .child(text("Without line numbers").size(12.0).color(Color::WHITE))
                                .child(
                                    code(sample_code)
                                        .syntax(SyntaxConfig::new(RustHighlighter::new()))
                                        .line_numbers(false)
                                        .font_size(12.0)
                                        .w_full(),
                                ),
                        ),
                ),
        )
}
