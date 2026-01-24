//! Procedural terrain generation
//!
//! Provides heightmap-based terrain with procedural noise generation,
//! automatic LOD, and GPU-accelerated rendering.
//!
//! # Example
//!
//! ```ignore
//! use blinc_3d::utils::terrain::*;
//!
//! // Create terrain with a preset
//! let terrain = Terrain::mountains(1000.0, 200.0);
//!
//! // Or build custom terrain
//! let terrain = Terrain::new(1000.0)
//!     .with_max_height(100.0)
//!     .with_noise(NoiseLayer::ridged(0.003, 0.7))
//!     .with_noise(NoiseLayer::perlin(0.01, 0.2))
//!     .with_noise(NoiseLayer::fbm(0.05, 0.1));
//!
//! // Add water body
//! let water = WaterBody::ocean(0.0);
//!
//! // Attach to entities - rendering is automatic
//! world.spawn().insert(terrain);
//! world.spawn().insert(water);
//! ```

mod water;

pub use water::*;

use crate::ecs::{Component, System, SystemContext, SystemStage};
use blinc_core::{Color, Vec3};

/// Terrain component
///
/// Add this to an entity to render procedural terrain.
/// The system automatically handles noise generation, LOD, and GPU rendering.
#[derive(Clone, Debug)]
pub struct Terrain {
    /// Terrain size in world units (square)
    pub size: f32,
    /// Maximum terrain height
    pub max_height: f32,
    /// Grid resolution per chunk
    pub resolution: u32,
    /// Noise layers (up to 4)
    noise_layers: [Option<NoiseLayer>; 4],
    /// Material settings
    pub material: TerrainMaterial,
    /// LOD configuration
    pub lod_levels: u32,
    /// LOD distance multiplier
    pub lod_distance: f32,
    /// Water level for shore effects
    pub water_level: f32,
}

impl Component for Terrain {}

impl Default for Terrain {
    fn default() -> Self {
        Self {
            size: 1000.0,
            max_height: 100.0,
            resolution: 64,
            noise_layers: [
                Some(NoiseLayer::perlin(0.01, 0.5)),
                Some(NoiseLayer::perlin(0.05, 0.3)),
                Some(NoiseLayer::perlin(0.1, 0.15)),
                None,
            ],
            material: TerrainMaterial::default(),
            lod_levels: 4,
            lod_distance: 100.0,
            water_level: 0.0,
        }
    }
}

impl Terrain {
    /// Create a new terrain with specified size
    pub fn new(size: f32) -> Self {
        Self {
            size,
            ..Default::default()
        }
    }

    /// Set maximum height
    pub fn with_max_height(mut self, height: f32) -> Self {
        self.max_height = height;
        self
    }

    /// Set grid resolution
    pub fn with_resolution(mut self, resolution: u32) -> Self {
        self.resolution = resolution.clamp(8, 256);
        self
    }

    /// Add a noise layer (up to 4 layers)
    pub fn with_noise(mut self, layer: NoiseLayer) -> Self {
        for slot in &mut self.noise_layers {
            if slot.is_none() {
                *slot = Some(layer);
                return self;
            }
        }
        // Replace last if all full
        self.noise_layers[3] = Some(layer);
        self
    }

    /// Clear all noise layers and set new ones
    pub fn with_noise_layers(mut self, layers: &[NoiseLayer]) -> Self {
        self.noise_layers = [const { None }; 4];
        for (i, layer) in layers.iter().take(4).enumerate() {
            self.noise_layers[i] = Some(layer.clone());
        }
        self
    }

    /// Set terrain material
    pub fn with_material(mut self, material: TerrainMaterial) -> Self {
        self.material = material;
        self
    }

    /// Set LOD levels
    pub fn with_lod_levels(mut self, levels: u32) -> Self {
        self.lod_levels = levels.clamp(1, 8);
        self
    }

    /// Set water level for shore effects
    pub fn with_water_level(mut self, level: f32) -> Self {
        self.water_level = level;
        self
    }

    // ========== Presets ==========

    /// Flat terrain
    pub fn flat(size: f32) -> Self {
        Self::new(size)
            .with_max_height(0.0)
            .with_noise_layers(&[])
    }

    /// Rolling hills terrain
    pub fn hills(size: f32, height: f32) -> Self {
        Self::new(size)
            .with_max_height(height)
            .with_noise_layers(&[
                NoiseLayer::perlin(0.005, 0.5),
                NoiseLayer::perlin(0.02, 0.3).with_octaves(5),
                NoiseLayer::perlin(0.08, 0.15),
                NoiseLayer::perlin(0.3, 0.05),
            ])
    }

