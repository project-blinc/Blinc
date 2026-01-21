//! Skybox system
//!
//! Provides GPU-driven skybox rendering with procedural, gradient, and cubemap options.
//!
//! # Example
//!
//! ```ignore
//! use blinc_3d::utils::skybox::{Skybox, SkyboxAsset};
//!
//! // Procedural atmospheric sky
//! let sky = Skybox::procedural()
//!     .with_sun_direction(Vec3::new(0.5, -0.8, 0.3))
//!     .with_turbidity(2.0);
//!
//! // Gradient sky
//! let sky = Skybox::gradient()
//!     .with_sky_color(Color::rgb(0.2, 0.4, 0.8))
//!     .with_horizon_color(Color::rgb(0.8, 0.9, 1.0));
//!
//! // From preset
//! let sky = SkyboxAsset::Sunset.load();
//!
//! // Attach to entity - rendering is automatic
//! world.spawn().insert(sky);
//! ```

mod download;

pub use download::{
    AssetDownloadHelper, DownloadConfig, DownloadRequest, DownloadStatus, HdriAsset,
    HdriResolution, HdriSource, filter_by_source, polyhaven_popular, search_by_tag,
};

use crate::ecs::{Component, System, SystemContext, SystemStage};
use crate::materials::TextureHandle;
use blinc_core::{Color, Vec3};
use std::f32::consts::PI;

/// Skybox component
///
/// Attach to an entity to render a skybox in the scene.
/// Rendering is handled entirely on the GPU via the skybox shader.
#[derive(Clone, Debug)]
pub struct Skybox {
    /// Skybox type
    mode: SkyboxMode,
    /// Sun direction (normalized, pointing towards sun)
    pub sun_direction: Vec3,
    /// Sun color and intensity
    pub sun_color: Color,
    /// Sun angular size (radians)
    pub sun_size: f32,
    /// Sky color (zenith)
    pub sky_color: Color,
    /// Horizon color
    pub horizon_color: Color,
    /// Ground color (below horizon)
    pub ground_color: Color,
    /// Atmosphere density (affects gradient sharpness)
    pub atmosphere_density: f32,
    /// Exposure adjustment
    pub exposure: f32,
    /// Cubemap texture (when mode is Cubemap)
    pub cubemap_texture: Option<TextureHandle>,
    /// Cubemap rotation (radians)
    pub cubemap_rotation: f32,
    /// Cubemap tint
    pub cubemap_tint: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum SkyboxMode {
    /// Procedural atmospheric scattering
    #[default]
    Procedural,
    /// Simple gradient
    Gradient,
    /// Cubemap texture
    Cubemap,
}

impl Component for Skybox {}

impl Default for Skybox {
    fn default() -> Self {
        Self::procedural()
    }
}

impl Skybox {
    /// Create a procedural atmospheric sky
    pub fn procedural() -> Self {
        Self {
            mode: SkyboxMode::Procedural,
            sun_direction: Vec3::new(0.0, -1.0, 0.0),
            sun_color: Color::rgba(1.0, 0.98, 0.92, 1.0),
            sun_size: 0.0093,
            sky_color: Color::rgb(0.25, 0.45, 0.85),
            horizon_color: Color::rgb(0.65, 0.8, 1.0),
            ground_color: Color::rgb(0.35, 0.32, 0.28),
            atmosphere_density: 1.0,
            exposure: 1.0,
            cubemap_texture: None,
            cubemap_rotation: 0.0,
            cubemap_tint: Color::WHITE,
        }
    }

    /// Create a simple gradient sky
    pub fn gradient() -> Self {
        Self {
            mode: SkyboxMode::Gradient,
            sun_direction: Vec3::new(0.0, -1.0, 0.0),
            sun_color: Color::rgba(1.0, 0.98, 0.92, 0.0), // No sun disc in gradient
            sun_size: 0.0,
            sky_color: Color::rgb(0.25, 0.45, 0.85),
            horizon_color: Color::rgb(0.65, 0.8, 1.0),
            ground_color: Color::rgb(0.35, 0.32, 0.28),
            atmosphere_density: 1.0,
            exposure: 1.0,
            cubemap_texture: None,
            cubemap_rotation: 0.0,
            cubemap_tint: Color::WHITE,
        }
    }

