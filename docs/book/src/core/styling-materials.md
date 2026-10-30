# Styling & Materials

Blinc provides comprehensive styling options from simple colors to advanced GPU-accelerated material effects.

## Colors

### Basic Colors

Colors are RGBA with values from 0.0 to 1.0:

```rust
// RGBA constructor
Color::rgba(0.2, 0.4, 0.8, 1.0)  // Blue, fully opaque
Color::rgba(1.0, 0.0, 0.0, 0.5)  // Red, 50% transparent

// From array (common pattern)
Color::from([0.2, 0.4, 0.8, 1.0])

// Predefined colors
Color::WHITE
Color::BLACK
Color::RED
Color::GREEN
Color::BLUE
Color::TRANSPARENT
```

### Background Colors

```rust
div()
    .bg(Color::rgba(0.1, 0.1, 0.15, 1.0))

// From array shorthand
div().bg([0.1, 0.1, 0.15, 1.0])
```

### Gradients

For gradients, use the `.background()` method with a `Brush`:

```rust
use blinc_core::{Brush, Gradient, GradientStop, Point};

div()
    .w(200.0)
    .h(100.0)
    .background(Brush::Gradient(Gradient::linear_with_stops(
        Point::new(0.0, 0.0),       // Start point
        Point::new(200.0, 0.0),     // End point
        vec![
            GradientStop::new(0.0, Color::rgba(0.9, 0.2, 0.5, 1.0)),
            GradientStop::new(0.5, Color::rgba(0.9, 0.5, 0.2, 1.0)),
            GradientStop::new(1.0, Color::rgba(0.2, 0.8, 0.6, 1.0)),
        ],
    )))
```

---

## Borders & Corners

### Corner Radius

```rust
div()
    .rounded(8.0)           // Uniform radius
    .rounded_full()         // Pill shape (50% of smallest dimension)
    .rounded_corners(
        16.0,  // Top-left
        16.0,  // Top-right
        8.0,   // Bottom-right
        8.0,   // Bottom-left
    )
```

---

## Shadows

### Preset Shadows

```rust
div()
    .shadow_sm()    // Small shadow
    .shadow_md()    // Medium shadow
    .shadow_lg()    // Large shadow
    .shadow_xl()    // Extra large shadow
```

### Custom Shadows

```rust
div().shadow_params(
    2.0,   // Offset X
    4.0,   // Offset Y
    12.0,  // Blur radius
    Color::rgba(0.0, 0.0, 0.0, 0.3)
)
```

---

## Opacity

```rust
div()
    .opacity(0.5)       // 50% opacity
    .opaque()           // opacity: 1.0
    .translucent()      // opacity: 0.5
    .invisible()        // opacity: 0.0
```

---

## Transforms

Apply 2D transforms to any element:

```rust
div()
    .translate(10.0, 20.0)    // Move by (x, y)
    .scale(1.5)               // Uniform scale
    .scale_xy(1.5, 0.8)       // Non-uniform scale
    .rotate(45.0_f32.to_radians())  // Rotate (radians)
    .rotate_deg(45.0)         // Rotate (degrees)
```

For combined transforms:

```rust
use blinc_core::Transform;

div().transform(
    Transform::translate(100.0, 50.0)
        .then_scale(1.2, 1.2)
        .then_rotate(0.1)
)
```

---

## Materials

Blinc includes GPU-accelerated material effects for modern, polished UIs.

### Glass Material

Creates a frosted glass effect with background blur:

```rust
use blinc_core::Color;
use blinc_layout::prelude::*;

// Quick glass
div().glass();

// Customized glass
div().effect(
    GlassMaterial::new()
        .blur(20.0)           // Blur intensity (0-50)
        .tint(Color::rgba(1.0, 1.0, 1.0, 0.1))
        .saturation(1.2)      // Color saturation
        .brightness(1.0)      // Brightness adjustment
        .noise(0.03)          // Frosted texture
        .border(0.8)          // Border highlight intensity
);
```

**Glass Presets:**

```rust
GlassMaterial::ultra_thin()  // Very subtle
GlassMaterial::thin()        // Light blur
GlassMaterial::regular()     // Standard (default)
GlassMaterial::thick()       // Heavy blur
GlassMaterial::frosted()     // Frosted window style
GlassMaterial::card()        // Card-like appearance
```

### Grouped and Nested Glass

Use the existing fluent element builders for grouped glass. Compatible native
panels composite their material and children together before applying the panel's
opacity and effects. Overlapping children therefore share the panel fade.

```rust
use blinc_layout::prelude::*;

let panel = div()
    .w(280.0)
    .p(16.0)
    .glass()
    .opacity(0.8)
    .child(text("Outer panel"))
    .child(
        div()
            .p(8.0)
            .effect(GlassMaterial::simple().blur(12.0))
            .child(text("Nested panel")),
    );
```

First-level panels use the prepared frame backdrop. A nested panel samples prior
paint within its enclosing glass isolation; content painted later is excluded.
The renderer derives nesting from the element tree. Applications do not set
`GroupGlass.nested` or construct `GroupGlass` commands.

Glass is also a brush. `.background(GlassStyle)` uses the same element grouping,
child ownership and automatic nesting as `.effect(GlassMaterial)`. The material
form lowers to `Brush::Glass` when drawn; `.glass()` is its default shorthand.

