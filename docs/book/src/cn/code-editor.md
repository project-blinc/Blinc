# Code Editor

`cn::code_editor` is the [layout code editor](../widgets/code-editor.md) with a find and replace bar
that floats over its top right corner. It edits the same `SharedCodeEditorState`, so everything
the layout editor does, from syntax highlighting to the minimap, works as it does there.

```rust
use blinc::layout::prelude::*;
use blinc::layout::syntax::{RustHighlighter, SyntaxConfig};
use blinc::layout::widgets::code::code_editor_state;
use blinc::cn::cn;

let code = code_editor_state("fn main() {}");
let editor = cn::code_editor(&code)
    .syntax(SyntaxConfig::new(RustHighlighter::new()))
    .line_numbers(true)
    .minimap(true)
    .w_full()
    .h(360.0);
```

Create the state once, outside the builder, so edits survive a resize or a theme change.

## Options

| Method | What it does |
| --- | --- |
| `syntax(config)` | Highlight the code |
| `line_numbers`, `code_folding`, `indent_guides`, `minimap` | Show the gutter, folds, guides and minimap |
| `font_size`, `line_height`, `padding`, `gutter_width` | Sizes |
| `read_only(true)` | The text can be read, selected and copied, and nothing changes it |
| `tab_size(n)` | Spaces per Tab, 4 unless set |
| `placeholder(text)` | Shown while there is no text |
| `on_change(callback)` | Called with the text after each change, including a replacement from the bar |
| `w`, `w_full`, `h` | The editor's size; it scrolls within its height |
| `find(false)` | Leave out the find and replace bar |

## Find and replace

| Keys | What they do |
| --- | --- |
| Cmd or Ctrl+F | Open the bar, searching for a selection on one line if there is one |
| Cmd+Option+F or Ctrl+H | Open it with the replace row |
| Enter, Shift+Enter | Next and previous match, from the find field |
| Cmd or Ctrl+G, F3 | Next match, previous with Shift |
| Enter, Cmd or Ctrl+Enter | Replace one, replace all, from the replace field |
| Alt+C, Alt+W, Alt+R | Match case, whole word, regular expression |
| Esc | Close the bar and return to the code |

The bar counts the matches and marks them in the code, and the arrows step through them. In regex
mode a replacement expands `$1`, `${name}`, `$&` (the whole match) and `$$` (a dollar sign), and a
pattern that does not compile says why under the fields. A replacement calls `on_change`, and replace all is one
undo step. A match does not span lines.

In a narrow editor the bar's controls wrap under its fields.

## Styling

The bar is made of cn inputs, toggles and buttons, styled by the cn stylesheet. Restyle it with
rules after the widget layer:

```css
.cn-code-editor-find { background: var(--surface); border-radius: var(--radius-lg); }
.cn-code-editor-find-count--none { color: var(--warning); }
.cn-code-editor .blinc-code-find-match { background: color-mix(in srgb, var(--primary) 25%, transparent); }
```

The editor itself takes its colours from the theme, as the layout editor does. The
[theming section](../widgets/code-editor.md#theming) lists its classes and the `--blinc-syntax-*`
variables. The cn stylesheet adds a border to it.

The classes are `cn-code-editor`, `cn-code-editor-find` (the bar), `cn-code-editor-find-row`,
`-find-query`, `-find-tools`, `-find-replace`, `-find-replace-tools`, `-find-chevron`,
`-find-option`, `-find-count`, `-find-count--none` and `-find-error`, and `-find-previous`, `-find-next`,
`-find-close`, `-find-replace-one`, `-find-replace-all` and `-find-replace-toggle` on the buttons.

