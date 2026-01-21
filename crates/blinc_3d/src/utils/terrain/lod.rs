//! Level of Detail (LOD) for terrain rendering

use super::ChunkCoord;
use blinc_core::Vec3;

/// LOD configuration
#[derive(Clone, Debug)]
pub struct LodConfig {
    /// Number of LOD levels (0 = highest detail)
    pub levels: u32,
    /// Distance thresholds for each LOD level
    pub distances: Vec<f32>,
    /// Morph factor (0-1) for smooth LOD transitions
    pub morph_enabled: bool,
    /// Screen-space error threshold (alternative to distance)
    pub error_threshold: Option<f32>,
}

impl Default for LodConfig {
    fn default() -> Self {
        Self::new(4, 100.0)
    }
}

impl LodConfig {
    /// Create a new LOD config
    pub fn new(levels: u32, base_distance: f32) -> Self {
        let levels = levels.clamp(1, 8);
        let mut distances = Vec::with_capacity(levels as usize);

        // Each LOD level is 2x the distance of the previous
        for i in 0..levels {
            distances.push(base_distance * (1 << i) as f32);
        }

        Self {
            levels,
            distances,
            morph_enabled: true,
            error_threshold: None,
        }
    }

    /// Create with custom distances
    pub fn with_distances(distances: Vec<f32>) -> Self {
        Self {
            levels: distances.len() as u32,
            distances,
            morph_enabled: true,
            error_threshold: None,
        }
    }

    /// Enable/disable morphing
    pub fn with_morphing(mut self, enabled: bool) -> Self {
        self.morph_enabled = enabled;
        self
    }

    /// Set screen-space error threshold
    pub fn with_error_threshold(mut self, threshold: f32) -> Self {
        self.error_threshold = Some(threshold);
        self
    }

    /// Get LOD level for distance
    pub fn get_lod_level(&self, distance: f32) -> u32 {
        for (i, &threshold) in self.distances.iter().enumerate() {
            if distance < threshold {
                return i as u32;
            }
        }
        (self.levels - 1).max(0)
    }

    /// Get morph factor for smooth LOD transitions (0 = current LOD, 1 = next LOD)
    pub fn get_morph_factor(&self, distance: f32, lod_level: u32) -> f32 {
        if !self.morph_enabled || lod_level >= self.levels - 1 {
            return 0.0;
        }

        let current_dist = if lod_level == 0 {
            0.0
        } else {
            self.distances[(lod_level - 1) as usize]
        };
        let next_dist = self.distances[lod_level as usize];

        let range = next_dist - current_dist;
        let local_dist = distance - current_dist;

        // Morph in the last 20% of each LOD range
        let morph_start = range * 0.8;
        if local_dist < morph_start {
            0.0
        } else {
            (local_dist - morph_start) / (range - morph_start)
        }
    }

    /// Get resolution for LOD level
    pub fn get_resolution(&self, base_resolution: u32, lod_level: u32) -> u32 {
        (base_resolution >> lod_level).max(2)
    }
}

/// LOD selection for terrain chunks
pub struct LodSelector {
    config: LodConfig,
}

impl LodSelector {
    /// Create a new LOD selector
    pub fn new(config: LodConfig) -> Self {
        Self { config }
    }

    /// Select LOD level for a chunk
    pub fn select(
        &self,
        chunk_coord: ChunkCoord,
        chunk_world_size: f32,
        camera_position: Vec3,
    ) -> LodSelection {
        let chunk_center = chunk_coord.world_center(chunk_world_size);
        let distance = Self::distance_to_chunk(chunk_center, chunk_world_size, camera_position);

        let lod_level = self.config.get_lod_level(distance);
        let morph_factor = self.config.get_morph_factor(distance, lod_level);

        LodSelection {
            lod_level,
            morph_factor,
            distance,
        }
    }

    /// Calculate distance from camera to chunk
    fn distance_to_chunk(chunk_center: Vec3, chunk_world_size: f32, camera_position: Vec3) -> f32 {
        // Distance to closest point on chunk bounds
        let half_size = chunk_world_size * 0.5;

        let dx = (camera_position.x - chunk_center.x).abs() - half_size;
        let dz = (camera_position.z - chunk_center.z).abs() - half_size;

        let dx = dx.max(0.0);
        let dz = dz.max(0.0);

        (dx * dx + dz * dz).sqrt()
    }

    /// Get config
    pub fn config(&self) -> &LodConfig {
        &self.config
    }
}

/// LOD selection result
#[derive(Clone, Debug)]
pub struct LodSelection {
    /// Selected LOD level
    pub lod_level: u32,
    /// Morph factor for smooth transition
    pub morph_factor: f32,
    /// Distance to camera
    pub distance: f32,
}

/// Manages LOD transitions and stitching
pub struct LodStitcher {
    /// LOD levels of neighboring chunks [N, E, S, W]
    neighbors: [u32; 4],
    /// Current chunk LOD level
    current_lod: u32,
}

impl LodStitcher {
    /// Create a new LOD stitcher
    pub fn new(current_lod: u32) -> Self {
        Self {
            neighbors: [current_lod; 4],
            current_lod,
        }
    }

    /// Set neighbor LOD levels (N, E, S, W)
    pub fn set_neighbors(&mut self, north: u32, east: u32, south: u32, west: u32) {
        self.neighbors = [north, east, south, west];
    }

    /// Check if edge needs stitching
    pub fn needs_stitch(&self, edge: Edge) -> bool {
        self.neighbors[edge as usize] > self.current_lod
    }

