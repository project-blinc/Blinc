//! Markdown Editor Demo
//!
//! A split-view markdown editor with:
//! - TextArea on the left for writing markdown source
//! - Scroll container on the right for live preview
//!
//! Run with: cargo run -p blinc_app --example markdown_demo --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::{State, WindowedApp, WindowedContext};
use blinc_core::{Color, SignalId};
use blinc_layout::markdown::markdown_light;
use std::sync::{Arc, Mutex};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let config = WindowConfig {
        title: "Markdown Editor".to_string(),
        width: 1200,
        height: 800,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

const DEFAULT_MARKDOWN: &str = r#"# Welcome to Markdown Editor

This is a **live preview** markdown editor built with Blinc.

## Features

- *Italic* and **bold** text
- ~~Strikethrough~~ text
- Inline `code` snippets

### Lists

Unordered list:
- First item
- Second item
- Third item

Ordered list:
1. Step one
2. Step two
3. Step three

### Task Lists

- [x] Implement markdown parser
- [x] Create preview component
- [ ] Add syntax highlighting
- [ ] Support images

### Code Blocks

```rust
fn main() {
    println!("Hello, Blinc!");
}
```

### Blockquotes

> "The best way to predict the future is to invent it."
> — Alan Kay

### Horizontal Rule

---

### Links

[Visit GitHub](https://github.com)

### Tables

| Feature | Status |
|---------|--------|
| Headings | Done |
| Lists | Done |
| Code | Done |

---

*Edit the markdown on the left to see changes!*
"#;

fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder {
    // Create text area state for editing that persists across rebuilds
    let markdown_state = ctx.use_state_keyed("markdown_source", || {
        let mut state = TextAreaState::new();
        state.set_value(DEFAULT_MARKDOWN);
        Arc::new(Mutex::new(state))
    });
    let signal_id = markdown_state.signal_id();

    let panel_width = (ctx.width - 48.0) / 2.0; // Split width minus padding and gap
    let panel_height = ctx.height - 100.0; // Leave room for header

    // Clone the state handle for use in child builders
    let editor_state = markdown_state.clone();
    let preview_state_handle = markdown_state.clone();

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
        .flex_col()
        .p(16.0)
        .gap(16.0)
        // Header
        .child(build_header())
        // Main content: split view
        .child(
            div()
                .h(panel_height)
                .flex_row()
                .gap(5.0)
                // Left panel: Editor
                .child(build_editor_panel(editor_state, panel_width, panel_height))
                // Right panel: Preview
                .child(build_preview_panel(
                    ctx,
                    preview_state_handle,
                    signal_id,
                    panel_width,
                    panel_height,
                )),
        )
}

fn build_header() -> impl ElementBuilder {
    div()
        .flex_col()
        .w_full()
        .justify_center()
        .items_center()
        .gap(4.0)
        .child(
            div()
                .flex_col()
                .gap(2.0)
                .items_center()
                .child(
                    h1("Markdown Editor")
                        .color(Color::WHITE)
                        .weight(FontWeight::Bold),
                )
                .child(
                    span("(Live Preview)")
                        .size(14.0)
                        .color(Color::rgba(0.5, 0.8, 1.0, 0.8)),
                ),
        )
        .child(
            span("Edit markdown on the left, see preview on the right")
                .size(14.0)
                .color(Color::rgba(0.6, 0.6, 0.6, 1.0)),
        )
}

fn build_editor_panel(
    text_state: State<Arc<Mutex<TextAreaState>>>,
    width: f32,
    height: f32,
) -> impl ElementBuilder {
    let theme = ThemeState::get();
    // Get the signal ID before calling .get() to use for change notifications
    let change_signal = text_state.signal_id();
    // Get the actual state value - this is fine here since text_area holds a reference
    let state_value = text_state.get();

    div()
        .w(width)
        .h(height)
        .flex_col()
        .gap(2.0)
        // Panel header
        .child(
            div()
                .flex_row()
                .items_center()
                .gap(2.0)
                .child(
                    span("Source")
                        .size(14.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::rgba(0.4, 0.8, 1.0, 1.0)),
                )
                .child(
                    span("(Markdown)")
                        .size(12.0)
                        .color(Color::rgba(0.5, 0.5, 0.5, 1.0)),
                ),
        )
        // Editor container
        .child(
            div()
                .w_full()
                .h(height - 200.0)
                .bg(theme.color(ColorToken::SurfaceElevated))
                .rounded(8.0)
                .border(1.0, Color::rgba(0.3, 0.3, 0.35, 1.0))
                .child(
                    text_area(&state_value)
                        .w_full()
                        .h_full()
                        .font_size(14.0)
                        .on_change_signal(change_signal),
                ),
        )
}

fn build_preview_panel(
    ctx: &mut WindowedContext,
    text_state_handle: State<Arc<Mutex<TextAreaState>>>,
    change_signal_id: SignalId,
    width: f32,
    height: f32,
) -> impl ElementBuilder {
    let preview_state = ctx.use_state(PreviewState::default());

    div()
        .w(width)
        .h(height)
        .flex_col()
        .gap(2.0)
        // Panel header
        .child(
            div()
                .flex_row()
                .items_center()
                .gap(2.0)
                .child(
                    span("Preview")
                        .size(14.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::rgba(0.4, 1.0, 0.8, 1.0)),
                )
                .child(
                    span("(Rendered)")
                        .size(12.0)
                        .color(Color::rgba(0.5, 0.5, 0.5, 1.0)),
                ),
        )
        // Preview container with scroll - stateful wraps only the markdown content
        .child({
            let theme = ThemeState::get();
            div()
                .w_full()
                .h(height - 200.0)
                .bg(theme.color(ColorToken::SurfaceElevated))
                .rounded(8.0)
                .border(1.0, Color::rgba(0.3, 0.3, 0.35, 1.0))
                .overflow_clip()
                .child(
                    scroll()
                        .w_full()
                        .h_full()
                        .direction(ScrollDirection::Vertical)
                        .child(
                            div().h_fit().w_full().justify_center().p(4.0).child(
                                stateful(preview_state)
                                    .w_full()
                                    .flex_grow()
                                    .deps(&[change_signal_id])
                                    .on_state(move |_state, container| {
                                        // Get the state value inside the reactive callback
                                        let text_state = text_state_handle.get();
                                        let markdown_content = text_state
                                            .lock()
                                            .map(|s| s.value())
                                            .unwrap_or_default();
                                        *container = std::mem::take(container)
                                            .child(markdown_light(&markdown_content));
                                    }),
                            ),
                        ),
                )
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
enum PreviewState {
    #[default]
    Active,
}

impl StateTransitions for PreviewState {
    fn on_event(&self, _event: u32) -> Option<Self> {
        None
    }
}
