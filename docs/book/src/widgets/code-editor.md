# Code Editor

There are two code widgets. `code(text)` shows code. `code_editor(&state)` edits it, with syntax
highlighting, line numbers, folding, a minimap, diagnostics and search. For a find and replace bar
made of cn components, use [`cn::code_editor`](../cn/code-editor.md) over the same state.

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

The lines scroll sideways in their own scroll container while the gutter stays
put. In a vertical `scroll()`, give the block `.w_full()` (after the other
setters) so it is as wide as the scroll and its long lines scroll inside it.

`Code::bound` takes a signal or computed of the text instead, and follows it in
place: a change rebuilds only the lines it touched, so a block showing a
compiler's output or a log keeps the rest of its rows.

```rust
let output = signal(String::new());
Code::bound(output)
    .syntax(SyntaxConfig::new(RustHighlighter::new()))
    .line_numbers(true)
```

## Editable Code Editor

```rust
let state = code_editor_state("let x = 42;");

code_editor(&state)
    .syntax(SyntaxConfig::new(RustHighlighter::new()))
    .line_numbers(true)
    .code_folding(true)
    .minimap(true)
    .on_change(|new_content| {
        println!("Content: {}", new_content);
    })
    .w_full()
    .h(400.0)
```

Create the state once, outside the builder, so that edits survive a resize or a theme change.
`state.lock().unwrap().value()` is the text.

### Size and scrolling

With `.h(pixels)`, the editor has that height and scrolls its text. Without a height, it is as tall
as its text. A long line scrolls sideways, and the gutter stays in place. A scroll bar shows only
while the text is taller or wider than its box, and only during a scroll.

The text is as tall as its lines plus the padding above and below. At the default sizes, 15 lines
need 325 pixels.

## Editing

| Keys | What they do |
|------|--------------|
| Enter | New line, with the indent of the line before |
| Backspace, Delete | Delete a character or the selection |
| Cmd or Ctrl with Backspace or Delete | Delete a word |
| Tab, Shift+Tab | Indent and dedent the selected lines, by `tab_size` spaces (4 unless set) |
| Cmd or Ctrl+C, X, V | Copy, cut, paste |
| Cmd or Ctrl+Z, with Shift | Undo, redo (the editor keeps 200 steps) |
| Esc | Take the keyboard off the editor |

Text from an input method, such as Chinese or Japanese input, is shown while it is composed.

`.read_only(true)` lets the reader move, select and copy, and nothing changes the text.
`.placeholder("…")` shows dimmed text while there is none.

## Moving and selecting

| Keys or action | What they do |
|----------------|--------------|
| Arrows, with Shift to select | Move by character or line |
| Cmd or Ctrl with Left or Right | Move by word |
| Home | To the first character that is not a space, then to the start of the line |
| End | To the end of the line |
| Page Up, Page Down | Move by the height of the view |
| Cmd or Ctrl+A | Select all |
| Press, drag | Place the cursor, select |
| Double press | Select a word |

On a touch screen, a tap places the cursor and a drag moves it. A long press or a double tap selects
a word and opens the system's edit menu.

## Folding

`.code_folding(true)` shows a chevron in the gutter beside each line that opens a block. A block
starts at a `{` and ends at the matching `}` on a later line. Press the chevron to fold the block:
the lines between the braces are hidden and the first line ends in `...`.

- The cursor keys move over a folded block.
- A block stays folded while you add or remove lines above it.
- A block opens when an edit breaks its braces, or when a search match, an undo or your code puts
  the cursor inside it.

`CodeEditorData::toggle_fold(line)` folds or opens the block on a line, counted from 0.

## Minimap

`.minimap(true)` shows the whole text in small bars at the right edge, coloured as the lines are.
When the text is taller than the view, a box marks the part in view. A press or a drag on the
minimap scrolls the editor. `code_minimap(&state)` makes a minimap that you place yourself.

## Marks in the text

The editor marks the line of the cursor, the selection and the bracket that matches the one beside
the cursor. `.indent_guides(true)` draws a line down each indented block.

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

