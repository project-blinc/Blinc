//! Lighting presets for common scenarios
//!
//! Provides pre-configured lighting setups for various use cases:
//!
//! - **Studio** - Professional 3-point lighting
//! - **Outdoor** - Sun-based natural lighting
//! - **Dramatic** - High-contrast cinematic lighting
//! - **Neon** - Colorful cyberpunk-style lighting

mod presets;
mod builder;

pub use presets::{LightingPreset, BuiltinPreset};
pub use builder::{LightingPresetBuilder, CustomPreset};

use crate::ecs::World;
use crate::lights::{AmbientLight, DirectionalLight, PointLight, SpotLight, HemisphereLight};
use crate::scene::Object3D;
use blinc_core::{Color, Vec3};

/// Configuration for a single light in a preset
#[derive(Clone, Debug)]
pub struct LightConfig {
    /// Type of light
    pub light_type: LightType,
    /// Light color
    pub color: Color,
    /// Light intensity
    pub intensity: f32,
    /// Position (for point/spot lights)
    pub position: Option<Vec3>,
    /// Direction (for directional/spot lights)
    pub direction: Option<Vec3>,
    /// Whether light casts shadows
    pub cast_shadows: bool,
    /// Additional parameters (varies by light type)
    pub params: LightParams,
}

/// Light type enum
#[derive(Clone, Debug)]
pub enum LightType {
    Ambient,
    Directional,
    Point,
    Spot,
    Hemisphere,
}

/// Additional light parameters
#[derive(Clone, Debug, Default)]
pub struct LightParams {
    /// For point lights: maximum distance
    pub distance: Option<f32>,
    /// For point lights: decay rate
    pub decay: Option<f32>,
    /// For spot lights: cone angle (radians)
    pub angle: Option<f32>,
    /// For spot lights: penumbra factor (0-1)
    pub penumbra: Option<f32>,
    /// For hemisphere lights: ground color
    pub ground_color: Option<Color>,
}

impl LightConfig {
    /// Create an ambient light config
    pub fn ambient(color: Color, intensity: f32) -> Self {
        Self {
            light_type: LightType::Ambient,
            color,
            intensity,
            position: None,
            direction: None,
            cast_shadows: false,
            params: LightParams::default(),
        }
    }

    /// Create a directional light config
    pub fn directional(color: Color, intensity: f32, direction: Vec3) -> Self {
        Self {
            light_type: LightType::Directional,
            color,
            intensity,
            position: None,
            direction: Some(direction),
            cast_shadows: true,
            params: LightParams::default(),
        }
    }

    /// Create a point light config
    pub fn point(color: Color, intensity: f32, position: Vec3) -> Self {
        Self {
            light_type: LightType::Point,
            color,
            intensity,
            position: Some(position),
            direction: None,
            cast_shadows: false,
            params: LightParams {
                distance: Some(10.0),
                decay: Some(2.0),
                ..Default::default()
            },
        }
    }

    /// Create a spot light config
    pub fn spot(color: Color, intensity: f32, position: Vec3, direction: Vec3) -> Self {
        Self {
            light_type: LightType::Spot,
            color,
            intensity,
            position: Some(position),
            direction: Some(direction),
            cast_shadows: true,
            params: LightParams {
                angle: Some(std::f32::consts::PI / 6.0),
                penumbra: Some(0.1),
                distance: Some(20.0),
                ..Default::default()
            },
        }
    }

    /// Create a hemisphere light config
    pub fn hemisphere(sky_color: Color, ground_color: Color, intensity: f32) -> Self {
        Self {
            light_type: LightType::Hemisphere,
            color: sky_color,
            intensity,
            position: None,
            direction: None,
            cast_shadows: false,
            params: LightParams {
                ground_color: Some(ground_color),
                ..Default::default()
            },
        }
    }

    /// Set shadow casting
    pub fn with_shadows(mut self, cast_shadows: bool) -> Self {
        self.cast_shadows = cast_shadows;
        self
    }

    /// Set distance (for point/spot lights)
    pub fn with_distance(mut self, distance: f32) -> Self {
        self.params.distance = Some(distance);
        self
    }

    /// Set decay (for point lights)
    pub fn with_decay(mut self, decay: f32) -> Self {
        self.params.decay = Some(decay);
        self
    }

    /// Set cone angle (for spot lights)
    pub fn with_angle(mut self, angle: f32) -> Self {
        self.params.angle = Some(angle);
        self
    }

    /// Set penumbra (for spot lights)
    pub fn with_penumbra(mut self, penumbra: f32) -> Self {
        self.params.penumbra = Some(penumbra);
        self
    }
}

/// Apply a set of light configs to a world
pub fn apply_lights(world: &mut World, lights: &[LightConfig]) {
    for config in lights {
        match config.light_type {
            LightType::Ambient => {
                let mut light = AmbientLight::new(config.color, config.intensity);
                world.spawn()
                    .insert(Object3D::new())
                    .insert(light);
            }
            LightType::Directional => {
                let mut light = DirectionalLight::new(config.color, config.intensity);
                light.cast_shadows = config.cast_shadows;
                let mut obj = Object3D::new();
                if let Some(dir) = config.direction {
                    // Set rotation to face direction
                    obj.position = Vec3::new(dir.x * -10.0, dir.y * -10.0, dir.z * -10.0);
                }
                world.spawn()
                    .insert(obj)
                    .insert(light);
            }
            LightType::Point => {
                let mut light = PointLight::new(config.color, config.intensity);
                if let Some(dist) = config.params.distance {
                    light.distance = dist;
                }
                if let Some(decay) = config.params.decay {
                    light.decay = decay;
                }
                light.cast_shadows = config.cast_shadows;
                let mut obj = Object3D::new();
                if let Some(pos) = config.position {
                    obj.position = pos;
                }
                world.spawn()
                    .insert(obj)
                    .insert(light);
            }
            LightType::Spot => {
                let mut light = SpotLight::new(config.color, config.intensity);
                if let Some(angle) = config.params.angle {
                    light.angle = angle;
                }
                if let Some(penumbra) = config.params.penumbra {
                    light.penumbra = penumbra;
                }
                if let Some(dist) = config.params.distance {
                    light.distance = dist;
                }
                light.cast_shadows = config.cast_shadows;
                let mut obj = Object3D::new();
                if let Some(pos) = config.position {
                    obj.position = pos;
                }
                world.spawn()
                    .insert(obj)
                    .insert(light);
            }
            LightType::Hemisphere => {
                let ground = config.params.ground_color.unwrap_or(Color::rgb(0.3, 0.2, 0.1));
                let light = HemisphereLight::new(config.color, ground, config.intensity);
                world.spawn()
                    .insert(Object3D::new())
                    .insert(light);
            }
        }
    }
}
