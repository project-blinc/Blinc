//! Procedural terrain generation
//!
//! Provides heightmap-based terrain with noise generation, LOD,
//! and chunked streaming.
//!
//! # Example
//!
//! ```ignore
//! use blinc_3d::utils::terrain::*;
//!
//! // Create a procedural terrain
//! let terrain = Terrain::new(TerrainConfig {
//!     size: 1024.0,
//!     resolution: 256,
//!     max_height: 100.0,
//!     ..Default::default()
//! });
//!
//! // Use noise layers for height generation
//! let heightmap = ProceduralHeightmap::new()
//!     .with_layer(NoiseLayer::perlin(0.01, 50.0))  // Base mountains
//!     .with_layer(NoiseLayer::ridged(0.05, 20.0))  // Ridges
//!     .with_layer(NoiseLayer::fbm(0.1, 5.0));      // Detail
//! ```

mod heightmap;
mod noise;
mod chunks;
mod lod;
mod water;

pub use heightmap::*;
pub use noise::*;
pub use chunks::*;
pub use lod::*;
pub use water::*;

use crate::ecs::{Component, System, SystemContext, SystemStage};
use crate::geometry::{Geometry, GeometryHandle};
use blinc_core::Vec3;

/// Terrain configuration
#[derive(Clone, Debug)]
pub struct TerrainConfig {
    /// Terrain size in world units (square)
    pub size: f32,
    /// Heightmap resolution (vertices per side)
    pub resolution: u32,
    /// Maximum terrain height
    pub max_height: f32,
    /// Chunk size (for streaming)
    pub chunk_size: u32,
    /// LOD levels
    pub lod_levels: u32,
    /// LOD distance multiplier
    pub lod_distance: f32,
    /// Enable terrain collision
    pub collision_enabled: bool,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            size: 1000.0,
            resolution: 256,
            max_height: 100.0,
            chunk_size: 64,
            lod_levels: 4,
            lod_distance: 100.0,
            collision_enabled: true,
        }
    }
}

impl TerrainConfig {
    /// Create a small terrain (for testing)
    pub fn small() -> Self {
        Self {
            size: 100.0,
            resolution: 64,
            max_height: 20.0,
            chunk_size: 32,
            lod_levels: 2,
            ..Default::default()
        }
    }

    /// Create a large terrain
    pub fn large() -> Self {
        Self {
            size: 4000.0,
            resolution: 512,
            max_height: 500.0,
            chunk_size: 128,
            lod_levels: 6,
            lod_distance: 200.0,
            ..Default::default()
        }
    }

    /// Create an infinite terrain configuration
    pub fn infinite() -> Self {
        Self {
            size: f32::INFINITY,
            resolution: 128,
            max_height: 200.0,
            chunk_size: 64,
            lod_levels: 5,
            lod_distance: 150.0,
            ..Default::default()
        }
    }

    /// Set terrain size
    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Set resolution
    pub fn with_resolution(mut self, resolution: u32) -> Self {
        self.resolution = resolution.max(2);
        self
    }

    /// Set max height
    pub fn with_max_height(mut self, height: f32) -> Self {
        self.max_height = height;
        self
    }

    /// Set chunk size
    pub fn with_chunk_size(mut self, size: u32) -> Self {
        self.chunk_size = size.max(8);
        self
    }

    /// Set LOD levels
    pub fn with_lod_levels(mut self, levels: u32) -> Self {
        self.lod_levels = levels.clamp(1, 8);
        self
    }
}

/// Terrain component
///
/// Represents a terrain in the world.
#[derive(Clone, Debug)]
pub struct Terrain {
    /// Configuration
    pub config: TerrainConfig,
    /// Heightmap source
    pub heightmap: HeightmapSource,
    /// Terrain material settings
    pub material: TerrainMaterial,
    /// Current chunk states
    pub(crate) chunks: ChunkManager,
    /// Whether terrain needs rebuild
    pub(crate) dirty: bool,
}

impl Component for Terrain {}

impl Terrain {
    /// Create a new terrain with configuration
    pub fn new(config: TerrainConfig) -> Self {
        let chunks = ChunkManager::new(&config);
        Self {
            config,
            heightmap: HeightmapSource::Flat,
            material: TerrainMaterial::default(),
            chunks,
            dirty: true,
        }
    }

    /// Create with default configuration
    pub fn default_terrain() -> Self {
        Self::new(TerrainConfig::default())
    }

    /// Set heightmap source
    pub fn with_heightmap(mut self, source: HeightmapSource) -> Self {
        self.heightmap = source;
        self.dirty = true;
        self
    }

    /// Set terrain material
    pub fn with_material(mut self, material: TerrainMaterial) -> Self {
        self.material = material;
        self
    }

