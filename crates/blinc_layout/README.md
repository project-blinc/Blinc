# blinc_layout

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

Flexbox layout engine for Blinc UI, powered by [Taffy](https://github.com/DioxusLabs/taffy).

## Overview

`blinc_layout` provides a declarative, builder-style API for constructing UI layouts. It combines Taffy's flexbox implementation with a rich set of interactive elements and rendering capabilities.

## Features

- **Flexbox Layout**: Full CSS Flexbox support via Taffy
- **Builder API**: Chainable, GPUI-style element construction
- **Rich Text**: Markdown rendering, syntax highlighting, text selection
- **Interactive Elements**: Buttons, checkboxes, text inputs, scroll containers
- **Media**: Images, SVGs with lazy loading support
- **Materials**: Glass, blur, and other visual effects
- **Animations**: Entry/exit motion animations
- **Overlays**: Modals, dialogs, tooltips, toasts

## Quick Start

```rust
use blinc_layout::prelude::*;

fn build_ui() -> impl ElementBuilder {
    div()
        .w_full()
        .h_full()
        .bg(Color::WHITE)
        .flex_col()
        .gap(16.0)
        .p(24.0)
        .child(
            text("Hello, Blinc!")
                .size(32.0)
                .weight(FontWeight::Bold)
                .color(Color::BLACK)
        )
        .child(
            div()
                .flex_row()
                .gap(8.0)
                .child(button("Click me").on_click(|| println!("Clicked!")))
                .child(button("Cancel").variant(ButtonVariant::Secondary))
        )
}
```

## Elements

### Container Elements

```rust
// Div - basic container
div().w(200.0).h(100.0).bg(Color::GRAY)

// Stack - overlay children on top of each other
stack()
    .child(img("background.jpg"))
    .child(text("Overlay text"))

// Scroll - scrollable container
scroll()
    .h(400.0)
    .child(/* long content */)
```

### Text Elements

```rust
// Basic text
text("Hello").size(16.0).color(Color::BLACK)

// Rich text with markdown
rich_text("**Bold** and *italic*")

// Code with syntax highlighting
code("let x = 42;").language("rust")
```

### Media Elements

```rust
// Image with object-fit
img("photo.jpg")
    .size(200.0, 150.0)
    .cover()
    .rounded(8.0)
    .border(2.0, Color::WHITE)

// SVG icon
svg(icons::CHECK)
    .size(24.0, 24.0)
    .color(Color::GREEN)

// Lazy-loaded image
img("large-photo.jpg")
    .lazy()
    .placeholder_color(Color::GRAY)
```

### Interactive Elements

```rust
// Button
button("Submit")
    .on_click(|| handle_submit())

// Text input
text_input()
    .placeholder("Enter name...")
    .on_change(|value| set_name(value))

// Checkbox
checkbox()
    .checked(is_enabled)
    .on_change(|checked| set_enabled(checked))

// Text area
text_area()
    .rows(5)
    .placeholder("Enter description...")
```

### Layout Properties

```rust
div()
    // Size
    .w(100.0).h(50.0)           // Fixed size
    .w_full().h_full()          // 100% of parent
    .min_w(50.0).max_w(200.0)   // Constraints

    // Flexbox
    .flex_row()                  // Row direction
    .flex_col()                  // Column direction
    .flex_wrap()                 // Allow wrapping
    .gap(8.0)                    // Gap between children
    .justify_center()            // Main axis alignment
    .items_center()              // Cross axis alignment

    // Spacing
    .p(16.0)                     // Padding all sides
    .px(8.0).py(16.0)           // Horizontal/vertical
    .m(8.0)                      // Margin all sides

    // Positioning
    .relative()                  // Position relative
    .absolute()                  // Position absolute
    .top(10.0).left(20.0)       // Offsets
```

### Styling

```rust
div()
    // Background
    .bg(Color::WHITE)
    .bg_gradient(LinearGradient::new(Color::RED, Color::BLUE))

    // Border
    .border(1.0, Color::GRAY)
    .border_radius(8.0)
    .rounded(8.0)                // Shorthand

    // Shadow
    .shadow(Shadow::new(0.0, 4.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.2)))

    // Materials
    .glass(GlassStyle::default())

    // Opacity
    .opacity(0.8)
```

## Event Handling

```rust
div()
    .on_click(|| println!("Clicked!"))
    .on_hover(|hovering| println!("Hover: {}", hovering))
    .on_mouse_down(|event| handle_mouse(event))
    .on_key_down(|key| handle_key(key))
```

## Animations

```rust
// Entry animation
div()
    .motion(Motion::fade_in().duration(300.ms()))

// Exit animation
div()
    .motion(Motion::slide_out_left())

// Spring physics
div()
    .motion(Motion::spring().stiffness(300.0).damping(20.0))
```

## Architecture

```
blinc_layout
├── div.rs          # Div element and ElementBuilder trait
├── text.rs         # Text elements
├── image.rs        # Image elements
├── svg.rs          # SVG elements
├── canvas.rs       # Custom canvas drawing
├── scroll.rs       # Scroll containers
├── button.rs       # Button widget
├── input.rs        # Text input widgets
├── checkbox.rs     # Checkbox widget
├── renderer.rs     # Layout tree rendering
├── tree.rs         # Layout tree structure
├── events.rs       # Event routing
└── motion.rs       # Animation system
```

## License

MIT OR Apache-2.0
