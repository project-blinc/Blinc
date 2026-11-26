# Card

Cards group related content and actions.

## Basic Usage

```rust
use blinc::cn::prelude::*;
use blinc::layout::prelude::{div, img, text};

card()
    .child(card_header()
        .title("Card Title")
        .description("Card description text"))
    .child(card_content()
        .child(text("Card content goes here.")))
    .child(card_footer()
        .child(button("Action")))
```

## Card Parts

### card()

`card()` is the container that wraps all card content.

```rust
card()
    .w(400.0)  // Custom width
    .child(text("Card content"))
```

### card_header()

The header contains the title and the description.

```rust
card_header()
    .title("Title")
    .description("Description")
```

The title has the class `.cn-card-title`, and the description has the class
`.cn-card-description`. The rules for these classes set their colors. The title
and the description each take a string or a signal. When a signal changes, it
replaces the text in place.

### card_content()

This is the main content area.

```rust
card_content()
    .child(text("Any content"))
```

### card_footer()

The footer holds actions and secondary information at the bottom.

```rust
card_footer()
    .child(button("Cancel").variant(ButtonVariant::Outline))
    .child(button("Save"))
```

## Examples

### Simple Card

```rust
use blinc::prelude::signal;

let email = signal(true);
let push = signal(false);

card()
    .child(card_header()
        .title("Notifications")
        .description("Configure notification settings"))
    .child(card_content()
        .child(
            div()
                .flex_col()
                .gap_px(12.0)
                .child(checkbox(email).label("Email notifications"))
                .child(checkbox(push).label("Push notifications"))
        ))
```

### Card with Image

```rust
card()
    .child(
        img("cover.jpg")
            .w_full()
            .h(200.0)
            .cover()
    )
    .child(card_header()
        .title("Beautiful Sunset")
        .description("Photo by @photographer"))
    .child(card_footer()
        .child(button("View").variant(ButtonVariant::Outline))
        .child(button("Download")))
```

### Card with Form

With `.w_full().max_w(350.0)`, a card is 350 px wide where there is room. In a
smaller container, it is narrower. `.w(350.0)` keeps the card at 350 px.

```rust
use blinc::layout::widgets::text_input::text_input_state;

let email = text_input_state();
let password = text_input_state();

card()
    .w_full()
    .max_w(350.0)
    .child(card_header()
        .title("Login")
        .description("Enter your credentials"))
    .child(card_content()
        .child(
            div()
                .flex_col()
                .gap_px(16.0)
                .child(input(&email).label("Email").placeholder("name@example.com"))
                .child(input(&password).label("Password").password())
        ))
    .child(card_footer()
        .child(button("Sign in")))
```

### Card Grid

The cards sit in a row. If the space is too narrow, they wrap onto new rows.

```rust
let plan = |name: &'static str, price: &'static str| {
    card()
        .w(220.0)
        .child(card_header().title(name))
        .child(card_content().child(text(price)))
        .child(card_footer().child(button("Select")))
};

div()
    .flex_row()
    .flex_wrap()
    .gap_px(16.0)
    .child(plan("Plan A", "$9/month"))
    .child(plan("Plan B", "$19/month"))
    .child(plan("Plan C", "$29/month"))
```

### Interactive Card

A card has no click handler of its own. To make the whole card clickable, put it in a `div` that has
one.

```rust
div()
    .cursor_pointer()
    .on_click(|_| {
        // Open the details here.
    })
    .child(card()
        .child(card_header()
            .title("Click Me")
            .description("This entire card is clickable"))
        .child(card_content()
            .child(text("Card content..."))))
```

## Styling

Cards take their colors from the theme. The `.cn-card` rule sets these:

- Background: `--cn-card-bg`, with `--surface` as the default
- Border: `--cn-card-border`, with `--border` as the default
- Radius: `--cn-card-radius`, with `--radius-xl` as the default

A card also has the small theme shadow.

To change the look, write rules for `.cn-card` in your CSS:

```css
.cn-card {
    background: var(--surface-elevated);
    border-color: var(--primary);
    border-radius: var(--radius-lg);
}
```

In Rust, `.bg(color)` sets the background, and `.shadow_md()` or `.shadow_lg()` sets a larger shadow.

```rust
card()
    .bg(Color::rgb(0.1, 0.1, 0.1))
    .shadow_lg()
```

## API Reference

### card()

| Method | Description |
|--------|-------------|
| `child` | Adds content |
| `w`, `h`, `w_full`, `max_w` | Set the size |
| `p`, `px`, `py`, `m` | Set padding and margin, in units of 4 px |
| `bg`, `shadow_md`, `shadow_lg` | Set the background and the shadow |
| `class`, `id` | Name the card for CSS |

Other `div` methods, for example `rounded` or `on_click`, are not on a card. Use CSS, or put the card
in a `div`.

### card_header()

| Method | Description |
|--------|-------------|
| `title` | The main heading. A string or a signal |
| `description` | The text below the title. A string or a signal |

### card_content() and card_footer()

| Method | Description |
|--------|-------------|
| `child` | Adds an element |