    /// Get height at world position
    pub fn get_height(&self, x: f32, z: f32) -> f32 {
        self.heightmap.sample(x, z, &self.config)
    }

    /// Get normal at world position
    pub fn get_normal(&self, x: f32, z: f32) -> Vec3 {
        let epsilon = self.config.size / self.config.resolution as f32;
        let h_center = self.get_height(x, z);
        let h_right = self.get_height(x + epsilon, z);
        let h_forward = self.get_height(x, z + epsilon);

        let dx = h_right - h_center;
        let dz = h_forward - h_center;

        let normal = Vec3::new(-dx, epsilon, -dz);
        normalize_vec3(normal)
    }

    /// Get slope at world position (0 = flat, 1 = vertical)
    pub fn get_slope(&self, x: f32, z: f32) -> f32 {
        let normal = self.get_normal(x, z);
        1.0 - normal.y.abs()
    }

    /// Mark terrain as dirty (needs rebuild)
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Check if terrain needs rebuild
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    // ========== Presets ==========

    /// Flat terrain
    pub fn flat(size: f32) -> Self {
        Self::new(TerrainConfig::default().with_size(size))
            .with_heightmap(HeightmapSource::Flat)
    }

    /// Rolling hills terrain
    pub fn hills(size: f32, height: f32) -> Self {
        Self::new(TerrainConfig::default()
            .with_size(size)
            .with_max_height(height))
            .with_heightmap(HeightmapSource::Procedural(
                ProceduralHeightmap::new()
                    .with_layer(NoiseLayer::perlin(0.005, 0.6))
                    .with_layer(NoiseLayer::perlin(0.02, 0.3))
                    .with_layer(NoiseLayer::perlin(0.1, 0.1))
            ))
    }

    /// Mountain terrain
    pub fn mountains(size: f32, height: f32) -> Self {
        Self::new(TerrainConfig::default()
            .with_size(size)
            .with_max_height(height))
            .with_heightmap(HeightmapSource::Procedural(
                ProceduralHeightmap::new()
                    .with_layer(NoiseLayer::ridged(0.003, 0.7))
                    .with_layer(NoiseLayer::perlin(0.01, 0.2))
                    .with_layer(NoiseLayer::fbm(0.05, 0.1))
            ))
    }

    /// Desert dunes terrain
    pub fn dunes(size: f32, height: f32) -> Self {
        Self::new(TerrainConfig::default()
            .with_size(size)
            .with_max_height(height))
            .with_heightmap(HeightmapSource::Procedural(
                ProceduralHeightmap::new()
                    .with_layer(NoiseLayer::billow(0.01, 0.8))
                    .with_layer(NoiseLayer::perlin(0.05, 0.2))
            ))
    }

    /// Island terrain (raised center, ocean around)
    pub fn island(size: f32, height: f32) -> Self {
        Self::new(TerrainConfig::default()
            .with_size(size)
            .with_max_height(height))
            .with_heightmap(HeightmapSource::Procedural(
                ProceduralHeightmap::new()
                    .with_layer(NoiseLayer::perlin(0.005, 0.5))
                    .with_layer(NoiseLayer::ridged(0.02, 0.3))
                    .with_falloff(FalloffType::Island { radius: size * 0.4 })
            ))
    }
}

/// Heightmap source types
#[derive(Clone, Debug)]
pub enum HeightmapSource {
    /// Flat terrain (height = 0)
    Flat,
    /// Height from texture
    Texture {
        /// Height data (row-major)
        data: Vec<f32>,
        /// Data width
        width: u32,
        /// Data height
        height: u32,
    },
    /// Procedural generation
    Procedural(ProceduralHeightmap),
    /// Custom function
    Function(HeightFunction),
}

impl HeightmapSource {
    /// Sample height at world position
    pub fn sample(&self, x: f32, z: f32, config: &TerrainConfig) -> f32 {
        match self {
            HeightmapSource::Flat => 0.0,
            HeightmapSource::Texture { data, width, height } => {
                // Normalize to 0-1 range
                let u = (x / config.size + 0.5).clamp(0.0, 1.0);
                let v = (z / config.size + 0.5).clamp(0.0, 1.0);

                // Sample with bilinear interpolation
                let fx = u * (*width - 1) as f32;
                let fz = v * (*height - 1) as f32;

                let ix = fx.floor() as u32;
                let iz = fz.floor() as u32;

                let tx = fx.fract();
                let tz = fz.fract();

                let sample = |sx: u32, sz: u32| {
                    let idx = (sz.min(*height - 1) * *width + sx.min(*width - 1)) as usize;
                    data.get(idx).copied().unwrap_or(0.0)
                };

                let h00 = sample(ix, iz);
                let h10 = sample(ix + 1, iz);
                let h01 = sample(ix, iz + 1);
                let h11 = sample(ix + 1, iz + 1);

                let h0 = h00 + (h10 - h00) * tx;
                let h1 = h01 + (h11 - h01) * tx;

                (h0 + (h1 - h0) * tz) * config.max_height
            }
            HeightmapSource::Procedural(proc) => {
                proc.sample(x, z) * config.max_height
            }
            HeightmapSource::Function(func) => {
                func.sample(x, z) * config.max_height
            }
        }
    }
}

