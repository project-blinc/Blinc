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

## Best Practices

1. **Set explicit height** - Scroll containers need a bounded height to work.

2. **Use overflow_clip on parent** - Ensure parent clips overflowing content.

3. **Prefer vertical for long content** - Horizontal scrolling is less intuitive for lists.

4. **Consider no-bounce for forms** - Disable bounce for content that needs precise positioning.

5. **Test nested scrolling** - Verify inner/outer scroll interactions work as expected.
