# Flow Shaders

Flow shaders are a DAG-based (directed acyclic graph) real-time shader compute system that compiles to WGSL. They support fragment, compute, vertex, and material targets — powering 2D effects, GPU simulation, and 3D mesh rendering from a single declarative language. Flows can be defined in CSS stylesheets or directly in Rust using the `flow!` macro.

## Quick Start

The fastest way to add a flow shader to an element:

```rust
use blinc_layout::flow;

let ripple = flow!(ripple, fragment, {
    input uv: builtin(uv);
    input time: builtin(time);
    node d = distance(uv, vec2(0.5, 0.5));
    node wave = sin(d * 20.0 - time * 4.0) * 0.5 + 0.5;
    output color = vec4(wave, wave, wave, 1.0);
});

div().flow(ripple).w(400.0).h(400.0)
```

The `flow!` macro produces a `FlowGraph` using Rust identifiers and primitives. Pass it directly to any element via `.flow()`.

## Anatomy of a Flow Shader

Every flow shader has a **name**, a **target**, and a body of declarations:

```
@flow <name> {
    target: fragment | compute | vertex | material;

    input <name>: builtin(<variable>);    // Input declarations
    step <name>: <step-type> { ... };     // Semantic steps (high-level)
    node <name> = <expression>;           // Raw computation nodes
    chain <name>: <step> | <step> | ...;  // Piped step chains
    use <flow-name>;                      // Compose other flows
    output <target> = <expression>;       // Output declarations
}
```

Declarations can appear in any order, but each node can only reference inputs and earlier nodes (the graph must be acyclic).

### Targets

| Target | Use Case | Output |
|--------|----------|--------|
| `fragment` | 2D visual effects on UI elements | `color` (vec4) |
| `compute` | GPU simulation, data processing | Named buffer writes |
| `vertex` | 3D mesh vertex transformation | `position` (vec4 clip-space) |
| `material` | 3D mesh surface/PBR shading | `albedo`, `metallic`, `roughness`, etc. |

## Builtin Variables

### Fragment / Compute Builtins

| Variable | Type | Description |
|----------|------|-------------|
| `uv` | `vec2` | Normalized element coordinates (0,0 = top-left, 1,1 = bottom-right) |
| `time` | `float` | Elapsed time in seconds (monotonic) |
| `resolution` | `vec2` | Element size in physical pixels |
| `pointer` | `vec2` | Cursor position relative to element (0-1 range) |
| `sdf` | `float` | Signed distance field value at the current fragment |
| `frame_index` | `float` | Current frame number |

### Vertex Target Builtins

| Variable | Type | Description |
|----------|------|-------------|
| `vertex_position` / `position` | `vec3` | Vertex position in model space |
| `vertex_normal` / `normal` | `vec3` | Vertex normal in model space |
| `vertex_tangent` / `tangent` | `vec4` | Tangent (xyz = dir, w = handedness) |
| `vertex_color` | `vec4` | Per-vertex color |
| `joints` | `vec4<u32>` | Joint indices for skeletal animation |
| `weights` | `vec4` | Joint weights |
| `vertex_index` | `float` | Vertex/instance index |
| `model_matrix` / `model` | `mat4` | Model-to-world transform |
| `view_proj` / `view_projection` | `mat4` | View-projection matrix |

### Material Target Builtins

| Variable | Type | Description |
|----------|------|-------------|
| `world_position` / `world_pos` | `vec3` | Interpolated world-space position |
| `world_normal` | `vec3` | Interpolated world-space normal |
| `world_tangent` | `vec3` | Interpolated world-space tangent |
| `tangent_handedness` | `float` | Tangent handedness (±1) |
| `camera_position` / `camera_pos` | `vec3` | Camera position in world space |
| `light_direction` / `light_dir` | `vec3` | Directional light direction |
| `light_intensity` | `float` | Light intensity |
| `uv` | `vec2` | Texture coordinates (also available in material) |
| `time` | `float` | Frame time (also available in material) |