    /// Create a cubemap skybox from a texture
    pub fn cubemap(texture: TextureHandle) -> Self {
        Self {
            mode: SkyboxMode::Cubemap,
            sun_direction: Vec3::new(0.0, -1.0, 0.0),
            sun_color: Color::TRANSPARENT,
            sun_size: 0.0,
            sky_color: Color::BLACK,
            horizon_color: Color::BLACK,
            ground_color: Color::BLACK,
            atmosphere_density: 1.0,
            exposure: 1.0,
            cubemap_texture: Some(texture),
            cubemap_rotation: 0.0,
            cubemap_tint: Color::WHITE,
        }
    }

    // ========== Builder Methods ==========

    /// Set sun direction (will be normalized)
    pub fn with_sun_direction(mut self, direction: Vec3) -> Self {
        let len = (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z).sqrt();
        if len > 1e-6 {
            self.sun_direction = Vec3::new(direction.x / len, direction.y / len, direction.z / len);
        }
        self
    }

    /// Set sun color and intensity
    pub fn with_sun_color(mut self, color: Color) -> Self {
        self.sun_color = color;
        self
    }

    /// Set sun angular size
    pub fn with_sun_size(mut self, size: f32) -> Self {
        self.sun_size = size.max(0.0);
        self
    }

    /// Set sky color (zenith)
    pub fn with_sky_color(mut self, color: Color) -> Self {
        self.sky_color = color;
        self
    }

    /// Set horizon color
    pub fn with_horizon_color(mut self, color: Color) -> Self {
        self.horizon_color = color;
        self
    }

    /// Set ground color (below horizon)
    pub fn with_ground_color(mut self, color: Color) -> Self {
        self.ground_color = color;
        self
    }

    /// Set atmosphere density (affects gradient transition)
    pub fn with_atmosphere_density(mut self, density: f32) -> Self {
        self.atmosphere_density = density.max(0.0);
        self
    }

    /// Set exposure
    pub fn with_exposure(mut self, exposure: f32) -> Self {
        self.exposure = exposure.max(0.0);
        self
    }

    /// Set turbidity (for procedural sky, 1.0 = clear, 10.0 = hazy)
    pub fn with_turbidity(mut self, turbidity: f32) -> Self {
        let t = turbidity.clamp(1.0, 10.0);
        self.atmosphere_density = 0.5 + t * 0.1;
        let haze = (t - 1.0) / 9.0;
        self.horizon_color = Color::lerp(&self.horizon_color, &Color::rgb(0.85, 0.88, 0.9), haze * 0.5);
        self
    }

    /// Set sun position from azimuth and elevation angles (radians)
    pub fn with_sun_angles(mut self, azimuth: f32, elevation: f32) -> Self {
        let cos_elev = elevation.cos();
        self.sun_direction = Vec3::new(
            cos_elev * azimuth.sin(),
            -elevation.sin(),
            cos_elev * azimuth.cos(),
        );
        self
    }

    /// Set sun position from time of day (0-24 hours)
    pub fn with_time_of_day(mut self, hour: f32) -> Self {
        let normalized = ((hour - 6.0) / 12.0).clamp(0.0, 1.0);
        let elevation = (normalized * PI).sin() * (PI / 2.2);
        let azimuth = (normalized - 0.5) * PI;

        let cos_elev = elevation.cos();
        self.sun_direction = Vec3::new(
            cos_elev * azimuth.sin(),
            -elevation.sin(),
            cos_elev * azimuth.cos(),
        );

        if hour < 7.0 || hour > 17.0 {
            let t = if hour < 7.0 {
                (hour - 5.0) / 2.0
            } else {
                (19.0 - hour) / 2.0
            }
            .clamp(0.0, 1.0);

            self.sun_color = Color::lerp(&Color::rgb(1.0, 0.4, 0.2), &Color::rgb(1.0, 0.98, 0.92), t);
            self.sun_color.a = 0.5 + t * 0.5;
        }

        self
    }

