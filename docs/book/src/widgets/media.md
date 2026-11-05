# Images & SVG

Blinc supports raster images and SVG graphics with flexible sizing and styling options.

## Images

### Basic Image

```rust
use blinc::layout::image::image;

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

### Lazy Loading

For content-heavy apps with many images (galleries, feeds, chat) lazy loading defers decode until the image is actually visible in the viewport. While the real bitmap is loading, Blinc renders a placeholder in its place; once the texture lands in cache, the image fades in over a configurable duration.

#### How it works

1. On each frame, the renderer tests every lazy image's quad against the current viewport AABB. Off-screen images are skipped — no decode, no GPU upload.
2. When an image first intersects the viewport, the loader is triggered and a placeholder is drawn at the image's final layout rect.
3. As soon as the decoded texture appears in the GPU cache, Blinc records `image_load_times[source] = Instant::now()` and starts the fade-in. `elapsed_ms / fade_duration_ms` is the alpha multiplier — once it reaches `1.0` the image is fully opaque and the fade-in flag is cleared.
4. Placeholder images (type 2) are eagerly preloaded so they're available the moment the lazy element appears — there's no flash of empty space waiting on the thumbnail itself.
5. While any image is fading in, the runtime keeps requesting redraws so the animation runs smoothly even when nothing else on screen is changing.

#### Builder API

```rust
use blinc::layout::prelude::*;
use std::time::Duration;

// Basic lazy loading — no placeholder, just defer decode
img("large-photo.jpg")
    .lazy()
    .w(300.0)
    .h(200.0)

// Solid color placeholder
img("photo.jpg")
    .lazy()
    .placeholder_color(Color::rgba(0.2, 0.2, 0.2, 1.0))
    .w(300.0)
    .h(200.0)

// Gradient (or any Brush) placeholder
img("photo.jpg")
    .lazy()
    .placeholder_brush(Brush::Gradient(Gradient::linear(
        Point::new(0.0, 0.0),
        Point::new(1.0, 1.0),
        Color::rgba(0.4, 0.6, 1.0, 1.0),
        Color::rgba(0.6, 0.4, 1.0, 1.0),
    )))
    .w(300.0)
    .h(200.0)

// Thumbnail / blur-hash placeholder — eagerly preloaded
img("large-photo.jpg")
    .lazy()
    .placeholder_image("thumbnail.jpg")
    .fade_in(Duration::from_millis(300))
    .w(300.0)
    .h(200.0)

// Skeleton shimmer (animated band sweeping left → right)
img("photo.jpg")
    .lazy()
    .skeleton()
    .fade_in(Duration::from_millis(250))
    .w(300.0)
    .h(200.0)

// Cross-fade off — image pops in instantly when ready
img("photo.jpg")
    .lazy()
    .no_fade()
    .w(300.0)
    .h(200.0)
```

#### Loading strategies

| Strategy | Description |
|----------|-------------|
| `Eager` (default) | Load and decode immediately when the element is created |
| `Lazy` | Defer load until the image's layout rect intersects the viewport |

#### Placeholder types

| Placeholder | Description |
|-------------|-------------|
| `None` | No placeholder — empty until the bitmap arrives |
| `Color(color)` | Solid color background |
| `Brush(brush)` | Any brush — gradients, glass effects, etc. |
| `Image(url)` | Another image (low-res thumbnail, blur hash). Preloaded on tree build |
| `Skeleton` | Shimmer band animation, no asset required |

#### CSS overrides

Lazy-loading behavior can be overridden from a stylesheet without touching the builder. This is useful for theming galleries or applying defaults to all images that match a class.

```css
.gallery-item {
    loading: lazy;
    image-placeholder-type: skeleton;
    fade-duration: 250ms;
}

.avatar {
    loading: lazy;
    image-placeholder-color: rgba(40, 40, 50, 1.0);
    fade-duration: 200ms;
}

.hero {
    loading: lazy;
    image-placeholder-image: "hero-blur.jpg";
    fade-duration: 400ms;
}
```

| Property | Values | Effect |
|----------|--------|--------|
| `loading` | `eager` \| `lazy` | Switches the loading strategy |
| `image-placeholder-type` | `none` \| `color` \| `skeleton` | Selects which placeholder to draw |
| `image-placeholder-color` | `<color>` | Solid color placeholder; implies `type: color` |
| `image-placeholder-image` | `<url>` | Thumbnail placeholder; implies `type: image` |
| `fade-duration` | `<time>` (`200ms`, `0.3s`) | Fade-in length once the bitmap is ready |

CSS values override builder values. Use the builder for one-off images and CSS for repeated patterns.

> **Note:** Lazy loading currently applies to raster images only. SVGs are vectorized and rasterized on demand, so deferring their decode rarely pays for itself; if you need a deferred SVG, wrap it in a `Stateful` and reveal it when needed.

---

## Emoji Images

Render emoji as images at arbitrary sizes using the system emoji font. Emoji images are **automatically lazy-loaded** for memory efficiency.

```rust
use blinc::layout::image::{emoji, emoji_sized};

// Default size (64px)
emoji("😀")

// Custom size
emoji_sized("🚀", 128.0)

// In a layout
div()
    .flex_row()
    .gap(8.0)
    .child(emoji_sized("👍", 32.0))
    .child(emoji_sized("🎉", 32.0))
    .child(emoji_sized("✨", 32.0))
```

Emoji images use the system color emoji font (Apple Color Emoji on macOS, Segoe UI Emoji on Windows, Noto Color Emoji on Linux).

---

## SVG

### Basic SVG

```rust
use blinc::layout::svg::svg;

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
use blinc::layout::stateful::stateful;

fn icon_button(icon_path: &str) -> impl ElementBuilder {
    stateful::<ButtonState>()
        .w(40.0)
        .h(40.0)
        .rounded(8.0)
        .flex_center()
        .on_state(|ctx| {
            let bg = match ctx.state() {
                ButtonState::Idle => Color::TRANSPARENT,
                ButtonState::Hovered => Color::rgba(0.2, 0.2, 0.25, 1.0),
                ButtonState::Pressed => Color::rgba(0.15, 0.15, 0.2, 1.0),
                _ => Color::TRANSPARENT,
            };
            div().bg(bg)
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

7. **Use lazy loading for galleries** - In scroll containers with many images, use `.lazy()` to reduce memory usage and improve initial load time.

8. **Use emoji images for large emoji** - For emoji larger than ~24px, use `emoji_sized()` instead of text for crisp rendering.
