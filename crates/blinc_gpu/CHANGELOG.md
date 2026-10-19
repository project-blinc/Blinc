# Changelog

All notable changes to `blinc_gpu` will be documented in this file.

## [Unreleased]

### Added
- Display-list notches: signed concave or convex corners, top and bottom scoop, bulge, cut and peak edges, the producer's border and gradients, and outer and inset shadows. `GpuPrimitive::notch_info` selects ABI coordinates and shadow mode in the existing notch pipeline; zero keeps native notch shading. Gradient tables, conic fills and local clips also reach the notch shaders. Layer translation moves notch gradient geometry, including conic centres.
- Display-list fading clips on boxes, shadows and glyphs, with a separate `GpuPrimitive::clip_fade_bounds` screen rectangle so intersecting clips retain the producer's fade edges. A zero width keeps native primitives fading relative to `clip_bounds`. Layer content moves the fade rectangle into its texture frame. Images under fading clips remain `Unsupported::ClipFade`.
- Clips in a primitive's own frame. `GpuPrimitive::local_clip_bounds` and `local_clip_radius` (a width of zero is none) are tested by `sdf_core` and its `vb` and `dt` variants against the sample point taken back through the primitive's transform, so a turned parent clips its children to its turned, rounded box and not to a bounding box on screen, and a rounded clip cut by a plain one is exact. `display_list` reads them for boxes and glyphs; an image under one is still `Unsupported::LocalClip`.
- `display_list::effects` draws a layer's mask: a linear, radial or conic gradient of alpha measured in the element's box, which the layer's alpha is multiplied by. The composite takes a target pixel back through the element's transform into fractions of its box and evaluates the stops (up to `MAX_MASK_STOPS`), folded by their spread. Every form the producer writes, two stops, a middle stop and a table, is read into that list; a mask with more stops than there is room for leaves its layer drawn flat and reported.
- Gradients with any number of stops, a spread (pad, repeat, reflect) and conic gradients on SDF primitives. `GpuPrimitive::gradient_table` names a stop table in `aux_data` (a header of the count and spread, the average colour, then a colour row and an offset row for each stop; zero means none, so two stops from 0 to 1 are drawn as before), `FillType::ConicGradient` is a fill of its own, and `sdf_core` and its `vb` and `dt` variants read the table, mixing stops in premultiplied colour. `display_list` reads all three of the producer's forms (a middle stop, other offsets, a table) and `plan::Run` carries the tables its primitives name.
- `display_list::effects` draws a layer's blur, colour filter and drop shadow as `blinc_abi` defines them: a Gaussian of a stated deviation in device pixels, an affine map of straight RGB, and the content's blurred alpha shifted and tinted, composited over the parent with straight alpha. Its weights are worked out once on the CPU and a blur wider than `FULL_RESOLUTION_DEVIATION` (8 px) is taken on the content shrunk by powers of two and drawn back at full size. Against a CPU Gaussian it stays within 2 levels of 255 up to that deviation and within 5 beyond it. `plan::Painter` draws a layer's content into a pooled texture that covers only the layer's bounds (and where its shadow is read from), moving the content into that frame, and keeps the pipelines, scratch textures and uniforms between frames; `plan::render` is a one-shot wrapper. `blinc_test_suite`'s `blur_cost` tests measure the blur against the Kawase blur `apply_blur` uses, and a layer drawn over the whole target against one over its element.
- `display_list::plan` nests a layer: the records from a layer's opening to its closing become an `Item::Layer`, drawn into a texture of exactly the target's size and composited with the layer's opacity, so what overlaps inside a faded group does not show through itself. `plan::render` takes the target's size for it. A layer that also blurs, filters its colour, casts a drop shadow or is masked is drawn flat and reported as `Unsupported::LayerEffect`.
- `display_list::plan` turns a display list into a `Plan` in paint order: runs of primitives with each image between them, since an image is a texture the consumer owns, drawn by the image pipeline in a pass of its own. `Record::image` reads an image record as a `GpuImageInstance` stretched over its box, with its opacity and rounded clip, and `plan::render` draws a plan over a target with `render_overlay` and `render_images`, finding each image by its slot. An image that is fitted, positioned or tiled is `Unsupported::ImageMapping`, and one under a clip with non-circular corners is `Unsupported::ClipCorner`.
- `display_list` reads a glyph record as a `PrimitiveType::Text` primitive over its rect in the producer's glyph atlas, and `display_list::atlas::AtlasTexture` keeps that atlas on the GPU from the producer's revisioned updates, for `GpuRenderer::set_glyph_atlas`. A colour glyph's shape drawn in one colour (a text shadow's) is reported as `Unsupported::ColorGlyphShape`.
- Polygon clips honour a fill rule: `GpuPaintContext::set_clip_fill_rule(FillRule::EvenOdd)` before `push_clip` makes the clip even-odd, carried in `clip_radius.x` and read by `calculate_polygon_clip_alpha` in all twelve SDF shaders.
- `GpuPrimitive` carries a border colour for each side (`side_color_top`, `_right`, `_bottom`, `_left`); alphas that sum to zero mean unset, and the border is `border_color` all round, so existing paths are unchanged. `sdf_core` and its `vb` and `dt` variants colour the border by the side whose edge the point is nearest in widths, so two sides meet on the diagonal at a corner as CSS draws them.
- `display_list`: reads `blinc_abi` display lists. `records` checks a list's version and length, `Record` is a typed view of one record, and `Record::to_primitive` turns a box or shadow record into the `GpuPrimitive` the SDF pipelines draw, at a given display scale. A record that needs something a primitive cannot yet carry (a local or fading clip on an image, a `clip-path`, a notch on an image, a layer mask of more than 16 stops, a backdrop) comes back as `Unsupported` naming it.
- Per-corner squircle / scoop / bevel support on `ClipShape::RoundedRect` propagated into the SDF clip primitive uniforms — all 12 SDF shader variants (`sdf_core` / `sdf_shadow` / `sdf_3d` / `sdf_notch` × `vb` / `dt` / msaa) now read and apply per-corner `n` when evaluating clip alpha.

### Fixed
- `GpuRenderer::render_msaa` now uploads the auxiliary data buffer, so polygon clips and 3D groups draw through it. It drew them as if their vertices were all zero.
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
