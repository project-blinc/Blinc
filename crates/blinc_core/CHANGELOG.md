# Changelog

All notable changes to `blinc_core` will be documented in this file.

## [Unreleased]

### Changed
- `ClipShape::RoundedRect` now carries a `CornerShape` (per-corner superellipse `n`) so clips can follow squircle / scoop / bevel outlines instead of always rounding. Constructors default to the existing `CornerShape::ROUND` so untouched callers behave identically.

### Added
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
