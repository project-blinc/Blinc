//! Procedural skybox with atmospheric scattering

use blinc_core::{Color, Vec3};
use std::f32::consts::PI;

/// Procedural skybox using atmospheric scattering
///
/// Simulates realistic sky colors based on sun position and atmospheric parameters.
///
/// # Example
///
/// ```ignore
/// let mut sky = ProceduralSkybox::new();
/// sky.sun_direction = Vec3::new(-0.5, -0.3, -0.8).normalize();
/// sky.turbidity = 2.0; // Clear sky
/// ```
#[derive(Clone, Debug)]
pub struct ProceduralSkybox {
    /// Sun direction (normalized, pointing towards sun)
    pub sun_direction: Vec3,
    /// Sun color
    pub sun_color: Color,
    /// Sun intensity
    pub sun_intensity: f32,
    /// Sun angular diameter (radians)
    pub sun_size: f32,

    /// Rayleigh scattering coefficient (controls sky blue color)
    pub rayleigh_coefficient: f32,
    /// Mie scattering coefficient (controls sun halo/glow)
    pub mie_coefficient: f32,
    /// Mie directional factor (controls halo tightness)
    pub mie_directional: f32,

    /// Atmospheric turbidity (haziness, 1.0 = clear, 10.0 = hazy)
    pub turbidity: f32,

    /// Ground color (for below-horizon reflection)
    pub ground_color: Color,

    /// Exposure adjustment
    pub exposure: f32,
}

impl ProceduralSkybox {
    /// Create with default settings (midday sun)
    pub fn new() -> Self {
        Self {
            sun_direction: Vec3::new(0.0, -1.0, 0.0),
            sun_color: Color::rgb(1.0, 0.98, 0.92),
            sun_intensity: 1.0,
            sun_size: 0.0093, // Approximate angular size of sun

            rayleigh_coefficient: 2.0,
            mie_coefficient: 0.005,
            mie_directional: 0.8,

            turbidity: 2.0,

            ground_color: Color::rgb(0.37, 0.35, 0.32),

            exposure: 1.0,
        }
    }

    /// Create with specific sun direction
    pub fn with_sun(direction: Vec3) -> Self {
        let mut sky = Self::new();
        sky.set_sun_direction(direction);
        sky
    }

    /// Set sun direction (will be normalized)
    pub fn set_sun_direction(&mut self, direction: Vec3) {
        let len = (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z).sqrt();
        if len > 1e-6 {
            self.sun_direction = Vec3::new(direction.x / len, direction.y / len, direction.z / len);
        }
    }

    /// Set sun position from azimuth and elevation angles (radians)
    pub fn set_sun_angles(&mut self, azimuth: f32, elevation: f32) {
        let cos_elev = elevation.cos();
        self.sun_direction = Vec3::new(
            cos_elev * azimuth.sin(),
            -elevation.sin(),
            cos_elev * azimuth.cos(),
        );
    }

    /// Set sun position from hour of day (0-24)
    pub fn set_time_of_day(&mut self, hour: f32) {
        // Simple approximation: sun rises at 6, sets at 18
        let normalized = ((hour - 6.0) / 12.0).clamp(0.0, 1.0);
        let elevation = (normalized * PI).sin() * (PI / 2.2); // Max ~80 degrees
        let azimuth = (normalized - 0.5) * PI; // East to West

        self.set_sun_angles(azimuth, elevation);

        // Adjust colors based on time
        if hour < 7.0 || hour > 17.0 {
            // Dawn/dusk warm colors
            let t = if hour < 7.0 {
                (hour - 5.0) / 2.0
            } else {
                (19.0 - hour) / 2.0
            }.clamp(0.0, 1.0);

            self.sun_color = Color::lerp(
                &Color::rgb(1.0, 0.4, 0.2),
                &Color::rgb(1.0, 0.98, 0.92),
                t,
            );
            self.sun_intensity = 0.5 + t * 0.5;
        } else {
            self.sun_color = Color::rgb(1.0, 0.98, 0.92);
            self.sun_intensity = 1.0;
        }
    }

    /// Get sun elevation angle (radians, 0 = horizon)
    pub fn sun_elevation(&self) -> f32 {
        (-self.sun_direction.y).asin()
    }

    /// Check if sun is above horizon
    pub fn is_daytime(&self) -> bool {
        self.sun_direction.y < 0.0
    }

