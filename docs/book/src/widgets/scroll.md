# Scroll Containers

Blinc provides scroll containers with WebKit-style momentum scrolling and bounce physics.

## Basic Scroll

```rust
use blinc_layout::widgets::scroll::scroll;

fn scrollable_content() -> impl ElementBuilder {
    scroll()
        .h(400.0)
        .child(
            div()
                .flex_col()
                .gap(8.0)
                .child(/* ... long content ... */)
        )
}
```

## Scroll Without Bounce

```rust
use blinc_layout::widgets::scroll::scroll_no_bounce;

scroll_no_bounce()
    .h(400.0)
    .child(content)
```

## Scroll Configuration

```rust
use blinc_layout::widgets::scroll::{Scroll, ScrollConfig, ScrollDirection};
use blinc_animation::SpringConfig;

Scroll::with_config(ScrollConfig {
    bounce_enabled: true,
    bounce_spring: SpringConfig::wobbly(),
    deceleration: 1500.0,
    velocity_threshold: 10.0,
    max_overscroll: 0.3,  // 30% of viewport
    direction: ScrollDirection::Vertical,
})
.h(400.0)
.child(content)
```

### Configuration Presets

```rust
ScrollConfig::default()       // Standard bounce
ScrollConfig::no_bounce()     // No bounce physics
ScrollConfig::stiff_bounce()  // Tight, minimal bounce
ScrollConfig::gentle_bounce() // Soft, more bounce
```

## Scroll Directions

```rust
// Vertical only (default)
Scroll::with_config(ScrollConfig {
    direction: ScrollDirection::Vertical,
    ..Default::default()
})

// Horizontal only
Scroll::with_config(ScrollConfig {
    direction: ScrollDirection::Horizontal,
    ..Default::default()
})

// Both directions
Scroll::with_config(ScrollConfig {
    direction: ScrollDirection::Both,
    ..Default::default()
})
```

## Scroll States

Scroll containers use `ScrollState` for physics-driven behavior:

```rust
ScrollState::Idle         // Not scrolling
ScrollState::Scrolling    // User is dragging
ScrollState::Decelerating // Momentum after release
ScrollState::Bouncing     // Edge bounce animation
```

## Example: Scrollable List

```rust
fn message_list() -> impl ElementBuilder {
    scroll()
        .h(500.0)
        .w_full()
        .child(
            div()
                .flex_col()
                .gap(8.0)
                .p(16.0)
                .child(
                    (0..50).map(|i| {
                        div()
                            .p(12.0)
                            .rounded(8.0)
                            .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
                            .child(
                                text(&format!("Message {}", i + 1))
                                    .color(Color::WHITE)
                            )
                    })
                )
        )
}
```

## Example: Horizontal Gallery

```rust
fn image_gallery() -> impl ElementBuilder {
    Scroll::with_config(ScrollConfig {
        direction: ScrollDirection::Horizontal,
        ..Default::default()
    })
    .h(200.0)
    .w_full()
    .child(
        div()
            .flex_row()
            .gap(16.0)
            .p(16.0)
            .child(
                (0..10).map(|i| {
                    div()
                        .w(150.0)
                        .h(150.0)
                        .rounded(12.0)
                        .bg(Color::rgba(0.2, 0.3, 0.5, 1.0))
                        .flex_center()
                        .child(text(&format!("{}", i + 1)).size(24.0).color(Color::WHITE))
                })
            )
    )
}
```

## Nested Scrolling

Scroll containers handle nested scrolling automatically. Inner scrolls consume events when they can scroll; outer scrolls take over at boundaries.

```rust
fn nested_scroll_example() -> impl ElementBuilder {
    // Outer vertical scroll
    scroll()
        .h(600.0)
        .child(
            div()
                .flex_col()
                .gap(16.0)
                .child(text("Section 1").size(24.0))
                // Inner horizontal scroll
                .child(
                    Scroll::with_config(ScrollConfig {
                        direction: ScrollDirection::Horizontal,
                        ..Default::default()
                    })
                    .h(120.0)
                    .child(horizontal_items())
                )
                .child(text("Section 2").size(24.0))
                .child(more_content())
        )
}
```

## Physics Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `deceleration` | 1500.0 | How quickly momentum decays (higher = faster stop) |
| `velocity_threshold` | 10.0 | Minimum velocity to continue momentum |
| `max_overscroll` | 0.3 | Maximum overscroll as fraction of viewport |
| `bounce_spring` | wobbly | Spring config for bounce animation |

## Programmatic Scroll Control

Blinc provides a powerful selector API for programmatic scroll control through `ScrollRef`. This allows you to scroll to specific elements, positions, or the top/bottom of content.

### Creating a ScrollRef

Use `ctx.use_scroll_ref()` to create a persistent scroll reference:

```rust
use blinc_layout::selector::{ScrollRef, ScrollOptions, ScrollBehavior, ScrollBlock};

fn my_component(ctx: &WindowedContext) -> impl ElementBuilder {
    // Create a ScrollRef - persists across rebuilds
    let scroll_ref = ctx.use_scroll_ref("my_scroll");

    scroll()
        .bind(&scroll_ref)  // Bind the ref to this scroll container
        .child(content)
}
```

### Element IDs

Assign IDs to elements you want to scroll to:

