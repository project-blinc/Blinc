# blinc_gpu

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

GPU renderer for Blinc UI using [wgpu](https://wgpu.rs/) with SDF-based rendering.

## Overview

`blinc_gpu` provides high-performance GPU rendering for the Blinc UI framework. It uses Signed Distance Field (SDF) techniques for crisp, resolution-independent rendering of UI primitives.

## Features

- **SDF Primitives**: Rounded rectangles, circles, ellipses with perfect anti-aliasing
- **Shadows**: Gaussian blur shadows via error function approximation
- **Gradients**: Linear and radial gradient fills
- **Glass/Vibrancy**: Backdrop blur effects for frosted glass UI
- **Text Rendering**: SDF-based text with glyph atlases
- **Image Rendering**: Efficient texture-based image display
- **Compositing**: Layer blending with various blend modes
- **Cross-Platform**: Works on macOS, Windows, Linux, iOS, Android, and WebGPU

## Architecture

```
blinc_gpu
├── renderer.rs        # Main GpuRenderer
├── primitives.rs      # GPU primitive types
├── paint.rs           # Paint context implementation
├── text/              # Text rendering pipeline
├── image/             # Image rendering pipeline
├── shaders/           # WGSL shader modules
│   ├── sdf.wgsl       # SDF primitive rendering
│   ├── text.wgsl      # Text/glyph rendering
│   ├── glass.wgsl     # Glass blur effects
│   ├── blur.wgsl      # Gaussian blur
│   └── image.wgsl     # Image rendering
└── backbuffer.rs      # Double/triple buffering
```

## Rendering Pipeline

1. **Primitive Collection**: UI elements converted to `GpuPrimitive` instances
2. **Batching**: Primitives grouped by type and texture
3. **SDF Rendering**: Shapes rendered using signed distance functions
4. **Compositing**: Layers blended in correct order
5. **Output**: Final frame presented to surface

## Shader Features

### SDF Primitives

```wgsl
// Rounded rectangle with per-corner radius
fn sdf_rounded_rect(p: vec2<f32>, size: vec2<f32>, radii: vec4<f32>) -> f32

// Perfect anti-aliasing at any scale
fn alpha_from_sdf(d: f32) -> f32
```

### Glass Effects

```wgsl
// Frosted glass with backdrop blur
fn glass_blur(uv: vec2<f32>, blur_radius: f32) -> vec4<f32>

// Vibrancy with tint and saturation
fn apply_vibrancy(color: vec4<f32>, tint: vec4<f32>, saturation: f32) -> vec4<f32>
```

### Shadows

```wgsl
// Gaussian shadow via error function
fn shadow_alpha(d: f32, blur: f32) -> f32
```

## Usage

```rust
use blinc_gpu::{GpuRenderer, GpuPrimitive};

// Create renderer
let renderer = GpuRenderer::new(&device, &queue, surface_format);

// Create primitives
let rect = GpuPrimitive::rect(0.0, 0.0, 100.0, 50.0)
    .with_color(1.0, 0.0, 0.0, 1.0)
    .with_corner_radius(8.0)
    .with_border(2.0, 1.0, 1.0, 1.0, 1.0);

// Render
renderer.render_primitives(&view, &[rect]);
```

## Performance

- **Instanced Rendering**: Single draw call for multiple primitives
- **Texture Atlasing**: Glyphs and icons packed into atlases
- **Batch Optimization**: Automatic primitive batching
- **GPU-side AA**: Anti-aliasing computed in shader

## Requirements

- wgpu-compatible GPU
- Vulkan, Metal, DX12, or WebGPU backend

## License

MIT OR Apache-2.0
