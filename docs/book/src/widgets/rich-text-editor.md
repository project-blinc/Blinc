# Rich Text Editor

The built-in editor stores a structured document and supports selection, formatting and keyboard editing. Create its shared document once outside the UI builder so resizes and theme changes preserve edits.

```rust
use blinc::layout::prelude::*;
use blinc::layout::widgets::rich_text_editor::{
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

Restyle `.blinc-rich-text` using application CSS after the widget layer:

```css
.blinc-rich-text {
  color: var(--text-primary);
  background: var(--surface-elevated);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
}
```
