# Keyframe Timelines

For time-based animations with precise control, use `AnimatedTimeline`. Timelines support multiple animation entries, looping, alternate (ping-pong) mode, and coordinated playback.

## Creating Timelines

### In WindowedContext

```rust
fn my_component(ctx: &WindowedContext) -> impl ElementBuilder {
    // Create a persisted timeline
    let timeline = ctx.use_animated_timeline();

    // With a custom key
    let loader_timeline = ctx.use_animated_timeline_for("loader");

    // ...
}
```

### In StateContext (Stateful Elements)

```rust
stateful::<ButtonState>().on_state(|ctx| {
    // use_timeline returns (entry_ids, TimelineHandle)
    let ((entry1, entry2), timeline) = ctx.use_timeline("fade", |t| {
        let e1 = t.add(0, 500, 0.0, 1.0);
        let e2 = t.add(250, 500, 0.0, 100.0);
        t.set_loop(-1);
        t.start();
        (e1, e2)
    });

    let value1 = timeline.get(entry1).unwrap_or(0.0);
    let value2 = timeline.get(entry2).unwrap_or(0.0);

    div()
})
```

## Configuring Timelines

Use the `configure()` method to set up animations once:

```rust
let timeline = ctx.use_animated_timeline();

// Configure returns entry IDs for accessing values later
let entry_id = timeline.lock().unwrap().configure(|t| {
    let id = t.add(0, 1000, 0.0, 360.0);  // 0ms start, 1000ms duration, 0° to 360°
    t.set_loop(-1);  // Loop forever (-1 = infinite)
    t.start();
    id
});
```

The closure only runs on first call. Subsequent calls return existing entry IDs.

## Adding Animations

### Basic Entry

```rust
timeline.lock().unwrap().configure(|t| {
    // add(offset_ms, duration_ms, start_value, end_value)
    let rotation = t.add(0, 1000, 0.0, 360.0);
    let scale = t.add(0, 500, 1.0, 1.5);       // Same start, shorter duration
    let opacity = t.add(500, 500, 1.0, 0.0);   // Starts at 500ms

    (rotation, scale, opacity)  // Return tuple of IDs
});
```

### With Easing

```rust
use blinc_animation::Easing;

timeline.lock().unwrap().configure(|t| {
    // add_with_easing(offset_ms, duration_ms, start, end, easing)
    let smooth = t.add_with_easing(0, 500, 0.0, 60.0, Easing::EaseInOut);
    let bouncy = t.add_with_easing(0, 500, 0.0, 1.0, Easing::EaseOutQuad);

    (smooth, bouncy)
});
```

### Using StaggerBuilder

For multiple entries with automatic offset calculation:

```rust
timeline.lock().unwrap().configure(|t| {
    // stagger(base_offset, stagger_amount)
    let mut stagger = t.stagger(0, 100);  // 0ms base, 100ms between each

    let bar1 = stagger.add(500, 0.0, 60.0);  // offset: 0ms
    let bar2 = stagger.add(500, 0.0, 60.0);  // offset: 100ms
    let bar3 = stagger.add(500, 0.0, 60.0);  // offset: 200ms

    // With easing
    let bar4 = stagger.add_with_easing(500, 0.0, 60.0, Easing::EaseInOut);

    (bar1, bar2, bar3, bar4)
});
```

## Reading Values

```rust
let value = timeline.lock().unwrap().get(entry_id).unwrap_or(0.0);

// Get entry progress (0.0 to 1.0)
let progress = timeline.lock().unwrap().entry_progress(entry_id);

// Get overall timeline progress
let total_progress = timeline.lock().unwrap().progress();
```

## Playback Control

```rust
let mut t = timeline.lock().unwrap();

t.start();              // Start playing
t.pause();              // Pause (can resume)
t.resume();             // Resume from pause
t.stop();               // Stop and reset
t.restart();            // Start from beginning
t.reverse();            // Toggle playback direction
t.seek(500.0);          // Jump to 500ms position

t.set_loop(3);          // Loop 3 times
t.set_loop(-1);         // Loop forever
t.set_alternate(true);  // Ping-pong mode
t.set_playback_rate(2.0); // 2x speed

t.is_playing();         // Check if playing
t.progress();           // Overall progress (0.0 to 1.0)
```

