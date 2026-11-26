# Card

Cards group related content and actions.

## Basic Usage

```rust
use blinc::cn::prelude::*;

card()
    .child(card_header()
        .child(card_title("Card Title"))
        .child(card_description("Card description text")))
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
    .child(/* card parts */)
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

### card_title()

This is the main heading of the card.

```rust
card_title("Account Settings")
```

### card_description()

This is the secondary text below the title.

```rust
card_description("Manage your account preferences")
```

### card_content()

This is the main content area.

```rust
card_content()
    .child(/* any content */)
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
card()
    .child(card_header()
        .child(card_title("Notifications"))
        .child(card_description("Configure notification settings")))
    .child(card_content()
        .child(
            div()
                .flex_col()
                .gap(12.0)
                .child(checkbox().checked(true).child(label("Email notifications")))
                .child(checkbox().child(label("Push notifications")))
        ))
```

### Card with Image

```rust
card()
    .overflow_clip()
    .child(
        img("cover.jpg")
            .w_full()
            .h(200.0)
            .cover()
    )
    .child(card_header()
        .child(card_title("Beautiful Sunset"))
        .child(card_description("Photo by @photographer")))
    .child(card_footer()
        .child(button("View").variant(ButtonVariant::Outline))
        .child(button("Download")))
```

### Card with Form

With `.w_full().max_w(350.0)`, a card is 350 px wide where there is room. In a
smaller container, it is narrower. `.w(350.0)` keeps the card at 350 px.

```rust
card()
    .w_full()
    .max_w(350.0)
    .child(card_header()
        .child(card_title("Login"))
        .child(card_description("Enter your credentials")))
    .child(card_content()
        .child(
            div()
                .flex_col()
                .gap(16.0)
                .child(
                    div()
                        .flex_col()
                        .gap(4.0)
                        .child(label("Email"))
                        .child(input().placeholder("name@example.com"))
                )
                .child(
                    div()
                        .flex_col()
                        .gap(4.0)
                        .child(label("Password"))
                        .child(input().input_type("password"))
                )
        ))
    .child(card_footer()
        .child(button("Sign in").full_width(true)))
```

### Card Grid

```rust
div()
    .grid()
    .grid_cols(3)
    .gap(16.0)
    .child(
        card()
            .child(card_header().child(card_title("Plan A")))
            .child(card_content().child(text("$9/month")))
            .child(card_footer().child(button("Select")))
    )
    .child(
        card()
            .child(card_header().child(card_title("Plan B")))
            .child(card_content().child(text("$19/month")))
            .child(card_footer().child(button("Select")))
    )
    .child(
        card()
            .child(card_header().child(card_title("Plan C")))
            .child(card_content().child(text("$29/month")))
            .child(card_footer().child(button("Select")))
    )
```

### Interactive Card

```rust
card()
    .on_click(|| navigate_to("/details"))
    .cursor("pointer")
    .child(card_header()
        .child(card_title("Click Me"))
        .child(card_description("This entire card is clickable")))
    .child(card_content()
        .child(text("Card content...")))
```

## Styling

Cards use theme tokens automatically:

- Background: `theme.colors.card`
- Border: `theme.colors.border`
- Radius: `theme.radius.lg`
- Shadow: `theme.shadows.sm`

To override them, set your own styles:

```rust
card()
    .bg(Color::rgb(0.1, 0.1, 0.1))
    .border(2.0, Color::BLUE)
    .rounded(16.0)
    .shadow(Shadow::lg())
```

## API Reference

### card()

| Prop | Type | Description |
|------|------|-------------|
| Standard div props | - | All div styling props |

### card_header()

| Prop | Type | Description |
|------|------|-------------|
| Standard div props | - | All div styling props |

### card_title()

| Prop | Type | Description |
|------|------|-------------|
| Text content | `&str` | Title text |

### card_description()

| Prop | Type | Description |
|------|------|-------------|
| Text content | `&str` | Description text |
