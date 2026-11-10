# Changelog

All notable changes to `blinc_core` will be documented in this file.

## [Unreleased]

### Added
- `system_fonts::LINUX_SANS`: the Linux desktop UI sans families, best first, shared by the text registry and the Linux theme.

### Fixed
- Dropping a `BuildScope` disposes the builds it holds, so a closed window's effects and computeds stop.
- Keyed initializers can create nested signals and keyed values without holding the graph or hook locks. Nested reactive values retain the persistent key’s lifetime across fresh builds.

## [0.6.0] - 2026-11-04

### Added
- `owner::BuildScope` is what a runner holds to build its UI under an `Owner`: `begin` a pass, then `finish` it when the build made a fresh tree (the scopes of the builds before it are disposed) or `keep` it when the build was merged into the tree that was there. `Owner::enter` makes a scope current until its guard drops, and `Owner::detached` runs code owned by no scope, for what a widget registers once for a signal that outlives builds.
- `DrawContext::draw_shader(shader, rect, inputs)` draws a flow shader over a rect in the current transform and clip, and `ShaderInputs` holds what it reads besides `uv`: `pointer`, `time` and `corner_radius`. Contexts without a GPU ignore it.
- `FlowShader` and `FlowBuffer`: a flow compiled to WGSL with what a pipeline binds for it (target, scene sampling, whether it reads `time`, dynamic uniform fields, storage buffers). `flow!` builds one at compile time as a constant.

### Changed
- `DrawContext::set_clip_fill_rule` documents path clips as well as polygons when supported by the drawing context.
- `ClipShape::RoundedRect` now carries a `CornerShape` (per-corner superellipse `n`) so clips can follow squircle / scoop / bevel outlines instead of always rounding. Constructors default to the existing `CornerShape::ROUND` so untouched callers behave identically.

### Added
- Optional glass-layer hooks let a panel and compatible native content share one visual group; unsupported contexts retain the existing path.
- Optional deferred image hooks reserve loaded textures or placeholders at native group paint positions.
- Optional deferred SVG hooks reserve collected raster content at native group paint positions while unsupported contexts retain fallback.
- Optional deferred text hooks let native group consumers reserve collected text at its paint position while contexts without a resolver keep collection fallback.
- `DrawContext::push_cross_stream_layer` optionally captures one native layer across background and foreground passes; unsupported contexts return false without pushing.
- `DrawContext::can_promote_composite_layer` lets an open ancestor group retain descendant draws in its batch instead of allowing independent texture promotion.
- `DrawContext::composite_layer_bounds` reports a scratch footprint including paths and group effects; contexts without scratch fall back to primitive-range bounds.
- `DrawContext::bg_batch_is_composite` identifies per-node scratch ownership of primitive ranges and layer indices. Other contexts default to false.
- `DrawContext::bg_batch_is_dynamic` identifies whether recorded primitive ranges and layer indices belong to the separately dispatched motion batch. Contexts without that split default to false.
- `FillRule` (`NonZero`, `EvenOdd`) and `DrawContext::set_clip_fill_rule`, which sets the rule the next polygon `push_clip` uses. The default is nonzero and a context that does not draw polygon clips ignores it.
- `blinc_core::intern` module with process-wide `Arc<str>` pool. `intern(s: &str) -> Arc<str>` deduplicates repeated identifier strings (CSS class names, type names, stable keys). Pool is append-only; only feed bounded sources into it.

## [0.4.0] - 2026-04-05

### Added

#### 3D Mesh Rendering Data Model
- `Vertex` struct extended with `tangent: [f32; 4]` (normal mapping), `joints: [u32; 4]` and `weights: [f32; 4]` (skeletal animation)
- `Vertex` builder: `.with_tangent()`, `.with_joints()`
- `Material` extended: `normal_map`, `normal_scale`, `displacement_map`, `displacement_scale`, `receives_shadows`, `casts_shadows`
- `TextureData` struct for material textures (RGBA pixels + dimensions)
- `AlphaMode` enum: `Opaque`, `Blend`, `Mask`

#### Skeletal Animation
- `Bone` struct: name, parent index, inverse bind matrix
- `Skeleton` struct: bone hierarchy
- `SkinningData` struct: per-frame joint matrices (max 256 joints)
- `MeshData.skin: Option<SkinningData>` for attaching skinning to meshes

#### Flow Shader 3D Extensions
- `FlowTarget::Vertex` — vertex shader for 3D mesh rendering
- `FlowTarget::Material` — material/surface shader with PBR output
- `FlowType::Mat4` — 4x4 matrix type for transforms and projections
- 3D vertex builtins: `vertex_position`, `vertex_normal`, `vertex_tangent`, `vertex_color`, `joints`, `weights`, `vertex_index`, `model_matrix`, `view_proj`
- 3D material builtins: `world_position`, `world_normal`, `world_tangent`, `tangent_handedness`, `camera_position`, `light_direction`, `light_intensity`
- Matrix functions: `Mat4MulVec4`, `Mat4Mul`, `Mat4Inverse`, `Mat4Transpose`, `TransformNormal`, `TranslationMatrix`, `RotationMatrix`, `ScaleMatrix`, `PerspectiveMatrix`, `LookAtMatrix`, `SampleTexture`
- 3D output targets: `Position`, `WorldNormalOut`, `WorldPositionOut`, `Albedo`, `Metallic`, `Roughness`, `Emissive`, `SurfaceNormal`, `AlphaOut`

#### Exports
- All mesh/material/skeleton types exported from `blinc_core`: `Vertex`, `MeshData`, `Material`, `TextureData`, `AlphaMode`, `Bone`, `Skeleton`, `SkinningData`

#### Media Types
- `draw_rgba_pixels()` on DrawContext for video/camera frame rendering
- `draw_mesh_data()` on DrawContext for 3D mesh rendering

## [0.1.15] - 2026-03-22

### Added
- Native bridge API: `native_call()`, `native_register()`, `native_stream()`
- `PlatformAdapter` trait for cross-platform native bridge transport

## [0.1.1] - Initial Release

- Initial public release with DrawContext, layer system, and reactive primitives
