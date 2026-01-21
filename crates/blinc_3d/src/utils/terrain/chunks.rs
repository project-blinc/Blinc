//! Chunk-based terrain streaming

use super::TerrainConfig;
use blinc_core::Vec3;
use std::collections::HashMap;

/// Chunk coordinate
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkCoord {
    pub x: i32,
    pub z: i32,
}

impl ChunkCoord {
    /// Create a new chunk coordinate
    pub fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// Get world position of chunk center
    pub fn world_center(&self, chunk_world_size: f32) -> Vec3 {
        Vec3::new(
            self.x as f32 * chunk_world_size + chunk_world_size * 0.5,
            0.0,
            self.z as f32 * chunk_world_size + chunk_world_size * 0.5,
        )
    }

    /// Get world position of chunk corner (min x, min z)
    pub fn world_min(&self, chunk_world_size: f32) -> Vec3 {
        Vec3::new(
            self.x as f32 * chunk_world_size,
            0.0,
            self.z as f32 * chunk_world_size,
        )
    }

    /// Distance to point (in chunk units)
    pub fn distance_to(&self, other: ChunkCoord) -> u32 {
        let dx = (self.x - other.x).unsigned_abs();
        let dz = (self.z - other.z).unsigned_abs();
        dx.max(dz)
    }
}

/// State of a terrain chunk
#[derive(Clone, Debug)]
pub enum ChunkState {
    /// Not loaded
    Unloaded,
    /// Currently loading
    Loading,
    /// Loaded and ready
    Loaded(ChunkData),
    /// Queued for unload
    Unloading,
}

/// Chunk terrain data
#[derive(Clone, Debug)]
pub struct ChunkData {
    /// Heightmap data (resolution x resolution)
    pub heights: Vec<f32>,
    /// Resolution (vertices per side)
    pub resolution: u32,
    /// Current LOD level
    pub lod_level: u32,
    /// Whether mesh needs rebuild
    pub mesh_dirty: bool,
    /// Bounds (min_height, max_height)
    pub height_bounds: (f32, f32),
}

impl ChunkData {
    /// Create new chunk data with given resolution
    pub fn new(resolution: u32) -> Self {
        Self {
            heights: vec![0.0; (resolution * resolution) as usize],
            resolution,
            lod_level: 0,
            mesh_dirty: true,
            height_bounds: (0.0, 0.0),
        }
    }

    /// Get height at local chunk position (0-1)
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        let fu = u.clamp(0.0, 1.0) * (self.resolution - 1) as f32;
        let fv = v.clamp(0.0, 1.0) * (self.resolution - 1) as f32;

        let x0 = fu.floor() as u32;
        let z0 = fv.floor() as u32;
        let x1 = (x0 + 1).min(self.resolution - 1);
        let z1 = (z0 + 1).min(self.resolution - 1);

        let tx = fu.fract();
        let tz = fv.fract();

        let idx = |x: u32, z: u32| (z * self.resolution + x) as usize;

        let h00 = self.heights[idx(x0, z0)];
        let h10 = self.heights[idx(x1, z0)];
        let h01 = self.heights[idx(x0, z1)];
        let h11 = self.heights[idx(x1, z1)];

        let h0 = h00 + (h10 - h00) * tx;
        let h1 = h01 + (h11 - h01) * tx;

        h0 + (h1 - h0) * tz
    }

    /// Update height bounds
    pub fn update_bounds(&mut self) {
        if self.heights.is_empty() {
            self.height_bounds = (0.0, 0.0);
            return;
        }

        let min = self.heights.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = self.heights.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        self.height_bounds = (min, max);
    }
}

/// Manages terrain chunks
#[derive(Clone, Debug)]
pub struct ChunkManager {
    /// Chunk states
    chunks: HashMap<ChunkCoord, ChunkState>,
    /// Chunk world size
    chunk_world_size: f32,
    /// Chunk resolution
    chunk_resolution: u32,
    /// View distance in chunks
    view_distance: u32,
    /// Current center chunk
    center_chunk: ChunkCoord,
    /// Chunks to load queue
    load_queue: Vec<ChunkCoord>,
    /// Chunks to unload queue
    unload_queue: Vec<ChunkCoord>,
}

