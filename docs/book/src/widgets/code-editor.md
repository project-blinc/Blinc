# Code Editor

There are two code widgets. `code(text)` shows code. `code_editor(&state)` edits code. The editor has
syntax highlighting, line numbers, folding, a minimap, diagnostics and search.

[`cn::code_editor`](../cn/code-editor.md) is the same editor with a find and replace bar made of cn
components.

## Read-Only Code Block

A code block shows code with syntax highlighting:

```rust
use blinc::layout::prelude::*;
use blinc::layout::syntax::{SyntaxConfig, RustHighlighter};

code(r#"fn main() { println!("Hello"); }"#)
    .syntax(SyntaxConfig::new(RustHighlighter::new()))
    .line_numbers(true)
    .font_size(14.0)
    .w_full()
```

Long lines scroll sideways, and the line numbers stay in place. If the block is inside a vertical
`scroll()`, call `.w_full()` after the other methods. The block then takes the width of the scroll,
and its long lines scroll inside it.

`Code::bound` takes a signal or a computed value as the text. The block follows each change of the
value. It builds again only the lines that changed. This helps a block that shows compiler output or
a log.

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

Create the state one time, outside the builder. Then your edits stay after a resize or a theme
change. `state.lock().unwrap().value()` gives the text.

### Size and scrolling

If you set `.h(pixels)`, the editor has that height and scrolls its text. If you do not, the editor
is as tall as its text. A long line scrolls sideways, and the line numbers stay in place. If there
is text to scroll to, a scroll bar shows while you scroll.

The height of the text is the height of its lines plus the space above and below. If the box hides
only the space below the last line, the editor does not scroll.

## Editing

| Keys | What they do |
|------|--------------|
| Enter | Start a new line with the same indent as the line before |
| Backspace, Delete | Delete a character or the selected text |
| Cmd or Ctrl with Backspace or Delete | Delete a word |
| Tab, Shift+Tab | Add or remove an indent on the selected lines (`tab_size` spaces, 4 by default) |
| Cmd or Ctrl+C, X, V | Copy, cut, paste |
| Cmd or Ctrl+Z | Undo. The editor keeps 200 steps |
| Cmd or Ctrl+Shift+Z | Redo |
| Esc | Move the keyboard away from the editor |

An input method is a tool that types characters such as Chinese or Japanese. The editor shows its
text while you compose it.

`.read_only(true)` lets the user move, select and copy, but not change the text. `.placeholder("…")`
shows faint text while the editor is empty.

## Moving and selecting

| Keys or action | What they do |
|----------------|--------------|
| Arrow keys | Move by one character or one line. With Shift, select |
| Cmd or Ctrl with Left or Right | Move by one word |
| Home | Move to the first character that is not a space, then to the start of the line |
| End | Move to the end of the line |
| Page Up, Page Down | Move by the height of the editor |
| Cmd or Ctrl+A | Select all |
| Click, drag | Place the cursor, select text |
| Double click | Select a word |

On a touch screen, a tap places the cursor and a drag moves it. A long press or a double tap selects
a word and opens the edit menu of the system.

## Folding

`.code_folding(true)` shows a small arrow beside each line that opens a block. A block starts at a `{`
and ends at the matching `}` on a later line. Click the arrow to fold the block. The lines between the
two braces are then hidden, and the first line ends in `...`.

- The cursor keys skip a folded block.
- A folded block stays folded while you add or remove lines above it.
- If you change the braces of a block, the block opens.
- If a search match, an undo or your code puts the cursor inside a block, the block opens.

`CodeEditorData::toggle_fold(line)` folds or opens the block on a line. The first line is line 0.

## Minimap

The minimap is a small picture of the whole text at the right edge. `.minimap(true)` shows it. Each
line has the color of its code. If the text is taller than the editor, a box shows the visible part.
Click or drag on the minimap to scroll. `code_minimap(&state)` makes a minimap that you place
yourself.

## Marks in the text

The editor marks these items:

- The line that has the cursor
- The selected text
- The bracket that matches the bracket beside the cursor

`.indent_guides(true)` also draws a vertical line beside each indented block.

### Diagnostics

A diagnostic is a problem that a tool reports, for example a compiler error. Call
`set_code_diagnostics` to show diagnostics in the editor. Each `CodeDiagnostic` has these parts:

- A position on one line (lines and columns start at 0)
- A `DiagnosticSeverity`: `Error`, `Warning`, `Info` or `Hint`
- A message

