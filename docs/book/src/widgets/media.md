# Images & SVG

Blinc supports raster images and SVG graphics. You can set their size and style.

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

To load an image from a URL, your app needs the `network` feature of `blinc_image`. Add it to your
`Cargo.toml`:

```toml
blinc_image = { version = "0.6", features = ["network"] }
```

### Object Fit

Set how the image fills its container:

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

Set the alignment of the image in its container:

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

Lazy loading is for content-heavy apps with many images, such as galleries, feeds, and chat. It delays decode until the image is within 100 px of the visible area. While the real bitmap loads, Blinc renders a placeholder in its place. After the texture is in the cache, the image fades in. You can set the duration of the fade.

#### How it works

1. On each frame, the renderer compares every lazy image with the visible area, plus 100 px on each
   side. Inside a scroll container, the visible area is that of the container. The renderer skips the
   other images, with no decode and no GPU upload.
2. When an image comes into that area for the first time, Blinc starts the loader. It draws a
   placeholder at the final layout rect of the image.
3. As soon as the decoded texture is in the GPU cache, Blinc records `image_load_times[source] = Instant::now()`. Then it starts the fade-in. The alpha multiplier is `elapsed_ms / fade_duration_ms`. When the multiplier reaches `1.0`, the image is fully opaque and Blinc clears the fade-in flag.
4. Blinc preloads placeholder images (type 2) eagerly. They are available as soon as the lazy element appears. You see no flash of empty space while you wait for the thumbnail itself.
5. While any image fades in, the runtime continues to request redraws. The animation thus runs smoothly, even on a screen where nothing else changes.

#### Builder API

```rust
use blinc::layout::prelude::*;
use std::time::Duration;

// Basic lazy loading, with the default gray placeholder
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
| `Eager` (default) | Load and decode the image immediately at element creation |
| `Lazy` | Delay the load until the image is within 100 px of the visible area |

#### Placeholder types

| Placeholder | Description |
|-------------|-------------|
| `None` | No placeholder. The area stays empty until the bitmap arrives |
| `Color(color)` | Solid color background. The default is a translucent gray |
| `Brush(brush)` | Not drawn. Use a color, an image or the skeleton |
| `Image(url)` | Another image (a low-res thumbnail or a blur hash). Preloaded on tree build |
| `Skeleton` | Shimmer band animation that needs no asset |

#### Failed loads

If an image fails to load, it shows its placeholder, and Blinc tries to load it again. Examples of a failure are a missing file and a request that returns an error. The first retry comes after half a second. Each later wait is twice the last wait, but never longer than half a minute. `.retry(..)` sets this behavior for each image, and `.no_retry()` turns it off. If hot reload sees a change to the file behind an image, Blinc tries that image again at once.

```rust
use blinc::layout::prelude::*;
use std::time::Duration;

// Try again after 1 s, 2 s, 4 s, then every 5 s, five times at most
img("https://example.com/photo.jpg")
    .retry(ImageRetry::backoff(Duration::from_secs(1), Duration::from_secs(5)).attempts(5))

// A failed load stays failed
img("optional-badge.png").no_retry()
```

If a retry is due while the window is idle, it runs on the next frame that the window draws.

#### CSS overrides

A stylesheet can override lazy-loading behavior, with no change to the builder. Use this to theme galleries, or to apply defaults to all images that match a class.

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
| `image-placeholder-color` | `<color>` | Solid color placeholder. Implies `type: color` |
| `image-placeholder-image` | `<url>` | Thumbnail placeholder. Implies `type: image` |
| `fade-duration` | `<time>` (`200ms`, `0.3s`) | Length of the fade-in after the bitmap is ready |

CSS values override builder values. Use the builder for one-off images and CSS for repeated patterns.

> Note: Lazy loading currently applies to raster images only. Blinc vectorizes and rasterizes SVGs on demand, so a delay of their decode is rarely worth the cost. If you need a deferred SVG, wrap it in a `Stateful`, and reveal it on demand.

---

## Emoji Images

You can render emoji as images at any size with the system emoji font. To save memory, Blinc lazy-loads emoji images automatically.

```rust
use blinc::layout::image::{emoji, emoji_sized};