---

## Alternate (Ping-Pong) Mode

Enable `alternate` mode for back-and-forth animations that maintain stagger across loops:

```rust
let ((bar1, bar2, bar3), timeline) = ctx.use_timeline("bars", |t| {
    // Three staggered entries
    let b1 = t.add_with_easing(0, 500, 0.0, 60.0, Easing::EaseInOut);
    let b2 = t.add_with_easing(100, 500, 0.0, 60.0, Easing::EaseInOut);
    let b3 = t.add_with_easing(200, 500, 0.0, 60.0, Easing::EaseInOut);

    t.set_alternate(true);  // Reverse on each loop
    t.set_loop(-1);         // Loop forever
    t.start();

    (b1, b2, b3)
});
```

With alternate mode:

- Timeline plays forward (0 → duration)
- On completion, reverses direction (duration → 0)
- Stagger offsets maintain their relative timing
- No jump back to start - smooth continuous motion

---

## Example: Staggered Wave Animation

```rust
fn sliding_bars() -> impl ElementBuilder {
    stateful::<NoState>().on_state(|ctx| {
        let ((bar1_id, bar2_id, bar3_id), timeline) = ctx.use_timeline("bars", |t| {
            // Staggered entries with easing
            let bar1 = t.add_with_easing(0, 500, 0.0, 60.0, Easing::EaseInOut);
            let bar2 = t.add_with_easing(100, 500, 0.0, 60.0, Easing::EaseInOut);
            let bar3 = t.add_with_easing(200, 500, 0.0, 60.0, Easing::EaseInOut);

            t.set_alternate(true);
            t.set_loop(-1);
            t.start();

            (bar1, bar2, bar3)
        });

        let bar1_x = timeline.get(bar1_id).unwrap_or(0.0);
        let bar2_x = timeline.get(bar2_id).unwrap_or(0.0);
        let bar3_x = timeline.get(bar3_id).unwrap_or(0.0);

        div()
            .flex_col()
            .gap(12.0)
            .child(div().w(30.0).h(12.0).bg(Color::GREEN)
                .transform(Transform::translate(bar1_x, 0.0)))
            .child(div().w(30.0).h(12.0).bg(Color::YELLOW)
                .transform(Transform::translate(bar2_x, 0.0)))
            .child(div().w(30.0).h(12.0).bg(Color::RED)
                .transform(Transform::translate(bar3_x, 0.0)))
    })
}
```

## Example: Spinning Loader

```rust
use std::f32::consts::PI;

fn spinning_loader(ctx: &WindowedContext) -> impl ElementBuilder {
    let timeline = ctx.use_animated_timeline();

    let entry_id = timeline.lock().unwrap().configure(|t| {
        let id = t.add(0, 1000, 0.0, 360.0);
        t.set_loop(-1);
        t.start();
        id
    });

    let render_timeline = Arc::clone(&timeline);

    canvas(move |draw_ctx, bounds| {
        let angle_deg = render_timeline.lock().unwrap().get(entry_id).unwrap_or(0.0);
        let angle_rad = angle_deg * PI / 180.0;

        let cx = bounds.width / 2.0;
        let cy = bounds.height / 2.0;
        let radius = 30.0;

        // Draw spinning arc
        // ... drawing code
    })
    .w(80.0)
    .h(80.0)
}
```

## Example: Pulsing Ring

