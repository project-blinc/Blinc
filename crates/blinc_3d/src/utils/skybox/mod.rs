//! Skybox system
//!
//! Provides various skybox implementations:
//!
//! - [`ProceduralSkybox`] - Atmospheric scattering for realistic skies
//! - [`CubemapSkybox`] - Texture-based skybox using cubemaps
//! - [`GradientSkybox`] - Simple gradient-based sky
//! - [`TimeOfDay`] - Day/night cycle system

mod procedural;
mod cubemap;
mod gradient;
mod time_of_day;
mod assets;
mod download;

pub use procedural::ProceduralSkybox;
pub use cubemap::CubemapSkybox;
pub use gradient::GradientSkybox;
pub use time_of_day::{TimeOfDay, TimeOfDaySystem, DayNightCycle};
pub use assets::{SkyboxAsset, EmbeddedHdri};
pub use download::{
    HdriSource, HdriResolution, HdriAsset, DownloadConfig, DownloadStatus,
    DownloadRequest, AssetDownloadHelper, polyhaven_popular, search_by_tag, filter_by_source,
};

use crate::ecs::Component;
use crate::materials::TextureHandle;
use blinc_core::{Color, Vec3};

/// Skybox component
///
/// Attach to an entity to render a skybox in the scene.
#[derive(Clone, Debug)]
pub enum Skybox {
    /// Procedurally generated atmospheric sky
    Procedural(ProceduralSkybox),
    /// Texture-based cubemap skybox
    Cubemap(CubemapSkybox),
    /// Simple gradient sky
    Gradient(GradientSkybox),
}

impl Component for Skybox {}

impl Skybox {
    /// Create a procedural sky with default settings
    pub fn procedural() -> Self {
        Self::Procedural(ProceduralSkybox::default())
    }

    /// Create a procedural sky with specific sun direction
    pub fn procedural_with_sun(sun_direction: Vec3) -> Self {
        Self::Procedural(ProceduralSkybox::with_sun(sun_direction))
    }

    /// Create from a cubemap texture handle
    pub fn cubemap(texture: TextureHandle) -> Self {
        Self::Cubemap(CubemapSkybox::new(texture))
    }

    /// Create a simple two-color gradient sky
    pub fn gradient(top_color: Color, bottom_color: Color) -> Self {
        Self::Gradient(GradientSkybox::two_color(top_color, bottom_color))
    }

    /// Create a gradient sky with horizon color
    pub fn gradient_with_horizon(top_color: Color, horizon_color: Color, bottom_color: Color) -> Self {
        Self::Gradient(GradientSkybox::three_color(top_color, horizon_color, bottom_color))
    }

    /// Quick preset: Clear blue sky
    pub fn clear_day() -> Self {
        Self::Procedural(ProceduralSkybox::clear_day())
    }

    /// Quick preset: Sunset sky
    pub fn sunset() -> Self {
        Self::Procedural(ProceduralSkybox::sunset())
    }

    /// Quick preset: Night sky
    pub fn night() -> Self {
        Self::Gradient(GradientSkybox::night())
    }

    /// Quick preset: Overcast sky
    pub fn overcast() -> Self {
        Self::Gradient(GradientSkybox::overcast())
    }
}

impl Default for Skybox {
    fn default() -> Self {
        Self::procedural()
    }
}

impl From<ProceduralSkybox> for Skybox {
    fn from(sky: ProceduralSkybox) -> Self {
        Self::Procedural(sky)
    }
}

impl From<CubemapSkybox> for Skybox {
    fn from(sky: CubemapSkybox) -> Self {
        Self::Cubemap(sky)
    }
}

impl From<GradientSkybox> for Skybox {
    fn from(sky: GradientSkybox) -> Self {
        Self::Gradient(sky)
    }
}
