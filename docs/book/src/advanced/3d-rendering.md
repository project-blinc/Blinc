# 3D Rendering

Blinc provides a GPU-accelerated 3D mesh rendering pipeline alongside its 2D UI. You can render PBR-lit meshes with shadow mapping, normal maps, skeletal animation, and custom shader passes — all within the same frame as your UI elements.

## Mesh Data

The interchange format for 3D geometry is `MeshData`. Users convert from any source format (glTF, OBJ, FBX, procedural) into this struct.

```rust
use blinc::core::{MeshData, Vertex, Material, Mat4};
use std::sync::Arc;

let mesh = MeshData {
    vertices: Arc::new(vec![
        Vertex::new([-0.5, -0.5, 0.0])
            .with_normal([0.0, 0.0, 1.0])
            .with_uv([0.0, 0.0])
            .with_color([1.0, 0.0, 0.0, 1.0]),
        Vertex::new([0.5, -0.5, 0.0])
            .with_normal([0.0, 0.0, 1.0])
            .with_uv([1.0, 0.0])
            .with_color([0.0, 1.0, 0.0, 1.0]),
        Vertex::new([0.0, 0.5, 0.0])
            .with_normal([0.0, 0.0, 1.0])
            .with_uv([0.5, 1.0])
            .with_color([0.0, 0.0, 1.0, 1.0]),
    ]),
    indices: Arc::new(vec![0, 1, 2]),
    material: Material::default(),
    skin: None,
    morph_targets: Arc::new(Vec::new()),
    morph_weights: Vec::new(),
};
```

### Vertex Format

Each vertex contains:

| Field | Type | Description |
|-------|------|-------------|
| `position` | `[f32; 3]` | XYZ world position |
| `normal` | `[f32; 3]` | Surface normal (for lighting) |
| `uv` | `[f32; 2]` | Texture coordinates |
| `color` | `[f32; 4]` | Per-vertex RGBA color |
| `tangent` | `[f32; 4]` | Tangent vector for normal mapping (xyz + handedness) |
| `joints` | `[u32; 4]` | Bone indices for skeletal animation |
| `weights` | `[f32; 4]` | Bone weights (should sum to 1.0) |

Builder methods chain naturally:

```rust
Vertex::new([0.0, 1.0, 0.0])
    .with_normal([0.0, 1.0, 0.0])
    .with_uv([0.5, 0.5])
    .with_tangent([1.0, 0.0, 0.0, 1.0])
    .with_joints([0, 1, 0, 0], [0.7, 0.3, 0.0, 0.0])
```

## Materials

The `Material` struct controls PBR shading:

```rust
use blinc::core::{Material, TextureData, AlphaMode};

let material = Material {
    base_color: [0.8, 0.2, 0.1, 1.0],  // Red-ish
    metallic: 0.0,                        // Dielectric
    roughness: 0.5,                       // Medium roughness
    emissive: [0.0, 0.0, 0.0],          // No emission
    base_color_texture: None,             // Or Some(TextureData { rgba, width, height })
    normal_map: None,                     // Tangent-space normal map
    normal_scale: 1.0,                    // Normal map strength
    displacement_map: None,               // Height map for parallax
    displacement_scale: 0.05,             // Displacement depth
    unlit: false,                         // true = skip lighting
    alpha_mode: AlphaMode::Opaque,
    receives_shadows: true,
    casts_shadows: true,
};
```

### Textures

Provide texture data as raw RGBA pixels:

```rust
let texture = TextureData {
    rgba: my_image_bytes,  // Vec<u8>, 4 bytes per pixel
    width: 512,
    height: 512,
};

let material = Material {
    base_color_texture: Some(texture),
    ..Material::default()
};
```

### Normal Mapping

Normal maps add surface detail without extra geometry. The shader uses the vertex tangent and bitangent to transform tangent-space normals to world space.