For each diagnostic, the editor underlines the text, colors the line and puts a dot beside it. After
the line, it shows the message of the most serious problem on that line. The marks stay until the next
call. An empty list removes them.

```rust
set_code_diagnostics(&state, vec![
    CodeDiagnostic::new(1, 12, 13, DiagnosticSeverity::Error, "expected an expression, found `;`"),
    CodeDiagnostic::new(2, 8, 9, DiagnosticSeverity::Warning, "unused variable: `y`"),
]);
```

Each part has one class for each severity, and the theme gives its color:

| Class | Part | Default |
|-------|------|---------|
| `blinc-code-underline-{error,warning,info,hint}` | The underline | `--error`, `--warning`, `--info`, `--text-tertiary` |
| `blinc-code-mark-{severity}` | The dot | The color of the underline |
| `blinc-code-line-{error,warning,info}` | The color of the line | The same color at 6 to 9% |
| `blinc-code-lens-{severity}` | The message after the line | The color of the underline |

## Search

`.search_enabled(true)` adds the search bar of this editor. The cn code editor has its own bar, so it
does not use this one.

- Cmd or Ctrl+F opens the bar. A selection on one line becomes the search text.
- The search runs while you type and shows the number of matches.
- Enter moves to the next match, and Shift+Enter moves to the one before. Cmd or Ctrl+G also works.
- Cmd or Ctrl+H opens the replace row. You can replace one match or all matches.
- Each replacement calls `on_change`. One undo reverses "replace all".
- Cmd or Ctrl+R turns regular expressions on or off. Two buttons set match case and whole word.
- With regular expressions, the replacement text can use `$1`, `${name}`, `$&` (the whole match)
  and `$$` (a dollar sign).
- If a pattern is not valid, the bar shows the reason where the match count was.
- A match is always on one line.

The `.blinc-find` rules in the default stylesheet give the bar its style. Rules in your CSS replace
them:

```css
.blinc-find { background: var(--surface); border-radius: var(--radius-md); }
.blinc-find-input:focus { border-color: var(--accent); }
.blinc-find-toggle--on { background: var(--accent); color: var(--text-inverse); }
.blinc-find-count--none { color: var(--warning); }
```

The bar has these classes:

- `blinc-find` (the bar), `blinc-find-row`, `blinc-find-field`, `blinc-find-input`
- `blinc-find-button`, with the endings `-chevron`, `-prev`, `-next`, `-close`, `-replace` and
  `-replace-all`
- `blinc-find-toggle`, `blinc-find-toggle--on`, `blinc-find-count` and `blinc-find-count--none`

## Theming

The code block and the editor take all their colors from the theme, with or without a highlighter. The
colors change with the theme, and between light and dark. Your CSS can change each part.

| Class | Part | Default |
|-------|------|---------|
| `blinc-code` | The block or editor | `--surface` background, `--text-primary` text, `--radius-md` corners |
| `blinc-code-line-number` | A line number | `--text-tertiary` |
| `blinc-code-fold`, `blinc-code-fold-hint` | A fold arrow, and the `...` of a folded block | `--text-tertiary` |
| `blinc-code-active-line` | The line that has the cursor | `--primary` at 5% |
| `blinc-code-selection` | The selected text | `--selection` |
| `blinc-code-guide` | An indent guide | `--border` |
| `blinc-code-bracket` | A matching bracket | `--primary` at 15%, with a border |
| `blinc-code-find-match` | A search match. The current one has `data-active` | `--warning` at 25%, or 50% for the current one |
| `blinc-code-placeholder` | The placeholder | `--text-tertiary` |
| `blinc-code-minimap` | The edge of the minimap | `--border` |

The editor draws the cursor and the minimap itself. These variables set their colors and the font:

| Variable | Part | Default |
|----------|------|---------|
| `--blinc-code-caret` | The cursor | `--accent` |
| `--blinc-code-minimap-line` | A minimap line that has no syntax color | `--text-primary` at 40% |
| `--blinc-code-minimap-slider` | The box over the visible part | `--text-primary` at 8% |
| `--blinc-code-font-family` | The font of the code | The generic monospace font |

The editor uses the same font to place the cursor and the selection. If the app does not have a font
family, the editor uses the monospace font. If a font is not installed, register it first with
`blinc_app::register_font`.

