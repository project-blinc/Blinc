//! Built-in lighting presets

use super::{LightConfig, apply_lights};
use crate::ecs::World;
use blinc_core::{Color, Vec3};
use std::f32::consts::PI;

/// Trait for lighting presets
pub trait LightingPreset {
    /// Get preset name
    fn name(&self) -> &'static str;

    /// Get light configurations
    fn lights(&self) -> Vec<LightConfig>;

    /// Apply preset to a world
    fn apply(&self, world: &mut World) {
        apply_lights(world, &self.lights());
    }
}

/// Built-in lighting presets
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinPreset {
    /// Classic 3-point studio lighting
    Studio,
    /// Softer studio lighting with fill
    StudioSoft,
    /// Outdoor sunlight
    Outdoor,
    /// Cloudy/overcast outdoor
    OutdoorCloudy,
    /// High-contrast dramatic lighting
    Dramatic,
    /// Colorful neon/cyberpunk lighting
    Neon,
    /// Warm sunset-like lighting
    Warm,
    /// Cool moonlight-like lighting
    Cool,
    /// Neutral balanced lighting
    Neutral,
    /// Dark ambient with spotlight
    Spotlight,
    /// Soft product photography lighting
    Product,
}

impl LightingPreset for BuiltinPreset {
    fn name(&self) -> &'static str {
        match self {
            BuiltinPreset::Studio => "Studio",
            BuiltinPreset::StudioSoft => "Studio Soft",
            BuiltinPreset::Outdoor => "Outdoor",
            BuiltinPreset::OutdoorCloudy => "Outdoor Cloudy",
            BuiltinPreset::Dramatic => "Dramatic",
            BuiltinPreset::Neon => "Neon",
            BuiltinPreset::Warm => "Warm",
            BuiltinPreset::Cool => "Cool",
            BuiltinPreset::Neutral => "Neutral",
            BuiltinPreset::Spotlight => "Spotlight",
            BuiltinPreset::Product => "Product",
        }
    }

    fn lights(&self) -> Vec<LightConfig> {
        match self {
            BuiltinPreset::Studio => studio_preset(),
            BuiltinPreset::StudioSoft => studio_soft_preset(),
            BuiltinPreset::Outdoor => outdoor_preset(),
            BuiltinPreset::OutdoorCloudy => outdoor_cloudy_preset(),
            BuiltinPreset::Dramatic => dramatic_preset(),
            BuiltinPreset::Neon => neon_preset(),
            BuiltinPreset::Warm => warm_preset(),
            BuiltinPreset::Cool => cool_preset(),
            BuiltinPreset::Neutral => neutral_preset(),
            BuiltinPreset::Spotlight => spotlight_preset(),
            BuiltinPreset::Product => product_preset(),
        }
    }
}

/// Classic 3-point studio lighting
fn studio_preset() -> Vec<LightConfig> {
    vec![
        // Key light (main)
        LightConfig::directional(
            Color::rgb(1.0, 0.98, 0.95),
            1.0,
            Vec3::new(-1.0, -1.0, -1.0),
        ).with_shadows(true),
        // Fill light (softer, from opposite side)
        LightConfig::directional(
            Color::rgb(0.8, 0.85, 1.0),
            0.4,
            Vec3::new(1.0, -0.5, -0.5),
        ).with_shadows(false),
        // Back/rim light
        LightConfig::directional(
            Color::rgb(1.0, 1.0, 1.0),
            0.3,
            Vec3::new(0.0, -1.0, 1.0),
        ).with_shadows(false),
        // Ambient fill
        LightConfig::ambient(Color::rgb(0.2, 0.2, 0.25), 0.3),
    ]
}

/// Softer studio lighting
fn studio_soft_preset() -> Vec<LightConfig> {
    vec![
        // Soft key light
        LightConfig::hemisphere(
            Color::rgb(1.0, 0.98, 0.95),
            Color::rgb(0.3, 0.25, 0.2),
            0.6,
        ),
        // Soft directional
        LightConfig::directional(
            Color::rgb(1.0, 0.98, 0.95),
            0.6,
            Vec3::new(-0.5, -1.0, -0.5),
        ).with_shadows(true),
        // Fill
        LightConfig::ambient(Color::rgb(0.4, 0.4, 0.45), 0.4),
    ]
}

/// Outdoor sunlight
fn outdoor_preset() -> Vec<LightConfig> {
    vec![
        // Sun
        LightConfig::directional(
            Color::rgb(1.0, 0.95, 0.85),
            1.2,
            Vec3::new(-0.5, -1.0, -0.3),
        ).with_shadows(true),
        // Sky hemisphere
        LightConfig::hemisphere(
            Color::rgb(0.6, 0.75, 1.0),
            Color::rgb(0.4, 0.35, 0.3),
            0.5,
        ),
        // Ambient
        LightConfig::ambient(Color::rgb(0.3, 0.35, 0.4), 0.2),
    ]
}

