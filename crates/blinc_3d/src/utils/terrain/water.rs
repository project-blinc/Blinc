//! Water body rendering for terrain

use crate::ecs::Component;
use blinc_core::{Color, Vec3};

/// Water body configuration
#[derive(Clone, Debug)]
pub struct WaterBody {
    /// Water surface level (Y coordinate)
    pub water_level: f32,
    /// Water color
    pub color: Color,
    /// Water transparency (0 = opaque, 1 = fully transparent)
    pub transparency: f32,
    /// Fresnel effect strength
    pub fresnel_strength: f32,
    /// Specular highlight intensity
    pub specular_intensity: f32,
    /// Wave configuration
    pub waves: WaveConfig,
    /// Enable reflections
    pub reflections_enabled: bool,
    /// Enable refractions
    pub refractions_enabled: bool,
    /// Foam configuration
    pub foam: Option<FoamConfig>,
    /// Caustics configuration
    pub caustics: Option<CausticsConfig>,
    /// Shore depth fade distance
    pub shore_fade_distance: f32,
}

impl Component for WaterBody {}

impl Default for WaterBody {
    fn default() -> Self {
        Self {
            water_level: 0.0,
            color: Color::rgba(0.1, 0.3, 0.5, 0.8),
            transparency: 0.6,
            fresnel_strength: 1.0,
            specular_intensity: 0.8,
            waves: WaveConfig::default(),
            reflections_enabled: true,
            refractions_enabled: true,
            foam: None,
            caustics: None,
            shore_fade_distance: 5.0,
        }
    }
}

impl WaterBody {
    /// Create a new water body at height
    pub fn new(water_level: f32) -> Self {
        Self {
            water_level,
            ..Default::default()
        }
    }

    /// Set water color
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set transparency
    pub fn with_transparency(mut self, transparency: f32) -> Self {
        self.transparency = transparency.clamp(0.0, 1.0);
        self
    }

    /// Set wave configuration
    pub fn with_waves(mut self, waves: WaveConfig) -> Self {
        self.waves = waves;
        self
    }

    /// Enable foam
    pub fn with_foam(mut self, foam: FoamConfig) -> Self {
        self.foam = Some(foam);
        self
    }

    /// Enable caustics
    pub fn with_caustics(mut self, caustics: CausticsConfig) -> Self {
        self.caustics = Some(caustics);
        self
    }

    /// Disable reflections
    pub fn without_reflections(mut self) -> Self {
        self.reflections_enabled = false;
        self
    }

    /// Disable refractions
    pub fn without_refractions(mut self) -> Self {
        self.refractions_enabled = false;
        self
    }

    /// Get water height at position (including waves)
    pub fn get_height(&self, x: f32, z: f32, time: f32) -> f32 {
        self.water_level + self.waves.sample(x, z, time)
    }

    /// Get wave normal at position
    pub fn get_normal(&self, x: f32, z: f32, time: f32) -> Vec3 {
        self.waves.get_normal(x, z, time)
    }

    // ========== Presets ==========

    /// Ocean water preset
    pub fn ocean(water_level: f32) -> Self {
        Self::new(water_level)
            .with_color(Color::rgba(0.05, 0.2, 0.4, 0.85))
            .with_waves(WaveConfig::ocean())
            .with_foam(FoamConfig::default())
    }

    /// Lake water preset
    pub fn lake(water_level: f32) -> Self {
        Self::new(water_level)
            .with_color(Color::rgba(0.1, 0.25, 0.35, 0.75))
            .with_waves(WaveConfig::calm())
    }

    /// River water preset
    pub fn river(water_level: f32) -> Self {
        Self::new(water_level)
            .with_color(Color::rgba(0.15, 0.3, 0.4, 0.7))
            .with_waves(WaveConfig::river())
            .with_foam(FoamConfig::subtle())
    }