## Expressions

Flow expressions support standard arithmetic, vector constructors, function calls, and swizzle access:

```
node a = sin(uv.x * 10.0 + time);
node b = vec4(a, a * 0.5, 1.0 - a, 1.0);
node c = mix(b, vec4(1.0, 0.0, 0.0, 1.0), 0.5);
node d = c.rgb;
```

### Operators

| Operator | Example |
|----------|---------|
| `+`, `-`, `*`, `/` | `a * 2.0 + b` |
| Unary `-` | `-a` |
| Swizzle | `v.xy`, `v.rgb`, `v.x` |

### Functions Reference

**Math (scalar)**

| Function | Signature | Description |
|----------|-----------|-------------|
| `sin`, `cos`, `tan` | `f32 -> f32` | Trigonometric |
| `abs`, `floor`, `ceil`, `fract` | `f32 -> f32` | Rounding / absolute |
| `sqrt`, `exp`, `log`, `sign` | `f32 -> f32` | Algebraic |
| `pow` | `(f32, f32) -> f32` | Power |
| `atan2` | `(f32, f32) -> f32` | Arc tangent |
| `mod` | `(f32, f32) -> f32` | Modulus |
| `min`, `max` | `(f32, f32) -> f32` | Comparative |
| `clamp` | `(f32, f32, f32) -> f32` | Clamp to range |
| `mix` | `(f32, f32, f32) -> f32` | Linear interpolation |
| `smoothstep` | `(f32, f32, f32) -> f32` | Smooth Hermite |
| `step` | `(f32, f32) -> f32` | Step function |

**Vector**

| Function | Description |
|----------|-------------|
| `length(v)` | Vector magnitude |
| `distance(a, b)` | Distance between two points |
| `dot(a, b)` | Dot product |
| `cross(a, b)` | Cross product (vec3) |
| `normalize(v)` | Unit vector |
| `reflect(v, n)` | Reflection |

**Noise**

| Function | Signature | Description |
|----------|-----------|-------------|
| `fbm(p, octaves)` | `(vec2, i32) -> f32` | Fractal Brownian motion |
| `fbm_ex(p, octaves, persistence)` | `(vec2, i32, f32) -> f32` | FBM with custom persistence |
| `worley(p)` | `vec2 -> f32` | Worley/cellular noise |
| `worley_grad(p)` | `vec2 -> vec3` | Worley with analytic gradient (x=dist, y=gx, z=gy) |
| `checkerboard(p, scale)` | `(vec2, f32) -> f32` | Checkerboard pattern |

**SDF Primitives**

| Function | Description |
|----------|-------------|
| `sdf_box(p, half_size)` | Box SDF |
| `sdf_circle(p, radius)` | Circle SDF |
| `sdf_ellipse(p, radii)` | Ellipse SDF |
| `sdf_round_rect(p, half_size, radius)` | Rounded rectangle SDF |

**SDF Combinators**

| Function | Description |
|----------|-------------|
| `sdf_union(a, b)` | Union of two SDFs |
| `sdf_intersect(a, b)` | Intersection |
| `sdf_subtract(a, b)` | Subtraction |
| `sdf_smooth_union(a, b, k)` | Smooth union with radius k |
| `sdf_smooth_intersect(a, b, k)` | Smooth intersection |
| `sdf_smooth_subtract(a, b, k)` | Smooth subtraction |

**Lighting**

| Function | Description |
|----------|-------------|
| `phong(normal, light_dir, view_dir, shininess)` | Phong shading |
| `blinn_phong(normal, light_dir, view_dir, shininess)` | Blinn-Phong shading |

**Matrix (for vertex/material targets)**