```rust
let material = Material {
    normal_map: Some(TextureData {
        rgba: normal_map_pixels,
        width: 1024,
        height: 1024,
    }),
    normal_scale: 1.5,  // Exaggerate the effect
    ..Material::default()
};
```

### Parallax Displacement

Height maps create the illusion of depth through parallax occlusion mapping (16-layer raymarching in the fragment shader):

```rust
let material = Material {
    displacement_map: Some(TextureData {
        rgba: height_map_pixels,  // Grayscale encoded as RGBA
        width: 512,
        height: 512,
    }),
    displacement_scale: 0.1,  // World-space depth
    ..Material::default()
};
```

### Alpha Modes and Transparency

Every material declares one of three `AlphaMode`s:

| Mode | Depth write | Blend | Use for |
|------|-------------|-------|---------|
| `Opaque` | yes | replace | solid surfaces (default) |
| `Mask` | yes | alpha-test (discard below `alpha_cutoff`) | hard cutouts — foliage, hair strands, decals with binary alpha |
| `Blend` | no | weighted blended OIT (see below) | genuine translucency — glass, smoke, soft edges |

**OIT, not back-to-front sort.** `AlphaMode::Blend` routes through Weighted Blended OIT (McGuire & Bavoil 2013). Every BLEND fragment writes into an accumulation texture and a transmission texture, and a composite pass divides-and-blends the result over the opaque HDR buffer at end of frame. **Callers don't need to sort meshes back-to-front** — the renderer handles overlapping BLEND layers statistically.

**Submission order doesn't matter at the API boundary.** `dispatch_pending_meshes` stable-sorts OPAQUE + MASK before BLEND before handing to the renderer, so you can call `draw_mesh_data` in scene-graph order. (This matters because WBOIT requires every opaque depth to be written before any BLEND fragment runs its depth test — otherwise BLEND pixels that should be occluded by a later-dispatched opaque mesh leak into the composite. The framework sort is what lets you ignore this invariant.)

**glTF loader auto-demotes misflagged BLEND.** Many DCC exporters flag every material as BLEND by default. `blinc_gltf::parse_material` analyses each base-color texture's alpha histogram on load:

| Texture profile | Demoted to | Reason |
|-----------------|------------|--------|
| ≥95% texels at α ≥ 0.95 | `Opaque` | dense coverage, no meaningful translucency |
| ≥99% texels at α ≤ 0.05 or α ≥ 0.95, <1% midrange | `Mask` | strict binary cutout |
| anything else | stays `Blend` | genuine partial alpha |

Decisions log at `info` level — run any demo with `RUST_LOG=blinc_gltf=info` to see per-material `authored=Blend resolved=Opaque` lines. A material whose BLEND looks wrong usually means either the heuristic matched poorly (report it) or the asset really is authored that way.

**OIT's limitation.** WBOIT approximates a weighted average of overlapping BLEND fragments — it can't perfectly resolve stacked translucent layers at the same depth. In practice this is invisible for single-layer BLEND (most assets) and for sparse translucent overlays (eyelashes, tearlines, decals). Dense BLEND stacks (e.g. foliage with many overlapping leaves) may look slightly washed compared to correct back-to-front sorting; moving such assets to `Mask` when the alpha is binary fixes it.

## Drawing in Canvas

Use `draw_mesh_data` on the `DrawContext` inside a canvas element:

```rust
canvas(|ctx: &mut dyn DrawContext, bounds| {
    ctx.draw_mesh_data(&mesh, Mat4::IDENTITY);
})
.w(800.0)
.h(600.0)
```

The `Mat4` transform positions the mesh in the scene. The renderer handles vertex/index buffer upload and PBR shading automatically.

## Shadow Mapping

The mesh pipeline includes a shadow depth pass. When rendering via `GpuRenderer::render_mesh_data()`, pass a `light_view_proj` matrix to enable shadows:

