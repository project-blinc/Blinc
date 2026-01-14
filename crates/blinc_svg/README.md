# blinc_svg

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

SVG loading and rendering for Blinc UI.

## Overview

`blinc_svg` provides SVG parsing and rendering capabilities using [usvg](https://github.com/RazrFalcon/resvg) and [resvg](https://github.com/RazrFalcon/resvg).

## Features

- **SVG Parsing**: Full SVG support via usvg
- **Two Render Modes**:
  - **Tessellation**: Fast, converts paths to triangles via Lyon
  - **Rasterization**: High-quality CPU rendering via resvg
- **Tinting**: Apply color tints to SVGs
- **Scaling**: Render at any size

## Quick Start

```rust
use blinc_svg::SvgDocument;

// Load SVG from file
let svg = SvgDocument::load("icon.svg")?;

// Load from string
let svg = SvgDocument::parse(r#"
    <svg viewBox="0 0 24 24">
        <path d="M12 2L2 7v10l10 5 10-5V7z"/>
    </svg>
"#)?;

// Get dimensions
let (width, height) = svg.size();
```

## Rasterization

```rust
use blinc_svg::RasterizedSvg;

// Rasterize at specific size
let rasterized = svg.rasterize(64, 64)?;

// Get pixel data
let pixels = rasterized.pixels();
let width = rasterized.width();
let height = rasterized.height();
```

## Usage in Layout

```rust
use blinc_layout::prelude::*;
use blinc_icons::icons;

// SVG from string
svg(r#"<svg>...</svg>"#)
    .size(24.0, 24.0)

// SVG from icon constant
svg(icons::CHECK)
    .size(16.0, 16.0)
    .color(Color::GREEN)

// Custom SVG file
svg_file("assets/logo.svg")
    .size(100.0, 50.0)
```

## Tinting

```rust
// Apply solid color tint
svg(icons::HEART)
    .size(24.0, 24.0)
    .color(Color::RED)

// SVGs are rendered with the tint color applied
// to all fill and stroke elements
```

## Performance

For best performance:

- Use **rasterization** for complex SVGs displayed at fixed sizes
- Use **tessellation** for simple icons that scale dynamically
- Cache rasterized SVGs when displaying the same icon repeatedly

## Architecture

```
blinc_svg
├── document.rs     # SVG document parsing
├── rasterize.rs    # CPU rasterization via resvg
├── tessellate.rs   # Path tessellation via lyon
└── commands.rs     # Drawing command generation
```

## License

MIT OR Apache-2.0
