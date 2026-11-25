# Rich Text Editor

The rich text editor stores a document with structure. Users can select, format and edit the text
with the pointer and the keyboard. Create the shared document one time, outside the UI builder. Then
the edits stay after a resize or a theme change.

```rust
use blinc::layout::prelude::*;
use blinc::cn::components::rich_text_editor::{
    RichDocument, RichTextTheme, rich_text_editor, rich_text_state,
};
use blinc::core::Color;

let document = rich_text_state(RichDocument::from_markdown(
    "## Your ideas\n\nWrite **something meaningful**.", Color::TRANSPARENT,
));
let editor = rich_text_editor(&document, RichTextTheme::default(), 380.0);
```

The editor wraps its text to its width, so the text follows the width of the parent. The last
argument is the width that the editor uses before the first layout. `RichTextTheme::padding` sets the
space around the text:

```rust
let editor = rich_text_editor(&document, RichTextTheme::default().padding(16.0), 380.0);
```

Text with a transparent color takes the CSS text color of the editor. If a part of the text has its
own color, that color stays when the theme changes.

You can select text with the pointer, move with the arrow keys and type. Enter, Backspace and the
usual editing shortcuts of your system also work. To use the format shortcuts, click the editor
first. The shared document holds the content. If your app must react to edits, read the changes of
the document. Do not build the editor again for each key.

## Toolbar

The toolbar uses cn components. It has a button with a tooltip for bold, italic, underline,
strikethrough and code. It also has the cn color picker and a link button.

The link button opens a small window with two fields. The Text field holds the selected text. The Link
field holds the address.

1. Type the address in the Link field.
2. Press Enter, or click Apply. The editor replaces the selected text with the text of the Text
   field and links it.

If you select no text, Apply inserts the text at the cursor, with the link. To remove a link, empty
the Link field and apply.

`RichTextTheme::toolbar` sets the place of the toolbar:

```rust
use blinc::cn::components::rich_text_editor::ToolbarPlacement;

// Over the selected text, with an arrow that points to it. This is the default.
let floating = RichTextTheme::default().toolbar(ToolbarPlacement::Floating);
// Always above the text, with a menu for the paragraph or heading level.
let fixed = RichTextTheme::default().toolbar(ToolbarPlacement::Fixed);
// No toolbar. Use shortcuts or your own controls to format.
let hidden = RichTextTheme::default().toolbar(ToolbarPlacement::Hidden);
```

If you select no text, a format button applies to the text that you type next. The floating toolbar
shows after you release the pointer from a selection. Its close button removes the selection. The
fixed toolbar has a menu for the paragraph or heading level.

If the editor is narrower than the toolbar, the controls scroll between two buttons. If there are more controls past an edge, that edge fades. The heading menu of the floating toolbar shows short names: P and H1
to H6. The fixed toolbar shows the full names.

You can change the style of both toolbars with CSS:

- The floating toolbar uses `.cn-rich-text-editor-floating-toolbar`. Its `background`, `border`,
  `border-radius` and `box-shadow` also apply to its point.
- The fixed toolbar uses `.cn-rich-text-editor-fixed-toolbar`.
- A pressed button in both toolbars has `.cn-rich-text-editor-button--active`.

## Lists

A block can be an item of a bulleted list, a numbered list or a checklist. A block is one part of the
document, for example a paragraph. To change the list type, use the list menu of the toolbar or these
shortcuts. On systems other than macOS, use Ctrl instead of Cmd.

| Keys | List |
| --- | --- |
| Cmd+Shift+8 | Bulleted list |
| Cmd+Shift+7 | Numbered list |
| Cmd+Shift+6 | Checklist |

- Tab moves an item one level in, and Shift+Tab moves it one level out.
- Enter starts a new item.
- Enter on an empty item moves it one level out. At the first level, it leaves the list.
- Backspace at the start of an item also moves it one level out.
- To tick a checklist item, click its box or press Cmd+Enter.

## Quotes

A quote is not a separate type of block. A paragraph or a heading can also be a quote. Then the menu
shows "Heading 2", for example, and the quote button looks pressed.

The quote button makes the selected blocks into quotes. If all of them are quotes, the button removes
the quote. If a list item becomes a quote, it changes to a quoted paragraph. Cmd+Shift+9 does the
same. In a table, the table menu takes the place of the quote button.

In your code, `Block::quote` marks a quote. `Block::quotable` returns true for a block that can be a quote. `block_ops::toggle_quote`
changes a range of blocks. Backspace at the start of a quote removes the
quote first. In Markdown, a quote is `>` before a paragraph or a heading.

## Tables

The table button of the toolbar opens a grid. Choose the size of the new table in the grid. The first
row is the header, and the columns share the width of the table. A cell holds one line of text. The
line wraps, and it can have all the text formats.

| Keys | What they do |
| --- | --- |
| Tab | Go to the next cell. In the last cell, add a row |
| Shift+Tab | Go to the cell before |
| Enter | Go to the cell below. In the last row, go to the text after the table |
| Arrow keys | Move through the lines of a cell and between rows |

If the cursor is in a table, a table menu takes the place of the list menu. It adds and removes rows
and columns, and removes the table. The alignment buttons align the column of the cursor.

In your code, `Block::table(rows, columns)` makes a table. The `table` module changes a table in a
document:

```rust
use blinc::cn::components::rich_text_editor::table;

table::insert_table(&mut doc, cursor, 3, 2);
```

A table is one block, `BlockKind::Table { columns }`. Its `lines` are the cells, row by row.
`columns` holds the alignment of each column.

## Alignment

Each block has an `align` value: `BlockAlign::Start`, `Center`, `End` or `Justify`. To align the
selected blocks, or the block of the cursor, use the alignment buttons of the toolbar. You can also
use Cmd+Shift+L (start), E (center), R (end) or J (justify). A new paragraph takes the alignment of
the paragraph that you split.

In a table, these controls align the column. A column cannot use `Justify`. In your code,
`block_ops::set_block_align` sets the alignment in a document.

## Markdown

`RichDocument::from_markdown` reads Markdown, and `to_markdown` writes it. If you write a document and
read it again, you get the same document. There is one exception: a format on a space at the start or
end of a formatted part is lost.

```rust
let markdown = document.lock().unwrap().document.to_markdown(Color::TRANSPARENT);
let same = RichDocument::from_markdown(&markdown, Color::TRANSPARENT);
```

Give the same color to both functions. This is the color of the text that has no color of its own,
and `to_markdown` does not write it.

If Markdown has no syntax for a format, `to_markdown` writes inline HTML, and `from_markdown` reads
it:

- `<u>` for underline
- `<span style="color:#rrggbb">` for a color
- `<br>` for an empty paragraph
- `<div align="center">` around aligned blocks
- HTML tags for emphasis next to punctuation, where Markdown marks cannot work

The editor writes tables as GitHub Flavored Markdown (GFM) tables.

## Changes

`on_change` calls your function with the document after an edit in the editor changes it. Typing,
format changes, paste, undo and redo call it. Cursor moves, selections and focus changes do not. Your
function can set signals and lock the state:

```rust
use blinc::cn::components::rich_text_editor::RichTextStateExt;

document.on_change(move |doc| characters.set(doc.plain_text().chars().count()));
```

## Styling

To change the style of the text area, write CSS rules for `.cn-rich-text-editor-document` in your
app:

```css
.cn-rich-text-editor-document {
  color: var(--text-primary);
  background: var(--surface-elevated);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
}
```
