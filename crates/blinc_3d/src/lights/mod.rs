//! Lighting system
//!
//! Provides various light types for illuminating 3D scenes.

mod ambient;
mod directional;
mod hemisphere;
mod point;
mod spot;

pub use ambient::AmbientLight;
pub use directional::DirectionalLight;
pub use hemisphere::HemisphereLight;
pub use point::PointLight;
pub use spot::SpotLight;

use crate::ecs::Component;
use blinc_core::Color;

/// Light trait for all light types
pub trait Light: Send + Sync {
    /// Get the light type
    fn light_type(&self) -> LightType;

    /// Get the light color
    fn color(&self) -> Color;

    /// Get the light intensity
    fn intensity(&self) -> f32;

    /// Whether this light casts shadows
    fn casts_shadows(&self) -> bool {
        false
    }
}

/// Light types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightType {
    /// Ambient light (uniform illumination)
    Ambient,
    /// Directional light (sun-like)
    Directional,
    /// Point light (omnidirectional)
    Point,
    /// Spot light (cone-shaped)
    Spot,
    /// Hemisphere light (sky/ground gradient)
    Hemisphere,
}

/// Handle to a light resource
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LightHandle(pub u64);

impl Default for LightHandle {
    fn default() -> Self {
        Self(0)
    }
}

/// Shadow configuration for lights that cast shadows
#[derive(Clone, Debug)]
pub struct ShadowConfig {
    /// Shadow map resolution
    pub map_size: u32,
    /// Shadow bias to prevent shadow acne
    pub bias: f32,
    /// Normal bias
    pub normal_bias: f32,
    /// Shadow camera near plane
    pub near: f32,
    /// Shadow camera far plane
    pub far: f32,
    /// Shadow darkness (0 = no shadow, 1 = fully dark)
    pub darkness: f32,
}

impl Default for ShadowConfig {
    fn default() -> Self {
        Self {
            map_size: 1024,
            bias: 0.0005,
            normal_bias: 0.02,
            near: 0.5,
            far: 500.0,
            darkness: 0.5,
        }
    }
}

/// Light uniform data for GPU
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightUniform {
    /// Position (for point/spot) or direction (for directional)
    pub position_or_direction: [f32; 4],
    /// Color and intensity packed
    pub color: [f32; 4],
    /// Additional parameters (depends on light type)
    /// For spot: [inner_cone, outer_cone, 0, 0]
    /// For point: [distance, decay, 0, 0]
    pub params: [f32; 4],
}

impl Default for LightUniform {
    fn default() -> Self {
        Self {
            position_or_direction: [0.0, 0.0, 0.0, 0.0],
            color: [1.0, 1.0, 1.0, 1.0],
            params: [0.0; 4],
        }
    }
}
