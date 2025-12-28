# Spring Physics

Blinc uses spring physics for natural, responsive animations. Springs provide smooth motion that feels organic compared to fixed-duration easing.

## SpringConfig

All spring animations are configured with `SpringConfig`:

```rust
use blinc_animation::SpringConfig;

// Custom spring
let config = SpringConfig {
    stiffness: 180.0,    // How "tight" the spring is
    damping: 12.0,       // How quickly oscillation settles
    mass: 1.0,           // Virtual mass of the object
    ..Default::default()
};
```

### Presets

Blinc provides common spring presets:

```rust
SpringConfig::stiff()    // Fast, minimal overshoot (stiffness: 400, damping: 30)
SpringConfig::snappy()   // Quick with slight bounce (stiffness: 300, damping: 20)
SpringConfig::gentle()   // Soft, slower motion (stiffness: 120, damping: 14)
SpringConfig::wobbly()   // Bouncy, playful (stiffness: 180, damping: 12)
```

### Choosing a Spring

| Use Case | Preset | Feel |
|----------|--------|------|
| Button press feedback | `stiff()` | Immediate, snappy |
| Menu/panel transitions | `snappy()` | Quick with character |
| Drag release | `gentle()` | Smooth, natural |
| Playful interactions | `wobbly()` | Fun, bouncy |

---

## AnimatedValue

`AnimatedValue` wraps a single f32 value with spring physics:

### Creating AnimatedValues

```rust
fn my_component(ctx: &WindowedContext) -> impl ElementBuilder {
    // Create a persisted animated value
    let scale = ctx.use_animated_value(1.0, SpringConfig::snappy());

    // With a custom key
    let x_pos = ctx.use_animated_value_for("card_x", 0.0, SpringConfig::gentle());

    // ...
}
```

### Reading Values

```rust
// Get current animated value
let current = scale.lock().unwrap().get();

// Use in transforms
div().scale(current)
```

### Setting Targets

```rust
// Animate to new target
scale.lock().unwrap().set_target(1.2);

// Immediate set (no animation)
scale.lock().unwrap().set(1.0);
```

### Example: Hover Scale with Spring Animation

For smooth spring-animated hover effects, use `motion()` with animated values:

```rust
use std::sync::Arc;
use blinc_layout::motion::motion;

fn hover_scale_card(ctx: &WindowedContext) -> impl ElementBuilder {
    let scale = ctx.use_animated_value(1.0, SpringConfig::snappy());

    let hover_scale = Arc::clone(&scale);
    let leave_scale = Arc::clone(&scale);

    // motion() is a container - apply transforms to it, style the child
    motion()
        .scale(scale.lock().unwrap().get())
        .on_hover_enter(move |_| {
            hover_scale.lock().unwrap().set_target(1.05);
        })
        .on_hover_leave(move |_| {
            leave_scale.lock().unwrap().set_target(1.0);
        })
        .child(
            div()
                .w(200.0)
                .h(120.0)
                .rounded(12.0)
                .bg(Color::rgba(0.2, 0.2, 0.3, 1.0))
                .flex_center()
                .child(text("Hover me").color(Color::WHITE))
        )
}
```

**Note:** For simple hover state changes without spring physics (e.g., just color changes), prefer `stateful(handle)` which is more efficient. Use `motion()` when you specifically need spring-animated values.

### Example: Drag Position

Use `motion()` for elements with animated position:

```rust
use blinc_layout::motion::motion;

fn draggable_element(ctx: &WindowedContext) -> impl ElementBuilder {
    let x = ctx.use_animated_value(100.0, SpringConfig::wobbly());
    let y = ctx.use_animated_value(100.0, SpringConfig::wobbly());

    let drag_x = Arc::clone(&x);
    let drag_y = Arc::clone(&y);

    // motion() handles the animated position, child has the styling
    motion()
        .absolute()
        .left(x.lock().unwrap().get())
        .top(y.lock().unwrap().get())
        .on_drag(move |evt| {
            let mut x = drag_x.lock().unwrap();
            let mut y = drag_y.lock().unwrap();
            x.set_target(x.target() + evt.drag_delta_x);
            y.set_target(y.target() + evt.drag_delta_y);
        })
        .child(
            div()
                .w(80.0)
                .h(80.0)
                .rounded(8.0)
                .bg(Color::rgba(0.4, 0.6, 1.0, 1.0))
        )
}
```

---

## Motion Containers

For declarative enter/exit animations, use `motion()`:

```rust
use blinc_layout::motion::motion;

motion()
    .fade_in(300)      // Fade in over 300ms
    .child(my_content())

motion()
    .scale_in(300)     // Scale from 0 to 1
    .child(my_content())

motion()
    .slide_in(SlideDirection::Left, 300)
    .child(my_content())
```

See [Motion Containers](./motion.md) for full details.

---

## Best Practices

1. **Match spring to interaction** - Use stiffer springs for immediate feedback, gentler for ambient motion.

2. **Persist animated values** - Use `ctx.use_animated_value()` so animations survive UI rebuilds.

3. **Clone Arc before closures** - Always `Arc::clone()` before moving into event handlers.

4. **Don't fight the spring** - Let animations complete naturally. Interrupting with new targets is fine.

5. **Use BlincComponent** - For complex components with multiple animations, use the derive macro for type-safe hooks.
