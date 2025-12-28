# Canvas Drawing

The `canvas()` element provides direct GPU drawing access for custom graphics, charts, and procedural content.

## Basic Usage

```rust
use blinc_core::{DrawContext, Rect, Brush, Color, CornerRadius};

canvas(|ctx: &mut dyn DrawContext, bounds| {
    // bounds contains the canvas size
    ctx.fill_rect(
        Rect::new(0.0, 0.0, bounds.width, bounds.height),
        CornerRadius::uniform(8.0),
        Brush::Solid(Color::RED),
    );
})
.w(200.0)
.h(100.0)
```

## Drawing Primitives

### Filled Rectangles

```rust
ctx.fill_rect(
    Rect::new(x, y, width, height),
    CornerRadius::uniform(8.0),  // Corner radius
    Brush::Solid(Color::BLUE),
);

// No corner radius
ctx.fill_rect(
    Rect::new(10.0, 10.0, 100.0, 50.0),
    CornerRadius::default(),
    Brush::Solid(Color::GREEN),
);
```

### Stroked Rectangles

```rust
ctx.stroke_rect(
    Rect::new(x, y, width, height),
    CornerRadius::uniform(4.0),
    2.0,  // Stroke width
    Brush::Solid(Color::WHITE),
);
```

### Circles

```rust
// Filled circle
ctx.fill_circle(
    Point::new(cx, cy),  // Center
    radius,
    Brush::Solid(Color::BLUE),
);

// Stroked circle
ctx.stroke_circle(
    Point::new(cx, cy),
    radius,
    2.0,  // Stroke width
    Brush::Solid(Color::WHITE),
);
```

### Text

```rust
use blinc_core::TextStyle;

ctx.draw_text(
    "Hello, Canvas!",
    Point::new(x, y),
    &TextStyle::new(16.0).with_color(Color::WHITE),
);
```

## Gradients

```rust
use blinc_core::{Gradient, GradientStop, Point};

// Linear gradient
let gradient = Brush::Gradient(Gradient::linear(
    Point::new(0.0, 0.0),      // Start
    Point::new(200.0, 0.0),    // End
    Color::rgba(0.9, 0.2, 0.5, 1.0),
    Color::rgba(0.2, 0.8, 0.6, 1.0),
));

ctx.fill_rect(
    Rect::new(0.0, 0.0, 200.0, 100.0),
    CornerRadius::default(),
    gradient,
);

// Multi-stop gradient
let gradient = Brush::Gradient(Gradient::linear_with_stops(
    Point::new(0.0, 0.0),
    Point::new(200.0, 0.0),
    vec![
        GradientStop::new(0.0, Color::RED),
        GradientStop::new(0.5, Color::YELLOW),
        GradientStop::new(1.0, Color::GREEN),
    ],
));
```

## Transforms

```rust
use blinc_core::Transform;

// Push transform
ctx.push_transform(Transform::translate(50.0, 50.0));

// Draw in transformed space
ctx.fill_rect(/* ... */);

// Pop transform
ctx.pop_transform();

// Rotation
ctx.push_transform(Transform::rotate(angle_radians));
// ... draw ...
ctx.pop_transform();

// Scale
ctx.push_transform(Transform::scale(2.0, 2.0));
// ... draw ...
ctx.pop_transform();
```

## Clipping

```rust
// Push clip region
ctx.push_clip(Rect::new(10.0, 10.0, 100.0, 100.0));

// Only content within clip region is visible
ctx.fill_rect(/* ... */);

// Pop clip
ctx.pop_clip();
```

## Example: Animated Spinner

```rust
use std::f32::consts::PI;

fn spinner(ctx: &WindowedContext) -> impl ElementBuilder {
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

        // Draw spinning segments
        for i in 0..8 {
            let segment_angle = angle_rad + (i as f32 * PI / 4.0);
            let alpha = 1.0 - (i as f32 * 0.1);

            let x = cx + segment_angle.cos() * radius;
            let y = cy + segment_angle.sin() * radius;

            draw_ctx.fill_circle(
                Point::new(x, y),
                4.0,
                Brush::Solid(Color::rgba(0.4, 0.6, 1.0, alpha)),
            );
        }
    })
    .w(80.0)
    .h(80.0)
}
```

## Example: Progress Ring

```rust
fn progress_ring(progress: f32) -> impl ElementBuilder {
    canvas(move |ctx, bounds| {
        let cx = bounds.width / 2.0;
        let cy = bounds.height / 2.0;
        let radius = bounds.width.min(bounds.height) / 2.0 - 4.0;

        // Background ring
        ctx.stroke_circle(
            Point::new(cx, cy),
            radius,
            4.0,
            Brush::Solid(Color::rgba(0.2, 0.2, 0.25, 1.0)),
        );

        // Progress arc (simplified - actual arc drawing would need path API)
        // For now, draw segments
        let segments = 32;
        let filled = (segments as f32 * progress) as i32;

        for i in 0..filled {
            let angle = (i as f32 / segments as f32) * 2.0 * PI - PI / 2.0;
            let x = cx + angle.cos() * radius;
            let y = cy + angle.sin() * radius;

            ctx.fill_circle(
                Point::new(x, y),
                3.0,
                Brush::Solid(Color::rgba(0.4, 0.6, 1.0, 1.0)),
            );
        }

        // Center text
        ctx.draw_text(
            &format!("{}%", (progress * 100.0) as i32),
            Point::new(cx - 15.0, cy + 6.0),
            &TextStyle::new(16.0).with_color(Color::WHITE),
        );
    })
    .w(80.0)
    .h(80.0)
}
```

## Example: Color Palette

```rust
fn color_palette() -> impl ElementBuilder {
    canvas(|ctx, bounds| {
        let cols = 8;
        let rows = 3;
        let cell_w = bounds.width / cols as f32;
        let cell_h = bounds.height / rows as f32;

        for row in 0..rows {
            for col in 0..cols {
                let hue = col as f32 / cols as f32;
                let sat = 1.0 - (row as f32 * 0.25);
                let color = hsv_to_rgb(hue, sat, 0.9);

                ctx.fill_rect(
                    Rect::new(
                        col as f32 * cell_w,
                        row as f32 * cell_h,
                        cell_w - 2.0,
                        cell_h - 2.0,
                    ),
                    CornerRadius::uniform(4.0),
                    Brush::Solid(color),
                );
            }
        }
    })
    .w(240.0)
    .h(90.0)
}
```

## Best Practices

1. **Set explicit size** - Canvas needs width and height to render.

2. **Use bounds parameter** - Draw relative to `bounds.width` and `bounds.height`.

3. **Clone Arcs for closures** - Animation values need `Arc::clone()` before the render closure.

4. **Push/pop transforms** - Always pop what you push to avoid state leaks.

5. **Prefer elements when possible** - Use `div()`, `text()` for standard UI; canvas for custom graphics.