## Search

`.search_enabled(true)` turns on the editor's own search bar. The cn code editor has a bar of its
own, so it does not need this.

- Cmd or Ctrl+F opens the bar. It starts from a selection on one line.
- The search runs while you type and shows the number of matches.
- Enter moves to the next match, Shift+Enter to the one before. Cmd or Ctrl+G does the same.
- Cmd or Ctrl+H opens the replace row, which replaces one match or all of them. A replacement calls
  `on_change`, and "replace all" is one undo step.
- Cmd or Ctrl+R turns regular expressions on and off. The bar also has buttons for match case and
  whole word.
- With regular expressions, the replacement expands `$1`, `${name}`, `$&` (the whole match) and
  `$$` (a dollar sign). A pattern that does not compile shows the reason in place of the count.
- A match is on one line.

The bar is built from elements with classes. Its look comes from the `.blinc-find` rules in the
default stylesheet, and your CSS can replace any of them:

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

## Theming

The code block and the editor take every colour from the theme, with or without a syntax
highlighter. They change with the theme and between light and dark. Your CSS changes any part.

| Class | Part | Default |
|-------|------|---------|
| `blinc-code` | the block or editor | `--surface` fill, `--text-primary` text, `--radius-md` corners |
| `blinc-code-line-number` | a line number | `--text-tertiary` |
| `blinc-code-fold`, `blinc-code-fold-hint` | a fold chevron, the `...` of a folded block | `--text-tertiary` |
| `blinc-code-active-line` | the line of the cursor | `--primary` at 5% |
| `blinc-code-selection` | the selection | `--selection` |
| `blinc-code-guide` | an indent guide | `--border` |
| `blinc-code-bracket` | a matching bracket | `--primary` at 15%, with a border |
| `blinc-code-find-match` | a search match, with `data-active` on the current one | `--warning` at 25%, 50% on the current one |
| `blinc-code-placeholder` | the placeholder | `--text-tertiary` |
| `blinc-code-minimap` | the minimap's edge | `--border` |

The editor draws the caret and the minimap itself, and reads these variables for them:

| Variable | Part | Default |
|----------|------|---------|
| `--blinc-code-caret` | the caret | `--accent` |
| `--blinc-code-minimap-line` | a minimap line without a syntax colour | `--text-primary` at 40% |
| `--blinc-code-minimap-slider` | the box over the part in view | `--text-primary` at 8% |
| `--blinc-code-font-family` | the face of the code | the generic monospace face |

The editor measures the caret and the selection in the same face. A family that it does not have
falls back to monospace for both. Register a face that is not installed with
`blinc_app::register_font`.

```css
:root {
    --blinc-code-caret: #f59e0b;
    --blinc-code-font-family: "JetBrains Mono", monospace;
}
.blinc-code-selection { background: rgba(245, 158, 11, 0.3); }
```

### Syntax colours

A highlighter names each token, and the token gets a class. The default stylesheet colours the
class from the theme. To change a colour everywhere, set its variable. To change more than the
colour, write a rule for the class.

| Class | Variable | Default |
|-------|----------|---------|
| `blinc-code-token-keyword` | `--blinc-syntax-keyword` | `--primary` mixed with `--error` |
| `blinc-code-token-lifetime` | `--blinc-syntax-lifetime` | the keyword colour |
| `blinc-code-token-string` | `--blinc-syntax-string` | `--success`, darkened or lightened toward the text |
| `blinc-code-token-number` | `--blinc-syntax-number` | `--warning`, toward the text |
| `blinc-code-token-constant` | `--blinc-syntax-constant` | the number colour |
| `blinc-code-token-comment` | `--blinc-syntax-comment` | between `--text-secondary` and `--text-tertiary` |
| `blinc-code-token-function` | `--blinc-syntax-function` | `--primary`, toward the text |
| `blinc-code-token-property` | `--blinc-syntax-property` | the function colour |
| `blinc-code-token-type` | `--blinc-syntax-type` | `--info` mixed with `--success`, toward the text |
| `blinc-code-token-macro` | `--blinc-syntax-macro` | the type colour |
| `blinc-code-token-operator` | `--blinc-syntax-operator` | `--text-secondary` |
| `blinc-code-token-variable` | `--blinc-syntax-variable` | `--text-primary` |

