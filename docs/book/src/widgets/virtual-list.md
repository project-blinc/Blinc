# Virtualized List

The `virtual_list` widget renders large datasets efficiently. It creates elements only for a window of visible items. Items can have variable heights, and the flexbox layout sets their size.

## Basic Usage

```rust
use blinc::layout::widgets::virtual_list::virtual_list;

let items: Vec<String> = (0..10_000)
    .map(|i| format!("Item {}", i))
    .collect();

virtual_list(items.len(), move |index| {
    div()
        .w_full()
        .p_px(8.0)
        .flex_row()
        .items_center()
        .child(text(&items[index]).size(14.0).color(Color::WHITE))
})
.w_full()
.h(400.0)
.into_div()
```

## Variable Height Items

Items do not need a fixed height. Flexbox sets their size:

```rust
virtual_list(messages.len(), move |i| {
    let msg = &messages[i];
    div()
        .w_full()
        .p_px(12.0)
        .flex_col()
        .gap_px(4.0)
        .child(text(&msg.author).size(12.0).bold().color(Color::WHITE))
        .child(text(&msg.body).size(14.0).color(Color::rgba(0.8, 0.8, 0.8, 1.0)))
        // Height is determined by content — short messages are small, long ones wrap
})
.w_full()
.h(600.0)
.into_div()
```

## Configuration

```rust
virtual_list(count, builder)
    .w_full()                       // Width
    .h(400.0)                       // Viewport height
    .bg(Color::BLACK)               // Background
    .rounded(8.0)                   // Corner radius
    .gap_px(4.0)                    // Gap between items
    .estimated_item_height(48.0)    // Hint for scroll spacer (default: 40px)
    .window_size(80)                // Items to render at once (default: 50)
    .into_div()
```

| Option | Default | Description |
|--------|---------|-------------|
| `estimated_item_height` | 40.0 | Estimate of the average item height, for the calculation of the scroll spacer |
| `window_size` | 50 | Number of items that the list renders at once |

## How It Works

1. The builder creates elements for the first `window_size` items.
2. The flexbox layout sets the actual height of each item.
3. A spacer div below the rendered items estimates the remaining scroll height.
4. The scroll container provides scrolling with momentum physics.
5. Items use the natural height that flexbox gives them, with no fixed constraints.