| Function | Signature | Description |
|----------|-----------|-------------|
| `mat4_mul_vec4(m, v)` | `(mat4, vec4) -> vec4` | Matrix-vector multiply |
| `mat4_mul(a, b)` | `(mat4, mat4) -> mat4` | Matrix-matrix multiply |
| `mat4_inverse(m)` / `inverse(m)` | `mat4 -> mat4` | Matrix inverse |
| `mat4_transpose(m)` / `transpose(m)` | `mat4 -> mat4` | Matrix transpose |
| `transform_normal(model, n)` | `(mat4, vec3) -> vec3` | Transform normal by model matrix (3x3 extract) |
| `translation_matrix(v)` | `vec3 -> mat4` | Translation matrix from offset |
| `rotation_matrix(axis, angle)` | `(vec3, float) -> mat4` | Rotation from axis + angle |
| `scale_matrix(v)` | `vec3 -> mat4` | Scale matrix from factors |
| `perspective(fov, aspect, near, far)` | `(f, f, f, f) -> mat4` | Perspective projection |
| `look_at(eye, target, up)` | `(vec3, vec3, vec3) -> mat4` | View matrix |
| `sample_texture(id, uv)` | `(float, vec2) -> vec4` | Sample a bound texture at UV |

**Scene**

| Function | Description |
|----------|-------------|
| `sample_scene(uv)` | Sample the background behind this element (for refraction/glass effects) |

## Semantic Steps

Steps are high-level operations that expand to multiple nodes automatically. They provide a more declarative way to build shader effects.

### Pattern Steps

Generate procedural textures. Output type: `float` (scalar field).

| Step Type | Key Parameters | Description |
|-----------|---------------|-------------|
| `pattern_noise` | `scale`, `detail`, `animation` | FBM noise pattern |
| `pattern_worley` | `scale`, `threshold`, `edge`, `mask`, `gradient` | Worley cellular pattern with analytic gradient |
| `pattern_ripple` | `center`, `density`, `speed` | Concentric ripple rings |
| `pattern_waves` | `direction`, `frequency`, `speed` | Directional sine waves |
| `pattern_grid` | `scale`, `line_width` | Grid lines |
| `pattern_gradient` | `direction`, `start`, `end` | Linear gradient (output: vec4) |
| `pattern_plasma` | `scale`, `speed` | Plasma texture (output: vec4) |

### Effect Steps

Post-processing effects that modify appearance.

| Step Type | Key Parameters | Description |
|-----------|---------------|-------------|
| `effect_refract` | `source`, `strength` | Lens refraction via Worley gradient |
| `effect_frost` | `source`, `strength`, `detail` | Frosted glass UV jitter |
| `effect_specular` | `source`, `intensity`, `power` | Specular highlight scattering |
| `effect_fog` | `density`, `source` | Fog/haze composite |
| `effect_light` | `source`, `direction`, `intensity`, `power` | Directional highlights from normals |

### Transform Steps

Spatial coordinate transformations. Output type: `vec2` (UV coordinate) or `float`.

| Step Type | Key Parameters | Description |
|-----------|---------------|-------------|
| `transform_wet` | `aspect`, `scroll_speed`, `offset` | Aspect-corrected gravity scroll (for rain/drip effects) |
| `transform_warp` | `source`, `amount` | Warp UV by a noise field |
| `transform_rotate` | `angle` | Rotate UV coordinates |
| `transform_scale` | `factor` | Scale UV coordinates |
| `transform_tile` | `count` | Tile/repeat UV |
| `transform_mirror` | `axis` | Mirror UV |
| `transform_polar` | `center` | Cartesian to polar coordinates |

### Color Steps

Map scalar values to colors. Output type: `vec4`.

| Step Type | Key Parameters | Description |
|-----------|---------------|-------------|
| `color_ramp` | `source`, `stops`, `opacity` | Map scalar to color gradient |
| `color_shift` | `source`, `hue` | Hue shift |
| `color_tint` | `source`, `color` | Color tinting |
| `color_invert` | `source` | Color inversion |

### Composition Steps