```rust
use blinc_core::GlassStyle;
use blinc_layout::prelude::*;

let panel = div()
    .w(280.0)
    .p(16.0)
    .background(GlassStyle::new().blur(12.0))
    .opacity(0.8)
    .child(text("Glass brush panel"));
```

The forms can be mixed in one tree. An explicit glass material takes precedence
over a glass background on the same element. Element shadow and border-color
settings override those embedded in the brush. Element nesting comes from the
tree, rather than `GlassStyle.depth`; canvas draws can set that depth themselves.

Grouping requires a drawing context with glass and collected-content support.
Custom Flow shaders, portal boundaries and 3D owners do not have the same grouped
glass guarantees. The `wet_glass_demo` uses a custom Flow shader with
`sample_scene()`, a separate path from this native glass material.

### Metallic Material

Creates reflective metallic surfaces:

```rust
use blinc_core::MetallicMaterial;

div().material(Material::Metallic(
    MetallicMaterial::new()
        .color(Color::WHITE)
        .roughness(0.3)       // 0 = mirror, 1 = matte
        .metallic(1.0)        // Metal intensity
        .reflection(0.5)      // Reflection strength
))
```

**Metallic Presets:**

```rust
MetallicMaterial::chrome()   // Polished chrome
MetallicMaterial::brushed()  // Brushed metal
MetallicMaterial::gold()     // Gold finish
MetallicMaterial::silver()   // Silver finish
MetallicMaterial::copper()   // Copper finish
```

### Quick Material Methods

```rust
div().glass()       // Default glass material
div().metallic()    // Default metallic material
div().chrome()      // Chrome preset
div().gold()        // Gold preset
```

---

## Render Layers

Control rendering order with layers:

```rust
use blinc_layout::RenderLayer;

div()
    .layer(RenderLayer::Background)  // Rendered first
    .child(background_content())

div()
    .layer(RenderLayer::Foreground)  // Rendered on top
    .child(overlay_content())
```

Glass materials select the glass render layer automatically. Put the background
and panel in the same scene:

```rust
use blinc_core::Color;
use blinc_layout::prelude::*;

stack()
    .child(div().w(320.0).h(200.0).bg(Color::BLUE))
    .child(
        div()
            .w(280.0)
            .h(160.0)
            .glass()
            .child(text("Glass panel")),
    );
```

---

## Common Styling Patterns

### Card Style

```rust
fn card() -> Div {
    div()
        .p(16.0)
        .rounded(12.0)
        .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
        .shadow_md()
}
```

### Glass Card

```rust
fn glass_card() -> Div {
    div()
        .p(16.0)
        .rounded(16.0)
        .glass()
        .shadow_lg()
}
```

### Button Styles

```rust
fn primary_button() -> Div {
    div()
        .px(4.0)
        .py(2.0)
        .rounded(8.0)
        .bg(Color::rgba(0.3, 0.5, 0.9, 1.0))
}

fn secondary_button() -> Div {
    div()
        .px(4.0)
        .py(2.0)
        .rounded(8.0)
        .bg(Color::rgba(0.2, 0.2, 0.25, 1.0))
}

fn ghost_button() -> Div {
    div()
        .px(4.0)
        .py(2.0)
        .rounded(8.0)
        .bg(Color::TRANSPARENT)
}
```

### Hover Effects with State

Use `stateful::<S>()` to create elements with automatic hover/press state transitions:

```rust
use blinc_layout::stateful::stateful;

fn hoverable_card() -> impl ElementBuilder {
    stateful::<ButtonState>()
        .p(16.0)
        .rounded(12.0)
        .on_state(|ctx| {
            let bg = match ctx.state() {
                ButtonState::Idle => Color::rgba(0.15, 0.15, 0.2, 1.0),
                ButtonState::Hovered => Color::rgba(0.18, 0.18, 0.24, 1.0),
                ButtonState::Pressed => Color::rgba(0.12, 0.12, 0.16, 1.0),
                _ => Color::rgba(0.15, 0.15, 0.2, 1.0),
            };
            div().bg(bg)
        })
        .child(text("Hover me").color(Color::WHITE))
}
```

---

## Dark Theme Color Palette

Common colors for dark-themed UIs:

```rust
// Backgrounds
let bg_primary = Color::rgba(0.08, 0.08, 0.12, 1.0);
let bg_secondary = Color::rgba(0.12, 0.12, 0.16, 1.0);
let bg_tertiary = Color::rgba(0.16, 0.16, 0.2, 1.0);

// Surfaces
let surface = Color::rgba(0.15, 0.15, 0.2, 1.0);
let surface_hover = Color::rgba(0.18, 0.18, 0.24, 1.0);

// Text
let text_primary = Color::WHITE;
let text_secondary = Color::rgba(0.7, 0.7, 0.8, 1.0);
let text_muted = Color::rgba(0.5, 0.5, 0.6, 1.0);

// Accent
let accent = Color::rgba(0.4, 0.6, 1.0, 1.0);
let accent_hover = Color::rgba(0.5, 0.7, 1.0, 1.0);

// Status
let success = Color::rgba(0.2, 0.8, 0.4, 1.0);
let warning = Color::rgba(0.9, 0.7, 0.2, 1.0);
let error = Color::rgba(0.9, 0.3, 0.3, 1.0);
```