    /// Set cubemap rotation (radians)
    pub fn with_rotation(mut self, rotation: f32) -> Self {
        self.cubemap_rotation = rotation;
        self
    }

    /// Set cubemap tint
    pub fn with_tint(mut self, tint: Color) -> Self {
        self.cubemap_tint = tint;
        self
    }

    // ========== Presets ==========

    /// Clear day preset
    pub fn clear_day() -> Self {
        Self::procedural()
            .with_sun_angles(0.0, PI / 3.0)
            .with_turbidity(2.0)
    }

    /// Midday preset
    pub fn midday() -> Self {
        Self::procedural()
            .with_sun_angles(0.0, PI / 2.2)
            .with_turbidity(2.0)
    }

    /// Sunset preset
    pub fn sunset() -> Self {
        Self::procedural()
            .with_sun_angles(PI * 0.4, PI / 12.0)
            .with_sun_color(Color::rgba(1.0, 0.5, 0.2, 0.8))
            .with_turbidity(4.0)
            .with_sky_color(Color::rgb(0.15, 0.2, 0.4))
            .with_horizon_color(Color::rgb(1.0, 0.5, 0.2))
    }

    /// Sunrise preset
    pub fn sunrise() -> Self {
        Self::procedural()
            .with_sun_angles(-PI * 0.4, PI / 12.0)
            .with_sun_color(Color::rgba(1.0, 0.6, 0.3, 0.7))
            .with_turbidity(3.0)
            .with_sky_color(Color::rgb(0.2, 0.25, 0.5))
            .with_horizon_color(Color::rgb(1.0, 0.6, 0.4))
    }

    /// Night preset
    pub fn night() -> Self {
        Self::gradient()
            .with_sky_color(Color::rgb(0.02, 0.02, 0.05))
            .with_horizon_color(Color::rgb(0.08, 0.1, 0.15))
            .with_ground_color(Color::rgb(0.02, 0.02, 0.02))
    }

    /// Overcast preset
    pub fn overcast() -> Self {
        Self::gradient()
            .with_sky_color(Color::rgb(0.5, 0.52, 0.55))
            .with_horizon_color(Color::rgb(0.6, 0.62, 0.65))
            .with_ground_color(Color::rgb(0.35, 0.35, 0.35))
            .with_atmosphere_density(0.5)
    }

    /// Hazy preset
    pub fn hazy() -> Self {
        Self::procedural()
            .with_sun_angles(0.2, PI / 4.0)
            .with_turbidity(8.0)
    }

    // ========== Queries ==========

    /// Check if this is a cubemap skybox
    pub fn is_cubemap(&self) -> bool {
        self.mode == SkyboxMode::Cubemap
    }

    /// Check if sun is above horizon
    pub fn is_daytime(&self) -> bool {
        self.sun_direction.y < 0.0
    }

    /// Get sun elevation angle (radians, 0 = horizon)
    pub fn sun_elevation(&self) -> f32 {
        (-self.sun_direction.y).asin()
    }
}

// ============================================================================
// Time of Day System
// ============================================================================

/// Time of day preset
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeOfDay {
    /// Early morning (5-7)
    Dawn,
    /// Morning (7-10)
    Morning,
    /// Midday (10-14)
    Noon,
    /// Afternoon (14-17)
    Afternoon,
    /// Evening (17-19)
    Dusk,
    /// Night (19-5)
    Night,
}

impl TimeOfDay {
    /// Get the approximate hour for this time of day
    pub fn hour(&self) -> f32 {
        match self {
            TimeOfDay::Dawn => 6.0,
            TimeOfDay::Morning => 8.5,
            TimeOfDay::Noon => 12.0,
            TimeOfDay::Afternoon => 15.5,
            TimeOfDay::Dusk => 18.0,
            TimeOfDay::Night => 22.0,
        }
    }

    /// Determine time of day from hour (0-24)
    pub fn from_hour(hour: f32) -> Self {
        let hour = hour % 24.0;
        if hour < 5.0 || hour >= 21.0 {
            TimeOfDay::Night
        } else if hour < 7.0 {
            TimeOfDay::Dawn
        } else if hour < 10.0 {
            TimeOfDay::Morning
        } else if hour < 14.0 {
            TimeOfDay::Noon
        } else if hour < 17.0 {
            TimeOfDay::Afternoon
        } else {
            TimeOfDay::Dusk
        }
    }