    /// Sample sky color at a given view direction
    pub fn sample(&self, view_direction: Vec3) -> Color {
        // Normalize view direction
        let len = (view_direction.x * view_direction.x
            + view_direction.y * view_direction.y
            + view_direction.z * view_direction.z).sqrt();
        if len < 1e-6 {
            return self.ground_color;
        }
        let dir = Vec3::new(view_direction.x / len, view_direction.y / len, view_direction.z / len);

        // Below horizon - return ground color
        if dir.y < 0.0 {
            return self.ground_color;
        }

        // Calculate angle between view and sun
        let cos_theta = -(dir.x * self.sun_direction.x
            + dir.y * self.sun_direction.y
            + dir.z * self.sun_direction.z);

        // Rayleigh scattering (blue sky)
        let rayleigh = self.rayleigh_coefficient * (1.0 + cos_theta * cos_theta);

        // Mie scattering (sun halo)
        let g = self.mie_directional;
        let mie_phase = (1.0 - g * g) / ((4.0 * PI) * (1.0 + g * g - 2.0 * g * cos_theta).powf(1.5));
        let mie = self.mie_coefficient * mie_phase;

        // Optical depth based on view angle (more atmosphere at horizon)
        let zenith_angle = (1.0 - dir.y).max(0.0);
        let optical_depth = 1.0 / (dir.y.max(0.01) + 0.15 * zenith_angle);

        // Base sky color from Rayleigh scattering
        let sky_beta = Vec3::new(
            5.5e-6 * self.turbidity,  // Red
            13.0e-6 * self.turbidity, // Green
            22.4e-6 * self.turbidity, // Blue
        );

        let extinction = Vec3::new(
            (-sky_beta.x * optical_depth * rayleigh).exp(),
            (-sky_beta.y * optical_depth * rayleigh).exp(),
            (-sky_beta.z * optical_depth * rayleigh).exp(),
        );

        // Combine scattering
        let sun_factor = if self.is_daytime() { self.sun_intensity } else { 0.1 };

        let r = (1.0 - extinction.x) * self.sun_color.r * sun_factor + mie * self.sun_color.r;
        let g = (1.0 - extinction.y) * self.sun_color.g * sun_factor + mie * self.sun_color.g;
        let b = (1.0 - extinction.z) * self.sun_color.b * sun_factor + mie * self.sun_color.b;

        // Apply exposure
        let r = 1.0 - (-r * self.exposure).exp();
        let g = 1.0 - (-g * self.exposure).exp();
        let b = 1.0 - (-b * self.exposure).exp();

        Color::rgb(r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0))
    }

    // ========== Presets ==========

    /// Clear day preset
    pub fn clear_day() -> Self {
        let mut sky = Self::new();
        sky.set_sun_angles(0.0, PI / 3.0); // 60 degrees elevation
        sky.turbidity = 2.0;
        sky
    }

    /// Sunset preset
    pub fn sunset() -> Self {
        let mut sky = Self::new();
        sky.set_sun_angles(PI * 0.4, PI / 12.0); // Low sun
        sky.sun_color = Color::rgb(1.0, 0.5, 0.2);
        sky.sun_intensity = 0.8;
        sky.turbidity = 4.0;
        sky.mie_coefficient = 0.01;
        sky
    }

    /// Sunrise preset
    pub fn sunrise() -> Self {
        let mut sky = Self::new();
        sky.set_sun_angles(-PI * 0.4, PI / 12.0);
        sky.sun_color = Color::rgb(1.0, 0.6, 0.3);
        sky.sun_intensity = 0.7;
        sky.turbidity = 3.0;
        sky.mie_coefficient = 0.008;
        sky
    }

    /// Midday preset
    pub fn midday() -> Self {
        let mut sky = Self::new();
        sky.set_sun_angles(0.0, PI / 2.2);
        sky.turbidity = 2.0;
        sky
    }

    /// Hazy day preset
    pub fn hazy() -> Self {
        let mut sky = Self::new();
        sky.set_sun_angles(0.2, PI / 4.0);
        sky.turbidity = 8.0;
        sky.mie_coefficient = 0.02;
        sky
    }
}

impl Default for ProceduralSkybox {
    fn default() -> Self {
        Self::new()
    }
}
