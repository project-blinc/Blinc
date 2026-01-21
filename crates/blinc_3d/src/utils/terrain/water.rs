//! Water body rendering for terrain
//!
//! Provides realistic water rendering with Gerstner wave simulation,
//! reflections, refractions, and foam effects.
//!
//! # Example
//!
//! ```ignore
//! use blinc_3d::utils::terrain::WaterBody;
//!
//! // Create ocean water at sea level
//! let ocean = WaterBody::ocean(0.0);
//!
//! // Create a custom lake
//! let lake = WaterBody::new(10.0)
//!     .with_color(Color::rgba(0.1, 0.3, 0.5, 0.8))
//!     .with_transparency(0.7)
//!     .calm();
//!
//! // Attach to entity - rendering is automatic
//! world.spawn().insert(ocean);
//! ```

use crate::ecs::Component;
use blinc_core::{Color, Vec3};

/// Water body component
///
/// Add this to an entity to render a water surface at the specified level.
/// The system automatically handles wave animation and GPU rendering.
#[derive(Clone, Debug)]
pub struct WaterBody {
    /// Water surface level (Y coordinate)
    pub water_level: f32,
    /// Water color (blends between shallow and deep based on depth)
    pub color: Color,
    /// Water transparency (0 = opaque, 1 = fully transparent)
    pub transparency: f32,
    /// Fresnel effect strength (higher = more reflection at glancing angles)
    pub fresnel_strength: f32,
    /// Specular highlight intensity
    pub specular_intensity: f32,
    /// Wave intensity (0 = still, 1 = normal, 2+ = stormy)
    pub wave_intensity: f32,
    /// Wave style preset
    pub wave_style: WaveStyle,
    /// Enable reflections
    pub reflections: bool,
    /// Enable refractions
    pub refractions: bool,
    /// Foam intensity (0 = none, 1 = normal)
    pub foam: f32,
    /// Shore fade distance
    pub shore_fade: f32,
    /// Mesh size in world units
    pub size: f32,
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
            wave_intensity: 1.0,
            wave_style: WaveStyle::Calm,
            reflections: true,
            refractions: true,
            foam: 0.0,
            shore_fade: 5.0,
            size: 1000.0,
        }
    }
}

impl WaterBody {
    /// Create a water body at the specified height
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

    /// Set transparency (0 = opaque, 1 = fully transparent)
    pub fn with_transparency(mut self, transparency: f32) -> Self {
        self.transparency = transparency.clamp(0.0, 1.0);
        self
    }

    /// Set wave intensity (0 = still, 1 = normal, 2+ = stormy)
    pub fn with_wave_intensity(mut self, intensity: f32) -> Self {
        self.wave_intensity = intensity.max(0.0);
        self
    }

    /// Set foam intensity (0 = none, 1 = normal)
    pub fn with_foam(mut self, foam: f32) -> Self {
        self.foam = foam.clamp(0.0, 2.0);
        self
    }

    /// Set mesh size
    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size.max(1.0);
        self
    }

    /// Disable reflections
    pub fn without_reflections(mut self) -> Self {
        self.reflections = false;
        self
    }

    /// Disable refractions
    pub fn without_refractions(mut self) -> Self {
        self.refractions = false;
        self
    }

    // ========== Wave Style Setters ==========

    /// Still water (no waves)
    pub fn still(mut self) -> Self {
        self.wave_style = WaveStyle::Still;
        self.wave_intensity = 0.0;
        self
    }

    /// Calm water (gentle ripples)
    pub fn calm(mut self) -> Self {
        self.wave_style = WaveStyle::Calm;
        self
    }

    /// Ocean waves
    pub fn oceanic(mut self) -> Self {
        self.wave_style = WaveStyle::Ocean;
        self
    }

    /// River flow (directional waves)
    pub fn flowing(mut self) -> Self {
        self.wave_style = WaveStyle::River;
        self
    }

    // ========== Presets ==========

    /// Ocean water preset
    pub fn ocean(water_level: f32) -> Self {
        Self::new(water_level)
            .with_color(Color::rgba(0.05, 0.2, 0.4, 0.85))
            .oceanic()
            .with_foam(1.0)
    }

    /// Lake water preset
    pub fn lake(water_level: f32) -> Self {
        Self::new(water_level)
            .with_color(Color::rgba(0.1, 0.25, 0.35, 0.75))
            .calm()
            .with_transparency(0.7)
    }

    /// River water preset
    pub fn river(water_level: f32) -> Self {
        Self::new(water_level)
            .with_color(Color::rgba(0.15, 0.3, 0.4, 0.7))
            .flowing()
            .with_foam(0.5)
    }

    /// Pool/clear water preset
    pub fn pool(water_level: f32) -> Self {
        Self::new(water_level)
            .with_color(Color::rgba(0.2, 0.5, 0.7, 0.6))
            .with_transparency(0.85)
            .still()
    }

    /// Swamp water preset
    pub fn swamp(water_level: f32) -> Self {
        Self::new(water_level)
            .with_color(Color::rgba(0.15, 0.2, 0.1, 0.9))
            .with_transparency(0.2)
            .still()
    }
}

/// Wave animation style
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WaveStyle {
    /// No waves
    Still,
    /// Gentle ripples
    #[default]
    Calm,
    /// Ocean swells
    Ocean,
    /// Directional river flow
    River,
}

// ============================================================================
// Internal implementation - not exposed to users
// ============================================================================

pub(crate) mod internal {
    use super::*;
    use bytemuck::{Pod, Zeroable};

