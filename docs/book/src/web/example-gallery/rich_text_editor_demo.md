# Rich Text Editor

Full editable rich text editor with cursor, selection, and inline
formatting. Demonstrates every block kind and inline mark currently
supported by the editor model:

- Headings (H1–H3)
- Paragraphs with mixed bold / italic / underline / strikethrough /
  inline-code / colored / linked spans
- Bullet and numbered lists, including nested lists via `indent`
- Block quote
- Horizontal divider

The editor follows the theme: with `RichTextTheme::default()` its colours
are the default stylesheet's rules, `.blinc-rich-text` for the text,
`.blinc-rich-text-marker`, `-quote-bar`, `-divider`, `-code` and
`-selection` for the parts, and `.blinc-rte-toolbar` with its buttons for the
selection toolbar. The caret is drawn in `--blinc-rich-text-caret`, the text
colour when unset. Text keeps a colour of its own; build a document with
`Color::TRANSPARENT` as its default so plain text takes the editor's.
`RichTextTheme::dark()` and `light()` are fixed palettes.

<iframe
  src="../../examples/rich_text_editor_demo/index.html"
  width="100%"
  height="560"
  loading="lazy"
  style="border:1px solid #45475a;border-radius:8px;background:#181825;"
  title="Blinc rich_text_editor_demo example"
></iframe>

> **Tip:** Some demos are best viewed in a full browser window. Click "Open in a new tab" below for the full experience.

[Open in a new tab](../../examples/rich_text_editor_demo/index.html) · [View source on GitHub](https://github.com/project-blinc/Blinc/blob/main/examples/blinc_app_examples/examples/rich_text_editor_demo.rs)