```css
:root { --blinc-syntax-keyword: var(--accent); }
.blinc-code-token-comment { color: var(--text-tertiary); }
```

A colour given to a builder (`code_bg`, `text_color`, a `CodeConfig` colour) or named by a
highlighter is set on the element, and it wins over these rules.

## Driving an editor from outside

An application, or a toolbar of its own, changes the editor through these:

```rust
use blinc_layout::widgets::code::{
    focus_code_editor, refresh_code_editor, replace_all_code_matches, set_code_value,
};

// Replace all the text. It is your change: `on_change` does not report it, and undo forgets it.
set_code_value(&state, "fn main() {}");

// Search with your own controls: set the query, mark the matches and draw the editor again.
{
    let mut editor = state.lock().unwrap();
    editor.apply_search_query("main".to_string());
    editor.find_open = true;
}
refresh_code_editor(&state);

// A replacement calls `on_change`.
replace_all_code_matches(&state);

// Give the editor the keyboard again, after one of your fields had it.
focus_code_editor(&state);
```

`CodeEditorData` holds the query, the options (`search_case_sensitive`, `search_whole_word`,
`search_regex`), the matches and, for a regex that does not compile, `search_error`.

## Syntax Highlighters

The built-in highlighters are `RustHighlighter`, `JsonHighlighter`, `LuaHighlighter` and
`PlainHighlighter`, which marks strings, numbers and comments in most languages.

```rust
use blinc::layout::syntax::{JsonHighlighter, SyntaxConfig};

let config = SyntaxConfig::new(JsonHighlighter::new());
```

### Custom Highlighter

Implement the `SyntaxHighlighter` trait. A rule is a regular expression, matched against one line at
a time. Where two matches overlap, the one that starts first wins.

```rust
use blinc::core::Color;
use blinc::layout::syntax::{SyntaxHighlighter, TokenRule, TokenType};

struct Ini {
    rules: Vec<TokenRule>,
}

impl Ini {
    fn new() -> Self {
        Self {
            rules: vec![
                TokenRule::themed(r"^\s*[;#].*$", TokenType::Comment),
                TokenRule::themed(r"^\[[^\]]*\]", TokenType::Keyword).bold(),
                TokenRule::themed(r"^[^=]+", TokenType::custom("property")),
                // A colour of its own, in every theme.
                TokenRule::new(r"\b(true|false)\b", Color::rgb(0.9, 0.5, 0.2)),
            ],
        }
    }
}

impl SyntaxHighlighter for Ini {
    fn token_rules(&self) -> &[TokenRule] {
        &self.rules
    }
}
```

`TokenRule::themed` leaves the colour to the stylesheet. A custom token type gets the class
`blinc-code-token-` plus its name, and your CSS colours it. `TokenRule::new` gives a fixed colour.

The trait also has `default_color`, `background_color` and `line_number_color`. They return
nothing unless you implement them, and then the theme applies.

## Configuration

```rust
code_editor(&state)
    .line_numbers(true)        // Show line numbers
    .gutter_width(48.0)        // Width of the line number column
    .font_size(13.0)           // Font size in pixels
    .line_height(1.5)          // Line height multiplier
    .padding(16.0)             // Space around the text
    .rounded(8.0)              // Corner radius over the theme's
    .code_bg(Color::BLACK)     // Background over the theme's
    .text_color(Color::WHITE)  // Plain text colour over the theme's
    .read_only(false)          // Let the text be changed
    .tab_size(4)               // Spaces per Tab
    .placeholder("Write…")     // Shown while empty
    .indent_guides(true)       // Lines down indented blocks
    .code_folding(true)        // Fold blocks from the gutter
    .minimap(true)             // The text in small at the right edge
    .search_enabled(true)      // The built-in search bar
    .on_change(|text| {})      // Called after each change
```