    /// Get a skybox for this time
    pub fn to_skybox(&self) -> Skybox {
        match self {
            TimeOfDay::Dawn => Skybox::sunrise(),
            TimeOfDay::Morning => Skybox::procedural().with_time_of_day(8.5),
            TimeOfDay::Noon => Skybox::midday(),
            TimeOfDay::Afternoon => Skybox::procedural().with_time_of_day(15.5),
            TimeOfDay::Dusk => Skybox::sunset(),
            TimeOfDay::Night => Skybox::night(),
        }
    }
}

/// Day/night cycle configuration
#[derive(Clone, Debug)]
pub struct DayNightCycle {
    /// Current time in hours (0-24)
    pub current_hour: f32,
    /// Speed multiplier (1.0 = real-time, 60.0 = 1 minute = 1 hour)
    pub speed: f32,
    /// Whether cycle is paused
    pub paused: bool,
}

impl DayNightCycle {
    /// Create a new day/night cycle starting at noon
    pub fn new() -> Self {
        Self {
            current_hour: 12.0,
            speed: 60.0,
            paused: false,
        }
    }

    /// Create starting at specific hour
    pub fn starting_at(hour: f32) -> Self {
        Self {
            current_hour: hour % 24.0,
            ..Self::new()
        }
    }

    /// Set speed
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    /// Get current time of day
    pub fn time_of_day(&self) -> TimeOfDay {
        TimeOfDay::from_hour(self.current_hour)
    }

    /// Update cycle
    pub fn update(&mut self, dt: f32) {
        if !self.paused {
            self.current_hour += (dt / 3600.0) * self.speed;
            self.current_hour %= 24.0;
        }
    }

    /// Get current skybox
    pub fn current_skybox(&self) -> Skybox {
        Skybox::procedural().with_time_of_day(self.current_hour)
    }

    /// Pause the cycle
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resume the cycle
    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// Set time directly (0-24)
    pub fn set_hour(&mut self, hour: f32) {
        self.current_hour = hour % 24.0;
    }

    /// Check if it's currently daytime
    pub fn is_daytime(&self) -> bool {
        self.current_hour >= 6.0 && self.current_hour < 20.0
    }

    /// Get sun elevation (0 = horizon, PI/2 = zenith)
    pub fn sun_elevation(&self) -> f32 {
        if !self.is_daytime() {
            return 0.0;
        }
        let normalized = ((self.current_hour - 6.0) / 14.0).clamp(0.0, 1.0);
        (normalized * PI).sin() * (PI / 2.2)
    }
}

impl Default for DayNightCycle {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for DayNightCycle {}

/// System that updates day/night cycle
pub struct TimeOfDaySystem;

impl System for TimeOfDaySystem {
    fn run(&mut self, ctx: &mut SystemContext) {
        let dt = ctx.delta_time;

        let entities_to_update: Vec<_> = ctx
            .world
            .query::<(&DayNightCycle,)>()
            .iter()
            .map(|(entity, _)| entity)
            .collect();

        for entity in entities_to_update {
            if let Some(cycle) = ctx.world.get_mut::<DayNightCycle>(entity) {
                cycle.update(dt);
            }
        }
    }

    fn name(&self) -> &'static str {
        "TimeOfDaySystem"
    }

    fn stage(&self) -> SystemStage {
        SystemStage::Update
    }

    fn priority(&self) -> i32 {
        -10
    }
}

// ============================================================================
// Built-in Skybox Assets
// ============================================================================

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
    /// Night sky
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

