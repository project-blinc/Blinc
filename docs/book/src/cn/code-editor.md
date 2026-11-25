# Code Editor

`cn::code_editor` is the [layout code editor](../widgets/code-editor.md) with a find and replace bar.
The bar floats at the top right corner of the editor. Both editors use the same
`SharedCodeEditorState`. All features of the layout editor, for example syntax highlighting and the
minimap, also work in this one.

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

Create the state one time, outside the builder. Then your edits stay after a resize or a theme
change.

## Options

| Method | What it does |
| --- | --- |
| `syntax(config)` | Highlights the code |
| `line_numbers`, `code_folding`, `indent_guides`, `minimap` | Show line numbers, folding, indent guides and the minimap |
| `font_size`, `line_height`, `padding`, `gutter_width` | Set sizes |
| `read_only(true)` | Lets the user read, select and copy the text, but not change it |
| `tab_size(n)` | Sets the number of spaces for Tab. The default is 4 |
| `placeholder(text)` | Shows this text while the editor is empty |
| `on_change(callback)` | Gives you the text after each change, also after a replacement from the bar |
| `w`, `w_full`, `h` | Set the size. The editor scrolls inside its height |
| `find(false)` | Removes the find and replace bar |

## Find and replace

| Keys | What they do |
| --- | --- |
| Cmd or Ctrl+F | Open the bar. A selection on one line becomes the search text |
| Cmd+Option+F or Ctrl+H | Open the bar with the replace row |
| Enter, Shift+Enter | In the find field, go to the next or the previous match |
| Cmd or Ctrl+G, F3 | Go to the next match. With Shift, go to the previous match |
| Enter, Cmd or Ctrl+Enter | In the replace field, replace one match or all matches |
| Alt+C, Alt+W, Alt+R | Turn on or off: match case, whole word, regular expression |
| Esc | Close the bar and go back to the code |

The bar shows the number of matches, and the editor marks each match in the code. The arrow buttons
move between the matches.

A regular expression is a pattern that describes the text to find. With a regular expression, the
replacement text can contain these items:

- `$1` or `${name}` for the text of a group
- `$&` for the whole match
- `$$` for a dollar sign

If a pattern is not valid, the bar shows the reason below the fields.

Each replacement calls `on_change`. One undo reverses "replace all". A match is always on one line.

If the editor is narrow, the buttons move below the fields.

## Styling

The bar uses cn inputs, toggles and buttons, and the cn stylesheet gives them their style. Rules in
your CSS replace the cn rules:

```css
.cn-code-editor-find { background: var(--surface); border-radius: var(--radius-lg); }
.cn-code-editor-find-count--none { color: var(--warning); }
.cn-code-editor .blinc-code-find-match { background: color-mix(in srgb, var(--primary) 25%, transparent); }
```

The editor takes its colors from the theme, as the layout editor does. The cn stylesheet adds a
border to it. The [theming section](../widgets/code-editor.md#theming) lists the classes for the code
and the `--blinc-syntax-*` variables.

The bar and the editor have these classes. A name that starts with `-find` is short for the same name
with `cn-code-editor` in front.

| Class | Part |
| --- | --- |
| `cn-code-editor` | The editor with its bar |
| `cn-code-editor-find` | The bar |
| `-find-row`, `-find-query`, `-find-tools` | The find row, its field and its buttons |
| `-find-replace`, `-find-replace-tools` | The replace row and its buttons |
| `-find-chevron`, `-find-replace-toggle` | The arrow that shows the replace row, and its button |
| `-find-option` | A search option |
| `-find-count`, `-find-count--none` | The match count. The `--none` class marks a count of zero |
| `-find-error` | The reason that a pattern is not valid |
| `-find-previous`, `-find-next`, `-find-close` | The previous, next and close buttons |
| `-find-replace-one`, `-find-replace-all` | The buttons that replace one match or all matches |