    /// Pool/clear water preset
    pub fn pool(water_level: f32) -> Self {
        Self::new(water_level)
            .with_color(Color::rgba(0.2, 0.5, 0.7, 0.6))
            .with_transparency(0.8)
            .with_waves(WaveConfig::calm())
            .with_caustics(CausticsConfig::default())
    }

    /// Swamp water preset
    pub fn swamp(water_level: f32) -> Self {
        Self::new(water_level)
            .with_color(Color::rgba(0.15, 0.2, 0.1, 0.9))
            .with_transparency(0.2)
            .with_waves(WaveConfig::still())
    }
}

/// Wave configuration
#[derive(Clone, Debug)]
pub struct WaveConfig {
    /// Wave layers
    pub layers: Vec<WaveLayer>,
    /// Global wave speed multiplier
    pub speed_multiplier: f32,
    /// Global wave height multiplier
    pub height_multiplier: f32,
}

impl Default for WaveConfig {
    fn default() -> Self {
        Self::calm()
    }
}

impl WaveConfig {
    /// Create a new wave config
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
            speed_multiplier: 1.0,
            height_multiplier: 1.0,
        }
    }

    /// Add a wave layer
    pub fn with_layer(mut self, layer: WaveLayer) -> Self {
        self.layers.push(layer);
        self
    }

    /// Set speed multiplier
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed_multiplier = speed;
        self
    }

    /// Set height multiplier
    pub fn with_height(mut self, height: f32) -> Self {
        self.height_multiplier = height;
        self
    }

    /// Sample wave height at position
    pub fn sample(&self, x: f32, z: f32, time: f32) -> f32 {
        let mut height = 0.0;
        for layer in &self.layers {
            height += layer.sample(x, z, time * self.speed_multiplier) * self.height_multiplier;
        }
        height
    }

    /// Get wave normal at position
    pub fn get_normal(&self, x: f32, z: f32, time: f32) -> Vec3 {
        let epsilon = 0.01;
        let h_center = self.sample(x, z, time);
        let h_right = self.sample(x + epsilon, z, time);
        let h_forward = self.sample(x, z + epsilon, time);

        let dx = h_right - h_center;
        let dz = h_forward - h_center;

        let normal = Vec3::new(-dx, epsilon, -dz);
        normalize_vec3(normal)
    }

    // ========== Presets ==========

    /// Still water (no waves)
    pub fn still() -> Self {
        Self::new()
    }

    /// Calm water
    pub fn calm() -> Self {
        Self::new()
            .with_layer(WaveLayer::new(0.02, 0.05, 1.0, Vec3::new(1.0, 0.0, 0.3)))
            .with_layer(WaveLayer::new(0.05, 0.02, 0.7, Vec3::new(0.7, 0.0, 1.0)))
    }

    /// Ocean waves
    pub fn ocean() -> Self {
        Self::new()
            .with_layer(WaveLayer::new(0.01, 0.5, 0.8, Vec3::new(1.0, 0.0, 0.0)))
            .with_layer(WaveLayer::new(0.02, 0.3, 1.2, Vec3::new(0.8, 0.0, 0.6)))
            .with_layer(WaveLayer::new(0.05, 0.1, 0.5, Vec3::new(0.5, 0.0, 0.9)))
            .with_layer(WaveLayer::new(0.1, 0.03, 0.3, Vec3::new(0.3, 0.0, 1.0)))
    }

    /// River flow
    pub fn river() -> Self {
        Self::new()
            .with_layer(WaveLayer::new(0.03, 0.08, 2.0, Vec3::new(1.0, 0.0, 0.0)))
            .with_layer(WaveLayer::new(0.08, 0.03, 1.5, Vec3::new(0.9, 0.0, 0.3)))
    }
}

/// Single wave layer (Gerstner-style)
#[derive(Clone, Debug)]
pub struct WaveLayer {
    /// Wave frequency (wavelength = 1/frequency)
    pub frequency: f32,
    /// Wave amplitude (height)
    pub amplitude: f32,
    /// Wave speed
    pub speed: f32,
    /// Wave direction (normalized)
    pub direction: Vec3,
    /// Steepness (0 = sine wave, 1 = sharp peaks)
    pub steepness: f32,
}

