# Changelog

All notable changes to `blinc_gpu` will be documented in this file.

## [Unreleased]

### Added
- Per-corner squircle / scoop / bevel support on `ClipShape::RoundedRect` propagated into the SDF clip primitive uniforms — all 12 SDF shader variants (`sdf_core` / `sdf_shadow` / `sdf_3d` / `sdf_notch` × `vb` / `dt` / msaa) now read and apply per-corner `n` when evaluating clip alpha.

### Fixed
- Box shadows in `sdf_shadow` (all three variants): a spread-only shadow now draws (the quad grows by the spread), an offset-only shadow with no blur and no spread now draws, a spread grows a corner only as far as it is already round (a square box keeps square shadow corners), and an unblurred outer or inset edge is antialiased over a pixel. A zero-blur inset shadow is now a crisp ring instead of a soft one.
- `pre_warm_pipelines` previously issued `pass.draw(...)` against the Path pipeline (and SDF pipelines on hosts without `VERTEX_STORAGE`) without first calling `set_vertex_buffer(0, ...)`. Both declare a vertex-buffer layout, so a missing slot 0 triggered the Vulkan validation panic `requires vertex buffer 0 to be set` on Linux at startup and prevented apps from launching. Binds a single-vertex dummy buffer for the warm-up draws.

### Changed
- `LayerTextureCache::evict_oversized` now also drives idle-frame eviction. After 60 frames without an `acquire()` it drops one texture per pool (largest first); after 120 it flushes all four pools. Reclaims GPU memory held by glass / blur intermediates when the UI is sitting still.

## [0.5.1] - 2026-04-13

### Changed
- Split monolithic SDF shader (~2400 lines) into 4 specialized pipelines: core (Rect/Circle/Ellipse), shadow, 3D raymarching, notch
- Back-to-front draw order for split pipelines preserving z-order
- Monolithic SDF_SHADER no longer compiled (split pipelines only)
- Non-sRGB surface format on WebGL2 GL adapter (avoids double gamma)
- Surface COPY_SRC conditionally dropped when unsupported
- Glass rendering skipped on WebGL2 (DT shader exists, per-frame plumbing pending)
- Particle rendering skipped on WebGL2 (no compute shader support)

### Added
- VERTEX_STORAGE fallback: instance-stepped vertex buffers when adapter lacks vertex storage
- WebGL2 data texture fallback: primitive/glyph/aux data packed into Rgba32Float textures via textureLoad, replacing all storage buffer reads
- Data texture variants for all shader types: SDF (core/shadow/3d/notch), text, glass, simple glass, mesh, particle render
- Early prim_type discard guards in all split shaders
- Frame generation counter for skip-redundant GPU uploads
- 28 shader parse tests (up from 16)

## [0.5.0] - 2026-04-10

### Fixed

- All WGSL shaders: `textureSample` → `textureSampleLevel` in non-uniform control flow paths for WebGPU compliance (glass, simple glass, image, composite, drop shadow, glow, path pipeline)
- Image shader: `fwidth` → constant 0.75px AA width (WebGPU rejects derivative functions from non-uniform flow)
- `device_required_limits`: wasm32 uses adapter's reported limits directly (`supported.clone()`) instead of requesting defaults that Safari/Firefox can't satisfy
- VERTEX_STORAGE capability check: descriptive error on browsers without storage buffer vertex access
- Monospace font fallback list: added "JetBrains Mono" and "Fira Code" to `find_generic_font_id` named fallbacks
- `preload_generic_styles` on `with_canvas` path for web font family resolution
- `TextRenderingContext::invalidate_generic_font_cache()` clears negative cache entries after font loads

## [0.4.0] - 2026-04-05

### Added

#### 3D Mesh Rendering Pipeline
- `MeshPipeline` with lazy initialization and PBR WGSL shader (`mesh.wgsl`)
- `render_mesh_data()` — vertex/index buffer upload, PBR shading, optional base color texture
- Blinn-Phong specular + Schlick Fresnel + Lambertian diffuse lighting model

#### Shadow Mapping
- 2048x2048 Depth32Float shadow map texture
- Shadow depth pass pipeline (`shadow.wgsl`) with front-face culling and depth bias
- 4-tap PCF soft shadow sampling in main fragment shader
- Per-material `receives_shadows` / `casts_shadows` control
- `light_view_proj` parameter for directional shadow projection