```rust
fn card_list() -> impl ElementBuilder {
    div()
        .flex_col()
        .children(
            (0..10).map(|i| {
                div()
                    .id(format!("card-{}", i))  // Assign unique ID
                    .child(text(&format!("Card {}", i)))
            })
        )
}
```

### Scrolling to Elements

Use `scroll_to()` or `scroll_to_with_options()` to scroll to an element by ID:

```rust
// Simple scroll to element
scroll_ref.scroll_to("card-5");

// Scroll with options
scroll_ref.scroll_to_with_options(
    "card-5",
    ScrollOptions {
        behavior: ScrollBehavior::Smooth,  // Animate the scroll
        block: ScrollBlock::Center,        // Center element in viewport
        ..Default::default()
    },
);
```

### ScrollOptions

Configure how the scroll behaves:

```rust
ScrollOptions {
    behavior: ScrollBehavior::Smooth,  // or ScrollBehavior::Auto (instant)
    block: ScrollBlock::Center,        // Vertical alignment
    inline: ScrollInline::Nearest,     // Horizontal alignment
}
```

| Block/Inline Value | Description |
|-------------------|-------------|
| `Start` | Align to top/left of viewport |
| `Center` | Align to center of viewport |
| `End` | Align to bottom/right of viewport |
| `Nearest` | Scroll minimum distance to make visible (default) |

### Other Scroll Operations

```rust
// Scroll to top/bottom
scroll_ref.scroll_to_top();
scroll_ref.scroll_to_bottom();

// With smooth animation
scroll_ref.scroll_to_bottom_with_behavior(ScrollBehavior::Smooth);

// Scroll by relative amount
scroll_ref.scroll_by(0.0, 100.0);  // Scroll down 100px

// Set absolute offset
scroll_ref.set_scroll_offset(0.0, 500.0);
```

### Querying Scroll State

```rust
// Current offset
let (x, y) = scroll_ref.offset();
let y = scroll_ref.scroll_y();

// Content and viewport sizes
let content_size = scroll_ref.content_size();
let viewport_size = scroll_ref.viewport_size();

// Position checks
if scroll_ref.is_at_top() { /* ... */ }
if scroll_ref.is_at_bottom() { /* ... */ }

// Scroll progress (0.0 = top, 1.0 = bottom)
let progress = scroll_ref.scroll_progress();
```

### Example: Carousel with Dot Navigation

Here's a complete example of a horizontal carousel with clickable navigation dots:

```rust
use blinc_app::prelude::*;
use blinc_layout::selector::{ScrollBehavior, ScrollBlock, ScrollOptions, ScrollRef};

fn carousel(ctx: &WindowedContext) -> impl ElementBuilder {
    let scroll_ref = ctx.use_scroll_ref("carousel_scroll");
    let current_index = ctx.use_state_keyed("current_index", || 0usize);

    div()
        .flex_col()
        .items_center()
        .gap(16.0)
        // Horizontal scroll carousel
        .child(
            scroll()
                .bind(&scroll_ref)
                .direction(ScrollDirection::Horizontal)
                .w(400.0)
                .h(300.0)
                .child(
                    div()
                        .flex_row()
                        .gap(20.0)
                        .px(60.0)  // Padding to center first/last cards
                        .children(
                            (0..5).map(|i| {
                                div()
                                    .id(format!("card-{}", i))  // Element ID
                                    .w(280.0)
                                    .h(280.0)
                                    .bg(Color::rgba(0.2, 0.3, 0.5, 1.0))
                                    .rounded(16.0)
                                    .child(text(&format!("Card {}", i + 1)))
                            })
                        ),
                ),
        )
        // Navigation dots
        .child(build_dots(ctx, &scroll_ref, &current_index))
}

fn build_dots(
    ctx: &WindowedContext,
    scroll_ref: &ScrollRef,
    current_index: &State<usize>,
) -> impl ElementBuilder {
    div()
        .flex_row()
        .gap(12.0)
        .children(
            (0..5).map(|i| {
                let scroll_ref = scroll_ref.clone();
                let current_index = current_index.clone();

                div()
                    .w(12.0)
                    .h(12.0)
                    .rounded(6.0)
                    .bg(if i == current_index.get() {
                        Color::rgba(0.4, 0.6, 1.0, 1.0)
                    } else {
                        Color::rgba(0.3, 0.3, 0.4, 1.0)
                    })
                    .on_click(move |_| {
                        current_index.set(i);
                        scroll_ref.scroll_to_with_options(
                            &format!("card-{}", i),
                            ScrollOptions {
                                behavior: ScrollBehavior::Smooth,
                                block: ScrollBlock::Center,
                                ..Default::default()
                            },
                        );
                    })
            })
        )
}
```

## Best Practices

1. **Set explicit height** - Scroll containers need a bounded height to work.

2. **Use overflow_clip on parent** - Ensure parent clips overflowing content.

3. **Prefer vertical for long content** - Horizontal scrolling is less intuitive for lists.

4. **Consider no-bounce for forms** - Disable bounce for content that needs precise positioning.

5. **Test nested scrolling** - Verify inner/outer scroll interactions work as expected.

6. **Use meaningful element IDs** - Choose descriptive IDs like `"message-123"` or `"section-intro"` for elements you need to scroll to.

7. **Prefer `ctx.use_scroll_ref()`** - Always use the context method rather than `ScrollRef::new()` for proper reactive integration.