```rust
// Orthographic light projection for directional shadows
let light_view_proj: [f32; 16] = compute_light_matrix(light_dir, scene_bounds);

renderer.render_mesh_data(
    &target_view,
    &mesh,
    &model_matrix,
    &view_proj,
    camera_pos,
    light_dir,
    1.0,                          // light intensity
    Some(&light_view_proj),       // enables shadow pass
);
```

The shadow system uses:

- **2048x2048 depth texture** (Depth32Float)
- **Front-face culling** in shadow pass (reduces shadow acne)
- **Depth bias** (constant=2, slope_scale=2.0) for further acne reduction
- **4-tap PCF** sampling for soft shadow edges

Materials control shadow behavior per-mesh:

```rust
let floor = Material {
    receives_shadows: true,   // Shadows appear on this surface
    casts_shadows: false,     // This mesh doesn't cast shadows
    ..Material::default()
};
```

## Skeletal Animation

Animate meshes with bone transforms. The GPU applies per-vertex skinning using up to 4 joint influences.

### Skeleton Definition

```rust
use blinc::core::{Bone, Skeleton, SkinningData};

let skeleton = Skeleton {
    bones: vec![
        Bone {
            name: "Root".into(),
            parent: None,
            inverse_bind_matrix: identity_matrix(),
        },
        Bone {
            name: "UpperArm".into(),
            parent: Some(0),
            inverse_bind_matrix: upper_arm_ibm,
        },
        Bone {
            name: "LowerArm".into(),
            parent: Some(1),
            inverse_bind_matrix: lower_arm_ibm,
        },
    ],
};
```

### Per-Frame Skinning

Each frame, compute the joint matrices and attach them to the mesh:

```rust
// joint_matrix[i] = current_world_transform[i] * inverse_bind_matrix[i]
let joint_matrices: Vec<[f32; 16]> = skeleton.bones.iter()
    .enumerate()
    .map(|(i, bone)| {
        multiply_mat4(&animated_world_transforms[i], &bone.inverse_bind_matrix)
    })
    .collect();

let mesh = MeshData {
    vertices: Arc::new(skinned_vertices),  // vertices with .joints and .weights set
    indices: Arc::new(indices),
    material: Material::default(),
    skin: Some(SkinningData { joint_matrices }),
    morph_targets: Arc::new(Vec::new()),
    morph_weights: Vec::new(),
};
```

### Vertex Skinning

Vertices reference bones by index:

```rust
Vertex::new([0.0, 1.0, 0.0])
    .with_joints(
        [0, 1, 0, 0],       // bone indices
        [0.6, 0.4, 0.0, 0.0] // weights (sum to 1.0)
    )
```

The GPU vertex shader computes:

```
skin_matrix = joint[0] * w0 + joint[1] * w1 + joint[2] * w2 + joint[3] * w3
position = skin_matrix * vertex_position
normal = skin_matrix * vertex_normal
```

Maximum **256 joints** per mesh, stored in a GPU storage buffer.

## Custom Render Passes

Inject your own GPU render passes into the pipeline. Passes execute at specific stages — before UI rendering (`PreRender`) or after (`PostProcess`).

### Basic Custom Pass

```rust
use blinc::gpu::{CustomRenderPass, RenderPassContext, RenderStage};

struct SkyboxPass {
    pipeline: Option<wgpu::RenderPipeline>,
}

impl CustomRenderPass for SkyboxPass {
    fn label(&self) -> &str { "skybox" }
    fn stage(&self) -> RenderStage { RenderStage::PreRender }

    fn initialize(&mut self, device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) {
        // Create your render pipeline, bind groups, etc.
    }

    fn render(&mut self, ctx: &RenderPassContext) {
        let mut encoder = ctx.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Skybox"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: ctx.target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            // Draw skybox...
        }
        ctx.queue.submit(std::iter::once(encoder.finish()));
    }
}

// Register with the renderer
renderer.register_custom_pass(Box::new(SkyboxPass { pipeline: None }));
```

### Render Stages

