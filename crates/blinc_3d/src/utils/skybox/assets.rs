//! Embedded skybox assets
//!
//! Provides basic HDRI skybox data for common scenarios.
//! These are compact representations, not full HDR images.

use super::{GradientSkybox, ProceduralSkybox, Skybox};
use blinc_core::Color;

/// Built-in skybox asset identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SkyboxAsset {
    /// Clear blue sky
    ClearDay,
    /// Overcast sky
    Cloudy,
    /// Golden hour sunset
    Sunset,
    /// Blue hour twilight
    Twilight,
    /// Night sky with stars
    Night,
    /// Studio environment (neutral gray)
    Studio,
    /// Abstract gradient (purple/pink)
    AbstractGradient,
    /// Desert environment
    Desert,
    /// Forest environment
    Forest,
    /// Ocean environment
    Ocean,
}

impl SkyboxAsset {
    /// Get all available assets
    pub fn all() -> &'static [SkyboxAsset] {
        &[
            SkyboxAsset::ClearDay,
            SkyboxAsset::Cloudy,
            SkyboxAsset::Sunset,
            SkyboxAsset::Twilight,
            SkyboxAsset::Night,
            SkyboxAsset::Studio,
            SkyboxAsset::AbstractGradient,
            SkyboxAsset::Desert,
            SkyboxAsset::Forest,
            SkyboxAsset::Ocean,
        ]
    }

    /// Get the display name
    pub fn name(&self) -> &'static str {
        match self {
            SkyboxAsset::ClearDay => "Clear Day",
            SkyboxAsset::Cloudy => "Cloudy",
            SkyboxAsset::Sunset => "Sunset",
            SkyboxAsset::Twilight => "Twilight",
            SkyboxAsset::Night => "Night",
            SkyboxAsset::Studio => "Studio",
            SkyboxAsset::AbstractGradient => "Abstract Gradient",
            SkyboxAsset::Desert => "Desert",
            SkyboxAsset::Forest => "Forest",
            SkyboxAsset::Ocean => "Ocean",
        }
    }

    /// Get a description of the asset
    pub fn description(&self) -> &'static str {
        match self {
            SkyboxAsset::ClearDay => "Bright blue sky with white clouds",
            SkyboxAsset::Cloudy => "Overcast gray sky",
            SkyboxAsset::Sunset => "Warm golden hour lighting",
            SkyboxAsset::Twilight => "Cool blue hour after sunset",
            SkyboxAsset::Night => "Dark sky with visible stars",
            SkyboxAsset::Studio => "Neutral studio lighting environment",
            SkyboxAsset::AbstractGradient => "Stylized purple-pink gradient",
            SkyboxAsset::Desert => "Warm sandy desert environment",
            SkyboxAsset::Forest => "Green forest canopy",
            SkyboxAsset::Ocean => "Blue ocean and sky",
        }
    }

    /// Load the skybox for this asset
    ///
    /// Returns a procedural or gradient skybox that approximates
    /// the intended environment without requiring external files.
    pub fn load(&self) -> Skybox {
        match self {
            SkyboxAsset::ClearDay => {
                Skybox::Procedural(ProceduralSkybox::midday())
            }
            SkyboxAsset::Cloudy => {
                let mut sky = GradientSkybox::new();
                sky.top_color = Color::rgb(0.6, 0.65, 0.7);
                sky.horizon_color = Color::rgb(0.75, 0.78, 0.8);
                sky.bottom_color = Color::rgb(0.5, 0.52, 0.55);
                sky.horizon_height = 0.1;
                Skybox::Gradient(sky)
            }
            SkyboxAsset::Sunset => {
                Skybox::Procedural(ProceduralSkybox::sunset())
            }
            SkyboxAsset::Twilight => {
                let mut sky = ProceduralSkybox::new();
                sky.set_time_of_day(19.5); // Just after sunset
                sky.sun_intensity = 0.3;
                Skybox::Procedural(sky)
            }
            SkyboxAsset::Night => {
                Skybox::Gradient(GradientSkybox::night())
            }
            SkyboxAsset::Studio => {
                let mut sky = GradientSkybox::new();
                sky.top_color = Color::rgb(0.3, 0.3, 0.32);
                sky.horizon_color = Color::rgb(0.4, 0.4, 0.42);
                sky.bottom_color = Color::rgb(0.25, 0.25, 0.27);
                sky.horizon_height = 0.0;
                Skybox::Gradient(sky)
            }
            SkyboxAsset::AbstractGradient => {
                let mut sky = GradientSkybox::new();
                sky.top_color = Color::rgb(0.2, 0.1, 0.4);
                sky.horizon_color = Color::rgb(0.6, 0.3, 0.5);
                sky.bottom_color = Color::rgb(0.1, 0.05, 0.15);
                sky.horizon_height = -0.1;
                Skybox::Gradient(sky)
            }
            SkyboxAsset::Desert => {
                let mut sky = GradientSkybox::new();
                sky.top_color = Color::rgb(0.4, 0.6, 0.85);
                sky.horizon_color = Color::rgb(0.9, 0.85, 0.7);
                sky.bottom_color = Color::rgb(0.85, 0.75, 0.6);
                sky.horizon_height = 0.05;
                Skybox::Gradient(sky)
            }
            SkyboxAsset::Forest => {
                let mut sky = GradientSkybox::new();
                sky.top_color = Color::rgb(0.3, 0.5, 0.7);
                sky.horizon_color = Color::rgb(0.5, 0.65, 0.5);
                sky.bottom_color = Color::rgb(0.2, 0.35, 0.2);
                sky.horizon_height = 0.15;
                Skybox::Gradient(sky)
            }
            SkyboxAsset::Ocean => {
                let mut sky = GradientSkybox::new();
                sky.top_color = Color::rgb(0.3, 0.5, 0.8);
                sky.horizon_color = Color::rgb(0.6, 0.75, 0.9);
                sky.bottom_color = Color::rgb(0.1, 0.3, 0.5);
                sky.horizon_height = 0.0;
                Skybox::Gradient(sky)
            }
        }
    }

    /// Check if this asset is suitable for a given time of day
    pub fn suitable_for_time(&self, hour: f32) -> bool {
        let hour = hour % 24.0;
        match self {
            SkyboxAsset::ClearDay => hour >= 9.0 && hour < 17.0,
            SkyboxAsset::Cloudy => true, // Suitable any time
            SkyboxAsset::Sunset => hour >= 17.0 && hour < 20.0,
            SkyboxAsset::Twilight => hour >= 19.0 && hour < 21.0,
            SkyboxAsset::Night => hour >= 21.0 || hour < 5.0,
            SkyboxAsset::Studio => true, // Studio lighting is time-independent
            SkyboxAsset::AbstractGradient => true, // Stylized, time-independent
            SkyboxAsset::Desert => hour >= 6.0 && hour < 19.0,
            SkyboxAsset::Forest => hour >= 7.0 && hour < 18.0,
            SkyboxAsset::Ocean => hour >= 6.0 && hour < 20.0,
        }
    }

    /// Get a recommended asset for a given time of day
    pub fn for_time(hour: f32) -> Self {
        let hour = hour % 24.0;
        if hour >= 5.0 && hour < 7.0 {
            SkyboxAsset::Twilight
        } else if hour >= 7.0 && hour < 17.0 {
            SkyboxAsset::ClearDay
        } else if hour >= 17.0 && hour < 20.0 {
            SkyboxAsset::Sunset
        } else if hour >= 20.0 && hour < 21.0 {
            SkyboxAsset::Twilight
        } else {
            SkyboxAsset::Night
        }
    }
}