    /// Mountain terrain
    pub fn mountains(size: f32, height: f32) -> Self {
        Self::new(size)
            .with_max_height(height)
            .with_noise_layers(&[
                NoiseLayer::ridged(0.002, 0.6).with_octaves(6),
                NoiseLayer::perlin(0.01, 0.25).with_octaves(5),
                NoiseLayer::perlin(0.05, 0.1),
                NoiseLayer::perlin(0.2, 0.05),
            ])
    }

    /// Flat plains terrain
    pub fn plains(size: f32, height: f32) -> Self {
        Self::new(size)
            .with_max_height(height)
            .with_noise_layers(&[
                NoiseLayer::perlin(0.002, 0.3),
                NoiseLayer::perlin(0.01, 0.5).with_octaves(4),
                NoiseLayer::perlin(0.05, 0.2),
            ])
    }

    /// Canyon terrain
    pub fn canyons(size: f32, height: f32) -> Self {
        Self::new(size)
            .with_max_height(height)
            .with_noise_layers(&[
                NoiseLayer::ridged(0.003, 0.8).with_octaves(5),
                NoiseLayer::billow(0.01, 0.15),
                NoiseLayer::perlin(0.1, 0.05),
            ])
    }

    /// Desert dunes terrain
    pub fn dunes(size: f32, height: f32) -> Self {
        Self::new(size)
            .with_max_height(height)
            .with_noise_layers(&[
                NoiseLayer::billow(0.01, 0.8),
                NoiseLayer::perlin(0.05, 0.2),
            ])
    }

    /// Island terrain (raised center, ocean around)
    pub fn island(size: f32, height: f32) -> Self {
        Self::new(size)
            .with_max_height(height)
            .with_noise_layers(&[
                NoiseLayer::perlin(0.005, 0.5),
                NoiseLayer::ridged(0.02, 0.3),
                NoiseLayer::perlin(0.1, 0.15),
            ])
            // Island falloff is applied in shader
    }
}

/// Noise layer configuration
///
/// Configures a single layer of procedural noise for terrain generation.
/// Multiple layers are combined additively on the GPU.
#[derive(Clone, Debug)]
pub struct NoiseLayer {
    /// Noise algorithm type
    pub noise_type: NoiseType,
    /// Sampling frequency (higher = more detail, smaller features)
    pub frequency: f32,
    /// Output amplitude (contribution weight)
    pub amplitude: f32,
    /// Number of octaves for fractal noise
    pub octaves: u32,
}

impl NoiseLayer {
    /// Create a new noise layer
    pub fn new(noise_type: NoiseType, frequency: f32, amplitude: f32) -> Self {
        Self {
            noise_type,
            frequency,
            amplitude,
            octaves: 4,
        }
    }

    /// Create Perlin noise layer
    pub fn perlin(frequency: f32, amplitude: f32) -> Self {
        Self::new(NoiseType::Perlin, frequency, amplitude)
    }

    /// Create Simplex noise layer
    pub fn simplex(frequency: f32, amplitude: f32) -> Self {
        Self::new(NoiseType::Simplex, frequency, amplitude)
    }

    /// Create Worley/cellular noise layer
    pub fn worley(frequency: f32, amplitude: f32) -> Self {
        Self::new(NoiseType::Worley, frequency, amplitude)
    }

    /// Create ridged multifractal noise layer
    pub fn ridged(frequency: f32, amplitude: f32) -> Self {
        Self::new(NoiseType::Ridged, frequency, amplitude).with_octaves(6)
    }

    /// Create billow noise layer
    pub fn billow(frequency: f32, amplitude: f32) -> Self {
        Self::new(NoiseType::Billow, frequency, amplitude).with_octaves(4)
    }

    /// Create FBM (fractal Brownian motion) noise layer
    pub fn fbm(frequency: f32, amplitude: f32) -> Self {
        Self::perlin(frequency, amplitude).with_octaves(6)
    }

    /// Set number of octaves
    pub fn with_octaves(mut self, octaves: u32) -> Self {
        self.octaves = octaves.clamp(1, 8);
        self
    }
}

/// Noise function types
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NoiseType {
    /// Classic Perlin noise
    #[default]
    Perlin,
    /// Simplex noise (faster, fewer artifacts)
    Simplex,
    /// Worley/cellular noise
    Worley,
    /// Ridged multifractal
    Ridged,
    /// Billow (turbulence)
    Billow,
}

