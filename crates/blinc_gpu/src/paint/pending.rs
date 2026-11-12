//! Mesh instances and canvas callbacks captured for the outer render loop and native groups.

use blinc_core::{Camera, CubemapData, Light, Mat4, MeshData};

/// A mesh instance and its captured scene state. Native groups retain compatible
/// consecutive instances together; the outer render loop drains ungrouped instances.
#[derive(Clone, Debug)]
pub struct PendingMesh {
    /// Shared geometry and PBR material; capture does not copy vertex/index data.
    pub mesh: std::sync::Arc<MeshData>,
    /// Model transform (world-space placement of this instance).
    pub transform: Mat4,
    /// Camera snapshot; perspective aspect follows the scene viewport at dispatch.
    pub camera: Camera,
    /// Light snapshot. Dispatch uses directional lights and a shared scene shadow map.
    pub lights: Vec<Light>,
    /// Screen-space viewport rect in physical pixels [x, y, w, h].
    /// When `Some`, the renderer applies `set_viewport` + `set_scissor_rect`
    /// to clip the mesh to this region. When `None`, the mesh renders to
    /// the full frame target.
    pub viewport: Option<[f32; 4]>,
    /// Optional shared environment cubemap uploaded before this instance is drawn.
    pub env_cubemap: Option<std::sync::Arc<CubemapData>>,
}

/// A canvas GPU callback and its physical viewport. Native group commands
/// retain this record; the outer render loop drains ungrouped callbacks.
#[derive(Clone)]
pub struct PendingGpuPass {
    /// Shared callback, initialized on its first execution.
    pub pass: crate::custom_pass::GpuPass,
    /// Physical `[x, y, width, height]` passed to the callback; None means full frame.
    pub viewport: Option<[f32; 4]>,
    /// A flow shader the renderer draws itself, with `pass` inert. `viewport` is then the part of
    /// `flow.bounds` the clip leaves visible.
    pub flow: Option<PendingFlow>,
}

/// A flow shader drawn by `DrawContext::draw_shader`.
#[derive(Clone)]
pub struct PendingFlow {
    pub shader: std::sync::Arc<blinc_core::FlowShader>,
    /// Physical `[x, y, width, height]` the shader's `uv` covers.
    pub bounds: [f32; 4],
    pub inputs: blinc_core::ShaderInputs,
    /// Physical corner radius.
    pub corner_radius: f32,
}

/// A mesh scene retained by a native visual group.
#[derive(Clone, Debug)]
pub struct GroupMeshes {
    /// Consecutive instances sharing camera, viewport and paint coverage.
    pub meshes: Vec<PendingMesh>,
    /// Coverage, explicit opacity and clips applied to the completed scene.
    pub paint: crate::primitives::DynamicImage,
    /// Original physical frame extent used by the scene pipeline.
    pub frame_size: (u32, u32),
}

/// Callback state retained by a native visual group.
#[derive(Clone)]
pub struct GroupGpuPass {
    /// Callback and its original viewport.
    pub pending: PendingGpuPass,
    /// Coverage, explicit opacity and push-time clips for replacing the edited pixels.
    pub paint: crate::primitives::DynamicImage,
    /// Physical frame extent exposed to the callback.
    pub frame_size: (u32, u32),
    /// Display scale exposed to the callback.
    pub scale_factor: f64,
}

impl std::fmt::Debug for GroupGpuPass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroupGpuPass")
            .field("viewport", &self.pending.viewport)
            .field("paint", &self.paint)
            .field("frame_size", &self.frame_size)
            .field("scale_factor", &self.scale_factor)
            .finish_non_exhaustive()
    }
}