impl ChunkManager {
    /// Create a new chunk manager
    pub fn new(config: &TerrainConfig) -> Self {
        let chunk_world_size = config.size / (config.size / (config.chunk_size as f32 * (config.size / config.resolution as f32))).ceil();
        let view_distance = (config.lod_distance / chunk_world_size).ceil() as u32 + 1;

        Self {
            chunks: HashMap::new(),
            chunk_world_size: chunk_world_size.max(1.0),
            chunk_resolution: config.chunk_size,
            view_distance,
            center_chunk: ChunkCoord::new(0, 0),
            load_queue: Vec::new(),
            unload_queue: Vec::new(),
        }
    }

    /// Get chunk at coordinate
    pub fn get_chunk(&self, coord: ChunkCoord) -> Option<&ChunkState> {
        self.chunks.get(&coord)
    }

    /// Get chunk data if loaded
    pub fn get_chunk_data(&self, coord: ChunkCoord) -> Option<&ChunkData> {
        match self.chunks.get(&coord) {
            Some(ChunkState::Loaded(data)) => Some(data),
            _ => None,
        }
    }

    /// Get mutable chunk data if loaded
    pub fn get_chunk_data_mut(&mut self, coord: ChunkCoord) -> Option<&mut ChunkData> {
        match self.chunks.get_mut(&coord) {
            Some(ChunkState::Loaded(data)) => Some(data),
            _ => None,
        }
    }

    /// Update chunk loading based on camera position
    pub fn update(&mut self, camera_position: Vec3, _config: &TerrainConfig) {
        // Calculate center chunk from camera position
        let new_center = ChunkCoord::new(
            (camera_position.x / self.chunk_world_size).floor() as i32,
            (camera_position.z / self.chunk_world_size).floor() as i32,
        );

        // If center changed, update load/unload queues
        if new_center != self.center_chunk {
            self.center_chunk = new_center;
            self.update_queues();
        }

        // Process some loads and unloads per frame
        self.process_queues();
    }

    /// Update load and unload queues
    fn update_queues(&mut self) {
        let view_dist = self.view_distance as i32;

        // Find chunks to load
        self.load_queue.clear();
        for dz in -view_dist..=view_dist {
            for dx in -view_dist..=view_dist {
                let coord = ChunkCoord::new(
                    self.center_chunk.x + dx,
                    self.center_chunk.z + dz,
                );

                if !self.chunks.contains_key(&coord) {
                    self.load_queue.push(coord);
                }
            }
        }

        // Sort by distance from center
        self.load_queue.sort_by_key(|c| c.distance_to(self.center_chunk));

        // Find chunks to unload
        self.unload_queue.clear();
        for (coord, _) in self.chunks.iter() {
            if coord.distance_to(self.center_chunk) > self.view_distance + 1 {
                self.unload_queue.push(*coord);
            }
        }
    }

    /// Process load/unload queues
    fn process_queues(&mut self) {
        // Load a few chunks per frame
        const LOADS_PER_FRAME: usize = 2;
        for _ in 0..LOADS_PER_FRAME {
            if let Some(coord) = self.load_queue.pop() {
                self.load_chunk(coord);
            }
        }

        // Unload chunks
        for coord in self.unload_queue.drain(..) {
            self.chunks.remove(&coord);
        }
    }

    /// Load a chunk (generates heightmap)
    fn load_chunk(&mut self, coord: ChunkCoord) {
        let mut data = ChunkData::new(self.chunk_resolution);

        // For now, fill with zeros (actual heightmap generation would go here)
        // In practice, this would sample the terrain's heightmap source

        data.update_bounds();
        self.chunks.insert(coord, ChunkState::Loaded(data));
    }

    /// Generate chunk from heightmap source
    pub fn generate_chunk(&mut self, coord: ChunkCoord, heights: Vec<f32>) {
        let mut data = ChunkData::new(self.chunk_resolution);
        data.heights = heights;
        data.update_bounds();
        self.chunks.insert(coord, ChunkState::Loaded(data));
    }

    /// Get world to chunk coordinate
    pub fn world_to_chunk(&self, x: f32, z: f32) -> ChunkCoord {
        ChunkCoord::new(
            (x / self.chunk_world_size).floor() as i32,
            (z / self.chunk_world_size).floor() as i32,
        )
    }

    /// Get chunk world size
    pub fn chunk_world_size(&self) -> f32 {
        self.chunk_world_size
    }

    /// Get all loaded chunks
    pub fn loaded_chunks(&self) -> impl Iterator<Item = (ChunkCoord, &ChunkData)> {
        self.chunks.iter().filter_map(|(coord, state)| {
            match state {
                ChunkState::Loaded(data) => Some((*coord, data)),
                _ => None,
            }
        })
    }