impl WaveLayer {
    /// Create a new wave layer
    pub fn new(frequency: f32, amplitude: f32, speed: f32, direction: Vec3) -> Self {
        Self {
            frequency,
            amplitude,
            speed,
            direction: normalize_vec3(direction),
            steepness: 0.5,
        }
    }

    /// Set steepness
    pub fn with_steepness(mut self, steepness: f32) -> Self {
        self.steepness = steepness.clamp(0.0, 1.0);
        self
    }

    /// Sample wave height at position
    pub fn sample(&self, x: f32, z: f32, time: f32) -> f32 {
        let dot = x * self.direction.x + z * self.direction.z;
        let phase = dot * self.frequency - time * self.speed;

        if self.steepness < 0.01 {
            // Simple sine wave
            phase.sin() * self.amplitude
        } else {
            // Gerstner-style wave (sharper peaks)
            let sin_phase = phase.sin();
            let exp_factor = (sin_phase * self.steepness * 4.0).exp();
            (exp_factor - 1.0) / (exp_factor + 1.0) * self.amplitude
        }
    }
}

/// Foam configuration
#[derive(Clone, Debug)]
pub struct FoamConfig {
    /// Foam color
    pub color: Color,
    /// Foam threshold (how much wave height triggers foam)
    pub threshold: f32,
    /// Foam intensity
    pub intensity: f32,
    /// Shore foam distance
    pub shore_distance: f32,
    /// Foam texture scale
    pub texture_scale: f32,
}

impl Default for FoamConfig {
    fn default() -> Self {
        Self {
            color: Color::rgba(1.0, 1.0, 1.0, 0.9),
            threshold: 0.3,
            intensity: 1.0,
            shore_distance: 3.0,
            texture_scale: 10.0,
        }
    }
}

impl FoamConfig {
    /// Subtle foam preset
    pub fn subtle() -> Self {
        Self {
            color: Color::rgba(1.0, 1.0, 1.0, 0.6),
            threshold: 0.5,
            intensity: 0.5,
            shore_distance: 1.5,
            texture_scale: 15.0,
        }
    }

    /// Heavy foam preset
    pub fn heavy() -> Self {
        Self {
            color: Color::rgba(1.0, 1.0, 1.0, 1.0),
            threshold: 0.2,
            intensity: 1.5,
            shore_distance: 5.0,
            texture_scale: 8.0,
        }
    }
}

/// Caustics configuration
#[derive(Clone, Debug)]
pub struct CausticsConfig {
    /// Caustics intensity
    pub intensity: f32,
    /// Caustics scale
    pub scale: f32,
    /// Caustics speed
    pub speed: f32,
    /// Maximum depth for caustics
    pub max_depth: f32,
}

impl Default for CausticsConfig {
    fn default() -> Self {
        Self {
            intensity: 0.5,
            scale: 5.0,
            speed: 1.0,
            max_depth: 10.0,
        }
    }
}

fn normalize_vec3(v: Vec3) -> Vec3 {
    let len = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if len < 0.0001 {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        let inv = 1.0 / len;
        Vec3::new(v.x * inv, v.y * inv, v.z * inv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_water_body() {
        let water = WaterBody::ocean(10.0);
        assert!((water.water_level - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_wave_sample() {
        let wave = WaveLayer::new(0.1, 1.0, 1.0, Vec3::new(1.0, 0.0, 0.0));
        let h = wave.sample(0.0, 0.0, 0.0);
        assert!(h.is_finite());
    }

    #[test]
    fn test_wave_config() {
        let config = WaveConfig::ocean();
        let h = config.sample(100.0, 100.0, 1.0);
        assert!(h.is_finite());
    }
}