    /// Load the skybox for this asset
    pub fn load(&self) -> Skybox {
        match self {
            SkyboxAsset::ClearDay => Skybox::midday(),
            SkyboxAsset::Cloudy => Skybox::overcast(),
            SkyboxAsset::Sunset => Skybox::sunset(),
            SkyboxAsset::Twilight => Skybox::procedural().with_time_of_day(19.5),
            SkyboxAsset::Night => Skybox::night(),
            SkyboxAsset::Studio => Skybox::gradient()
                .with_sky_color(Color::rgb(0.3, 0.3, 0.32))
                .with_horizon_color(Color::rgb(0.4, 0.4, 0.42))
                .with_ground_color(Color::rgb(0.25, 0.25, 0.27)),
            SkyboxAsset::AbstractGradient => Skybox::gradient()
                .with_sky_color(Color::rgb(0.2, 0.1, 0.4))
                .with_horizon_color(Color::rgb(0.6, 0.3, 0.5))
                .with_ground_color(Color::rgb(0.1, 0.05, 0.15)),
            SkyboxAsset::Desert => Skybox::gradient()
                .with_sky_color(Color::rgb(0.4, 0.6, 0.85))
                .with_horizon_color(Color::rgb(0.9, 0.85, 0.7))
                .with_ground_color(Color::rgb(0.85, 0.75, 0.6)),
            SkyboxAsset::Forest => Skybox::gradient()
                .with_sky_color(Color::rgb(0.3, 0.5, 0.7))
                .with_horizon_color(Color::rgb(0.5, 0.65, 0.5))
                .with_ground_color(Color::rgb(0.2, 0.35, 0.2)),
            SkyboxAsset::Ocean => Skybox::gradient()
                .with_sky_color(Color::rgb(0.3, 0.5, 0.8))
                .with_horizon_color(Color::rgb(0.6, 0.75, 0.9))
                .with_ground_color(Color::rgb(0.1, 0.3, 0.5)),
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

/// Embedded HDRI lighting data
pub struct EmbeddedHdri {
    /// Asset identifier
    pub asset: SkyboxAsset,
    /// Approximate dominant color
    pub dominant_color: Color,
    /// Approximate ambient light color
    pub ambient_color: Color,
    /// Suggested light direction
    pub light_direction: Vec3,
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
                light_direction: Vec3::new(-0.5, -1.0, -0.3),
                light_intensity: 1.0,
            },
            SkyboxAsset::Cloudy => Self {
                asset,
                dominant_color: Color::rgb(0.7, 0.72, 0.75),
                ambient_color: Color::rgb(0.5, 0.52, 0.55),
                light_direction: Vec3::new(0.0, -1.0, 0.0),
                light_intensity: 0.6,
            },
            SkyboxAsset::Sunset => Self {
                asset,
                dominant_color: Color::rgb(1.0, 0.6, 0.3),
                ambient_color: Color::rgb(0.5, 0.3, 0.2),
                light_direction: Vec3::new(-0.9, -0.2, -0.3),
                light_intensity: 0.8,
            },
            SkyboxAsset::Twilight => Self {
                asset,
                dominant_color: Color::rgb(0.3, 0.4, 0.6),
                ambient_color: Color::rgb(0.2, 0.25, 0.35),
                light_direction: Vec3::new(-0.95, -0.1, -0.2),
                light_intensity: 0.3,
            },
            SkyboxAsset::Night => Self {
                asset,
                dominant_color: Color::rgb(0.05, 0.08, 0.15),
                ambient_color: Color::rgb(0.02, 0.03, 0.05),
                light_direction: Vec3::new(0.3, -0.8, 0.5),
                light_intensity: 0.1,
            },
            SkyboxAsset::Studio => Self {
                asset,
                dominant_color: Color::rgb(0.4, 0.4, 0.42),
                ambient_color: Color::rgb(0.3, 0.3, 0.32),
                light_direction: Vec3::new(-0.5, -0.7, -0.5),
                light_intensity: 0.8,
            },
            SkyboxAsset::AbstractGradient => Self {
                asset,
                dominant_color: Color::rgb(0.4, 0.2, 0.5),
                ambient_color: Color::rgb(0.2, 0.1, 0.25),
                light_direction: Vec3::new(0.0, -1.0, 0.0),
                light_intensity: 0.5,
            },
            SkyboxAsset::Desert => Self {
                asset,
                dominant_color: Color::rgb(0.9, 0.8, 0.65),
                ambient_color: Color::rgb(0.6, 0.55, 0.45),
                light_direction: Vec3::new(-0.4, -0.9, -0.2),
                light_intensity: 1.2,
            },
            SkyboxAsset::Forest => Self {
                asset,
                dominant_color: Color::rgb(0.4, 0.55, 0.4),
                ambient_color: Color::rgb(0.25, 0.35, 0.25),
                light_direction: Vec3::new(-0.3, -0.8, -0.5),
                light_intensity: 0.7,
            },
            SkyboxAsset::Ocean => Self {
                asset,
                dominant_color: Color::rgb(0.4, 0.6, 0.8),
                ambient_color: Color::rgb(0.3, 0.4, 0.5),
                light_direction: Vec3::new(-0.5, -0.8, -0.3),
                light_intensity: 0.9,
            },
        }
    }

