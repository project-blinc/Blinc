# Virtualized List

The `virtual_list` widget efficiently renders large datasets by only creating elements for a window of visible items. Items can have **variable heights** — flexbox layout determines their size.

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

Items don't need a fixed height. Flexbox handles sizing:

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
| `estimated_item_height` | 40.0 | Average item height estimate for scroll spacer calculation |
| `window_size` | 50 | Number of items rendered at once |

## How It Works

1. The builder creates elements for the first `window_size` items
2. Flexbox layout determines each item's actual height
3. A spacer div below the rendered items estimates the remaining scroll height
4. The scroll container provides momentum physics scrolling
5. Items use their natural flex-determined height — no fixed constraints