#### Normal Mapping & Parallax Displacement
- Tangent-space normal mapping with configurable `normal_scale`
- TBN matrix construction from vertex tangent + bitangent
- Parallax occlusion mapping: 16-layer raymarching with interpolated relief
- Per-material `displacement_scale` for height map depth

#### Skeletal Animation (GPU Skinning)
- Storage buffer (binding 8) for joint matrices (max 256 joints)
- Vertex shader computes weighted skin matrix from 4 joint influences
- Joints/weights vertex attributes (locations 5, 6)

#### Custom Render Pass API
- `CustomRenderPass` trait with `PreRender` / `PostProcess` stages
- `RenderPassContext` provides device, queue, target, viewport, format, scale
- `CustomPassManager` for registration, stage execution, resize notification, label-based removal
- `GpuRenderer::register_custom_pass()`, `remove_custom_pass()`, `execute_custom_passes()`

#### Custom Bind Groups
- `BindGroupBuilder` — declarative bind group + layout creation
- Supports uniform buffers, storage buffers (read/write), textures, storage textures, samplers, comparison samplers

#### Compute Dispatch
- `ComputeDispatch` struct for single-call compute execution
- `create_compute_pipeline()` — WGSL source to ComputePipeline convenience
- `create_buffer()` — initialized buffer creation helper

#### Post-Processing Pipeline
- `PostProcessEffect` trait: input → output texture effect interface
- `PostProcessChain` — ping-pong effect chaining as `CustomRenderPass`
- Auto-manages intermediate textures, fullscreen blit pipeline, resize
- `create_fullscreen_pipeline()` — WGSL to fullscreen quad RenderPipeline

#### Render Region Culling
- AABB visibility test before GPU buffer upload
- Conservative expansion for shadows, borders, rotation (half-diagonal), local affine
- 3D perspective primitives always pass (complex projection)
- Applied in both `render_with_clear_simple` and `render_primitives_excluding`

#### GPU Memory Budget
- `GpuMemoryBudget` tracks texture memory across layer cache and mask images
- `RendererConfig.gpu_memory_budget` (default 128 MB, env var `BLINC_GPU_MEMORY_BUDGET_MB`)
- `enforce_memory_budget()` evicts largest pooled textures first, then mask image cache
- `LayerTextureCache::evict_to_budget()` — budget-aware pool eviction

#### Flow Shader 3D Codegen
- `emit_vertex_3d_shader()` — generates vertex shader with mesh vertex input struct
- `emit_material_shader()` — generates fragment shader with inline PBR evaluation
- 3D builtin bindings: vertex attributes, world-space interpolants, matrices, camera, light
- Matrix function WGSL emission: multiply, inverse, transpose, transform_normal, translation/rotation/scale/perspective/lookAt
- `FlowType::Mat4` support in `flow_type_to_wgsl()`
- `FlowTarget::Vertex` / `Material` pipeline creation with mesh vertex buffer layout

#### Dynamic Image Rendering
- `render_dynamic_images()` — per-frame RGBA texture upload via `GpuImage::from_rgba`
- `DynamicImage` in `PrimitiveBatch` for video frames, camera preview, procedural textures

### Changed
- `RendererConfig` uses `..RendererConfig::default()` in all construction sites for forward compatibility

## [0.1.13] - 2026-02-18

### Added

#### Flow Codegen

- Underscore variants for all `StepType` identifiers (e.g. `pattern_noise` alongside `pattern-noise`) for `flow!` macro compatibility

### Changed

- `GpuImageInstance` params layout: `params[2..3]` changed from sin_rot/cos_rot to border_width/packed_border_color; rotation now uses the `transform` field
- Image shader (`image.wgsl`) consolidated VertexOutput from 16 locations to 13 by passing combined `params` vec4

### Removed

- `GpuImageInstance::with_rotation_sincos()` — superseded by `with_transform()` for full 2x2 affine support

### Added

#### 3D SDF Raymarching Pipeline

- Per-element 3D shapes via `shape-3d: box | sphere | cylinder | torus | capsule | group`
- `depth` property for 3D extrusion depth
- `perspective` property for camera distance
- `rotate-x`, `rotate-y` for 3D axis rotation
- `translate-z` for Z-axis translation (closer/farther positioning)
- 32-step raymarching with analytical ray-AABB intersection
- Edge anti-aliasing via closest-approach distance tracking
- Blinn-Phong lighting with configurable `ambient`, `specular`, `light-direction`, `light-intensity`

#### 3D Boolean Operations

