# Virtualized List

The `virtual_list` widget renders large datasets efficiently. It creates elements only for a window of items around the visible part. Items can have variable heights, and the flexbox layout sets their size.

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
    .estimated_item_height(48.0)    // Height of a row not yet measured (default: 40px)
    .window_size(80)                // Rows to build, with those around the view (default: 50)
    .gap_px(8.0)                    // Space between rows (default: 0)
    .into_div()
```

| Option | Default | Description |
|--------|---------|-------------|
| `estimated_item_height` | 40.0 | Height that the list uses for a row that it did not measure yet |
| `window_size` | 50 | Number of rows that the list builds, with the rows around the view |
| `gap_px` | 0.0 | Space between rows, in pixels |

The list always builds all the visible rows. Their number can be more than `window_size`. To put space
between rows, use `.gap_px()`. The list does not measure the margin of a row.

## How It Works

1. The list builds rows only for a window around the visible part.
2. When you scroll, the window follows.
3. The flexbox layout sets the height of each row, and the list measures it.
4. A spacer above the window and a spacer below it stand for the other rows.
5. The scroll container provides scrolling with momentum physics.
