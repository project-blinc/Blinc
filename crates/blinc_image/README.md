# blinc_image

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

Image loading and rendering for Blinc UI.

## Overview

`blinc_image` provides image loading, decoding, and rendering utilities for the Blinc UI framework.

## Features

- **Multiple Sources**: Load from files, URLs, and base64 data
- **Format Support**: PNG, JPEG, GIF, WebP, BMP
- **Object Fit**: CSS-style object-fit options
- **Image Filters**: Grayscale, sepia, brightness, contrast, blur
- **Async Loading**: Non-blocking URL loading (with `network` feature)
- **Platform Assets**: Load from app bundles

## Quick Start

```rust
use blinc_image::{ImageData, ImageSource, ObjectFit};

// Load from file
let image = ImageData::load("path/to/image.png")?;

// Load from URL (async)
let image = ImageData::load_url("https://example.com/image.jpg").await?;

// Load from base64
let image = ImageData::load_base64("data:image/png;base64,...")?;
```

## Object Fit

```rust
use blinc_image::ObjectFit;

// CSS object-fit equivalent
ObjectFit::Cover      // Fill container, crop if needed
ObjectFit::Contain    // Fit within container, letterbox
ObjectFit::Fill       // Stretch to fill (ignores aspect ratio)
ObjectFit::ScaleDown  // Scale down only if larger
ObjectFit::None       // No scaling, original size
```

## Object Position

```rust
use blinc_image::ObjectPosition;

// CSS object-position equivalent
ObjectPosition::CENTER      // Center (default)
ObjectPosition::TOP_LEFT    // Align to top-left
ObjectPosition::BOTTOM_RIGHT // Align to bottom-right
ObjectPosition::new(0.25, 0.75) // Custom position (0-1 range)
```

## Image Filters

```rust
use blinc_image::ImageFilter;

let filter = ImageFilter::new()
    .grayscale(0.5)      // 0-1 (0 = none, 1 = full)
    .sepia(0.2)          // 0-1
    .brightness(1.2)     // 1 = normal, >1 = brighter
    .contrast(1.1)       // 1 = normal, >1 = more contrast
    .saturate(1.5)       // 1 = normal, >1 = more saturated
    .blur(2.0);          // Blur radius in pixels
```

## Usage in Layout

```rust
use blinc_layout::prelude::*;

// Basic image
img("photo.jpg")
    .size(200.0, 150.0)

// With object-fit
img("photo.jpg")
    .size(200.0, 150.0)
    .cover()              // ObjectFit::Cover
    .rounded(8.0)

// Lazy loading
img("large-photo.jpg")
    .lazy()
    .placeholder_color(Color::GRAY)

// With border
img("avatar.jpg")
    .size(64.0, 64.0)
    .circular()
    .border(2.0, Color::WHITE)

// With filters
img("photo.jpg")
    .grayscale(1.0)
    .blur(2.0)
```

## Supported Formats

| Format | Extension | Notes |
|--------|-----------|-------|
| PNG | `.png` | Full support including transparency |
| JPEG | `.jpg`, `.jpeg` | Standard JPEG |
| GIF | `.gif` | Static only (no animation) |
| WebP | `.webp` | Lossy and lossless |
| BMP | `.bmp` | Basic support |

## License

MIT OR Apache-2.0