/// Embedded HDRI data (placeholder for actual embedded images)
///
/// In a production implementation, this would contain actual
/// compressed HDRI data. For now, it provides procedural approximations.
pub struct EmbeddedHdri {
    /// Asset identifier
    pub asset: SkyboxAsset,
    /// Approximate dominant color (for quick previews)
    pub dominant_color: Color,
    /// Approximate ambient light color
    pub ambient_color: Color,
    /// Suggested sun/key light direction
    pub light_direction: blinc_core::Vec3,
    /// Suggested light intensity
    pub light_intensity: f32,
}

impl EmbeddedHdri {
    /// Get embedded HDRI data for an asset
    pub fn get(asset: SkyboxAsset) -> Self {
        match asset {
            SkyboxAsset::ClearDay => Self {
                asset,
                dominant_color: Color::rgb(0.5, 0.7, 1.0),
                ambient_color: Color::rgb(0.4, 0.5, 0.6),
                light_direction: blinc_core::Vec3::new(-0.5, -1.0, -0.3),
                light_intensity: 1.0,
            },
            SkyboxAsset::Cloudy => Self {
                asset,
                dominant_color: Color::rgb(0.7, 0.72, 0.75),
                ambient_color: Color::rgb(0.5, 0.52, 0.55),
                light_direction: blinc_core::Vec3::new(0.0, -1.0, 0.0),
                light_intensity: 0.6,
            },
            SkyboxAsset::Sunset => Self {
                asset,
                dominant_color: Color::rgb(1.0, 0.6, 0.3),
                ambient_color: Color::rgb(0.5, 0.3, 0.2),
                light_direction: blinc_core::Vec3::new(-0.9, -0.2, -0.3),
                light_intensity: 0.8,
            },
            SkyboxAsset::Twilight => Self {
                asset,
                dominant_color: Color::rgb(0.3, 0.4, 0.6),
                ambient_color: Color::rgb(0.2, 0.25, 0.35),
                light_direction: blinc_core::Vec3::new(-0.95, -0.1, -0.2),
                light_intensity: 0.3,
            },
            SkyboxAsset::Night => Self {
                asset,
                dominant_color: Color::rgb(0.05, 0.08, 0.15),
                ambient_color: Color::rgb(0.02, 0.03, 0.05),
                light_direction: blinc_core::Vec3::new(0.3, -0.8, 0.5),
                light_intensity: 0.1,
            },
            SkyboxAsset::Studio => Self {
                asset,
                dominant_color: Color::rgb(0.4, 0.4, 0.42),
                ambient_color: Color::rgb(0.3, 0.3, 0.32),
                light_direction: blinc_core::Vec3::new(-0.5, -0.7, -0.5),
                light_intensity: 0.8,
            },
            SkyboxAsset::AbstractGradient => Self {
                asset,
                dominant_color: Color::rgb(0.4, 0.2, 0.5),
                ambient_color: Color::rgb(0.2, 0.1, 0.25),
                light_direction: blinc_core::Vec3::new(0.0, -1.0, 0.0),
                light_intensity: 0.5,
            },
            SkyboxAsset::Desert => Self {
                asset,
                dominant_color: Color::rgb(0.9, 0.8, 0.65),
                ambient_color: Color::rgb(0.6, 0.55, 0.45),
                light_direction: blinc_core::Vec3::new(-0.4, -0.9, -0.2),
                light_intensity: 1.2,
            },
            SkyboxAsset::Forest => Self {
                asset,
                dominant_color: Color::rgb(0.4, 0.55, 0.4),
                ambient_color: Color::rgb(0.25, 0.35, 0.25),
                light_direction: blinc_core::Vec3::new(-0.3, -0.8, -0.5),
                light_intensity: 0.7,
            },
            SkyboxAsset::Ocean => Self {
                asset,
                dominant_color: Color::rgb(0.4, 0.6, 0.8),
                ambient_color: Color::rgb(0.3, 0.4, 0.5),
                light_direction: blinc_core::Vec3::new(-0.5, -0.8, -0.3),
                light_intensity: 0.9,
            },
        }
    }

    /// Load the skybox for this HDRI
    pub fn to_skybox(&self) -> Skybox {
        self.asset.load()
    }
}
