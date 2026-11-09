# Code Editor

The `code_editor` widget provides a full-featured code editing experience with syntax highlighting, line numbers, folding, search, and more.

## Read-Only Code Block

Display syntax-highlighted code:

```rust
use blinc::layout::prelude::*;
use blinc::layout::syntax::{SyntaxConfig, RustHighlighter};

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

The bar is built from classed elements, and its look is the default
stylesheet's `.blinc-find` rules. Override any of them in your application's
CSS, using the theme's tokens:

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

### Colours

A code block or editor without a syntax theme takes its colours from the
default stylesheet's rules, so it follows the theme:

| Class | Part | Default |
|-------|------|---------|
| `blinc-code` | the block or editor | `--surface` fill, `--text-primary` text |
| `blinc-code-line-number` | a line number | `--text-tertiary` |
| `blinc-code-fold`, `blinc-code-fold-hint` | a fold chevron, the `...` of a folded block | `--text-tertiary` |
| `blinc-code-active-line` | the line the caret is on | `--primary` at 5% |
| `blinc-code-selection` | the selection | `--selection` |
| `blinc-code-guide` | an indent guide | `--border` at 55% |
| `blinc-code-bracket` | a matching bracket | `--primary` at 15%, with a border |
| `blinc-code-find-match` | a search match, `data-active` for the current one | `--warning` at 25%, 50% when current |
| `blinc-code-minimap`, `-slider`, `-line` | the minimap, its viewport, a plain line | `--border` edge, `--text-primary` tints |

The caret is drawn in `--blinc-code-caret`, the accent when unset, and code is
set in `--blinc-code-font-family`, the generic monospace face when unset. The
editor measures the caret and selection in the same face, so a family it does
not have falls back to monospace for both:

```css
:root {
    --blinc-code-caret: #f59e0b;
    --blinc-code-font-family: "JetBrains Mono", monospace;
}
.blinc-code-selection { background: rgba(245, 158, 11, 0.3); }
```

A face that is not installed is registered first, with `blinc_app::register_font`.

### Diagnostics

A compiler or linter marks problems in the editor with `set_code_diagnostics`.
Each `CodeDiagnostic` is a span on one line (lines and columns from 0), a
`DiagnosticSeverity` (`Error`, `Warning`, `Info`, `Hint`) and a message. The
editor underlines each span, tints its line, puts a dot in the gutter, and shows
the line's most severe message after its text. It keeps them until the next
call; an empty list clears them.

```rust
set_code_diagnostics(&state, vec![
    CodeDiagnostic::new(1, 12, 13, DiagnosticSeverity::Error, "expected an expression, found `;`"),
    CodeDiagnostic::new(2, 8, 9, DiagnosticSeverity::Warning, "unused variable: `y`"),
]);
```

Each part is a class per severity, coloured from the theme:

| Class | Part | Default |
|-------|------|---------|
| `blinc-code-underline-{error,warning,info,hint}` | the underline | `--error`, `--warning`, `--info`, `--text-tertiary` |
| `blinc-code-mark-{severity}` | the gutter dot | as the underline |
| `blinc-code-line-{error,warning,info}` | the line's tint | the colour at 6 to 9% |
| `blinc-code-lens-{severity}` | the message after the line | as the underline |

A syntax theme colours the text, fill and line numbers itself, and a colour
given to the editor (`code_bg`, `text_color`, a `CodeConfig` colour) is set on
the element; both win over the rules.

## Syntax Highlighters

Built-in highlighters:

```rust
use blinc::layout::syntax::*;

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
