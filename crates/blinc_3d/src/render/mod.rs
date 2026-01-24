//! Render system
//!
//! Provides shader pipelines and rendering infrastructure for 3D scenes.

mod pipelines;
mod post_process;
mod shaders;

pub use pipelines::{BlendConfig, CullMode, GamePipelines, PipelineKey, VertexLayout};
pub use post_process::{
    Bloom, ColorGrading, PostEffect, PostProcessContext, PostProcessStack, ToneMapping,
    ToneMappingMode, Vignette, FXAA,
};
pub use shaders::{ShaderId, ShaderRegistry};

use crate::math::{mat4_mul, Mat4Ext};
use blinc_core::{Camera, Mat4, Vec3};

/// Camera uniform data for GPU
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    /// View matrix
    pub view: [[f32; 4]; 4],
    /// Projection matrix
    pub projection: [[f32; 4]; 4],
    /// View-projection matrix
    pub view_projection: [[f32; 4]; 4],
    /// Inverse view matrix
    pub inverse_view: [[f32; 4]; 4],
    /// Camera position in world space
    pub position: [f32; 4],
    /// Camera direction
    pub direction: [f32; 4],
    /// Near and far planes
    pub near_far: [f32; 4],
}

impl CameraUniform {
    /// Create from view and projection matrices
    pub fn new(view: Mat4, projection: Mat4, position: Vec3, direction: Vec3, near: f32, far: f32) -> Self {
        let view_projection = mat4_mul(&projection, &view);
        let inverse_view = view.inverse();

        Self {
            view: view.to_cols_array_2d(),
            projection: projection.to_cols_array_2d(),
            view_projection: view_projection.to_cols_array_2d(),
            inverse_view: inverse_view.to_cols_array_2d(),
            position: [position.x, position.y, position.z, 1.0],
            direction: [direction.x, direction.y, direction.z, 0.0],
            near_far: [near, far, 0.0, 0.0],
        }
    }
}

impl Default for CameraUniform {
    fn default() -> Self {
        Self {
            view: Mat4::IDENTITY.to_cols_array_2d(),
            projection: Mat4::IDENTITY.to_cols_array_2d(),
            view_projection: Mat4::IDENTITY.to_cols_array_2d(),
            inverse_view: Mat4::IDENTITY.to_cols_array_2d(),
            position: [0.0, 0.0, 0.0, 1.0],
            direction: [0.0, 0.0, -1.0, 0.0],
            near_far: [0.1, 100.0, 0.0, 0.0],
        }
    }
}

/// Model uniform data for GPU
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelUniform {
    /// Model matrix (local to world)
    pub model: [[f32; 4]; 4],
    /// Normal matrix (transpose of inverse model matrix)
    pub normal_matrix: [[f32; 4]; 4],
}

impl ModelUniform {
    /// Create from model matrix
    pub fn new(model: Mat4) -> Self {
        let normal_matrix = model.inverse().transpose();
        Self {
            model: model.to_cols_array_2d(),
            normal_matrix: normal_matrix.to_cols_array_2d(),
        }
    }
}

impl Default for ModelUniform {
    fn default() -> Self {
        Self {
            model: Mat4::IDENTITY.to_cols_array_2d(),
            normal_matrix: Mat4::IDENTITY.to_cols_array_2d(),
        }
    }
}

/// Time uniform for animated shaders
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TimeUniform {
    /// Total elapsed time in seconds
    pub time: f32,
    /// Delta time since last frame
    pub delta_time: f32,
    /// Frame count
    pub frame: u32,
    /// Padding
    pub _padding: u32,
}

impl Default for TimeUniform {
    fn default() -> Self {
        Self {
            time: 0.0,
            delta_time: 0.016,
            frame: 0,
            _padding: 0,
        }
    }
}
