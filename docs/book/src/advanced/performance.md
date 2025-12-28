# Performance Tips

Blinc is designed for high performance, but following these guidelines ensures your UI stays smooth.

## Use Stateful for Visual States

**Do:** Use `stateful(handle)` for hover, press, and focus effects:

```rust
use blinc_layout::stateful::stateful;

fn hover_button(ctx: &WindowedContext) -> impl ElementBuilder {
    let handle = ctx.use_state(ButtonState::Idle);

    stateful(handle)
        .px(16.0)
        .py(8.0)
        .rounded(8.0)
        .on_state(|state, div| {
            let bg = match state {
                ButtonState::Idle => Color::RED,
                ButtonState::Hovered => Color::BLUE,
                _ => Color::RED,
            };
            div.set_bg(bg);
        })
        .child(text("Hover me").color(Color::WHITE))
}
```

**Don't:** Use if-else or signals for visual-only state changes:

```rust
// AVOID - causes full tree rebuild on every hover
let is_hovered = ctx.use_signal(false);
div()
    .on_hover_enter(move |_| ctx.set(is_hovered, true))
    .on_hover_leave(move |_| ctx.set(is_hovered, false))
    .bg(if ctx.get(is_hovered).unwrap_or(false) {
        Color::BLUE
    } else {
        Color::RED
    })
```

The `stateful(handle)` pattern only updates the affected element, while signals with if-else rebuild the entire UI tree.

## Minimize Signal Updates

Signals trigger UI rebuilds. Batch related updates:

```rust
// Good - single rebuild
ctx.batch(|g| {
    g.set(x, 10);
    g.set(y, 20);
    g.set(z, 30);
});

// Avoid - three rebuilds
ctx.set(x, 10);
ctx.set(y, 20);
ctx.set(z, 30);
```

## Use Keyed State Appropriately

Keyed state persists across rebuilds. Use it for:
- Form input values
- Toggle states
- Selected items

Don't overuse - each key adds memory overhead.

## Efficient List Rendering

For large lists, consider:

1. **Virtualization** - Only render visible items
2. **Stable keys** - Use consistent identifiers for list items
3. **Memoization** - Cache expensive computations

```rust
// For very long lists, wrap in scroll and limit rendered items
scroll()
    .h(500.0)
    .child(
        div()
            .flex_col()
            .child(
                visible_items.iter().map(|item| render_item(item))
            )
    )
```

## Canvas Optimization

For custom drawing:

1. **Minimize state reads** - Read animated values once, not per-shape
2. **Use transforms** - Push/pop transforms instead of recalculating positions
3. **Batch similar draws** - Group shapes by color/brush

```rust
canvas(move |ctx, bounds| {
    // Read once
    let angle = timeline.lock().unwrap().get(entry_id).unwrap_or(0.0);

    // Use transform for rotation
    ctx.push_transform(Transform::rotate(angle));
    // ... draw ...
    ctx.pop_transform();
})
```

## Animation Performance

1. **Use appropriate spring stiffness** - Stiffer springs settle faster
2. **Limit simultaneous animations** - Too many can cause jank
3. **Use timelines for loops** - More efficient than many spring values

```rust
// Good - single timeline with multiple entries
let timeline = ctx.use_animated_timeline();
let (x, y, scale) = timeline.lock().unwrap().configure(|t| {
    (t.add(0, 1000, 0.0, 100.0),
     t.add(0, 1000, 0.0, 100.0),
     t.add(0, 500, 1.0, 1.5))
});
```

## Memory Management

1. **Clone Arc, not data** - Use `Arc::clone()` for shared state
2. **Drop unused state** - Clean up keyed state when no longer needed
3. **Avoid closures capturing large data** - Clone only what's needed

```rust
// Good - clone the Arc, not the data
let data = Arc::clone(&shared_data);

// Avoid - captures entire struct
let large_struct = expensive_struct.clone();
div().on_click(move |_| use_struct(&large_struct))
```

## Debugging Performance

Enable tracing to identify bottlenecks:

```rust
tracing_subscriber::fmt()
    .with_env_filter("blinc_layout=debug")
    .init();
```

Look for:
- Frequent tree rebuilds
- Long frame times
- Excessive state updates

## Summary

| Do | Don't |
|----|-------|
| Use `Stateful` for hover/press | Use signals for visual-only changes |
| Batch signal updates | Update signals one at a time |
| Use `Arc::clone()` | Clone large data into closures |
| Use timelines for loops | Create many spring values |
| Read animated values once | Read repeatedly in draw loops |