Combine two sources. Output type: `vec4`.

| Step Type | Key Parameters | Description |
|-----------|---------------|-------------|
| `compose_blend` | `a`, `b`, `mode` | Blend two layers (screen, multiply, overlay, etc.) |
| `compose_mask` | `source`, `mask` | Alpha mask one input by another |
| `compose_layer` | `base`, `overlay`, `opacity` | Stack with opacity |

### Adjust Steps

Value curve shaping. Output type: `float`.

| Step Type | Key Parameters | Description |
|-----------|---------------|-------------|
| `adjust_falloff` | `radius`, `center` | Distance-based fade |
| `adjust_remap` | `source`, `in_min`, `in_max`, `out_min`, `out_max` | Remap value range |
| `adjust_threshold` | `source`, `value` | Hard threshold |
| `adjust_ease` | `source`, `curve` | Apply easing curve |
| `adjust_clamp` | `source`, `min`, `max` | Clamp value range |

## Chains

Chains pipe the output of one step into the next, creating a processing pipeline:

```
chain effect:
    pattern_ripple(center: vec2(0.5, 0.5), density: 25.0)
    | adjust_falloff(radius: 0.5)
    ;
```

Each link in the chain implicitly receives the previous link's output as its `source` parameter.

## Flow Composition with `use`

Flows can import nodes from other flows using `use`:

```
@flow base_noise {
    target: fragment;
    input uv: builtin(uv);
    node n = fbm(uv * 4.0, 6);
    output color = vec4(n, n, n, 1.0);
}

@flow enhanced {
    target: fragment;
    use base_noise;
    node bright = smoothstep(0.3, 0.7, n);
    output color = vec4(bright, bright * 0.5, 0.1, 1.0);
}
```

The `use` directive imports all nodes from the referenced flow into the current graph.

## Scene Sampling

For glass, refraction, or frosted effects, use `sample_scene()` to read the rendered background behind the element:

```rust
let glass = flow!(glass, fragment, {
    input uv: builtin(uv);
    input time: builtin(time);
    node offset = fbm(uv * 8.0 + vec2(time * 0.1, 0.0), 3) * 0.02;
    node scene = sample_scene(uv + vec2(offset, offset));
    output color = scene;
});
```

The scene texture is automatically captured before flow rendering begins. Elements using `sample_scene()` see everything rendered behind them.

## Applying Flow Shaders

There are three ways to apply flow shaders to elements:

### 1. `flow!` Macro (Recommended)

Define the shader in Rust and pass it directly to the element:

```rust
use blinc_layout::flow;

let shader = flow!(my_effect, fragment, {
    input uv: builtin(uv);
    input time: builtin(time);
    node wave = sin(uv.x * 10.0 + time) * 0.5 + 0.5;
    output color = vec4(wave, 0.2, 0.5, 1.0);
});

div().flow(shader).w(300.0).h(300.0)
```

The `FlowGraph` carries its own name and is auto-persisted by the GPU pipeline cache.

### 2. CSS Stylesheet

Define flows in CSS and reference them by name:

```rust
ctx.add_css(r#"
    @flow terrain {
        target: fragment;
        input uv: builtin(uv);
        step noise: pattern-noise { scale: 4.0; detail: 6; };
        output color = vec4(noise, noise, noise, 1.0);
    }

    #my-element {
        flow: terrain;
        border-radius: 16px;
    }
"#);

div().id("my-element").w(300.0).h(300.0)
```

### 3. Style Macros

Reference CSS-defined flows from `css_style!` or `style!` macros:

```rust
let style = css_style! {
    flow: "terrain";
    border-radius: 16px;
};

// Or with style! macro:
let style = style! {
    flow: "terrain",
    corner_radius: 16.0,
};
```

### 4. Name Reference

Reference a previously-defined flow by name string:

```rust
div().flow("terrain").w(300.0).h(300.0)
```

## Complete Example