    /// Get stitch level difference for edge
    pub fn stitch_level(&self, edge: Edge) -> u32 {
        let neighbor = self.neighbors[edge as usize];
        if neighbor > self.current_lod {
            neighbor - self.current_lod
        } else {
            0
        }
    }

    /// Generate indices with stitching for LOD boundaries
    pub fn generate_stitched_indices(
        &self,
        resolution: u32,
        lod_level: u32,
    ) -> Vec<u32> {
        let step = 1u32 << lod_level;
        let res = resolution / step;

        let mut indices = Vec::new();

        // Generate interior triangles (not on edges)
        for z in 1..(res - 2) {
            for x in 1..(res - 2) {
                let i00 = z * res + x;
                let i10 = z * res + x + 1;
                let i01 = (z + 1) * res + x;
                let i11 = (z + 1) * res + x + 1;

                indices.push(i00);
                indices.push(i01);
                indices.push(i10);

                indices.push(i10);
                indices.push(i01);
                indices.push(i11);
            }
        }

        // Generate edge triangles with stitching
        self.stitch_edge(&mut indices, res, Edge::North);
        self.stitch_edge(&mut indices, res, Edge::East);
        self.stitch_edge(&mut indices, res, Edge::South);
        self.stitch_edge(&mut indices, res, Edge::West);

        indices
    }

    fn stitch_edge(&self, indices: &mut Vec<u32>, res: u32, edge: Edge) {
        let stitch_level = self.stitch_level(edge);

        if stitch_level == 0 {
            // No stitching needed, use normal triangles
            self.generate_edge_normal(indices, res, edge);
        } else {
            // Stitch by skipping vertices on the edge
            self.generate_edge_stitched(indices, res, edge, stitch_level);
        }
    }

    fn generate_edge_normal(&self, indices: &mut Vec<u32>, res: u32, edge: Edge) {
        match edge {
            Edge::North => {
                let z = res - 2;
                for x in 0..(res - 1) {
                    let i00 = z * res + x;
                    let i10 = z * res + x + 1;
                    let i01 = (z + 1) * res + x;
                    let i11 = (z + 1) * res + x + 1;

                    indices.push(i00);
                    indices.push(i01);
                    indices.push(i10);

                    indices.push(i10);
                    indices.push(i01);
                    indices.push(i11);
                }
            }
            Edge::South => {
                let z = 0;
                for x in 0..(res - 1) {
                    let i00 = z * res + x;
                    let i10 = z * res + x + 1;
                    let i01 = (z + 1) * res + x;
                    let i11 = (z + 1) * res + x + 1;

                    indices.push(i00);
                    indices.push(i01);
                    indices.push(i10);

                    indices.push(i10);
                    indices.push(i01);
                    indices.push(i11);
                }
            }
            Edge::East => {
                let x = res - 2;
                for z in 0..(res - 1) {
                    let i00 = z * res + x;
                    let i10 = z * res + x + 1;
                    let i01 = (z + 1) * res + x;
                    let i11 = (z + 1) * res + x + 1;

                    indices.push(i00);
                    indices.push(i01);
                    indices.push(i10);

                    indices.push(i10);
                    indices.push(i01);
                    indices.push(i11);
                }
            }
            Edge::West => {
                let x = 0;
                for z in 0..(res - 1) {
                    let i00 = z * res + x;
                    let i10 = z * res + x + 1;
                    let i01 = (z + 1) * res + x;
                    let i11 = (z + 1) * res + x + 1;

                    indices.push(i00);
                    indices.push(i01);
                    indices.push(i10);

                    indices.push(i10);
                    indices.push(i01);
                    indices.push(i11);
                }
            }
        }
    }

    fn generate_edge_stitched(&self, indices: &mut Vec<u32>, res: u32, edge: Edge, _level: u32) {
        // Simplified stitching - skip every other vertex on the edge
        // Full implementation would handle multiple LOD level differences
        match edge {
            Edge::North => {
                let z = res - 2;
                for x in (0..(res - 1)).step_by(2) {
                    let i00 = z * res + x;
                    let i10 = z * res + (x + 2).min(res - 1);
                    let i01 = (z + 1) * res + x;
                    let i11 = (z + 1) * res + (x + 2).min(res - 1);
                    let i_mid = z * res + x + 1;

                    // Fan triangles
                    indices.push(i00);
                    indices.push(i01);
                    indices.push(i_mid);

                    indices.push(i_mid);
                    indices.push(i01);
                    indices.push(i11);

                    indices.push(i_mid);
                    indices.push(i11);
                    indices.push(i10);
                }
            }
            // Similar for other edges...
            _ => {
                self.generate_edge_normal(indices, res, edge);
            }
        }
    }
}

/// Chunk edge direction
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    North = 0,
    East = 1,
    South = 2,
    West = 3,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lod_config() {
        let config = LodConfig::new(4, 100.0);
        assert_eq!(config.levels, 4);
        assert!((config.distances[0] - 100.0).abs() < 0.001);
        assert!((config.distances[1] - 200.0).abs() < 0.001);
    }

    #[test]
    fn test_lod_selection() {
        let config = LodConfig::new(4, 100.0);
        assert_eq!(config.get_lod_level(50.0), 0);
        assert_eq!(config.get_lod_level(150.0), 1);
        assert_eq!(config.get_lod_level(350.0), 2);
    }

    #[test]
    fn test_lod_resolution() {
        let config = LodConfig::default();
        assert_eq!(config.get_resolution(64, 0), 64);
        assert_eq!(config.get_resolution(64, 1), 32);
        assert_eq!(config.get_resolution(64, 2), 16);
    }
}