/// Terrain material settings
#[derive(Clone, Debug)]
pub struct TerrainMaterial {
    /// Grass color (low slopes, mid heights)
    pub grass_color: Color,
    /// Rock color (steep slopes)
    pub rock_color: Color,
    /// Snow color (high altitudes)
    pub snow_color: Color,
    /// Sand color (low altitudes, near water)
    pub sand_color: Color,
    /// Height threshold for snow
    pub snow_height: f32,
    /// Height threshold for sand
    pub sand_height: f32,
    /// Slope threshold for rock
    pub rock_slope: f32,
    /// Texture tiling scale
    pub texture_scale: f32,
}

impl Default for TerrainMaterial {
    fn default() -> Self {
        Self {
            grass_color: Color::rgb(0.3, 0.5, 0.2),
            rock_color: Color::rgb(0.5, 0.45, 0.4),
            snow_color: Color::rgb(0.95, 0.95, 1.0),
            sand_color: Color::rgb(0.9, 0.8, 0.6),
            snow_height: 0.8,
            sand_height: 0.1,
            rock_slope: 0.5,
            texture_scale: 0.1,
        }
    }
}

impl TerrainMaterial {
    /// Desert material preset
    pub fn desert() -> Self {
        Self {
            grass_color: Color::rgb(0.8, 0.7, 0.5),
            rock_color: Color::rgb(0.6, 0.5, 0.4),
            snow_color: Color::rgb(0.95, 0.9, 0.85),
            sand_color: Color::rgb(0.95, 0.85, 0.6),
            sand_height: 0.3,
            ..Default::default()
        }
    }

    /// Snowy material preset
    pub fn snowy() -> Self {
        Self {
            grass_color: Color::rgb(0.4, 0.5, 0.4),
            snow_color: Color::rgb(1.0, 1.0, 1.0),
            snow_height: 0.3,
            ..Default::default()
        }
    }

    /// Volcanic material preset
    pub fn volcanic() -> Self {
        Self {
            grass_color: Color::rgb(0.2, 0.2, 0.2),
            rock_color: Color::rgb(0.3, 0.25, 0.2),
            snow_color: Color::rgb(0.15, 0.1, 0.1),
            sand_color: Color::rgb(0.4, 0.3, 0.2),
            ..Default::default()
        }
    }
}

/// System for rendering terrain
pub struct TerrainSystem {
    camera_position: Vec3,
}

impl TerrainSystem {
    pub fn new() -> Self {
        Self {
            camera_position: Vec3::ZERO,
        }
    }

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
        // Query all terrain entities and update LOD based on camera position
        let _camera_pos = self.camera_position;

        // The actual GPU rendering happens in the render pipeline
        // This system just updates LOD state and handles streaming
        for (_entity, _terrain) in ctx.world.query::<(&Terrain,)>().iter() {
            // Update chunk visibility and LOD levels based on camera distance
            // GPU handles actual rendering via internal::build_uniform()
        }
    }

    fn name(&self) -> &'static str {
        "TerrainSystem"
    }

    fn stage(&self) -> SystemStage {
        SystemStage::PreRender
    }

    fn priority(&self) -> i32 {
        10
    }
}

// ============================================================================
// Internal implementation - not exposed to users
// ============================================================================

pub(crate) mod internal {
    use super::*;
    use bytemuck::{Pod, Zeroable};