    /// Get number of loaded chunks
    pub fn loaded_count(&self) -> usize {
        self.chunks.values()
            .filter(|s| matches!(s, ChunkState::Loaded(_)))
            .count()
    }

    /// Get view distance
    pub fn view_distance(&self) -> u32 {
        self.view_distance
    }

    /// Set view distance
    pub fn set_view_distance(&mut self, distance: u32) {
        self.view_distance = distance;
        self.update_queues();
    }
}

/// Chunk mesh generation utilities
pub struct ChunkMeshGen;

impl ChunkMeshGen {
    /// Generate vertex count for LOD level
    pub fn vertex_count(base_resolution: u32, lod_level: u32) -> u32 {
        let res = base_resolution >> lod_level;
        res.max(2) * res.max(2)
    }

    /// Generate index count for LOD level
    pub fn index_count(base_resolution: u32, lod_level: u32) -> u32 {
        let res = base_resolution >> lod_level;
        let quads = (res.max(2) - 1) * (res.max(2) - 1);
        quads * 6
    }

    /// Generate vertices for a chunk
    pub fn generate_vertices(
        chunk: &ChunkData,
        chunk_coord: ChunkCoord,
        chunk_world_size: f32,
        max_height: f32,
        lod_level: u32,
    ) -> Vec<ChunkVertex> {
        let step = 1u32 << lod_level;
        let res = chunk.resolution / step;

        let mut vertices = Vec::with_capacity((res * res) as usize);

        for z in 0..res {
            for x in 0..res {
                let u = x as f32 / (res - 1) as f32;
                let v = z as f32 / (res - 1) as f32;

                let height = chunk.sample(u, v) * max_height;

                let world_pos = Vec3::new(
                    chunk_coord.x as f32 * chunk_world_size + u * chunk_world_size,
                    height,
                    chunk_coord.z as f32 * chunk_world_size + v * chunk_world_size,
                );

                // Calculate normal from neighbors
                let epsilon = 1.0 / chunk.resolution as f32;
                let h_l = chunk.sample((u - epsilon).max(0.0), v) * max_height;
                let h_r = chunk.sample((u + epsilon).min(1.0), v) * max_height;
                let h_d = chunk.sample(u, (v - epsilon).max(0.0)) * max_height;
                let h_u = chunk.sample(u, (v + epsilon).min(1.0)) * max_height;

                let scale = chunk_world_size * epsilon;
                let normal = normalize_vec3(Vec3::new(
                    (h_l - h_r) / scale,
                    2.0,
                    (h_d - h_u) / scale,
                ));

                vertices.push(ChunkVertex {
                    position: world_pos,
                    normal,
                    uv: Vec3::new(u, v, 0.0),
                });
            }
        }

        vertices
    }

    /// Generate indices for a chunk
    pub fn generate_indices(resolution: u32, lod_level: u32) -> Vec<u32> {
        let step = 1u32 << lod_level;
        let res = resolution / step;

        let mut indices = Vec::with_capacity(((res - 1) * (res - 1) * 6) as usize);

        for z in 0..(res - 1) {
            for x in 0..(res - 1) {
                let i00 = z * res + x;
                let i10 = z * res + x + 1;
                let i01 = (z + 1) * res + x;
                let i11 = (z + 1) * res + x + 1;

                // Triangle 1
                indices.push(i00);
                indices.push(i01);
                indices.push(i10);

                // Triangle 2
                indices.push(i10);
                indices.push(i01);
                indices.push(i11);
            }
        }

        indices
    }
}

/// Terrain chunk vertex
#[derive(Clone, Debug)]
pub struct ChunkVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec3, // xy = terrain UV, z = unused
}

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
    fn test_chunk_coord() {
        let c1 = ChunkCoord::new(0, 0);
        let c2 = ChunkCoord::new(3, 4);
        assert_eq!(c1.distance_to(c2), 4);
    }

    #[test]
    fn test_chunk_data_sample() {
        let mut data = ChunkData::new(3);
        data.heights = vec![0.0, 0.5, 1.0, 0.5, 0.75, 0.5, 1.0, 0.5, 0.0];

        assert!((data.sample(0.0, 0.0) - 0.0).abs() < 0.001);
        assert!((data.sample(1.0, 0.0) - 1.0).abs() < 0.001);
        assert!((data.sample(0.5, 0.0) - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_mesh_gen() {
        let indices = ChunkMeshGen::generate_indices(4, 0);
        assert_eq!(indices.len(), 3 * 3 * 6); // 3x3 quads, 6 indices each
    }
}