/// Custom height function
#[derive(Clone)]
pub struct HeightFunction {
    func: std::sync::Arc<dyn Fn(f32, f32) -> f32 + Send + Sync>,
}

impl HeightFunction {
    /// Create from closure
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(f32, f32) -> f32 + Send + Sync + 'static,
    {
        Self {
            func: std::sync::Arc::new(f),
        }
    }

    /// Sample height
    pub fn sample(&self, x: f32, z: f32) -> f32 {
        (self.func)(x, z)
    }
}

impl std::fmt::Debug for HeightFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeightFunction").finish()
    }
}

/// Terrain material settings
#[derive(Clone, Debug)]
pub struct TerrainMaterial {
    /// Base texture layers
    pub layers: Vec<TerrainTextureLayer>,
    /// Triplanar mapping scale
    pub triplanar_scale: f32,
    /// Normal map strength
    pub normal_strength: f32,
    /// Ambient occlusion strength
    pub ao_strength: f32,
    /// Detail texture scale
    pub detail_scale: f32,
}

impl Default for TerrainMaterial {
    fn default() -> Self {
        Self {
            layers: vec![TerrainTextureLayer::default()],
            triplanar_scale: 0.1,
            normal_strength: 1.0,
            ao_strength: 1.0,
            detail_scale: 20.0,
        }
    }
}

/// Terrain texture layer
#[derive(Clone, Debug)]
pub struct TerrainTextureLayer {
    /// Layer name
    pub name: String,
    /// Diffuse/albedo texture (optional)
    pub diffuse_texture: Option<String>,
    /// Normal map texture (optional)
    pub normal_texture: Option<String>,
    /// Tiling scale
    pub scale: f32,
    /// Height blend range (for height-based blending)
    pub height_start: f32,
    pub height_end: f32,
    /// Slope blend range
    pub slope_start: f32,
    pub slope_end: f32,
}

impl Default for TerrainTextureLayer {
    fn default() -> Self {
        Self {
            name: "grass".to_string(),
            diffuse_texture: None,
            normal_texture: None,
            scale: 10.0,
            height_start: 0.0,
            height_end: 1.0,
            slope_start: 0.0,
            slope_end: 0.7,
        }
    }
}

impl TerrainTextureLayer {
    /// Create a new layer
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Default::default()
        }
    }

    /// Set height range
    pub fn with_height_range(mut self, start: f32, end: f32) -> Self {
        self.height_start = start;
        self.height_end = end;
        self
    }

    /// Set slope range
    pub fn with_slope_range(mut self, start: f32, end: f32) -> Self {
        self.slope_start = start;
        self.slope_end = end;
        self
    }

    /// Set tiling scale
    pub fn with_scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }
}

/// System for updating terrain
pub struct TerrainSystem {
    /// Camera position for LOD calculations
    camera_position: Vec3,
}

impl TerrainSystem {
    /// Create a new terrain system
    pub fn new() -> Self {
        Self {
            camera_position: Vec3::ZERO,
        }
    }

    /// Update camera position
    pub fn set_camera_position(&mut self, position: Vec3) {
        self.camera_position = position;
    }
}

impl Default for TerrainSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl System for TerrainSystem {
    fn run(&mut self, ctx: &mut SystemContext) {
        let camera_pos = self.camera_position;

        // Update all terrains
        let entities: Vec<_> = ctx.world
            .query::<(&Terrain,)>()
            .iter()
            .map(|(e, _)| e)
            .collect();

        for entity in entities {
            if let Some(terrain) = ctx.world.get_mut::<Terrain>(entity) {
                // Update chunk loading based on camera position
                terrain.chunks.update(camera_pos, &terrain.config);
            }
        }
    }

    fn name(&self) -> &'static str {
        "TerrainSystem"
    }

    fn stage(&self) -> SystemStage {
        SystemStage::Update
    }

    fn priority(&self) -> i32 {
        5
    }
}

// Helper function
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
    fn test_terrain_config() {
        let config = TerrainConfig::default();
        assert!((config.size - 1000.0).abs() < 0.001);
    }

    #[test]
    fn test_terrain_presets() {
        let flat = Terrain::flat(100.0);
        assert!((flat.get_height(50.0, 50.0) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_heightmap_sampling() {
        let config = TerrainConfig::default();
        let source = HeightmapSource::Flat;
        assert!((source.sample(100.0, 100.0, &config) - 0.0).abs() < 0.001);
    }
}