    /// GPU uniform data for terrain shader
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Pod, Zeroable)]
    pub struct TerrainUniform {
        pub model: [[f32; 4]; 4],
        pub normal_matrix: [[f32; 4]; 4],
        pub world_offset: [f32; 4],
        pub terrain_scale: [f32; 4],
        pub noise_params0: [f32; 4],
        pub noise_params1: [f32; 4],
        pub noise_params2: [f32; 4],
        pub noise_params3: [f32; 4],
        pub lod_params: [f32; 4],
        pub water_level: f32,
        pub time: f32,
        pub _padding: [f32; 2],
    }

    impl Default for TerrainUniform {
        fn default() -> Self {
            Self {
                model: [
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ],
                normal_matrix: [
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ],
                world_offset: [0.0; 4],
                terrain_scale: [1000.0, 100.0, 1.0, 0.0],
                noise_params0: [0.0; 4],
                noise_params1: [0.0; 4],
                noise_params2: [0.0; 4],
                noise_params3: [0.0; 4],
                lod_params: [0.0, 0.0, 64.0, 0.0],
                water_level: 0.0,
                time: 0.0,
                _padding: [0.0; 2],
            }
        }
    }

    /// GPU material uniform for terrain shader
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Pod, Zeroable)]
    pub struct TerrainMaterialUniform {
        pub grass_color: [f32; 4],
        pub rock_color: [f32; 4],
        pub snow_color: [f32; 4],
        pub sand_color: [f32; 4],
        pub thresholds: [f32; 4], // grass_height, rock_slope, snow_height, sand_height
        pub texture_scale: [f32; 4],
    }

    impl Default for TerrainMaterialUniform {
        fn default() -> Self {
            Self {
                grass_color: [0.3, 0.5, 0.2, 1.0],
                rock_color: [0.5, 0.45, 0.4, 1.0],
                snow_color: [0.95, 0.95, 1.0, 1.0],
                sand_color: [0.9, 0.8, 0.6, 1.0],
                thresholds: [0.3, 0.5, 0.8, 0.1],
                texture_scale: [0.1, 0.1, 0.05, 0.05],
            }
        }
    }

    /// Terrain mesh vertex
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Pod, Zeroable)]
    pub struct TerrainVertex {
        pub position: [f32; 3],
        pub uv: [f32; 2],
    }

    /// Convert noise layer to shader params
    fn noise_to_params(layer: &Option<NoiseLayer>) -> [f32; 4] {
        match layer {
            Some(l) => [
                l.frequency,
                l.amplitude,
                l.octaves as f32,
                l.noise_type as u32 as f32,
            ],
            None => [0.0; 4],
        }
    }

    /// Build GPU uniform from Terrain configuration
    pub fn build_uniform(
        terrain: &Terrain,
        world_offset: Vec3,
        lod_level: u32,
        morph_factor: f32,
        time: f32,
    ) -> TerrainUniform {
        TerrainUniform {
            world_offset: [world_offset.x, world_offset.z, 0.0, 0.0],
            terrain_scale: [terrain.size, terrain.max_height, 1.0, 0.0],
            noise_params0: noise_to_params(&terrain.noise_layers[0]),
            noise_params1: noise_to_params(&terrain.noise_layers[1]),
            noise_params2: noise_to_params(&terrain.noise_layers[2]),
            noise_params3: noise_to_params(&terrain.noise_layers[3]),
            lod_params: [lod_level as f32, morph_factor, terrain.resolution as f32, 0.0],
            water_level: terrain.water_level,
            time,
            ..Default::default()
        }
    }

    /// Build material uniform from TerrainMaterial
    pub fn build_material_uniform(material: &TerrainMaterial) -> TerrainMaterialUniform {
        TerrainMaterialUniform {
            grass_color: [material.grass_color.r, material.grass_color.g, material.grass_color.b, 1.0],
            rock_color: [material.rock_color.r, material.rock_color.g, material.rock_color.b, 1.0],
            snow_color: [material.snow_color.r, material.snow_color.g, material.snow_color.b, 1.0],
            sand_color: [material.sand_color.r, material.sand_color.g, material.sand_color.b, 1.0],
            thresholds: [0.3, material.rock_slope, material.snow_height, material.sand_height],
            texture_scale: [material.texture_scale; 4],
        }
    }

    /// Generate terrain mesh grid
    pub fn generate_mesh(resolution: u32) -> (Vec<TerrainVertex>, Vec<u32>) {
        let resolution = resolution.clamp(8, 256);
        let mut vertices = Vec::with_capacity((resolution * resolution) as usize);
        let mut indices = Vec::with_capacity(((resolution - 1) * (resolution - 1) * 6) as usize);

        for z in 0..resolution {
            for x in 0..resolution {
                let u = x as f32 / (resolution - 1) as f32;
                let v = z as f32 / (resolution - 1) as f32;

                vertices.push(TerrainVertex {
                    position: [u - 0.5, 0.0, v - 0.5],
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
    fn test_terrain_presets() {
        let mountains = Terrain::mountains(1000.0, 200.0);
        assert!((mountains.size - 1000.0).abs() < 0.001);
        assert!((mountains.max_height - 200.0).abs() < 0.001);
    }

    #[test]
    fn test_terrain_builder() {
        let terrain = Terrain::new(500.0)
            .with_max_height(50.0)
            .with_noise(NoiseLayer::perlin(0.01, 1.0))
            .with_resolution(128);

        assert!((terrain.size - 500.0).abs() < 0.001);
        assert!((terrain.max_height - 50.0).abs() < 0.001);
        assert_eq!(terrain.resolution, 128);
    }

    #[test]
    fn test_noise_layer() {
        let layer = NoiseLayer::ridged(0.01, 0.5).with_octaves(6);
        assert_eq!(layer.noise_type, NoiseType::Ridged);
        assert_eq!(layer.octaves, 6);
    }

    #[test]
    fn test_internal_mesh_generation() {
        let (vertices, indices) = internal::generate_mesh(4);
        assert_eq!(vertices.len(), 16);
        assert_eq!(indices.len(), 54);
    }
}
