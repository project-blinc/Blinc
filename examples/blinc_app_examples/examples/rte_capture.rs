//! Offscreen capture of the rich text editor, for debugging its layout.
//!
//! The windowed demo needs a real window and a screen grab, which makes
//! iteration slow and the result non-deterministic. This renders the
//! same document through `BlincApp::render_to_rgba8` and writes a PNG.
//!
//! cargo run -p blinc_app_examples --features cn --example rte_capture -- out.png

use blinc_app::{BlincApp, BlincConfig};
use blinc_core::Color;
use blinc_layout::prelude::*;
use blinc_layout::widgets::rich_text_editor::{
    document::RichDocument, editor::rich_text_editor, render::RichTextTheme, state::rich_text_state,
};
use blinc_theme::{ColorScheme, ThemeState};

const W: u32 = 820;
const H: u32 = 760;
const COLUMN: f32 = 720.0;

/// The paragraphs that exercise the multi-run path: text with inline
/// code between prose, which is where runs are positioned absolutely.
const SAMPLE: &str = r#"# The Rich Text Editor

A WYSIWYG editor for styled prose. The page is authored as Markdown and parsed into a `RichDocument` via `RichDocument::from_markdown`.

## Inline marks

Inline marks: **bold**, *italic*, ~~strikethrough~~, `inline code`, and a [hyperlink](https://example.com).

## Lists

- first item — flat list, no indent
- second item — also at depth 0
"#;

fn scene() -> impl ElementBuilder {
    let theme = RichTextTheme::dark();
    let state = rich_text_state(RichDocument::from_markdown(SAMPLE, Color::WHITE));

    div()
        .w(W as f32)
        .h(H as f32)
        .bg(Color::rgba(0.07, 0.07, 0.10, 1.0))
        .flex_col()
        .padding_x_px(32.0)
        .padding_y_px(24.0)
        .child(
            div()
                .w(COLUMN)
                .child(rich_text_editor(&state, theme, COLUMN)),
        )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "rte_capture.png".to_string());

    ThemeState::init(
        blinc_theme::themes::universal::HybridTheme::bundle(),
        ColorScheme::Dark,
    );
    blinc_layout::text_measurer::init_text_measurer();

    // The editor keeps its state in the context store.
    if !blinc_core::context_state::BlincContextState::is_initialized() {
        blinc_core::context_state::BlincContextState::init(
            blinc_core::reactive::global_graph(),
            std::sync::Arc::new(std::sync::Mutex::new(
                blinc_core::context_state::HookState::new(),
            )),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
    }

    let mut app = BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })?;

    // Two passes: the editor caches measurement state on the first
    // build, and the windowed runner always renders at least twice.
    let mut pixels = Vec::new();
    for _ in 0..2 {
        pixels = app.render_to_rgba8(&scene(), W, H)?;
    }

    let img: image::RgbaImage = image::ImageBuffer::from_raw(W, H, pixels)
        .ok_or("pixel buffer did not match the requested size")?;
    img.save(&out)?;
    eprintln!("wrote {out}");
    Ok(())
}
