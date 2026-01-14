# blinc_paint

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

2D drawing primitives and path rendering for Blinc UI.

## Overview

`blinc_paint` provides a Canvas-like API for 2D drawing, similar to HTML Canvas or Skia.

## Features

- **Path Drawing**: Lines, curves, arcs, beziers
- **Shape Primitives**: Rectangles, circles, rounded rectangles
- **Fills & Strokes**: Solid colors, gradients
- **Text Rendering**: Basic text drawing
- **GPU Backend**: DrawContext implementation for GPU rendering

## Quick Start

```rust
use blinc_paint::{PaintContext, Color, Brush};

fn draw(ctx: &mut PaintContext) {
    // Draw a filled rectangle
    ctx.fill_rect(0.0, 0.0, 100.0, 50.0, Color::BLUE);

    // Draw a stroked circle
    ctx.stroke_circle(50.0, 50.0, 25.0, Color::RED, 2.0);

    // Draw a rounded rectangle
    ctx.fill_rounded_rect(10.0, 10.0, 80.0, 40.0, 8.0, Color::GREEN);
}
```

## Paths

```rust
use blinc_paint::{Path, PathCommand};

// Create a path
let mut path = Path::new();
path.move_to(0.0, 0.0);
path.line_to(100.0, 0.0);
path.line_to(100.0, 100.0);
path.quad_to(50.0, 150.0, 0.0, 100.0);
path.close();

// Draw the path
ctx.fill_path(&path, Color::BLUE);
ctx.stroke_path(&path, Color::BLACK, 2.0);
```

## Gradients

```rust
use blinc_paint::{LinearGradient, RadialGradient, GradientStop};

// Linear gradient
let linear = LinearGradient::new(0.0, 0.0, 100.0, 0.0)
    .add_stop(0.0, Color::RED)
    .add_stop(0.5, Color::YELLOW)
    .add_stop(1.0, Color::GREEN);

ctx.fill_rect_gradient(0.0, 0.0, 100.0, 50.0, &linear);

// Radial gradient
let radial = RadialGradient::new(50.0, 50.0, 50.0)
    .add_stop(0.0, Color::WHITE)
    .add_stop(1.0, Color::BLUE);

ctx.fill_circle_gradient(50.0, 50.0, 50.0, &radial);
```

## Transforms

```rust
// Save current transform
ctx.save();

// Apply transforms
ctx.translate(50.0, 50.0);
ctx.rotate(45.0_f32.to_radians());
ctx.scale(2.0, 2.0);

// Draw transformed
ctx.fill_rect(-25.0, -25.0, 50.0, 50.0, Color::BLUE);

// Restore previous transform
ctx.restore();
```

## Clipping

```rust
// Set clip rectangle
ctx.clip_rect(10.0, 10.0, 80.0, 80.0);

// All subsequent drawing is clipped
ctx.fill_rect(0.0, 0.0, 100.0, 100.0, Color::RED);
// Only the intersection is drawn

// Reset clip
ctx.reset_clip();
```

## Shapes

```rust
// Rectangle
ctx.fill_rect(x, y, width, height, color);
ctx.stroke_rect(x, y, width, height, color, stroke_width);

// Rounded rectangle
ctx.fill_rounded_rect(x, y, width, height, radius, color);
ctx.stroke_rounded_rect(x, y, width, height, radius, color, stroke_width);

// Circle
ctx.fill_circle(cx, cy, radius, color);
ctx.stroke_circle(cx, cy, radius, color, stroke_width);

// Ellipse
ctx.fill_ellipse(cx, cy, rx, ry, color);
ctx.stroke_ellipse(cx, cy, rx, ry, color, stroke_width);

// Line
ctx.draw_line(x1, y1, x2, y2, color, stroke_width);
```

## Architecture

```
blinc_paint
├── context.rs      # PaintContext API
├── path.rs         # Path and PathCommand
├── brush.rs        # Brush, Gradient types
├── color.rs        # Color utilities
└── transform.rs    # 2D transforms
```

## License

MIT OR Apache-2.0
