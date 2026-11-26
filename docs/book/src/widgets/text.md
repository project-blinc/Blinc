# Text & Rich Text

Blinc has two main elements that show text. Use `text()` for plain text.
Use `rich_text()` for text with inline formatting in HTML-like markup.

For headings, paragraphs, captions, and inline composition, see the
[Typography helpers](../cn/typography.md) guide.

## Plain Text

The simplest way to show text is the `text()` element:

```rust
use blinc::layout::prelude::*;

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
| `.wrap(bool)` | Turns text wrapping on or off |

## Rich Text

The `rich_text()` element supports inline formatting with HTML-like tags. If a single block of text needs mixed styles, use this element.

```rust
use blinc::layout::prelude::*;

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

The `<span color="...">` tag accepts several color formats:

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

Supported named colors:

- black, white, red, green, blue, yellow, cyan, magenta, gray, silver
- maroon, olive, navy, purple, teal, orange, pink, brown, lime, coral
- gold, indigo, violet, crimson, salmon, tomato, skyblue, steelblue, transparent

### HTML Entity Decoding

Rich text decodes common HTML entities automatically:

```rust
rich_text("Use &lt;b&gt; for bold")  // Renders: Use <b> for bold
rich_text("&copy; 2024 &bull; All Rights Reserved &trade;")
rich_text("&ldquo;Smart quotes&rdquo; &mdash; and &hellip;")
```

Supported entities:

- `&lt;`, `&gt;`, `&amp;`, `&quot;`, `&apos;`, `&nbsp;`
- `&copy;`, `&reg;`, `&trade;`
- `&mdash;`, `&ndash;`, `&hellip;`
- `&lsquo;`, `&rsquo;`, `&ldquo;`, `&rdquo;`, `&bull;`, `&middot;`
- Numeric entities (`&#65;`, `&#x41;`)

### Range-Based API

To control styles from code, use the range-based API with byte indices:

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

These range methods are available:
- `.bold_range(Range<usize>)`
- `.italic_range(Range<usize>)`
- `.underline_range(Range<usize>)`
- `.strikethrough_range(Range<usize>)`
- `.color_range(Range<usize>, Color)`
- `.link_range(Range<usize>, url: &str)`

### Interactive Links

Links in rich text are fully interactive:

- A click on a link opens the URL in the default browser of the system.
- Over a link, the cursor changes to a pointer.
- Rich text underlines links automatically to make them visible.

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
    .font_size(18.0)
    .text_color(Color::CYAN)
    .no_underline()

// Underline only on hover
link("Hover to see underline", "https://example.com")
    .underline_on_hover()
```

### Default colours

Links, blockquotes, rules, lists, and tables take their colors from rules
in the default stylesheet. Thus, a stylesheet can restyle them:

| Class | Element | Default |
|-------|---------|---------|
| `blinc-link` | a `link()` | `--text-link`, `--primary-hover` on hover |
| `blinc-blockquote` | a `blockquote()` | `--surface-overlay` fill, `--border` left edge |
| `blinc-hr` | an `hr()` | `--border` |
| `blinc-list-marker` | a bullet or number | `--text-tertiary` |
| `blinc-task-box` | the box of a task item, `checked` for a done item | `--text-tertiary` |
| `blinc-thead`, `blinc-tfoot` | the head and foot of a table | `--surface-overlay` |
| `blinc-th`, `blinc-td` | text from `th_text()`, `td_text()` | `--text-primary`, `--text-secondary` |
| `blinc-tr-alt` | an odd row from `striped_tr()` | `--accent-subtle` |

```css
.blinc-link { color: #0ea5e9; }
.blinc-tr-alt { background: transparent; }
```

If you give a color to the widget (`text_color`, `hr_color`, `header_bg`, a
configuration color), the widget sets it on the element. That color then has
priority over the rule.

## From StyledText

To integrate with syntax highlighting or markdown rendering, create rich text from a pre-built `StyledText`:

```rust
use blinc::layout::styled_text::{StyledText, StyledLine, TextSpan};

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

This complete example shows various text features:

```rust
use blinc::app::prelude::*;
use blinc::core::Color;

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

To see all features at work, run the rich text demo:

```bash
cargo run -p blinc_app_examples --example rich_text_demo --features windowed
```
