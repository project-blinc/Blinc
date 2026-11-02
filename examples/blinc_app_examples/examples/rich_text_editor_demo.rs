//! Rich Text Editor
//!
//! Full editable rich text editor with cursor, selection, and inline
//! formatting. Demonstrates every block kind and inline mark currently
//! supported by the editor model:
//!
//! - Headings (H1–H3)
//! - Paragraphs with mixed bold / italic / underline / strikethrough /
//!   inline-code / colored / linked spans
//! - Bullet and numbered lists, including nested lists via `indent`
//! - Block quote
//! - Horizontal divider
//!
//! Run with: `cargo run -p blinc_app_examples --example rich_text_editor_demo`

use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;
use blinc_core::Color;
use blinc_core::context_state::BlincContextState;
use blinc_layout::widgets::rich_text_editor::{
    document::RichDocument,
    editor::rich_text_editor,
    render::RichTextTheme,
    state::{RichTextState, rich_text_state},
};

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Blinc Rich Text Editor Demo".to_string(),
        width: 900,
        height: 800,
        resizable: true,
        fullscreen: false,
        ..Default::default()
    };

    blinc_app::windowed::WindowedApp::run(config, build_ui)
}

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    // Colours from the stylesheet, so the editor follows the theme.
    let theme = RichTextTheme::default();

    // Compute the explicit pixel width of the column the document renders
    // into. RichText needs this so each line wraps to the correct width
    // instead of overflowing the container on the right.
    const HORIZONTAL_PADDING: f32 = 32.0;
    const MAX_COLUMN_WIDTH: f32 = 720.0;
    let column_width = (ctx.width - 2.0 * HORIZONTAL_PADDING).min(MAX_COLUMN_WIDTH);

    // Persist the editor state across rebuilds via the context state store.
    let blinc = BlincContextState::get();
    let state_signal: blinc_core::State<Option<RichTextState>> =
        blinc.use_state_keyed("rich_text_editor_demo", || None);
    let state: RichTextState = match state_signal.get() {
        Some(s) => s,
        None => {
            let s = rich_text_state(sample_document(&theme));
            state_signal.set(Some(s.clone()));
            s
        }
    };

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.07, 0.07, 0.10, 1.0))
        .flex_col()
        .child(
            // Header bar — every spacing here is in raw pixels via _px helpers
            // because the rest of the layout API uses 4-unit spacing.
            div()
                .w_full()
                .h(56.0)
                .padding_x_px(32.0)
                .bg(Color::rgba(0.11, 0.11, 0.15, 1.0))
                .border_bottom(1.0, Color::rgba(0.20, 0.20, 0.26, 1.0))
                .flex_row()
                .items_center()
                .gap_px(16.0)
                .child(
                    text("Rich Text Editor")
                        .size(18.0)
                        .weight(FontWeight::Bold)
                        .color(Color::WHITE),
                )
                .child(
                    text("editing + undo")
                        .size(12.0)
                        .color(Color::rgba(0.55, 0.55, 0.65, 1.0)),
                ),
        )
        .child(
            // Document area inside a scroll container.
            // Outer flex_row + justify_center centers a fixed-width column.
            scroll().w_full().h(ctx.height - 56.0).child(
                div()
                    .w_full()
                    .flex_row()
                    .justify_center()
                    .padding_x_px(HORIZONTAL_PADDING)
                    .padding_y_px(32.0)
                    .child(div().w(column_width).child(rich_text_editor(
                        &state,
                        theme.clone(),
                        column_width,
                    ))),
            ),
        )
}

// =============================================================================
// Sample document
// =============================================================================
//
// The whole document is authored as a Markdown string and parsed into
// a `RichDocument` via `RichDocument::from_markdown`. This is the
// expected workflow for editor users — author content as plain
// Markdown, hand it to the editor, get a fully-styled document.
//
// `SpanTemplate` and `StyledLine::from_segments` are still available
// in `blinc_layout::styled_text` for cases where Markdown isn't the
// right authoring format (programmatic / data-driven content).

const SAMPLE_MARKDOWN: &str = r#"# The Rich Text Editor

A WYSIWYG editor for styled prose, built on the existing styled-text primitives. The whole page below is authored as Markdown and parsed into a `RichDocument` via `RichDocument::from_markdown`.

## Inline marks

Inline marks: **bold**, *italic*, ~~strikethrough~~, `inline code`, and a [hyperlink](https://example.com).

## Lists

### Bullet list

- first item — flat list, no indent
- second item — also at depth 0
- back to depth 0

### Numbered list

1. ordinals are computed at render time
2. they restart after a non-numbered block
3. …so reordering items needs no bookkeeping

A plain paragraph breaks the run.

1. a fresh run begins again at 1
2. then 2

## Block quote

> The best way to predict the future is to invent it. — Alan Kay

---

## Mixed prose

You can **combine multiple marks** on a single *run*, or ~~strike them out~~ entirely. 

Even `code spans` sit happily inline.

Editing, undo/redo, and the floating selection toolbar all work — select any text and try the toolbar buttons or Cmd+B / Cmd+I / Cmd+U / Cmd+E.
"#;

fn sample_document(_theme: &RichTextTheme) -> RichDocument {
    // Transparent: text without a colour of its own takes the editor's.
    RichDocument::from_markdown(SAMPLE_MARKDOWN, Color::TRANSPARENT)
}
