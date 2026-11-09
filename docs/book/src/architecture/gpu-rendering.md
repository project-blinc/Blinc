# GPU Rendering

Blinc renders on the GPU via wgpu. The pipeline has two parts that work
together:

1. **An SDF-based primitive renderer.** Rounded rects, circles, shadows,
   glass, text, images. Resolution-independent, analytical, no
   tessellation.
2. **A damage-aware compositor.** A cached static texture plus targeted
   re-dispatch, so a hover or animation tick only redraws what changed.

Most documentation around the web treats GPU UI as "rasterise the tree,
present". Blinc does that exactly once per tree state. After that, frames
are usually "blit the cache, then re-dispatch the N changed primitives
scissored to a damage rect".

## The frame loop

Every frame goes through five phases:

```
1. diff       apply reactive signal updates; classify changes
2. layout     Taffy compute_layout on dirty nodes
3. tick       advance springs, CSS keyframes, transitions
4. paint      fast path or full walker, then GPU dispatch
5. composite  blit static cache + overlays, present, re-arm if mid-flight
```

Phase 4 is the one with branches.

### Phase 4: paint

The frame loop computes a `try_fast_paint` predicate at the start of paint:

```rust
try_fast_paint =
       !did_rebuild
    && !needs_relayout
    && !css_blocks_fast        // no layout-affecting CSS change
    && !scroll_animating
    && !bounds_anim_active     // no bounds / clip / size animation
    && !new_overlay_active
    && has_render_cache();
```

If every gate holds: **compositor fast path**. The tree is *not* re-walked.
Two delta paths run in sequence:

- **Motion deltas (`apply_binding_deltas`).** Patch motion-bound primitives
  in place in the dynamic batch. Translates, scales, rotations, opacities
  rewrite GPU primitive fields directly.
- **CSS deltas (`apply_css_deltas`).** Patch CSS-animated primitives in
  place in the static batch. Opacity, background colour, border colour,
  border width, corner radius, shadow, and 3D rotations are all in-place.

Both paths build a list of damage rects (pre and post AABB of each changed
primitive, padded for SDF antialiasing). The GPU pass,
`render_static_layer_damaged`, unions those rects into a single
`scissor_rect`, clears that region with a REPLACE-blend quad, and
re-dispatches only the primitives that intersect.

If any gate trips, the **full walker** runs. The static cache is rebuilt
from scratch. This happens on tree rebuilds, layout changes, scroll, and
overlay transitions.

### Phase 5: composite

`composite_frame` finishes the frame:

1. `copy_texture_to_texture` from the cached static texture onto the
   swapchain target.
2. An overlay pass with `LoadOp::Load` dispatches the dynamic batch and
   motion subtree blits on top.
3. Submit, present.

If any animation is still mid-flight and any of its primitives are inside
the viewport, Phase 5 calls `request_redraw()` to re-arm.

## Signed Distance Fields

The primary primitive shader is an SDF. It computes the signed distance
from each pixel to the geometry's edge:

- Negative distance: pixel is inside the shape.
- Positive distance: pixel is outside.
- Zero: pixel sits on the edge.

This gets you smooth antialiasing at any scale, per-corner rounded
rectangles, soft shadows from a closed-form Gaussian integral, and sharp
text at any zoom.

```wgsl
fn sd_rounded_rect(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    let q = select(r.xy, r.zw, p.x > 0.0);
    let corner = select(q.x, q.y, p.y > 0.0);
    let d = abs(p) - b + corner;
    return min(max(d.x, d.y), 0.0) + length(max(d, vec2(0.0))) - corner;
}
```

Antialiasing uses fragment-derivative width:

```wgsl
let aa_width = fwidth(distance) * 0.5;
let alpha    = smoothstep(aa_width, -aa_width, distance);
```

Shadows use an `erf` approximation for the Gaussian integral. Analytical,
no texture lookups.

### `corner-shape`

A single exponent `n` morphs corners between geometries:

| `n`  | Shape    |
|------|----------|
| 1    | Round (circular arc) |
| 2    | Squircle |
| >2   | Closer to a true square |

`sd_shaped_rect` evaluates `|x/r|^p + |y/r|^p = 1` with `p_exp = 2^|n|`.

## GPU primitive

A single `GpuPrimitive` carries everything one shape needs the SDF shaders
to render it:

```rust
#[repr(C)]
struct GpuPrimitive {
    bounds: [f32; 4],
    corner_radius: [f32; 4],
    color: [f32; 4],
    color2: [f32; 4],       // gradient end colour
    border: [f32; 4],
    border_color: [f32; 4],
    shadow: [f32; 4],
    shadow_color: [f32; 4],
    rotation: [f32; 4],     // 2D Z + Y rotation sin/cos
    perspective: [f32; 4],  // 3D rot_x, persp distance, shape type
    sdf_3d: [f32; 4],       // depth, ambient, specular, translate_z
    light: [f32; 4],        // 3D light dir + intensity
    filter_a: [f32; 4],     // grayscale / invert / sepia / hue
    filter_b: [f32; 4],     // brightness / contrast / saturate
    type_info: [u32; 4],    // primitive_type, fill_type, …
    // … plus clip + local_affine fields
}
```

Primitives are batched per pipeline and dispatched via GPU instancing: one
draw call per pipeline per layer.

### Primitive types

| Type          | Description                          |
|---------------|--------------------------------------|
| `Rect`        | Rounded rectangle, per-corner radius |
| `Circle`      | Perfect circle                       |
| `Ellipse`     | Axis-aligned ellipse                 |
| `Shadow`      | Drop shadow (Gaussian blur)          |
| `InnerShadow` | Inset shadow                         |
| `Text`        | Glyph sampled from atlas             |

### Fill types

`Solid`, `LinearGradient`, `RadialGradient`. Gradients painted onto 3D
shapes have their UVs auto-mapped onto the hit point (face / spherical /
cylindrical depending on the shape).

## The cache

Two GPU-side batches sit on the render context:

- **Static batch.** Primitives whose pixels won't change without a layout,
  structural, or state event.
- **Dynamic batch.** Motion-bound primitives. Rewritten in place by
  `apply_binding_deltas` every frame they're animating.

A walker pass classifies each node via `AnimationStatus`. `Static` nodes go
to the static batch; `Animating(kind)` nodes go to the dynamic batch.
`kind` is one of:

| `DynamicKind`           | Role                                                                |
|-------------------------|---------------------------------------------------------------------|
| `Canvas`                | User draw closure re-invoked every frame                            |
| `MotionSubtree`         | Subtree re-walked each frame with current binding values            |
| `MotionSubtreeTexture`  | Subtree baked to an offscreen texture and transformed-blitted       |
| `CssAnimated`           | CSS-animated subtree baked to a composite texture                   |

`MotionSubtreeTexture` and `CssAnimated` are the "subtree-as-texture"
cases. The subtree's primitives don't re-rasterise during motion; only the
texture gets blitted with the current transform applied.

Cache invalidation is explicit: `invalidate_render_cache_tagged()`. Called
on structural change, layout-prop animation start, and motion settle.

## Damage-rect re-dispatch

`render_static_layer_damaged` is the GPU pass for the fast path. Given a
set of damage rects:

1. Compute the union, pad by 4 px (cover SDF antialias edges), clamp to
   layer extent.
2. Set as `scissor_rect` on a `LoadOp::Load` pass. Pixels outside the rect
   stay as they are in the cached texture.
3. Dispatch a REPLACE-blend clear quad to zero the scissor region.
4. Re-dispatch SDF primitives that intersect the scissor.

Net result: pixels outside the damage rect keep last frame's content;
pixels inside get the fresh primitive content. No full re-rasterise.

The damage path currently handles SDF primitives. It bails to a full layer
re-render when the batch contains vector paths, 3D viewports, or
particles. Text, SVG, and image dispatch through the damage path is
scaffolded but gated behind `BLINC_DAMAGE_RECT=1` while it's finished off.

## Viewport culling

Scroll containers cull by default (`.viewport_cull(false)` turns it off):

```rust
scroll().child(big_list)
```

A child whose subtree lies outside the container's on-screen rect (plus a
200 px overscan band) emits zero primitives during the walker and is skipped
by element collection and hit testing. The test covers the child's whole
subtree, stopping at descendants that clip, so content overflowing into view
is kept. It does not see past transforms, motions, layout animations, or
fixed and sticky children, which are always walked.

A node that clips and has no area skips its children entirely.

For animation gating, the same viewport intersection clips the set of
animating nodes Phase 5 considers when deciding whether to re-arm the next
frame. Off-screen animations still tick on the background thread, but they
don't burn frames.

## Glass / vibrancy

Apple-style frosted glass with backdrop blur. Five presets:

| Type        | Blur | Saturation | Use case          |
|-------------|------|------------|-------------------|
| `UltraThin` | 10px | 1.8×       | Subtle overlays   |
| `Thin`      | 15px | 1.6×       | Light panels      |
| `Regular`   | 20px | 1.4×       | Default glass     |
| `Thick`     | 30px | 1.2×       | Strong blur       |
| `Chrome`    | 25px | 0.0×       | Metallic effect   |

The glass shader samples a backbuffer (or a per-layer texture for stacked
glass), applies Kawase blur, saturates, tints, adds procedural noise, and
optionally refracts rim light. The backbuffer is re-blitted before the
glass pass, so glass composes correctly with damage-rect repainting.

## Three-layer rendering

When glass is in the scene, content separates into three layers:

```
┌─────────────────────────────────┐
│  Foreground   text, icons       │
├─────────────────────────────────┤
│  Glass        frosted blur      │
├─────────────────────────────────┤
│  Background   content behind    │
└─────────────────────────────────┘
```

The renderer paints the background into the backbuffer, paints glass
elements sampling the backbuffer, then paints the foreground on top.

## Text

Text goes through its own primitive path:

1. **Font loading.** TTF / OTF parsed via rustybuzz.
2. **Shaping.** HarfBuzz-compatible shaping for complex scripts.
3. **Atlas rasterisation.** Glyphs rendered into a texture atlas.
4. **Emission.** Each glyph becomes a `PrimitiveType::Text` with atlas UV
   coordinates and colour.
5. **Render.** Text shader samples the atlas with antialiased coverage.

Text inherits CSS transforms from ancestor divs via an
`inherited_css_affine` threaded through the collect pass, then routed
through the SDF pipeline (not the glyph pipeline) so rotations stay
correct.

## Batching & instancing

Primitives are grouped by pipeline and dispatched with GPU instancing: one
draw call per pipeline per layer.

```rust
batch.add_primitive(rect1);
batch.add_primitive(rect2);
batch.add_primitive(rect3);
// single instanced draw for all three
```

Approximate soft caps:

| Resource         | Soft cap          |
|------------------|-------------------|
| SDF primitives   | ~10 000 / batch   |
| Glass primitives | ~1 000 / batch    |
| Glyphs           | ~50 000 / batch   |

## MSAA

Configurable 1× / 2× / 4×, resolved during composite. The MSAA fast path
honours per-layer effects so a blurred layer correctly composes through
the resolve.

## What goes through the slow path

A few cases bypass the cache and require a full repaint:

- Layout-affecting CSS changes (width, padding, gap, flex direction).
- Scroll physics that move the visible window.
- Bounds, clip, or size animations.
- Overlay open / close transitions.
- Canvas closures (called every frame by contract).
- Structural rebuilds from stateful flips or signal-driven subtree changes.

These show up in `frame_timing` traces as `did_rebuild=true` or
`needs_relayout=true`. Writing code that *avoids* the slow path is the
topic of the [Performance Tips](../advanced/performance.md) chapter.

## DrawContext

The bridge between layout (and canvas closures) and the GPU is the
`DrawContext` trait:

```rust
trait DrawContext {
    fn push_transform(&mut self, transform: Transform);
    fn pop_transform(&mut self);

    fn push_opacity(&mut self, opacity: f32);
    fn push_clip(&mut self, shape: ClipShape);

    fn fill_rect(&mut self, rect: Rect, corner_radius: CornerRadius, brush: Brush);
    fn stroke_rect(&mut self, rect: Rect, corner_radius: CornerRadius, stroke: &Stroke, brush: Brush);
    fn fill_circle(&mut self, center: Point, radius: f32, brush: Brush);
    fn draw_shadow(&mut self, rect: Rect, corner_radius: CornerRadius, shadow: Shadow);
    fn draw_text(&mut self, text: &str, origin: Point, style: &TextStyle);

    fn sdf_build(&mut self, f: &mut dyn FnMut(&mut dyn SdfBuilder));

    fn push_layer(&mut self, config: LayerConfig);
    fn pop_layer(&mut self);

    // … 3D, dimension bridging, etc.
}
```

Three concrete impls cover every use case:

- **`RecordingContext`.** Captures `DrawCommand`s for deferred replay.
  Used by canvases.
- **`GpuPaintContext`.** Emits `GpuPrimitive`s into a batch. Used by the
  internal renderers.
- **`PaintContext`.** Canvas-style convenience API wrapping
  `RecordingContext`.

The render-tree traversal calls `DrawContext` methods, which accumulate
GPU primitives for the render passes.
