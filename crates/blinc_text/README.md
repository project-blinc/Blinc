# blinc_text

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

High-quality text rendering for Blinc UI.

## Overview

`blinc_text` provides comprehensive text rendering capabilities including font loading, text shaping, glyph rasterization, and layout.

## Features

- **Font Loading**: TTF/OTF support via ttf-parser
- **Text Shaping**: Complex script support via rustybuzz (HarfBuzz)
- **Glyph Rasterization**: High-quality rendering via swash
- **Glyph Atlases**: Efficient GPU texture caching
- **Text Layout**: Line breaking, word wrapping, alignment
- **Emoji Support**: Full color emoji rendering
- **Memory Efficient**: Global shared font registry

## Quick Start

```rust
use blinc_text::{global_font_registry, TextRenderer, PreparedText};

// Load fonts
let registry = global_font_registry();
registry.load_font_file("path/to/font.ttf")?;

// Create renderer
let renderer = TextRenderer::new(&registry);

// Prepare text for rendering
let prepared = renderer.prepare(
    "Hello, World!",
    16.0,                    // font size
    Some("Inter"),           // font family
    FontWeight::Regular,
    FontStyle::Normal,
);

// Get glyph positions for GPU rendering
for glyph in prepared.glyphs() {
    // Render glyph at position
}
```

## Font Registry

```rust
use blinc_text::global_font_registry;

let registry = global_font_registry();

// Load single font
registry.load_font_file("fonts/Inter-Regular.ttf")?;

// Load directory
registry.load_font_directory("fonts/")?;

// Load system fonts
for path in blinc_app::system_font_paths() {
    registry.load_font_directory(&path)?;
}
```

## Text Shaping

```rust
use blinc_text::{TextShaper, ShapedText};

let shaper = TextShaper::new(&font);
let shaped: ShapedText = shaper.shape("Hello مرحبا 你好");

// Shaped text contains positioned glyphs
for glyph in shaped.glyphs() {
    println!("Glyph {} at ({}, {})", glyph.id, glyph.x, glyph.y);
}
```

## Text Layout

```rust
use blinc_text::{TextLayout, TextLayoutEngine, TextAlign};

let engine = TextLayoutEngine::new(&registry);

let layout = engine.layout(
    "Long text that needs to wrap to multiple lines...",
    TextLayoutParams {
        max_width: Some(300.0),
        font_size: 14.0,
        line_height: 1.5,
        align: TextAlign::Left,
        ..Default::default()
    }
);

// Get line positions
for line in layout.lines() {
    println!("Line at y={}: '{}'", line.y, line.text);
}
```

## Glyph Atlases

```rust
use blinc_text::GlyphAtlas;

// Create atlas for GPU rendering
let atlas = GlyphAtlas::new(1024, 1024);

// Add glyphs
let uv = atlas.add_glyph(glyph_id, &rasterized_glyph)?;

// Get texture for GPU
let texture = atlas.texture();
```

## Emoji Support

```rust
use blinc_text::{EmojiRenderer, is_emoji, contains_emoji};

// Check for emoji
assert!(is_emoji('😀'));
assert!(contains_emoji("Hello 👋 World"));

// Render emoji
let renderer = EmojiRenderer::new();
let image = renderer.render_emoji("🚀", 64)?;
```

## Architecture

```
blinc_text
├── font.rs          # Font loading and parsing
├── shaper.rs        # Text shaping (rustybuzz)
├── rasterizer.rs    # Glyph rasterization (swash)
├── atlas.rs         # Glyph atlas management
├── layout.rs        # Text layout engine
├── emoji.rs         # Emoji rendering
└── registry.rs      # Global font registry
```

## License

MIT OR Apache-2.0