```rust
fn pulsing_ring() -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(|ctx| {
        let is_running = ctx.use_signal("running", || false);

        // Keyframe animations with ping-pong
        let scale = ctx.use_keyframes("scale", |k| {
            k.at(0, 0.8)
             .at(800, 1.2)
             .ease(Easing::EaseInOut)
             .ping_pong()
             .loop_infinite()
        });

        let opacity = ctx.use_keyframes("opacity", |k| {
            k.at(0, 0.4)
             .at(800, 1.0)
             .ease(Easing::EaseInOut)
             .ping_pong()
             .loop_infinite()
        });

        // Toggle on click
        if let Some(event) = ctx.event() {
            if event.event_type == POINTER_UP {
                if is_running.get() {
                    scale.stop();
                    opacity.stop();
                    is_running.set(false);
                } else {
                    scale.start();
                    opacity.start();
                    is_running.set(true);
                }
            }
        }

        let s = scale.get();
        let o = opacity.get();

        div()
            .w(60.0).h(60.0)
            .border(4.0, Color::rgba(1.0, 0.5, 0.3, o))
            .rounded(30.0)
            .transform(Transform::scale(s, s))
    })
}
```

## Example: Progress Bar

```rust
fn animated_progress(ctx: &WindowedContext) -> impl ElementBuilder {
    let timeline = ctx.use_animated_timeline();

    let entry_id = timeline.lock().unwrap().configure(|t| {
        let id = t.add(0, 2000, 0.0, 1.0);  // 2 second fill
        t.start();
        id
    });

    let click_timeline = Arc::clone(&timeline);
    let render_timeline = Arc::clone(&timeline);

    div()
        .w(200.0)
        .h(20.0)
        .rounded(10.0)
        .bg(Color::rgba(0.2, 0.2, 0.25, 1.0))
        .on_click(move |_| {
            // Restart on click
            let mut t = click_timeline.lock().unwrap();
            t.stop();
            t.start();
        })
        .child(
            canvas(move |draw_ctx, bounds| {
                let progress = render_timeline.lock().unwrap()
                    .get(entry_id)
                    .unwrap_or(0.0);

                let fill_width = bounds.width * progress;
                draw_ctx.fill_rect(
                    Rect::new(0.0, 0.0, fill_width, bounds.height),
                    CornerRadius::uniform(10.0),
                    Brush::Solid(Color::rgba(0.4, 0.6, 1.0, 1.0)),
                );
            })
            .w_full()
            .h_full()
        )
}
```

---

## ConfigureResult Types

The `configure()` method supports various return types:

```rust
// Single entry
let id: TimelineEntryId = t.configure(|t| t.add(...));

// Tuple of entries
let (a, b): (TimelineEntryId, TimelineEntryId) = t.configure(|t| {
    (t.add(...), t.add(...))
});

// Triple
let (a, b, c) = t.configure(|t| {
    (t.add(...), t.add(...), t.add(...))
});

// Vec for dynamic counts
let ids: Vec<TimelineEntryId> = t.configure(|t| {
    (0..5).map(|i| t.add(i * 100, 500, 0.0, 1.0)).collect()
});
```

---

## Available Easing Functions

```rust
use blinc_animation::Easing;

Easing::Linear          // No easing
Easing::EaseIn          // Slow start (cubic)
Easing::EaseOut         // Slow end (cubic)
Easing::EaseInOut       // Slow start and end (cubic)
Easing::EaseInQuad      // Quadratic ease in
Easing::EaseOutQuad     // Quadratic ease out
Easing::EaseInOutQuad   // Quadratic ease in-out
Easing::EaseInCubic     // Cubic ease in
Easing::EaseOutCubic    // Cubic ease out
Easing::EaseInOutCubic  // Cubic ease in-out
Easing::EaseInQuart     // Quartic ease in
Easing::EaseOutQuart    // Quartic ease out
Easing::EaseInOutQuart  // Quartic ease in-out
Easing::CubicBezier(x1, y1, x2, y2)  // Custom bezier curve
```

---

## Timeline vs Spring

| Feature | Timeline | Spring |
|---------|----------|--------|
| Duration | Fixed | Physics-based |
| Looping | Built-in | Manual |
| Multiple values | Single timeline | Individual values |
| Ping-pong | set_alternate(true) | Manual reverse |
| Interruption | Restart needed | Natural blend |
| Use case | Continuous loops, sequences | Interactive, responsive |

**Use timelines for:**
- Loading spinners
- Background animations
- Sequenced animations
- Staggered wave effects
- Precise timing control

**Use springs for:**
- User interactions
- Drag and drop
- Hover effects
- Natural motion