    /// GPU uniform data for water shader
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Pod, Zeroable)]
    pub struct WaterUniform {
        pub model: [[f32; 4]; 4],
        pub water_level: f32,
        pub time: f32,
        pub transparency: f32,
        pub fresnel_strength: f32,
        pub wave0: [f32; 4],
        pub wave1: [f32; 4],
        pub wave2: [f32; 4],
        pub wave3: [f32; 4],
        pub wave_dir01: [f32; 4],
        pub wave_dir23: [f32; 4],
        pub shallow_color: [f32; 4],
        pub deep_color: [f32; 4],
        pub foam_color: [f32; 4],
        pub foam_threshold: f32,
        pub foam_intensity: f32,
        pub shore_fade_distance: f32,
        pub specular_intensity: f32,
    }

    impl Default for WaterUniform {
        fn default() -> Self {
            Self {
                model: [
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ],
                water_level: 0.0,
                time: 0.0,
                transparency: 0.6,
                fresnel_strength: 1.0,
                wave0: [0.0; 4],
                wave1: [0.0; 4],
                wave2: [0.0; 4],
                wave3: [0.0; 4],
                wave_dir01: [1.0, 0.0, 0.8, 0.6],
                wave_dir23: [0.5, 0.9, 0.3, 1.0],
                shallow_color: [0.1, 0.4, 0.5, 1.0],
                deep_color: [0.0, 0.15, 0.3, 1.0],
                foam_color: [1.0, 1.0, 1.0, 0.9],
                foam_threshold: 0.3,
                foam_intensity: 0.0,
                shore_fade_distance: 5.0,
                specular_intensity: 0.8,
            }
        }
    }

    /// Water mesh vertex
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Pod, Zeroable)]
    pub struct WaterVertex {
        pub position: [f32; 3],
        pub uv: [f32; 2],
    }

    /// Build GPU uniform from WaterBody configuration
    pub fn build_uniform(water: &WaterBody, time: f32) -> WaterUniform {
        let intensity = water.wave_intensity;

        // Wave parameters based on style
        let (wave0, wave1, wave2, wave3) = match water.wave_style {
            WaveStyle::Still => (
                [0.0; 4],
                [0.0; 4],
                [0.0; 4],
                [0.0; 4],
            ),
            WaveStyle::Calm => (
                [0.02, 0.05 * intensity, 1.0, 0.1],
                [0.05, 0.02 * intensity, 0.7, 0.1],
                [0.0; 4],
                [0.0; 4],
            ),
            WaveStyle::Ocean => (
                [0.01, 0.5 * intensity, 0.8, 0.6],
                [0.02, 0.3 * intensity, 1.2, 0.5],
                [0.05, 0.1 * intensity, 0.5, 0.3],
                [0.1, 0.03 * intensity, 0.3, 0.2],
            ),
            WaveStyle::River => (
                [0.03, 0.08 * intensity, 2.0, 0.4],
                [0.08, 0.03 * intensity, 1.5, 0.3],
                [0.0; 4],
                [0.0; 4],
            ),
        };

        // Derive deep color from base color (darker)
        let deep_color = [
            water.color.r * 0.5,
            water.color.g * 0.7,
            water.color.b * 0.8,
            water.color.a,
        ];

        WaterUniform {
            water_level: water.water_level,
            time,
            transparency: water.transparency,
            fresnel_strength: water.fresnel_strength,
            wave0,
            wave1,
            wave2,
            wave3,
            shallow_color: [water.color.r, water.color.g, water.color.b, water.color.a],
            deep_color,
            foam_intensity: water.foam,
            foam_threshold: 0.3,
            shore_fade_distance: water.shore_fade,
            specular_intensity: water.specular_intensity,
            ..Default::default()
        }
    }

    /// Generate water mesh grid
    pub fn generate_mesh(size: f32, resolution: u32) -> (Vec<WaterVertex>, Vec<u32>) {
        let resolution = resolution.clamp(8, 256);
        let mut vertices = Vec::with_capacity((resolution * resolution) as usize);
        let mut indices = Vec::with_capacity(((resolution - 1) * (resolution - 1) * 6) as usize);

        let half_size = size * 0.5;

        for z in 0..resolution {
            for x in 0..resolution {
                let u = x as f32 / (resolution - 1) as f32;
                let v = z as f32 / (resolution - 1) as f32;

                vertices.push(WaterVertex {
                    position: [u * size - half_size, 0.0, v * size - half_size],
                    uv: [u, v],
                });
            }
        }

        for z in 0..(resolution - 1) {
            for x in 0..(resolution - 1) {
                let i00 = z * resolution + x;
                let i10 = i00 + 1;
                let i01 = i00 + resolution;
                let i11 = i01 + 1;

                indices.push(i00);
                indices.push(i01);
                indices.push(i10);
                indices.push(i10);
                indices.push(i01);
                indices.push(i11);
            }
        }

        (vertices, indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_water_presets() {
        let ocean = WaterBody::ocean(10.0);
        assert!((ocean.water_level - 10.0).abs() < 0.001);
        assert_eq!(ocean.wave_style, WaveStyle::Ocean);
        assert!(ocean.foam > 0.0);
    }

    #[test]
    fn test_water_builder() {
        let water = WaterBody::new(5.0)
            .with_transparency(0.8)
            .calm()
            .with_foam(0.5);

        assert!((water.water_level - 5.0).abs() < 0.001);
        assert!((water.transparency - 0.8).abs() < 0.001);
        assert_eq!(water.wave_style, WaveStyle::Calm);
    }

    #[test]
    fn test_wave_styles() {
        let still = WaterBody::new(0.0).still();
        assert_eq!(still.wave_style, WaveStyle::Still);
        assert!((still.wave_intensity - 0.0).abs() < 0.001);

        let ocean = WaterBody::new(0.0).oceanic();
        assert_eq!(ocean.wave_style, WaveStyle::Ocean);
    }
}