    /// Load the skybox for this HDRI
    pub fn to_skybox(&self) -> Skybox {
        self.asset.load()
    }
}

// ============================================================================
// Internal implementation - not exposed to users
// ============================================================================

pub(crate) mod internal {
    use super::*;
    use bytemuck::{Pod, Zeroable};

    /// GPU uniform data for skybox shader
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Pod, Zeroable)]
    pub struct SkyboxUniform {
        pub sun_direction: [f32; 4],
        pub sun_color: [f32; 4],
        pub sky_color: [f32; 4],
        pub horizon_color: [f32; 4],
        pub ground_color: [f32; 4],
        pub sun_size: f32,
        pub atmosphere_density: f32,
        pub use_cubemap: u32,
        pub _padding: u32,
    }

    impl Default for SkyboxUniform {
        fn default() -> Self {
            Self {
                sun_direction: [0.0, -1.0, 0.0, 0.0],
                sun_color: [1.0, 0.98, 0.92, 1.0],
                sky_color: [0.25, 0.45, 0.85, 1.0],
                horizon_color: [0.65, 0.8, 1.0, 1.0],
                ground_color: [0.35, 0.32, 0.28, 1.0],
                sun_size: 0.0093,
                atmosphere_density: 1.0,
                use_cubemap: 0,
                _padding: 0,
            }
        }
    }

    /// Build GPU uniform from Skybox configuration
    pub fn build_uniform(skybox: &Skybox) -> SkyboxUniform {
        SkyboxUniform {
            sun_direction: [
                skybox.sun_direction.x,
                skybox.sun_direction.y,
                skybox.sun_direction.z,
                0.0,
            ],
            sun_color: [
                skybox.sun_color.r * skybox.exposure,
                skybox.sun_color.g * skybox.exposure,
                skybox.sun_color.b * skybox.exposure,
                skybox.sun_color.a,
            ],
            sky_color: [skybox.sky_color.r, skybox.sky_color.g, skybox.sky_color.b, 1.0],
            horizon_color: [
                skybox.horizon_color.r,
                skybox.horizon_color.g,
                skybox.horizon_color.b,
                1.0,
            ],
            ground_color: [
                skybox.ground_color.r,
                skybox.ground_color.g,
                skybox.ground_color.b,
                1.0,
            ],
            sun_size: skybox.sun_size,
            atmosphere_density: skybox.atmosphere_density,
            use_cubemap: if skybox.is_cubemap() { 1 } else { 0 },
            _padding: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skybox_presets() {
        let sky = Skybox::clear_day();
        assert!(sky.is_daytime());

        let night = Skybox::night();
        assert!(!night.is_daytime() || night.sun_direction.y >= 0.0);
    }

    #[test]
    fn test_skybox_builder() {
        let sky = Skybox::procedural()
            .with_sun_direction(Vec3::new(0.5, -0.8, 0.3))
            .with_turbidity(4.0);

        assert!(sky.sun_direction.y < 0.0);
    }

    #[test]
    fn test_skybox_assets() {
        for asset in SkyboxAsset::all() {
            let _ = asset.load();
        }
    }

    #[test]
    fn test_time_of_day() {
        let sky = Skybox::procedural().with_time_of_day(12.0);
        assert!(sky.is_daytime());
    }
}