Here's the wet glass demo that creates a realistic rain-on-glass effect using semantic steps:

```rust
use blinc_layout::flow;

let wetglass = flow!(wetglass, fragment, {
    input uv: builtin(uv);
    input time: builtin(time);
    input resolution: builtin(resolution);

    // Gravity gradient: more moisture at bottom
    node grav = smoothstep(0.0, 1.0, uv.y);

    // Background mist
    step mist: pattern_noise { scale: 3.0; detail: 5; animation: time * 0.02; };
    node moist = mist * (0.35 + grav * 0.65);

    // Multi-scale water drops with aspect correction and gravity scroll
    step uv1: transform_wet { aspect: resolution; scroll_speed: 0.001; };
    step uv2: transform_wet { aspect: resolution; scroll_speed: 0.0015; offset: vec2(0.38, 0.21); };
    step uv3: transform_wet { aspect: resolution; scroll_speed: 0.002; offset: vec2(0.17, 0.63); };

    // Worley drops at different scales
    step drops1: pattern_worley { uv: uv1; scale: 7.0; threshold: 0.22; edge: 0.05; mask: step(0.3, moist); gradient: true; };
    step drops2: pattern_worley { uv: uv2; scale: 12.0; threshold: 0.18; edge: 0.04; mask: step(0.2, moist); gradient: true; };
    step drops3: pattern_worley { uv: uv3; scale: 20.0; threshold: 0.13; edge: 0.03; mask: step(0.12, moist); gradient: true; };

    // Combine drops
    node drops_raw = clamp(drops1 + drops2 * 0.6 + drops3 * 0.3, 0.0, 1.0);
    node drops = smoothstep(0.05, 0.4, drops_raw);

    // Specular highlights
    step highlight: effect_specular {
        sources: drops1 drops2 drops3;
        weights: 1.0 0.6 0.3;
        direction: vec2(0.7071068, 0.7071067);
        intensity: 0.25;
        power: 64.0;
    };

    // Fog and lens distortion
    node fog = (1.0 - drops) * (0.12 + mist * 0.05);
    step lens: effect_refract { source: drops; strength: 0.025; };

    // Sample background scene through distorted UVs
    node scene = sample_scene(uv + lens);

    // Composite
    node out_r = scene.x * (1.0 - fog) + fog + highlight;
    node out_g = scene.y * (1.0 - fog) + fog + highlight;
    node out_b = scene.z * (1.0 - fog) + fog + highlight;
    output color = vec4(out_r, out_g, out_b, 0.97);
});

div().flow(wetglass).w(800.0).h(600.0)
```

## Output Targets

Each flow target has specific output variables:

### Fragment Outputs

| Output | Type | Description |
|--------|------|-------------|
| `color` | `vec4` | Fragment color (required) |
| `alpha` | `float` | Override alpha channel |
| `displacement` | `float` | SDF displacement |

### Compute Outputs

| Output | Type | Description |
|--------|------|-------------|
| `<buffer>[idx]` | varies | Write to named storage buffer |

### Vertex Outputs

| Output | Type | Description |
|--------|------|-------------|
| `position` | `vec4` | Clip-space position (**required**) |
| `world_normal` | `vec3` | World-space normal to pass to material |
| `world_position` | `vec3` | World-space position to pass to material |

### Material Outputs

| Output | Type | Description |
|--------|------|-------------|
| `albedo` / `base_color` | `vec4` | Base color RGBA (**required**) |
| `metallic` | `float` | Metallic factor (0–1) |
| `roughness` | `float` | Roughness factor (0–1) |
| `emissive` | `vec3` | Emissive color |
| `surface_normal` | `vec3` | Overridden surface normal |
| `alpha_out` | `float` | Alpha override |

## 3D Flow Shaders

Flow shaders can drive 3D mesh rendering through `vertex` and `material` targets. These compile to vertex and fragment shaders that receive mesh geometry data and produce PBR-lit output.