```css
:root {
    --blinc-code-caret: #f59e0b;
    --blinc-code-font-family: "JetBrains Mono", monospace;
}
.blinc-code-selection { background: rgba(245, 158, 11, 0.3); }
```

### Syntax colors

A highlighter finds the tokens, for example keywords and strings. Each token gets a class, and the
default stylesheet gives that class a color from the theme. To change a color, set its variable. To
change more than the color, write a rule for the class.

| Class | Variable | Default |
|-------|----------|---------|
| `blinc-code-token-keyword` | `--blinc-syntax-keyword` | `--primary` mixed with `--error` |
| `blinc-code-token-lifetime` | `--blinc-syntax-lifetime` | The keyword color |
| `blinc-code-token-string` | `--blinc-syntax-string` | `--success` mixed with the text color |
| `blinc-code-token-number` | `--blinc-syntax-number` | `--warning` mixed with the text color |
| `blinc-code-token-constant` | `--blinc-syntax-constant` | The number color |
| `blinc-code-token-comment` | `--blinc-syntax-comment` | A mix of `--text-secondary` and `--text-tertiary` |
| `blinc-code-token-function` | `--blinc-syntax-function` | `--primary` mixed with the text color |
| `blinc-code-token-property` | `--blinc-syntax-property` | The function color |
| `blinc-code-token-type` | `--blinc-syntax-type` | A mix of `--info`, `--success` and the text color |
| `blinc-code-token-macro` | `--blinc-syntax-macro` | The type color |
| `blinc-code-token-operator` | `--blinc-syntax-operator` | `--text-secondary` |
| `blinc-code-token-variable` | `--blinc-syntax-variable` | `--text-primary` |

```css
:root { --blinc-syntax-keyword: var(--accent); }
.blinc-code-token-comment { color: var(--text-tertiary); }
```

Some colors come from your code: `code_bg`, `text_color`, a `CodeConfig` color, or a color that a
highlighter gives. The widget sets these colors on the element, and they replace the rules above.

## Driving an editor from outside

Your app, or a toolbar that you build, can change the editor with these functions:

```rust
use blinc::layout::widgets::code::{
    focus_code_editor, refresh_code_editor, replace_all_code_matches, set_code_value,
};

// Replace all the text. `on_change` does not report this change, and undo cannot reverse it.
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

// Give the keyboard back to the editor after one of your fields used it.
focus_code_editor(&state);
```

`CodeEditorData` holds the search text, the options (`search_case_sensitive`, `search_whole_word`,
`search_regex`) and the matches. If a regular expression is not valid, `search_error` holds the
reason.

## Syntax Highlighters

The built-in highlighters are `RustHighlighter`, `JsonHighlighter`, `LuaHighlighter` and
`PlainHighlighter`. `PlainHighlighter` marks strings, numbers and comments in most languages.

```rust
use blinc::layout::syntax::{JsonHighlighter, SyntaxConfig};

let config = SyntaxConfig::new(JsonHighlighter::new());
```

### Custom Highlighter

To make your own highlighter, implement the `SyntaxHighlighter` trait. A rule is a regular expression
that the highlighter applies to one line at a time. If two matches overlap, the match that starts
first wins.

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
                // This color is the same in every theme.
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

`TokenRule::themed` lets the stylesheet choose the color. `TokenRule::new` sets a fixed color. A
custom token type gets the class `blinc-code-token-` plus its name, and your CSS gives it a color.

The trait also has `default_color`, `background_color` and `line_number_color`. They return nothing
unless you implement them. The theme then gives these colors.

## Configuration

```rust
code_editor(&state)
    .line_numbers(true)        // Show line numbers
    .gutter_width(48.0)        // Width of the line number column
    .font_size(13.0)           // Font size in pixels
    .line_height(1.5)          // Line height, times the font size
    .padding(16.0)             // Space around the text
    .rounded(8.0)              // Corner radius instead of the theme radius
    .code_bg(Color::BLACK)     // Background instead of the theme color
    .text_color(Color::WHITE)  // Plain text color instead of the theme color
    .read_only(false)          // Let the user change the text
    .tab_size(4)               // Spaces per Tab
    .placeholder("Write…")     // Text for an empty editor
    .indent_guides(true)       // Lines beside indented blocks
    .code_folding(true)        // Fold blocks
    .minimap(true)             // Small picture of the text at the right edge
    .search_enabled(true)      // The search bar
    .on_change(|text| {})      // Called after each change
```
