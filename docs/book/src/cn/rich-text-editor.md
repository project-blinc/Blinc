# Rich Text Editor

The built-in editor stores a structured document and supports selection, formatting and keyboard editing. Create its shared document once outside the UI builder so resizes and theme changes preserve edits.

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

The editor wraps its text to the width it is laid out at, so it grows and shrinks with its parent. The last argument is the width it wraps to until its first layout. `RichTextTheme::padding` sets the space around the text:

```rust
let editor = rich_text_editor(&document, RichTextTheme::default().padding(16.0), 380.0);
```

Transparent document colours inherit the editor's CSS text colour. Explicit span colours are document formatting and remain explicit when the theme changes.

The editor supports pointer selection, arrow navigation, typing, Enter, Backspace and platform editing shortcuts. Focus the editor before using formatting shortcuts. The shared document holds the content; derive application feedback from document changes rather than rebuilding the editor for every keystroke.

## Toolbar

The formatting toolbar is built from cn components: an icon button with a tooltip for bold, italic, underline, strikethrough and code, the cn colour picker, and a link button. The link button opens a popover with a Text field, which holds the selected text, and a Link field for the URL. Press Enter or Apply to give the selection that text and link it. An empty Link field removes the link. With no selection, Apply inserts the text at the cursor, linked.

`RichTextTheme::toolbar` chooses where the toolbar sits:

```rust
use blinc::cn::components::rich_text_editor::ToolbarPlacement;

// Over the selection, with a pointer toward it (the default).
let floating = RichTextTheme::default().toolbar(ToolbarPlacement::Floating);
// Above the text all the time, with a menu for the paragraph or heading level.
let fixed = RichTextTheme::default().toolbar(ToolbarPlacement::Fixed);
// No toolbar: formatting comes from shortcuts or your own controls.
let hidden = RichTextTheme::default().toolbar(ToolbarPlacement::Hidden);
```

With a collapsed cursor, a mark button applies to the text typed next. The floating toolbar appears when the press that made the selection is released, and its close button collapses the selection. The fixed toolbar holds a select for the paragraph or heading level. In an editor narrower than its controls, they scroll between two buttons.

The floating toolbar is a notch, so it takes its fill, border and shadow from the theme tokens. The fixed toolbar takes the `.blinc-rte-fixed-toolbar` rule. Both mark a pressed button with `.blinc-rte-button--active`.

The floating toolbar's heading select shows short names, P and H1 to H6; the fixed toolbar's shows them in full. Where controls scroll, an edge with more beyond it fades.

## Markdown

`RichDocument::from_markdown` reads Markdown and `to_markdown` writes it. A document comes back from its Markdown as it was, but for emphasis on the spaces at the ends of a run:

```rust
let markdown = document.lock().unwrap().document.to_markdown(Color::TRANSPARENT);
let same = RichDocument::from_markdown(&markdown, Color::TRANSPARENT);
```

Give both the same colour: it is the colour of text that has none of its own, and is not written. What Markdown has no syntax for is written as inline HTML, which `from_markdown` reads: `<u>` for underline, `<span style="color:#rrggbb">` for a colour, `<br>` for an empty paragraph, and tags for emphasis that Markdown's markers cannot mark beside punctuation.

## Changes

`on_change` calls back with the document after an edit made in the editor changes it: typing, formatting, paste, undo and redo. Moving the cursor, selecting and focus do not. The callback may set signals, and may lock the state:

```rust
use blinc::cn::components::rich_text_editor::RichTextStateExt;

document.on_change(move |doc| characters.set(doc.plain_text().chars().count()));
```

## Styling

Restyle `.blinc-rich-text` using application CSS after the widget layer:

```css
.blinc-rich-text {
  color: var(--text-primary);
  background: var(--surface-elevated);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
}
```