### Vertex Shader Flow

Transform vertex positions, apply skeletal animation, or create procedural geometry:

```rust
let vertex_flow = flow!(custom_vertex, vertex, {
    input pos: builtin(vertex_position);
    input normal: builtin(vertex_normal);
    input model: builtin(model_matrix);
    input vp: builtin(view_proj);
    input time: builtin(time);

    // Wave deformation
    node wave = sin(pos.x * 4.0 + time * 2.0) * 0.1;
    node deformed = vec3(pos.x, pos.y + wave, pos.z);

    // Standard MVP transform
    node world = mat4_mul_vec4(model, vec4(deformed.x, deformed.y, deformed.z, 1.0));
    node clip = mat4_mul_vec4(vp, world);
    node w_normal = transform_normal(model, normal);

    output position = clip;
    output world_normal = w_normal;
    output world_position = world.xyz;
});
```

### Material Shader Flow

Define surface properties using the DAG — the PBR evaluation is done automatically:

```rust
let material_flow = flow!(pbr_material, material, {
    input uv: builtin(uv);
    input world_pos: builtin(world_position);
    input normal: builtin(world_normal);
    input time: builtin(time);

    // Procedural texture
    node noise = fbm(uv * 8.0, 4);
    node base = vec4(0.8 * noise, 0.3, 0.1, 1.0);

    // Metallic varies with noise
    node metal = smoothstep(0.4, 0.6, noise);

    output albedo = base;
    output metallic = metal;
    output roughness = 0.3;
    output emissive = vec3(0.0, 0.0, 0.0);
});
```

### CSS-Defined 3D Flows

3D flows work identically in CSS stylesheets:

```css
@flow terrain_vertex {
    target: vertex;
    input pos: builtin(vertex_position);
    input normal: builtin(vertex_normal);
    input model: builtin(model_matrix);
    input vp: builtin(view_proj);

    node world = mat4_mul_vec4(model, vec4(pos.x, pos.y, pos.z, 1.0));

    output position = mat4_mul_vec4(vp, world);
    output world_normal = transform_normal(model, normal);
    output world_position = world.xyz;
}

@flow terrain_material {
    target: material;
    input uv: builtin(uv);
    input normal: builtin(world_normal);

    node height = fbm(uv * 10.0, 6);
    node grass = vec4(0.2, 0.6, 0.1, 1.0);
    node rock = vec4(0.5, 0.45, 0.4, 1.0);
    node surface = mix(rock, grass, smoothstep(0.3, 0.6, height));

    output albedo = surface;
    output roughness = mix(0.8, 0.4, height);
}
```

### Compute → 3D Pipeline

Use `compute` flows to simulate particle systems, physics, or procedural geometry, then feed the storage buffer data into `MeshData`:

```rust
// Compute flow updates particle positions
let sim = flow!(particle_sim, compute, {
    input time: builtin(time);
    buffer positions: vec4 [read_write];
    node p = positions[idx];
    node new_y = p.y + sin(time + f32(idx) * 0.1) * 0.01;
    output positions[idx] = vec4(p.x, new_y, p.z, 1.0);
});
```

The compute output can be read back and used to construct `MeshData` vertices, or joint matrices for skeletal animation.

## Performance Tips

- **Analytic gradients**: `pattern_worley` with `gradient: true` uses `worley_grad()` which computes distance + gradient in a single 3x3 grid pass (5x faster than finite-difference).
- **Pipeline caching**: Compiled WGSL pipelines are cached by flow name in `FlowPipelineCache`. Reusing the same flow name across frames is free after first compile.
- **Scene copy**: `sample_scene()` triggers a single texture copy per frame (not per element). Multiple elements sharing a scene-sampling flow share the same copy.
- **Step expansion**: Semantic steps expand to optimized node graphs at parse time, not at render time. There's zero per-frame overhead from using steps vs raw nodes.
