# Keyframe Timelines

For time-based animations with precise control, use `AnimatedTimeline`. Timelines support multiple animation entries, looping, and coordinated playback.

## Creating Timelines

```rust
fn my_component(ctx: &WindowedContext) -> impl ElementBuilder {
    // Create a persisted timeline
    let timeline = ctx.use_animated_timeline();

    // With a custom key
    let loader_timeline = ctx.use_animated_timeline_for("loader");

    // ...
}
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

```rust
timeline.lock().unwrap().configure(|t| {
    // add(offset_ms, duration_ms, start_value, end_value)
    let rotation = t.add(0, 1000, 0.0, 360.0);
    let scale = t.add(0, 500, 1.0, 1.5);       // Same start, shorter duration
    let opacity = t.add(500, 500, 1.0, 0.0);   // Starts at 500ms

    (rotation, scale, opacity)  // Return tuple of IDs
});
```

## Reading Values

```rust
let value = timeline.lock().unwrap().get(entry_id).unwrap_or(0.0);
```

## Playback Control

```rust
let mut t = timeline.lock().unwrap();

t.start();           // Start playing
t.pause();           // Pause playback
t.stop();            // Stop and reset
t.set_loop(3);       // Loop 3 times
t.set_loop(-1);      // Loop forever
t.is_playing();      // Check if playing
```

---

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

## Example: Pulsing Dots

```rust
fn pulsing_dots(ctx: &WindowedContext) -> impl ElementBuilder {
    // Create staggered timelines for each dot
    let timelines: Vec<_> = (0..3)
        .map(|i| ctx.use_animated_timeline_for(format!("dot_{}", i)))
        .collect();

    let entry_ids: Vec<_> = timelines
        .iter()
        .enumerate()
        .map(|(i, timeline)| {
            timeline.lock().unwrap().configure(|t| {
                let offset = i as i32 * 200;  // Stagger by 200ms
                let scale = t.add(offset, 600, 0.5, 1.0);
                let opacity = t.add(offset, 600, 0.3, 1.0);
                t.set_loop(-1);
                t.start();
                (scale, opacity)
            })
        })
        .collect();

    // Use in canvas or div transforms...
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

## Timeline vs Spring

| Feature | Timeline | Spring |
|---------|----------|--------|
| Duration | Fixed | Physics-based |
| Looping | Built-in | Manual |
| Multiple values | Single timeline | Individual values |
| Interruption | Restart needed | Natural blend |
| Use case | Continuous loops, sequences | Interactive, responsive |

**Use timelines for:**
- Loading spinners
- Background animations
- Sequenced animations
- Precise timing control

**Use springs for:**
- User interactions
- Drag and drop
- Hover effects
- Natural motion
