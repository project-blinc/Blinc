# Markdown Rendering

Blinc includes a built-in markdown renderer that converts CommonMark + GFM markdown to native layout elements.

## Basic Usage

```rust
use blinc_layout::markdown::markdown;

// Render markdown to a Div
let content = markdown(r#"
# Hello World

This is **bold** and *italic* text.

- List item 1
- List item 2
"#);

// Use in your layout
div()
    .flex_col()
    .child(content)
```

## Themes

The renderer supports light and dark themes:

```rust
use blinc_layout::markdown::{markdown, markdown_light, markdown_with_config, MarkdownConfig};

// Dark theme (default) - for dark backgrounds
let dark_content = markdown("# Dark Theme");

// Light theme - for white/light backgrounds
let light_content = markdown_light("# Light Theme");

// Custom configuration
let custom = markdown_with_config("# Custom", MarkdownConfig {
    h1_size: 36.0,
    body_size: 16.0,
    ..MarkdownConfig::default()
});
```

## Supported Elements

### Text Formatting

| Markdown | Result |
|----------|--------|
| `**bold**` | **bold** text |
| `*italic*` | *italic* text |
| `~~strikethrough~~` | ~~strikethrough~~ text |
| `` `inline code` `` | inline code |
| `[link](url)` | clickable link |

```rust
markdown(r#"
This is **bold**, *italic*, and ~~strikethrough~~ text.

Here's some `inline code` and a [link](https://example.com).
"#)
```

### Headings

```rust
markdown(r#"
# Heading 1
## Heading 2
### Heading 3
#### Heading 4
##### Heading 5
###### Heading 6
"#)
```

### Lists

Unordered lists:

```rust
markdown(r#"
- First item
- Second item
  - Nested item
  - Another nested
- Third item
"#)
```

Ordered lists:

```rust
markdown(r#"
1. First step
2. Second step
3. Third step
"#)
```

Task lists (GFM extension):

```rust
markdown(r#"
- [x] Completed task
- [ ] Pending task
- [x] Another done
"#)
```

### Code Blocks

Fenced code blocks with optional language:

````rust
markdown(r#"
```rust
fn main() {
    println!("Hello, Blinc!");
}
```
"#)
````

Supported languages for syntax highlighting include Rust, Python, JavaScript, TypeScript, and more.

### Blockquotes

```rust
markdown(r#"
> This is a blockquote.
> It can span multiple lines.
>
> And have multiple paragraphs.
"#)
```

### Tables (GFM)

```rust
markdown(r#"
| Header 1 | Header 2 | Header 3 |
|----------|----------|----------|
| Cell 1   | Cell 2   | Cell 3   |
| Cell 4   | Cell 5   | Cell 6   |
"#)
```

### Horizontal Rules

```rust
markdown(r#"
Content above

---

Content below
"#)
```

### Images

```rust
markdown(r#"
![Alt text](path/to/image.png)

![Remote image](https://example.com/photo.jpg)
"#)
```

## Configuration

Customize the renderer with `MarkdownConfig`:

```rust
use blinc_layout::markdown::{markdown_with_config, MarkdownConfig};
use blinc_core::Color;

let config = MarkdownConfig {
    // Typography sizes
    h1_size: 32.0,
    h2_size: 28.0,
    h3_size: 24.0,
    h4_size: 20.0,
    h5_size: 18.0,
    h6_size: 16.0,
    body_size: 16.0,
    code_size: 14.0,

    // Colors
    text_color: Color::WHITE,
    heading_color: Color::WHITE,
    link_color: Color::rgba(0.4, 0.6, 1.0, 1.0),
    code_bg: Color::rgba(0.1, 0.1, 0.12, 1.0),
    code_text: Color::rgba(0.9, 0.6, 0.3, 1.0),

    // Spacing
    paragraph_spacing: 16.0,
    heading_margin_top: 24.0,
    heading_margin_bottom: 12.0,

    // Lists
    list_indent: 24.0,
    list_item_spacing: 4.0,

    ..Default::default()
};

let content = markdown_with_config("# Custom Styled", config);
```

### Preset Themes

```rust
// Dark theme (default) - white text on dark backgrounds
let dark = MarkdownConfig::default();

// Light theme - dark text on light backgrounds
let light = MarkdownConfig::light();
```

## Live Editor Example

A full markdown editor with live preview is available in the examples:

```bash
cargo run -p blinc_app_examples --example markdown_demo --features windowed
```

This demonstrates:
- TextArea for markdown source editing
- Live preview with `markdown_light()`
- Stateful reactive updates on text change

## HTML Entities

The renderer automatically decodes HTML entities in text:

```rust
markdown(r#"
&copy; 2025 &mdash; All rights reserved

Temperature: 72&deg;F

Price: &euro;99.99
"#)
```

Common entities: `&amp;` (`&`), `&lt;` (`<`), `&gt;` (`>`), `&quot;` (`"`), `&nbsp;` (non-breaking space), `&copy;` (`©`), `&trade;` (`™`), and many more.

## Best Practices

1. **Use `markdown_light()` for light backgrounds** - The default theme assumes dark backgrounds.

2. **Wrap in scroll for long content** - Markdown can produce tall content:

   ```rust
   scroll()
       .h(600.0)
       .direction(ScrollDirection::Vertical)
       .child(markdown(long_content))
   ```

3. **Set container width** - Markdown content respects parent width:

   ```rust
   div()
       .w(800.0)
       .child(markdown(content))
   ```

4. **Code blocks need height** - For syntax highlighting to render properly, ensure the container has adequate height.

5. **Images need explicit dimensions** - While images will render, they work best when the markdown container has width constraints.
