# Code Editor

The `code_editor` widget provides a full-featured code editing experience with syntax highlighting, line numbers, folding, search, and more.

## Read-Only Code Block

Display syntax-highlighted code:

```rust
use blinc_layout::prelude::*;
use blinc_layout::syntax::{SyntaxConfig, RustHighlighter};

code(r#"fn main() { println!("Hello"); }"#)
    .syntax(SyntaxConfig::new(RustHighlighter::new()))
    .line_numbers(true)
    .font_size(14.0)
    .w_full()
```

## Editable Code Editor

Full editor with Stateful incremental updates:

```rust
let state = code_editor_state("let x = 42;");

code_editor(&state)
    .syntax(SyntaxConfig::new(RustHighlighter::new()))
    .line_numbers(true)
    .font_size(13.0)
    .on_change(|new_content| {
        println!("Content: {}", new_content);
    })
    .w_full()
    .h(400.0)
```

## Features

### Editing
- Type, Enter (auto-indent), Backspace, Delete
- Tab / Shift+Tab: indent/dedent selected lines
- Cmd+Backspace/Delete: delete word backward/forward

### Navigation
- Arrow keys (with Shift for selection)
- Cmd+Left/Right: word jump
- Smart Home: toggle between first non-whitespace and column 0
- Page Up/Down
- Mouse click cursor positioning

### Clipboard & Undo
- Cmd+C/X/V: copy/cut/paste
- Cmd+Z / Cmd+Shift+Z: undo/redo (200-entry history)
- Cmd+A: select all

### Visual Features
- Syntax highlighting (Rust, JSON, or custom highlighters)
- Line numbers with gutter
- Current line highlight
- Selection rendering
- Bracket matching
- Indentation guides
- Code folding (click gutter chevrons)
- Minimap (optional scaled-down overview)

### Search (Cmd+F)
- VS Code-style search bar overlay (opt in with `.search_enabled(true)`)
- Case sensitive, whole word, regex toggles
- The search runs as you type, with a match count
- Enter steps to the next match, Shift+Enter to the previous
- Find and replace with replace all (Cmd+H opens the replace row)

The bar is built from classed elements, and its design is a stylesheet. Add
`CODE_EDITOR_STYLES` to your application's CSS to get it (the cn theme bundle,
`blinc_cn::cn_bundle()`, already includes it):

```rust
ctx.add_css(blinc_layout::widgets::CODE_EDITOR_STYLES);
```

Without it the bar has a plain fallback look. Override any rule after it to
restyle the bar, using the theme's tokens:

```css
.blinc-find { background: var(--surface); border-radius: var(--radius-md); }
.blinc-find-input:focus { border-color: var(--accent); }
.blinc-find-toggle--on { background: var(--accent); color: var(--text-inverse); }
.blinc-find-count--none { color: var(--warning); }
```

The classes are `blinc-find` (the panel), `blinc-find-row`, `blinc-find-field`,
`blinc-find-input`, `blinc-find-button` (with `-chevron`, `-prev`, `-next`,
`-close`, `-replace`, `-replace-all`), `blinc-find-toggle` and
`blinc-find-toggle--on`, and `blinc-find-count` and `blinc-find-count--none`.

## Syntax Highlighters

Built-in highlighters:

```rust
use blinc_layout::syntax::*;

// Rust
SyntaxConfig::new(RustHighlighter::new())

// JSON
SyntaxConfig::new(JsonHighlighter::new())

// Plain text with custom colors
SyntaxConfig::new(
    PlainHighlighter::new()
        .text_color(Color::rgba(0.8, 0.9, 0.8, 1.0))
        .background(Color::rgba(0.1, 0.12, 0.1, 1.0))
)
```

### Custom Highlighter

Implement the `SyntaxHighlighter` trait:

```rust
struct MyHighlighter;

impl SyntaxHighlighter for MyHighlighter {
    fn token_rules(&self) -> &[TokenRule] {
        &[
            TokenRule::new(r"//.*$", Color::GREEN, false, TokenType::Comment),
            TokenRule::new(r#""[^"]*""#, Color::ORANGE, false, TokenType::String),
            TokenRule::new(r"\b(fn|let|if|else)\b", Color::PURPLE, true, TokenType::Keyword),
        ]
    }

    fn default_color(&self) -> Color { Color::WHITE }
    fn background_color(&self) -> Color { Color::rgb(0.1, 0.1, 0.12) }
}
```

## Configuration

```rust
code_editor(&state)
    .line_numbers(true)        // Show line numbers
    .font_size(13.0)           // Font size in pixels
    .line_height(1.5)          // Line height multiplier
    .padding(16.0)             // Content padding
    .code_bg(Color::BLACK)     // Background color
    .text_color(Color::WHITE)  // Default text color
    .edit(true)                // Enable editing (default for code_editor)
    .indent_guides(true)       // Show vertical indent guides
    .code_folding(true)        // Enable fold/unfold
    .minimap(true)             // Show minimap sidebar
```
