# Images & SVG

Blinc supports raster images and SVG graphics with flexible sizing and styling options.

## Images

### Basic Image

```rust
use blinc_layout::image::image;

image("path/to/photo.png")
    .w(200.0)
    .h(150.0)
```

### Image from URL

```rust
image("https://example.com/image.jpg")
    .w(300.0)
    .h(200.0)
```

### Object Fit

Control how the image fills its container:

```rust
image(src)
    .w(200.0)
    .h(200.0)
    .cover()      // Fill container, crop if needed (default)

image(src)
    .contain()    // Fit entirely, may letterbox

image(src)
    .fill()       // Stretch to fill exactly

image(src)
    .scale_down() // Scale down only if larger

image(src)
    .no_scale()   // No scaling, original size
```

### Object Position

Control alignment within the container:

```rust
image(src)
    .cover()
    .center()         // Center (default)

image(src)
    .cover()
    .top_left()

image(src)
    .cover()
    .top_center()

image(src)
    .cover()
    .bottom_right()

// Custom position (0.0 to 1.0)
image(src)
    .cover()
    .position_xy(0.25, 0.75)
```

### Image Filters

```rust
image(src)
    .w(200.0)
    .h(200.0)
    .grayscale(0.5)      // 0.0 = color, 1.0 = grayscale
    .sepia(0.3)          // Sepia tone
    .brightness(1.2)     // > 1.0 brighter, < 1.0 darker
    .contrast(1.1)       // > 1.0 more contrast
    .saturate(0.8)       // < 1.0 less saturated
    .hue_rotate(45.0)    // Rotate hue (degrees)
    .invert(0.2)         // Color inversion
    .blur(2.0)           // Blur radius
```

---

## SVG

### Basic SVG

```rust
use blinc_layout::svg::svg;

svg("icons/menu.svg")
    .w(24.0)
    .h(24.0)
```

### SVG with Tint

Apply a color tint to monochrome SVGs:

```rust
svg("icons/settings.svg")
    .w(24.0)
    .h(24.0)
    .tint(Color::WHITE)

svg("icons/error.svg")
    .w(20.0)
    .h(20.0)
    .tint(Color::rgba(0.9, 0.3, 0.3, 1.0))
```

### SVG Sizing

```rust
// Fixed size
svg(src).w(32.0).h(32.0)

// Square shorthand
svg(src).square(24.0)

// Aspect ratio preserved
svg(src).w(48.0).h_auto()
```

---

## Common Patterns

### Avatar Image

```rust
fn avatar(url: &str, size: f32) -> impl ElementBuilder {
    image(url)
        .w(size)
        .h(size)
        .cover()
        .rounded_full()  // Circular
}
```

### Icon Button

```rust
use blinc_layout::stateful::stateful;

fn icon_button(ctx: &WindowedContext, icon_path: &str) -> impl ElementBuilder {
    // Use use_state_for with icon_path as key for reusable component
    let handle = ctx.use_state_for(icon_path, ButtonState::Idle);

    stateful(handle)
        .w(40.0)
        .h(40.0)
        .rounded(8.0)
        .flex_center()
        .on_state(|state, div| {
            let bg = match state {
                ButtonState::Idle => Color::TRANSPARENT,
                ButtonState::Hovered => Color::rgba(0.2, 0.2, 0.25, 1.0),
                ButtonState::Pressed => Color::rgba(0.15, 0.15, 0.2, 1.0),
                _ => Color::TRANSPARENT,
            };
            div.set_bg(bg);
        })
        .child(
            svg(icon_path)
                .w(20.0)
                .h(20.0)
                .tint(Color::WHITE)
        )
}
```

### Image Card

```rust
fn image_card(image_url: &str, title: &str) -> impl ElementBuilder {
    div()
        .w(300.0)
        .rounded(12.0)
        .overflow_clip()
        .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
        .child(
            image(image_url)
                .w_full()
                .h(180.0)
                .cover()
        )
        .child(
            div()
                .p(16.0)
                .child(
                    text(title)
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE)
                )
        )
}
```

### Gallery Grid

```rust
fn gallery(images: &[&str]) -> impl ElementBuilder {
    div()
        .flex_row()
        .flex_wrap()
        .gap(8.0)
        .child(
            images.iter().map(|url| {
                image(*url)
                    .w(150.0)
                    .h(150.0)
                    .cover()
                    .rounded(8.0)
            })
        )
}
```

### Placeholder with Fallback

```rust
fn image_with_placeholder(url: Option<&str>) -> impl ElementBuilder {
    match url {
        Some(src) => image(src)
            .w(200.0)
            .h(200.0)
            .cover()
            .rounded(8.0),
        None => div()
            .w(200.0)
            .h(200.0)
            .rounded(8.0)
            .bg(Color::rgba(0.2, 0.2, 0.25, 1.0))
            .flex_center()
            .child(
                svg("icons/image-placeholder.svg")
                    .w(48.0)
                    .h(48.0)
                    .tint(Color::rgba(0.4, 0.4, 0.5, 1.0))
            ),
    }
}
```

---

## Supported Formats

### Images

- PNG
- JPEG
- WebP
- GIF (first frame)
- BMP
- ICO

### SVG

- Standard SVG 1.1
- Path elements
- Basic shapes (rect, circle, ellipse, line, polyline, polygon)
- Transforms
- Fill and stroke

---

## Best Practices

1. **Set explicit dimensions** - Images need width and height for layout.

2. **Use `cover` for photos** - Fills container nicely without distortion.

3. **Use `contain` for diagrams** - Ensures nothing is cropped.

4. **Tint icons** - Use `.tint()` to match your color scheme.

5. **Use SVG for icons** - Scales perfectly at any size.

6. **Optimize images** - Use appropriate formats and compression for web.
