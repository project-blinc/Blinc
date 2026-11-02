# Layout Components

Components for layout and structure: avatar, separator, accordion, and more.

## Avatar

User profile images with fallback:

```rust
use blinc_cn::prelude::*;

avatar()
    .src("user.jpg")
    .fallback("JD")
```

### Avatar Sizes

```rust
avatar().size(AvatarSize::Sm)    // 32px
avatar().size(AvatarSize::Md)    // 40px (default)
avatar().size(AvatarSize::Lg)    // 48px
avatar().size(AvatarSize::Xl)    // 64px
```

### Avatar Fallback

```rust
// Initials fallback
avatar()
    .src("user.jpg")  // If fails to load...
    .fallback("JD")   // Show initials

// Icon fallback
avatar()
    .fallback_icon(icons::USER)
```

### Avatar Group

```rust
avatar_group()
    .max(3)  // Show max 3, then "+N"
    .child(avatar().src("user1.jpg"))
    .child(avatar().src("user2.jpg"))
    .child(avatar().src("user3.jpg"))
    .child(avatar().src("user4.jpg"))
    .child(avatar().src("user5.jpg"))
// Displays: [avatar1] [avatar2] [avatar3] [+2]
```

## Separator

Visual divider:

```rust
// Horizontal (default)
separator()

// Vertical
separator().vertical()
```

It carries `data-orientation`; its colour is `.cn-separator`, recoloured by
`--cn-separator-color`.

### With Label

```rust
div()
    .flex_row()
    .items_center()
    .gap(8.0)
    .child(separator().flex_1())
    .child(text("or").color(Color::GRAY))
    .child(separator().flex_1())
```

## Aspect Ratio

Maintain aspect ratio:

```rust
aspect_ratio(16.0 / 9.0)
    .child(img("video-thumbnail.jpg").cover())
```

### Common Ratios

```rust
// 16:9 (video)
aspect_ratio(16.0 / 9.0)

// 4:3 (classic)
aspect_ratio(4.0 / 3.0)

// 1:1 (square)
aspect_ratio(1.0)

// 3:4 (portrait)
aspect_ratio(3.0 / 4.0)
```

## Scroll Area

Custom scrollbars:

```rust
scroll_area()
    .h(400.0)
    .child(
        div()
            .flex_col()
            .gap(8.0)
            .children((0..50).map(|i| text(format!("Item {}", i))))
    )
```

### Horizontal Scroll

```rust
scroll_area()
    .orientation(Orientation::Horizontal)
    .w(300.0)
    .child(
        div()
            .flex_row()
            .gap(8.0)
            .children((0..20).map(|i|
                card().w(150.0).child(text(format!("Card {}", i)))
            ))
    )
```

## Collapsible

Expandable content:

```rust
let is_open = use_state_keyed("collapsible_open", || false);

collapsible()
    .open(is_open.clone())
    .on_open_change({
        let is_open = is_open.clone();
        move |open| is_open.set(open)
    })
    .child(collapsible_trigger()
        .child(
            div().flex_row().items_center().gap(8.0)
                .child(text("Show more"))
                .child(icon(if is_open.get() { icons::CHEVRON_UP } else { icons::CHEVRON_DOWN }))
        ))
    .child(collapsible_content()
        .child(text("Hidden content that expands...")))
```

## Accordion

Multiple collapsible sections:

```rust
accordion()
    .accordion_type(AccordionType::Single)  // Only one open at a time
    .child(accordion_item("item-1")
        .child(accordion_trigger()
            .child(text("Section 1")))
        .child(accordion_content()
            .child(text("Content for section 1"))))
    .child(accordion_item("item-2")
        .child(accordion_trigger()
            .child(text("Section 2")))
        .child(accordion_content()
            .child(text("Content for section 2"))))
    .child(accordion_item("item-3")
        .child(accordion_trigger()
            .child(text("Section 3")))
        .child(accordion_content()
            .child(text("Content for section 3"))))
```

### Multiple Open

```rust
accordion()
    .accordion_type(AccordionType::Multiple)  // Multiple can be open
    // ... accordion items
```

## Resizable

Panels with handles between them that drag to resize. Sizes are pixels; a
`flex_grow()` panel takes the rest:

```rust
cn::resizable_group()
    .horizontal()
    .panel(
        cn::resizable_panel()
            .default_size(200.0)
            .min_size(100.0)
            .max_size(400.0)
            .child(text("Left Panel")),
    )
    .panel(cn::resizable_panel().flex_grow().child(text("Right Panel")))
    .on_resize(|sizes| println!("{sizes:?}"))
```

### Vertical Resizable

```rust
cn::resizable_group()
    .vertical()
    .panel(cn::resizable_panel().default_size(150.0).child(text("Top Panel")))
    .panel(cn::resizable_panel().flex_grow().child(text("Bottom Panel")))
```

Each handle is a `.cn-resizable-handle` around a `.cn-resizable-line`; the line
turns `--primary` on hover and while its handle carries
`data-state="dragging"`.

## Examples

### User List Item

```rust
div()
    .flex_row()
    .items_center()
    .gap(12.0)
    .p(12.0)
    .child(avatar().src(&user.avatar).fallback(&user.initials))
    .child(
        div().flex_col()
            .child(text(&user.name).weight(FontWeight::Medium))
            .child(text(&user.email).size(14.0).color(Color::GRAY))
    )
```

### FAQ Accordion

```rust
accordion()
    .accordion_type(AccordionType::Single)
    .child(accordion_item("faq-1")
        .child(accordion_trigger()
            .child(text("How do I get started?")))
        .child(accordion_content()
            .child(text("To get started, first install the package..."))))
    .child(accordion_item("faq-2")
        .child(accordion_trigger()
            .child(text("What are the system requirements?")))
        .child(accordion_content()
            .child(text("You need Rust 1.70+ and..."))))
```

### Split Pane Editor

```rust
resizable()
    .direction(ResizeDirection::Horizontal)
    .h_full()
    .child(resizable_panel()
        .default_size(25.0)
        .min_size(15.0)
        .child(sidebar()))  // File tree
    .child(resizable_handle())
    .child(resizable_panel()
        .default_size(75.0)
        .child(
            resizable()
                .direction(ResizeDirection::Vertical)
                .child(resizable_panel()
                    .default_size(70.0)
                    .child(editor()))  // Code editor
                .child(resizable_handle())
                .child(resizable_panel()
                    .default_size(30.0)
                    .child(terminal()))  // Terminal
        ))
```
