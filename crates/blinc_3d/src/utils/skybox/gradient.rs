//! Gradient-based skybox

use blinc_core::Color;

/// Gradient skybox
///
/// Simple sky using vertical color gradient. Lightweight and fast.
///
/// # Example
///
/// ```ignore
/// let sky = GradientSkybox::three_color(
///     Color::rgb(0.4, 0.6, 1.0),  // Top (zenith)
///     Color::rgb(0.8, 0.9, 1.0),  // Horizon
///     Color::rgb(0.3, 0.25, 0.2), // Bottom (ground)
/// );
/// ```
#[derive(Clone, Debug)]
pub struct GradientSkybox {
    /// Color at zenith (straight up)
    pub top_color: Color,
    /// Color at horizon
    pub horizon_color: Color,
    /// Color below horizon (ground reflection)
    pub bottom_color: Color,
    /// Horizon position (0 = bottom, 1 = top, 0.5 = middle)
    pub horizon_height: f32,
    /// Gradient sharpness (higher = sharper horizon line)
    pub horizon_sharpness: f32,
    /// Sun glow effect
    pub sun_glow: Option<SunGlow>,
}

/// Sun glow effect for gradient skybox
#[derive(Clone, Debug)]
pub struct SunGlow {
    /// Sun direction (normalized)
    pub direction: blinc_core::Vec3,
    /// Glow color
    pub color: Color,
    /// Glow intensity
    pub intensity: f32,
    /// Glow size (larger = wider glow)
    pub size: f32,
}

impl GradientSkybox {
    /// Create a new gradient skybox with default settings (clear day)
    pub fn new() -> Self {
        Self::clear_day()
    }

    /// Create a two-color gradient (top to bottom)
    pub fn two_color(top: Color, bottom: Color) -> Self {
        Self {
            top_color: top,
            horizon_color: Color::lerp(&top, &bottom, 0.5),
            bottom_color: bottom,
            horizon_height: 0.5,
            horizon_sharpness: 1.0,
            sun_glow: None,
        }
    }

    /// Create a three-color gradient
    pub fn three_color(top: Color, horizon: Color, bottom: Color) -> Self {
        Self {
            top_color: top,
            horizon_color: horizon,
            bottom_color: bottom,
            horizon_height: 0.5,
            horizon_sharpness: 2.0,
            sun_glow: None,
        }
    }

    /// Set horizon position (0-1)
    pub fn with_horizon_height(mut self, height: f32) -> Self {
        self.horizon_height = height.clamp(0.0, 1.0);
        self
    }

    /// Set horizon sharpness
    pub fn with_horizon_sharpness(mut self, sharpness: f32) -> Self {
        self.horizon_sharpness = sharpness.max(0.0);
        self
    }

    /// Add sun glow effect
    pub fn with_sun_glow(mut self, glow: SunGlow) -> Self {
        self.sun_glow = Some(glow);
        self
    }

    /// Sample color at a given vertical position (-1 to 1, -1 = down, 1 = up)
    pub fn sample(&self, y: f32) -> Color {
        let y_normalized = (y + 1.0) / 2.0; // Convert to 0-1

        if y_normalized >= self.horizon_height {
            // Above horizon: blend from horizon to top
            let t = ((y_normalized - self.horizon_height) / (1.0 - self.horizon_height))
                .powf(1.0 / self.horizon_sharpness);
            Color::lerp(&self.horizon_color, &self.top_color, t)
        } else {
            // Below horizon: blend from bottom to horizon
            let t = (y_normalized / self.horizon_height).powf(self.horizon_sharpness);
            Color::lerp(&self.bottom_color, &self.horizon_color, t)
        }
    }

    // ========== Presets ==========

    /// Clear blue sky
    pub fn clear_day() -> Self {
        Self::three_color(
            Color::rgb(0.25, 0.45, 0.85), // Deep blue zenith
            Color::rgb(0.65, 0.8, 1.0),   // Light blue horizon
            Color::rgb(0.35, 0.32, 0.28), // Brown ground
        )
    }

    /// Sunset gradient
    pub fn sunset() -> Self {
        Self::three_color(
            Color::rgb(0.15, 0.2, 0.4),  // Dark blue top
            Color::rgb(1.0, 0.5, 0.2),   // Orange horizon
            Color::rgb(0.2, 0.15, 0.1),  // Dark ground
        ).with_horizon_sharpness(1.5)
    }

    /// Night sky
    pub fn night() -> Self {
        Self::three_color(
            Color::rgb(0.02, 0.02, 0.05), // Very dark blue
            Color::rgb(0.08, 0.1, 0.15),  // Slightly lighter horizon
            Color::rgb(0.02, 0.02, 0.02), // Nearly black ground
        )
    }

    /// Overcast sky
    pub fn overcast() -> Self {
        Self::three_color(
            Color::rgb(0.5, 0.52, 0.55),  // Gray top
            Color::rgb(0.6, 0.62, 0.65),  // Lighter horizon
            Color::rgb(0.35, 0.35, 0.35), // Gray ground
        ).with_horizon_sharpness(0.5)
    }

    /// Dawn gradient
    pub fn dawn() -> Self {
        Self::three_color(
            Color::rgb(0.2, 0.25, 0.5),   // Deep blue-purple
            Color::rgb(1.0, 0.6, 0.4),    // Pink-orange horizon
            Color::rgb(0.15, 0.12, 0.1),  // Dark ground
        ).with_horizon_height(0.45)
    }

    /// Dusk gradient
    pub fn dusk() -> Self {
        Self::three_color(
            Color::rgb(0.1, 0.12, 0.25),  // Dark purple
            Color::rgb(0.8, 0.4, 0.3),    // Red-orange horizon
            Color::rgb(0.1, 0.08, 0.06),  // Very dark ground
        ).with_horizon_height(0.4)
    }

    /// Alien/sci-fi sky
    pub fn alien() -> Self {
        Self::three_color(
            Color::rgb(0.1, 0.3, 0.2),    // Teal top
            Color::rgb(0.4, 0.8, 0.5),    // Green horizon
            Color::rgb(0.15, 0.1, 0.2),   // Purple ground
        )
    }
}

impl Default for GradientSkybox {
    fn default() -> Self {
        Self::clear_day()
    }
}
