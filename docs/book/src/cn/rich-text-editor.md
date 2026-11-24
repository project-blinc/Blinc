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

Transparent document colours inherit the editor's CSS text colour. Explicit span colours are document formatting and remain explicit when the theme changes.

The editor supports pointer selection, arrow navigation, typing, Enter, Backspace and platform editing shortcuts. Focus the editor before using formatting shortcuts. The shared document holds the content; derive application feedback from document changes rather than rebuilding the editor for every keystroke.

## Toolbar

The formatting toolbar is built from cn components: an icon button with a tooltip for bold, italic, underline, strikethrough and code, the cn colour picker, and a link button that opens a field for the URL. Press Enter or Apply in the field to link the selection. An empty field removes the link.

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

With a collapsed cursor, a mark button applies to the text typed next. The floating toolbar's close button collapses the selection.

The floating toolbar is a notch, so it takes its fill, border and shadow from the theme tokens. The fixed toolbar takes the `.blinc-rte-fixed-toolbar` rule. Both mark a pressed button with `.blinc-rte-button--active`.

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