- `ShapeDesc` struct for per-shape descriptors in group composition (64 bytes, 4 vec4s)
- `MAX_GROUP_SHAPES` constant (16 shapes per group)
- Boolean SDF operations: `union`, `subtract`, `intersect`
- Smooth boolean operations: `smooth-union`, `smooth-subtract`, `smooth-intersect` with configurable blend radius
- `shape-3d: group` for collecting children into compound SDF via storage buffer

#### UV Mapping

- Automatic UV mapping of background (solid/gradient) onto 3D surface hit points
- Box: face-based projection (front/back, top/bottom, left/right)
- Sphere: spherical coordinate mapping
- Cylinder/torus/capsule: cylindrical coordinate mapping

#### GpuPrimitive Extensions

- `perspective[4]` field: `(sin_rx, cos_rx, persp_d, shape_type)`
- `sdf_3d[4]` field: `(depth, ambient, specular_power, translate_z)`
- `light[4]` field: `(dir_x, dir_y, dir_z, intensity)`
- `filter_a[4]` field: `(grayscale, invert, sepia, hue_rotate_rad)` for CSS filters
- `filter_b[4]` field: `(brightness, contrast, saturate, 0)` for CSS filters

#### Image/SVG Rotation Support

- `GpuImageInstance::with_rotation_sincos()` for applying rotation to image quads
- Image vertex shader (`image.wgsl`) rotates quad vertices around center using sin/cos parameters
- Fixed default `GpuImageInstance` params: `cos_rot` now defaults to 1.0 (was 0.0, collapsed all vertices)

#### Corner Radius Clamping

- Clamp `border-radius` to `min(half_width, half_height)` per CSS spec in:
  - SDF border rendering (fragment shader)
  - Glass shader `sd_rounded_rect`
  - Clip shader `sd_rounded_rect_clip`

#### CSS Filters (GPU)

- `apply_css_filter()` WGSL function: grayscale, invert, sepia, hue-rotate, brightness, contrast, saturate
- Identity-skip guard in fragment shader for zero-cost when no filter is active

#### Kawase Blur Pipeline

- Multi-pass Kawase blur shader with 3 modes: CSS filter (RGBA), shadow (alpha-only), passthrough
- Batched blur passes into single GPU command encoder (was per-pass submission)
- Pre-allocated 8 uniform buffers in `blur_uniforms_pool` (eliminates per-frame buffer creation)
- `apply_blur()` and `apply_shadow_blur()` convenience wrappers
- `calculate_effect_expansion()` for automatic layer texture sizing based on effect parameters
- DPI-scaled layer effects in `render_layer_with_motion`
- XLarge texture pooling in `LayerTextureCache` (>512px textures were previously dropped every frame)

#### Paint Context

- `set_3d_shape()` for configuring per-element 3D shape parameters
- `set_3d_translate_z()` for Z-axis offset
- `set_3d_group_raw()` for compound shape composition from raw float arrays
- 3D transient state management with `clear_3d()` reset
- `set_css_filter()` / `clear_css_filter()` for per-element CSS filter state

#### Glass Backdrop Blur

- Golden-angle spiral blur for simple glass shader (72 samples, 6 rings × 12) replacing 5×5 box blur
- CSS-spec-correct sigma: blur radius = standard deviation (was `radius × 0.5`, producing half-strength blur)
- Sampling extent increased to 2.5× sigma with linear ring spacing for proper Gaussian kernel coverage
- Consistent blur algorithm across both liquid glass (GLASS_SHADER) and simple glass (SIMPLE_GLASS_SHADER) pipelines

### Fixed

- Mix blend mode layers were invisible because `has_layer_effects()` didn't check `blend_mode != Normal`, causing the interleaved z-layer path to strip layer commands from the batch
- Z>0 overlay pass re-rendered blend-mode primitives without blend, overwriting correctly composited results
- Simple glass shader pixelation: replaced crude 25-sample box blur with 72-sample golden-angle spiral, eliminating visible grid artifacts
- Glass blur intensity too weak: corrected Gaussian sigma from `radius * 0.5` to `radius` per CSS spec (blur radius = standard deviation)
- `set_css_filter` and `clear_css_filter` now properly override the `DrawContext` trait (previously only defined as inherent methods, causing no-op dispatch via `&mut dyn DrawContext`)
- Clippy warnings across image.rs, particles.rs, path.rs, primitives.rs, renderer.rs, text.rs

## [0.1.1] - Initial Release

- Initial public release with GPU-accelerated 2D rendering pipeline