/// Cloudy outdoor lighting
fn outdoor_cloudy_preset() -> Vec<LightConfig> {
    vec![
        // Diffused sun
        LightConfig::directional(
            Color::rgb(0.85, 0.85, 0.9),
            0.6,
            Vec3::new(-0.2, -1.0, -0.2),
        ).with_shadows(true),
        // Sky hemisphere
        LightConfig::hemisphere(
            Color::rgb(0.7, 0.75, 0.8),
            Color::rgb(0.35, 0.35, 0.35),
            0.6,
        ),
        // Higher ambient for diffused light
        LightConfig::ambient(Color::rgb(0.5, 0.5, 0.55), 0.4),
    ]
}

/// High-contrast dramatic lighting
fn dramatic_preset() -> Vec<LightConfig> {
    vec![
        // Strong single light source
        LightConfig::spot(
            Color::rgb(1.0, 0.9, 0.8),
            2.0,
            Vec3::new(5.0, 8.0, 5.0),
            Vec3::new(-0.5, -0.8, -0.5),
        )
        .with_angle(PI / 4.0)
        .with_penumbra(0.3)
        .with_shadows(true),
        // Very dark ambient
        LightConfig::ambient(Color::rgb(0.05, 0.05, 0.08), 0.2),
        // Subtle rim light
        LightConfig::directional(
            Color::rgb(0.3, 0.4, 0.5),
            0.2,
            Vec3::new(0.5, 0.0, 1.0),
        ).with_shadows(false),
    ]
}

/// Colorful neon/cyberpunk lighting
fn neon_preset() -> Vec<LightConfig> {
    vec![
        // Pink neon
        LightConfig::point(
            Color::rgb(1.0, 0.2, 0.6),
            1.5,
            Vec3::new(-5.0, 2.0, 3.0),
        ).with_distance(15.0),
        // Cyan neon
        LightConfig::point(
            Color::rgb(0.2, 0.8, 1.0),
            1.5,
            Vec3::new(5.0, 2.0, -3.0),
        ).with_distance(15.0),
        // Purple accent
        LightConfig::point(
            Color::rgb(0.6, 0.2, 1.0),
            0.8,
            Vec3::new(0.0, 5.0, 0.0),
        ).with_distance(12.0),
        // Dark ambient
        LightConfig::ambient(Color::rgb(0.03, 0.02, 0.05), 0.3),
    ]
}

/// Warm sunset-like lighting
fn warm_preset() -> Vec<LightConfig> {
    vec![
        // Warm sun
        LightConfig::directional(
            Color::rgb(1.0, 0.7, 0.4),
            1.0,
            Vec3::new(-1.0, -0.3, -0.5),
        ).with_shadows(true),
        // Orange-pink sky
        LightConfig::hemisphere(
            Color::rgb(1.0, 0.6, 0.4),
            Color::rgb(0.3, 0.2, 0.15),
            0.4,
        ),
        // Warm ambient
        LightConfig::ambient(Color::rgb(0.3, 0.2, 0.15), 0.3),
    ]
}

/// Cool moonlight-like lighting
fn cool_preset() -> Vec<LightConfig> {
    vec![
        // Moon
        LightConfig::directional(
            Color::rgb(0.7, 0.8, 1.0),
            0.4,
            Vec3::new(0.5, -1.0, 0.3),
        ).with_shadows(true),
        // Night sky
        LightConfig::hemisphere(
            Color::rgb(0.1, 0.15, 0.3),
            Color::rgb(0.05, 0.05, 0.08),
            0.3,
        ),
        // Dark blue ambient
        LightConfig::ambient(Color::rgb(0.05, 0.07, 0.12), 0.4),
    ]
}

/// Neutral balanced lighting
fn neutral_preset() -> Vec<LightConfig> {
    vec![
        // Main light
        LightConfig::directional(
            Color::WHITE,
            0.8,
            Vec3::new(-1.0, -1.0, -1.0),
        ).with_shadows(true),
        // Neutral hemisphere
        LightConfig::hemisphere(
            Color::rgb(0.8, 0.8, 0.8),
            Color::rgb(0.4, 0.4, 0.4),
            0.4,
        ),
        // Neutral ambient
        LightConfig::ambient(Color::rgb(0.3, 0.3, 0.3), 0.3),
    ]
}

/// Dark ambient with spotlight
fn spotlight_preset() -> Vec<LightConfig> {
    vec![
        // Main spotlight
        LightConfig::spot(
            Color::WHITE,
            2.0,
            Vec3::new(0.0, 10.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
        )
        .with_angle(PI / 8.0)
        .with_penumbra(0.2)
        .with_distance(20.0)
        .with_shadows(true),
        // Very dark ambient
        LightConfig::ambient(Color::rgb(0.02, 0.02, 0.02), 0.5),
    ]
}

/// Soft product photography lighting
fn product_preset() -> Vec<LightConfig> {
    vec![
        // Large soft key light (simulated with hemisphere)
        LightConfig::hemisphere(
            Color::WHITE,
            Color::rgb(0.6, 0.6, 0.6),
            0.8,
        ),
        // Front fill
        LightConfig::directional(
            Color::WHITE,
            0.4,
            Vec3::new(0.0, -0.3, -1.0),
        ).with_shadows(false),
        // Top light
        LightConfig::directional(
            Color::WHITE,
            0.3,
            Vec3::new(0.0, -1.0, 0.0),
        ).with_shadows(true),
        // High ambient for soft shadows
        LightConfig::ambient(Color::rgb(0.5, 0.5, 0.5), 0.4),
    ]
}
