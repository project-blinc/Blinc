//! Lighting preset builder

use super::{LightConfig, LightingPreset, apply_lights};
use crate::ecs::World;
use blinc_core::{Color, Vec3};

/// Builder for creating custom lighting presets
///
/// # Example
///
/// ```ignore
/// let preset = LightingPresetBuilder::new("My Custom Lighting")
///     .ambient(Color::rgb(0.1, 0.1, 0.15), 0.3)
///     .directional(Color::WHITE, 1.0, Vec3::new(-1.0, -1.0, -1.0))
///         .with_shadows(true)
///     .point(Color::rgb(1.0, 0.5, 0.2), 0.8, Vec3::new(3.0, 2.0, 0.0))
///     .build();
///
/// preset.apply(&mut world);
/// ```
#[derive(Clone, Debug)]
pub struct LightingPresetBuilder {
    name: String,
    lights: Vec<LightConfig>,
}

impl LightingPresetBuilder {
    /// Create a new preset builder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            lights: Vec::new(),
        }
    }

    /// Add an ambient light
    pub fn ambient(mut self, color: Color, intensity: f32) -> Self {
        self.lights.push(LightConfig::ambient(color, intensity));
        self
    }

    /// Add a directional light
    pub fn directional(mut self, color: Color, intensity: f32, direction: Vec3) -> Self {
        self.lights.push(LightConfig::directional(color, intensity, direction));
        self
    }

    /// Add a point light
    pub fn point(mut self, color: Color, intensity: f32, position: Vec3) -> Self {
        self.lights.push(LightConfig::point(color, intensity, position));
        self
    }

    /// Add a spot light
    pub fn spot(mut self, color: Color, intensity: f32, position: Vec3, direction: Vec3) -> Self {
        self.lights.push(LightConfig::spot(color, intensity, position, direction));
        self
    }

    /// Add a hemisphere light
    pub fn hemisphere(mut self, sky_color: Color, ground_color: Color, intensity: f32) -> Self {
        self.lights.push(LightConfig::hemisphere(sky_color, ground_color, intensity));
        self
    }

    /// Add a custom light config
    pub fn light(mut self, config: LightConfig) -> Self {
        self.lights.push(config);
        self
    }

    /// Modify the last added light to cast shadows
    pub fn with_shadows(mut self, cast_shadows: bool) -> Self {
        if let Some(light) = self.lights.last_mut() {
            light.cast_shadows = cast_shadows;
        }
        self
    }

    /// Modify the last added light's distance
    pub fn with_distance(mut self, distance: f32) -> Self {
        if let Some(light) = self.lights.last_mut() {
            light.params.distance = Some(distance);
        }
        self
    }

    /// Modify the last added light's decay
    pub fn with_decay(mut self, decay: f32) -> Self {
        if let Some(light) = self.lights.last_mut() {
            light.params.decay = Some(decay);
        }
        self
    }

    /// Modify the last added light's cone angle
    pub fn with_angle(mut self, angle: f32) -> Self {
        if let Some(light) = self.lights.last_mut() {
            light.params.angle = Some(angle);
        }
        self
    }

    /// Modify the last added light's penumbra
    pub fn with_penumbra(mut self, penumbra: f32) -> Self {
        if let Some(light) = self.lights.last_mut() {
            light.params.penumbra = Some(penumbra);
        }
        self
    }

    /// Build the preset
    pub fn build(self) -> CustomPreset {
        CustomPreset {
            name: self.name,
            lights: self.lights,
        }
    }
}

/// A custom lighting preset
#[derive(Clone, Debug)]
pub struct CustomPreset {
    name: String,
    lights: Vec<LightConfig>,
}

impl LightingPreset for CustomPreset {
    fn name(&self) -> &'static str {
        // This is a bit of a hack since we can't return &self.name
        // In practice, custom presets would be used directly
        "Custom"
    }

    fn lights(&self) -> Vec<LightConfig> {
        self.lights.clone()
    }
}

impl CustomPreset {
    /// Get the actual name of this custom preset
    pub fn custom_name(&self) -> &str {
        &self.name
    }

    /// Apply this preset to a world
    pub fn apply(&self, world: &mut World) {
        apply_lights(world, &self.lights);
    }
}
