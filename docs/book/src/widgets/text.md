# Text & Rich Text

Blinc provides two main elements for displaying text: `text()` for plain text and `rich_text()` for inline-formatted text with HTML-like markup.

## Plain Text

The `text()` element is the simplest way to display text:

```rust
use blinc_layout::prelude::*;

// Basic text
text("Hello, World!")

// Styled text
text("Styled text")
    .size(24.0)
    .color(Color::BLUE)
    .bold()
    .italic()

// Text with decorations
text("Underlined and struck")
    .underline()
    .strikethrough()
```

### Text Properties

| Method | Description |
|--------|-------------|
| `.size(f32)` | Font size in pixels |
| `.color(Color)` | Text color |
| `.bold()` | Bold weight |
| `.italic()` | Italic style |
| `.underline()` | Underline decoration |
| `.strikethrough()` | Strikethrough decoration |
| `.align(TextAlign)` | Horizontal alignment (Left, Center, Right) |
| `.v_align(TextVerticalAlign)` | Vertical alignment (Top, Middle, Bottom) |
| `.font_family(FontFamily)` | Custom font family |
| `.line_height(f32)` | Line height multiplier (default: 1.2) |
| `.wrap(bool)` | Enable/disable text wrapping |

## Rich Text

The `rich_text()` element supports inline formatting using HTML-like tags. This is ideal for text that needs mixed styling within a single block.

```rust
use blinc_layout::prelude::*;

// Basic formatting
rich_text("This has <b>bold</b> and <i>italic</i> text.")
    .size(16.0)
    .default_color(Color::WHITE)

// Nested tags
rich_text("<b>Bold with <i>nested italic</i></b>")

// Inline colors
rich_text(r#"Colors: <span color="#FF0000">red</span> and <span color="blue">blue</span>"#)

// Links (clickable, opens in browser)
rich_text(r#"Visit <a href="https://example.com">our website</a> for more info."#)
```

### Supported Tags

| Tag | Effect |
|-----|--------|
| `<b>`, `<strong>` | Bold text |
| `<i>`, `<em>` | Italic text |
| `<u>` | Underlined text |
| `<s>`, `<strike>`, `<del>` | Strikethrough text |
| `<a href="url">` | Clickable link (auto-underlined) |
| `<span color="...">` | Inline color |

### Color Formats

The `<span color="...">` tag supports multiple color formats:

```rust
// Hex colors
rich_text(r#"<span color="#FF0000">Red</span>"#)
rich_text(r#"<span color="#F00">Short hex</span>"#)
rich_text(r#"<span color="#FF000080">With alpha</span>"#)

// Named colors (CSS subset)
rich_text(r#"<span color="crimson">Crimson</span>"#)
rich_text(r#"<span color="steelblue">Steel Blue</span>"#)

// RGB/RGBA
rich_text(r#"<span color="rgb(255, 128, 0)">Orange</span>"#)
```

**Supported named colors:** black, white, red, green, blue, yellow, cyan, magenta, gray, silver, maroon, olive, navy, purple, teal, orange, pink, brown, lime, coral, gold, indigo, violet, crimson, salmon, tomato, skyblue, steelblue, transparent

### HTML Entity Decoding

Rich text automatically decodes common HTML entities:

```rust
rich_text("Use &lt;b&gt; for bold")  // Renders: Use <b> for bold
rich_text("&copy; 2024 &bull; All Rights Reserved &trade;")
rich_text("&ldquo;Smart quotes&rdquo; &mdash; and &hellip;")
```

**Supported entities:** `&lt;`, `&gt;`, `&amp;`, `&quot;`, `&apos;`, `&nbsp;`, `&copy;`, `&reg;`, `&trade;`, `&mdash;`, `&ndash;`, `&hellip;`, `&lsquo;`, `&rsquo;`, `&ldquo;`, `&rdquo;`, `&bull;`, `&middot;`, and numeric entities (`&#65;`, `&#x41;`)

### Range-Based API

For programmatic control, use the range-based API with byte indices:

```rust
// Style specific byte ranges
rich_text("Hello World")
    .bold_range(0..5)           // "Hello" is bold
    .color_range(6..11, Color::CYAN)  // "World" is cyan
    .size(18.0)
    .default_color(Color::WHITE)

// Multiple overlapping styles
rich_text("Important Notice: Please read carefully!")
    .bold_range(0..16)           // "Important Notice" bold
    .color_range(0..9, Color::ORANGE)  // "Important" orange
    .underline_range(18..39)     // "Please read carefully" underlined
```

Available range methods:
- `.bold_range(Range<usize>)`
- `.italic_range(Range<usize>)`
- `.underline_range(Range<usize>)`
- `.strikethrough_range(Range<usize>)`
- `.color_range(Range<usize>, Color)`
- `.link_range(Range<usize>, url: &str)`

### Interactive Links

Links in rich text are fully interactive:

- **Click to open**: Clicking a link opens the URL in the system's default browser
- **Pointer cursor**: The cursor changes to a pointer when hovering over links
- **Auto-underlined**: Links are automatically underlined for visibility

```rust
rich_text(r#"
    Check the <a href="https://docs.example.com">documentation</a>
    or view the <a href="https://github.com/example">source code</a>.
"#)
    .size(14.0)
    .default_color(Color::WHITE)
```

### Standalone Links

For simple clickable text, use the `link()` widget:

```rust
// Default behavior - opens URL in browser
link("Click here", "https://example.com")

// Custom styling
link("Styled link", "https://example.com")
    .size(18.0)
    .color(Color::CYAN)
    .no_underline()

// Underline only on hover
link("Hover to see underline", "https://example.com")
    .underline_on_hover()
```

## From StyledText

For integration with syntax highlighting or markdown rendering, create rich text from a pre-built `StyledText`:

```rust
use blinc_layout::styled_text::{StyledText, StyledLine, TextSpan};

let styled = StyledText {
    lines: vec![
        StyledLine {
            text: "Hello World".to_string(),
            spans: vec![
                TextSpan {
                    start: 0,
                    end: 5,
                    bold: true,
                    color: Color::RED,
                    ..Default::default()
                },
            ],
        },
    ],
};

rich_text_styled(styled)
    .size(16.0)
    .default_color(Color::WHITE)
```

## Example

Here's a complete example demonstrating various text features:

```rust
use blinc_app::prelude::*;
use blinc_core::Color;

fn demo_ui() -> impl ElementBuilder {
    div()
        .flex_col()
        .gap(16.0)
        .p(20.0)
        // Plain text
        .child(
            text("Plain Text Example")
                .size(24.0)
                .color(Color::WHITE)
                .bold()
        )
        // Rich text with inline formatting
        .child(
            rich_text("This is <b>bold</b>, <i>italic</i>, and <span color=\"#00FF00\">green</span>.")
                .size(16.0)
                .default_color(Color::WHITE)
        )
        // Interactive link
        .child(
            rich_text(r#"Visit <a href="https://github.com">GitHub</a> for more."#)
                .size(16.0)
                .default_color(Color::WHITE)
        )
        // Range-based styling
        .child(
            rich_text("Programmatic styling with ranges")
                .bold_range(0..13)
                .color_range(14..21, Color::CYAN)
                .underline_range(22..32)
                .size(16.0)
                .default_color(Color::WHITE)
        )
}
```

Run the rich text demo to see all features in action:

```bash
cargo run -p blinc_app --example rich_text_demo --features windowed
```