| Stage | When | Use Cases |
|-------|------|-----------|
| `PreRender` | Before UI primitives | Skyboxes, 3D scene backgrounds, grid overlays |
| `PostProcess` | After all UI rendering | Bloom, tone mapping, FXAA, vignette, debug overlays |

## Custom Bind Groups

The `BindGroupBuilder` creates matched layout + bind group pairs:

```rust
use blinc::gpu::BindGroupBuilder;

let mut builder = BindGroupBuilder::new("my_effect");
builder.add_uniform_buffer(uniforms_buffer.as_entire_binding());
builder.add_texture(&my_texture_view);
builder.add_sampler(&my_sampler);
builder.add_storage_buffer(data_buffer.as_entire_binding(), true); // read-only

let (layout, bind_group) = builder.build(device);
```

Supported binding types:

| Method | Shader Type | Notes |
|--------|-------------|-------|
| `add_uniform_buffer()` | `var<uniform>` | Per-frame data (transforms, time, etc.) |
| `add_storage_buffer(_, read_only)` | `var<storage>` | Large data arrays, particle buffers |
| `add_texture()` | `texture_2d<f32>` | Sampled textures (filterable) |
| `add_storage_texture()` | `texture_storage_2d` | Compute write targets |
| `add_sampler()` | `sampler` | Filtering sampler |
| `add_comparison_sampler()` | `sampler_comparison` | Shadow map sampling |

## Compute Shaders

Execute compute shaders for simulation, particle updates, or data processing:

```rust
use blinc::gpu::{create_compute_pipeline, ComputeDispatch, BindGroupBuilder};

// Create pipeline from WGSL
let pipeline = create_compute_pipeline(
    device,
    "particle_sim",
    include_str!("shaders/particle_sim.wgsl"),
    "cs_main",
    &bind_group_layout,
);

// Dispatch
let dispatch = ComputeDispatch {
    pipeline: &pipeline,
    bind_group: &bind_group,
    workgroups: (particle_count / 64, 1, 1),
    label: "particle_sim",
};
dispatch.execute(device, queue);
```

## Post-Processing Chain

Chain multiple screen-space effects with automatic ping-pong texture management:

```rust
use blinc::gpu::{PostProcessChain, PostProcessEffect};

struct BloomEffect { /* ... */ }

impl PostProcessEffect for BloomEffect {
    fn label(&self) -> &str { "bloom" }

    fn initialize(&mut self, device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) {
        // Create bloom pipeline, intermediate textures, etc.
    }

    fn apply(&mut self, device: &wgpu::Device, queue: &wgpu::Queue,
             input: &wgpu::TextureView, output: &wgpu::TextureView,
             width: u32, height: u32) {
        // Read from input, write bloom result to output
    }
}

// Build a chain
let mut chain = PostProcessChain::new("my_effects");
chain.add_effect(Box::new(BloomEffect::new()));
chain.add_effect(Box::new(ToneMappingEffect::new()));

// Register as a custom pass (runs at PostProcess stage)
renderer.register_custom_pass(Box::new(chain));
```

The chain automatically:
- Copies the framebuffer to a ping texture
- Chains effects: ping → pong → ping → ... → framebuffer
- Manages texture lifetimes and resizing
- Skips disabled effects

## Flow Shader Integration

The [flow shader system](./flow-shaders.md) extends beyond 2D effects — it is a general-purpose DAG compute system that compiles to WGSL for any target. For 3D rendering, use `vertex` and `material` flow targets.

### Declarative Vertex Shader

Instead of writing raw WGSL, define vertex transforms as a flow DAG:

```rust
use blinc::layout::flow;

let wave_vertex = flow!(wave_vertex, vertex, {
    input pos: builtin(vertex_position);
    input normal: builtin(vertex_normal);
    input model: builtin(model_matrix);
    input vp: builtin(view_proj);
    input time: builtin(time);

    node wave = sin(pos.x * 4.0 + time * 2.0) * 0.1;
    node deformed = vec3(pos.x, pos.y + wave, pos.z);
    node world = mat4_mul_vec4(model, vec4(deformed.x, deformed.y, deformed.z, 1.0));

    output position = mat4_mul_vec4(vp, world);
    output world_normal = transform_normal(model, normal);
    output world_position = world.xyz;
});
```

