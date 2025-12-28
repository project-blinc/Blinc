# GPU Rendering

Blinc uses GPU-accelerated rendering via wgpu with **Signed Distance Field (SDF)** shaders for smooth, resolution-independent UI primitives.

## Signed Distance Fields

An SDF shader computes the signed distance from each pixel to the geometry's edge:

- **Negative distance**: pixel is inside the shape
- **Positive distance**: pixel is outside the shape
- **Zero**: pixel is exactly on the edge

This enables:
- Smooth anti-aliasing at any scale
- Per-corner rounded rectangles
- Soft shadows with Gaussian blur
- Sharp text at any zoom level

## GPU Primitive Structure

All UI elements are converted to `GpuPrimitive` structs (192 bytes each):

```rust
struct GpuPrimitive {
    // Geometry
    bounds: [f32; 4],           // x, y, width, height
    corner_radii: [f32; 4],     // top-left, top-right, bottom-right, bottom-left

    // Fill
    fill_type: FillType,        // Solid, LinearGradient, RadialGradient
    fill_colors: [Color; 4],    // gradient stops or solid color

    // Border
    border_width: f32,
    border_color: Color,

    // Shadow
    shadow_offset: [f32; 2],
    shadow_blur: f32,
    shadow_color: Color,

    // Clipping
    clip_bounds: [f32; 4],
    clip_radii: [f32; 4],
}
```

### Primitive Types

| Type | Description |
|------|-------------|
| `Rect` | Rounded rectangle with per-corner radius |
| `Circle` | Perfect circle |
| `Ellipse` | Axis-aligned ellipse |
| `Shadow` | Drop shadow with Gaussian blur |
| `InnerShadow` | Inset shadow |
| `Text` | Glyph sampled from texture atlas |

### Fill Types

| Type | Description |
|------|-------------|
| `Solid` | Single color fill |
| `LinearGradient` | Gradient between two points |
| `RadialGradient` | Gradient radiating from center |

## Rendering Pipeline

Rendering happens in multiple passes for proper layering:

```
1. Background Pass (non-glass elements)
   └── SDF shader for rects, circles, shadows

2. Glass Pass (frosted glass elements)
   └── Glass shader samples backbuffer + blur

3. Foreground Pass (text, overlays)
   └── Text shader with glyph atlases
   └── SDF shader for overlays

4. Composite Pass
   └── Blend layers + MSAA resolve
```

### SDF Shader

The main SDF shader handles rectangles with:

```wgsl
// Signed distance to rounded rectangle
fn sdf_rounded_rect(p: vec2f, size: vec2f, radii: vec4f) -> f32 {
    // Select corner radius based on quadrant
    let radius = select_corner_radius(p, radii);

    // Compute distance to rounded corner
    let q = abs(p) - size + radius;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2f(0.0))) - radius;
}
```

Anti-aliasing uses the SDF gradient:

```wgsl
// Smooth alpha based on distance
let alpha = 1.0 - smoothstep(-0.5, 0.5, distance);
```

### Shadow Rendering

Soft shadows use Gaussian blur approximated with the error function:

```wgsl
fn shadow_alpha(distance: f32, blur_radius: f32) -> f32 {
    // Error function approximation for Gaussian
    let t = distance / (blur_radius * 0.5);
    return 0.5 * (1.0 - erf(t));
}
```

## Glass Effects

Blinc implements Apple-style frosted glass (vibrancy) with backdrop blur:

### Glass Types

| Type | Blur | Saturation | Use Case |
|------|------|------------|----------|
| `UltraThin` | Light | High | Subtle overlays |
| `Thin` | Medium | Medium | Sidebars |
| `Regular` | Standard | Standard | Modals, cards |
| `Thick` | Heavy | Low | Headers |
| `Chrome` | Very heavy | Low | Window chrome |

### Glass Shader

The glass shader:

1. **Samples backbuffer** - Reads rendered content behind the glass
2. **Applies Gaussian blur** - Multi-tap sampling with weights
3. **Adjusts saturation** - Controls color vibrancy
4. **Adds tint color** - Overlays glass tint
5. **Applies noise grain** - Frosted texture effect
6. **Rim bending** - Light refraction at edges

```wgsl
// Simplified glass computation
fn glass_color(uv: vec2f, glass_type: u32) -> vec4f {
    // Sample and blur background
    var color = blur_sample(backbuffer, uv, blur_radius);

    // Adjust saturation
    color = saturate_color(color, saturation);

    // Apply tint
    color = mix(color, tint_color, tint_strength);

    // Add noise grain
    color += noise(uv) * grain_amount;

    return color;
}
```

### Three-Layer Rendering

Glass requires separating background from foreground:

```
┌─────────────────────────────────┐
│  Foreground Layer               │  Text, icons on glass
├─────────────────────────────────┤
│  Glass Layer                    │  Frosted glass with blur
├─────────────────────────────────┤
│  Background Layer               │  Content behind glass
└─────────────────────────────────┘
```

The renderer:
1. Renders background to backbuffer texture
2. Renders glass elements sampling the backbuffer
3. Renders foreground elements on top
4. Composites all layers

## Text Rendering

Text uses a separate glyph atlas system:

1. **Font Loading** - Parses TTF/OTF via rustybuzz
2. **Glyph Shaping** - HarfBuzz-compatible shaping for complex scripts
3. **Atlas Generation** - Rasterizes glyphs to texture atlas
4. **SDF Text** - Stores distance field for each glyph
5. **Rendering** - Samples atlas with color spans

```rust
// Text rendering produces glyph primitives
for glyph in shaped_text.glyphs() {
    emit_text_primitive(GpuPrimitive {
        primitive_type: PrimitiveType::Text,
        bounds: glyph.bounds,
        texture_coords: glyph.atlas_uv,
        color: text_color,
        // ...
    });
}
```

## Batching & Instancing

Primitives are batched by type and rendered with GPU instancing:

```rust
// All rects in one draw call
batch.add_primitive(rect1);
batch.add_primitive(rect2);
batch.add_primitive(rect3);
// Single instanced draw call for all rects
```

Benefits:
- Minimal CPU-GPU communication
- Efficient use of GPU parallelism
- Scales to thousands of primitives

## MSAA Support

Blinc supports multi-sample anti-aliasing:

| Sample Count | Quality | Performance |
|--------------|---------|-------------|
| 1x | Baseline | Fastest |
| 2x | Improved edges | Slight cost |
| 4x | Good quality | Moderate cost |
| 8x | High quality | Higher cost |

MSAA is resolved in the final composite pass.

## DrawContext Trait

The bridge between layout and GPU is the `DrawContext` trait:

```rust
trait DrawContext {
    fn draw_rect(&mut self, bounds: Rect, props: &RenderProps);
    fn draw_text(&mut self, text: &ShapedText, position: Point);
    fn draw_shadow(&mut self, bounds: Rect, shadow: &Shadow);
    fn push_clip(&mut self, bounds: Rect, radii: [f32; 4]);
    fn pop_clip(&mut self);
    fn push_transform(&mut self, transform: Transform);
    fn pop_transform(&mut self);
}
```

The RenderTree traverses nodes and calls DrawContext methods, which accumulate GPU primitives for the render passes.