// Default size (64px)
emoji("😀")

// Custom size
emoji_sized("🚀", 128.0)

// In a layout
div()
    .flex_row()
    .gap(2.0)
    .child(emoji_sized("👍", 32.0))
    .child(emoji_sized("🎉", 32.0))
    .child(emoji_sized("✨", 32.0))
```

Emoji images use the system color emoji font (Apple Color Emoji on macOS, Segoe UI Emoji on Windows, Noto Color Emoji on Linux).

---

## SVG

### Basic SVG

`svg()` takes SVG markup, not a file path. To use a file, read it first, for example with
`include_str!("icons/menu.svg")`.

```rust
use blinc::layout::svg::svg;

const MENU: &str = r#"<svg viewBox="0 0 24 24">
    <path d="M4 6h16M4 12h16M4 18h16" stroke="currentColor" stroke-width="2"/>
</svg>"#;

svg(MENU).size(24.0, 24.0)
```

### SVG with Tint

Apply a color tint to monochrome (single-color) SVGs:

```rust
svg(MENU)
    .square(24.0)
    .tint(Color::WHITE)

svg(MENU)
    .square(20.0)
    .tint(Color::rgba(0.9, 0.3, 0.3, 1.0))
```

### SVG Sizing

```rust
// Width and height
svg(MENU).size(32.0, 16.0)

// Square shorthand
svg(MENU).square(24.0)
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
        .circular()
}
```

### Icon Button

```rust
use blinc::layout::stateful::stateful;

fn icon_button(icon: &'static str) -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(move |ctx| {
        let bg = match ctx.state() {
            ButtonState::Hovered => Color::rgba(0.2, 0.2, 0.25, 1.0),
            ButtonState::Pressed => Color::rgba(0.15, 0.15, 0.2, 1.0),
            _ => Color::TRANSPARENT,
        };
        div()
            .w(40.0)
            .h(40.0)
            .rounded(8.0)
            .items_center()
            .justify_center()
            .bg(bg)
            .child(svg(icon).square(20.0).tint(Color::WHITE))
    })
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
                .p(4.0)
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
        .gap(2.0)
        .children(images.iter().map(|url| {
            image(*url)
                .w(150.0)
                .h(150.0)
                .cover()
                .rounded(8.0)
        }))
}
```

### Placeholder with Fallback

```rust
fn image_with_placeholder(url: Option<&str>, icon: &str) -> Div {
    match url {
        Some(src) => div().child(image(src).w(200.0).h(200.0).cover().rounded(8.0)),
        None => div()
            .w(200.0)
            .h(200.0)
            .rounded(8.0)
            .bg(Color::rgba(0.2, 0.2, 0.25, 1.0))
            .items_center()
            .justify_center()
            .child(svg(icon).square(48.0).tint(Color::rgba(0.4, 0.4, 0.5, 1.0))),
    }
}
```

---

## Supported Formats

### Images

PNG and JPEG work by default. For the other formats, add `blinc_image` with the matching feature to
your `Cargo.toml`:

- WebP: `webp`
- GIF (first frame): `gif`
- BMP: `bmp`

### SVG

- Standard SVG 1.1
- Path elements
- Basic shapes (rect, circle, ellipse, line, polyline, polygon)
- Transforms
- Fill and stroke

---

## Best Practices

1. Set explicit dimensions. An image without a size is 100 by 100 px.

2. Use `cover` for photos. It fills the container well, without distortion.

3. Use `contain` for diagrams. It makes sure that nothing is cropped.

4. Tint icons. Use `.tint()` to match your color scheme.

5. Use SVG for icons. SVG scales perfectly to any size.

6. Optimize images. Use suitable formats and compression for the web.

7. Use lazy loading for galleries. In scroll containers with many images, use `.lazy()`. It reduces memory usage and improves the initial load time.

8. Use emoji images for large emoji. For crisp rendering of emoji larger than ~24px, use `emoji_sized()` instead of text.