### Declarative Material Shader

Define PBR surface properties — the flow compiler injects Blinn-Phong + Fresnel evaluation automatically:

```rust
let terrain_mat = flow!(terrain_mat, material, {
    input uv: builtin(uv);
    input normal: builtin(world_normal);

    node height = fbm(uv * 10.0, 6);
    node grass = vec4(0.2, 0.6, 0.1, 1.0);
    node rock = vec4(0.5, 0.45, 0.4, 1.0);

    output albedo = mix(rock, grass, smoothstep(0.3, 0.6, height));
    output roughness = mix(0.8, 0.4, height);
    output metallic = 0.0;
});
```

### CSS-Defined 3D Shaders

Flow shaders for 3D work in CSS stylesheets too:

```css
@flow ocean_vertex {
    target: vertex;
    input pos: builtin(position);
    input model: builtin(model);
    input vp: builtin(view_proj);
    input time: builtin(time);

    node wave = sin(pos.x * 3.0 + time) * 0.2 + sin(pos.z * 2.0 + time * 1.3) * 0.15;
    node displaced = vec3(pos.x, pos.y + wave, pos.z);
    node world = mat4_mul_vec4(model, vec4(displaced.x, displaced.y, displaced.z, 1.0));

    output position = mat4_mul_vec4(vp, world);
    output world_position = world.xyz;
}
```

> **Tip:** See the [Flow Shaders](./flow-shaders.md) chapter for the complete function reference, semantic steps, chains, and composition with `use`.

## Raw Pixel Drawing

For video frames, camera previews, or procedural textures, use `draw_rgba_pixels`:

```rust
canvas(|ctx: &mut dyn DrawContext, bounds| {
    // Upload and render RGBA pixel data in one call
    ctx.draw_rgba_pixels(
        &rgba_data,     // &[u8], 4 bytes per pixel
        width,          // u32
        height,         // u32
        Rect::new(0.0, 0.0, bounds.width, bounds.height),
    );
})
.w(640.0)
.h(480.0)
```

This creates a GPU texture each frame — ideal for dynamic content like video playback or camera streams.

## GPU Memory Budget

The renderer tracks GPU texture memory and enforces a configurable budget:

```rust
// Default: 128 MB, override with BLINC_GPU_MEMORY_BUDGET_MB env var
let config = RendererConfig {
    gpu_memory_budget: 256 * 1024 * 1024,  // 256 MB
    ..RendererConfig::default()
};
```

Call `renderer.enforce_memory_budget()` once per frame to evict cached textures when over budget. Eviction is largest-first (XLarge pool textures → mask image cache).

## Architecture

The 3D rendering pipeline sits alongside the 2D SDF pipeline:

```
Frame
 ├── PreRender custom passes (skybox, 3D scene)
 ├── UI Rendering
 │   ├── SDF primitives (2D shapes)
 │   ├── Glass / vibrancy effects
 │   ├── Text glyphs
 │   ├── Canvas callbacks → draw_mesh_data / draw_rgba_pixels
 │   └── Layer effects (blur, shadow, glow)
 ├── PostProcess custom passes (bloom, tone mapping)
 └── Memory budget enforcement
```

The mesh pipeline (`MeshPipeline`) is lazily created on first use and includes:
- Main PBR render pipeline with normal/displacement/shadow support
- Shadow depth pass pipeline (Depth32Float, front-face culling)
- Default textures (white, flat normal, black displacement)
- Joint matrix storage buffer (for skeletal animation)
- Comparison sampler (for PCF shadow sampling)

> **Tip:** For static 3D scenes, render to an offscreen texture once, then display it as an image. Only re-render when the camera or scene changes.
