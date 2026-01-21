//! Heightmap generation for terrain

use super::noise::NoiseLayer;
use blinc_core::Vec3;

/// Procedural heightmap generator
#[derive(Clone, Debug)]
pub struct ProceduralHeightmap {
    /// Noise layers
    pub layers: Vec<NoiseLayer>,
    /// Falloff function
    pub falloff: Option<FalloffType>,
    /// Global seed
    pub seed: u64,
    /// Output range
    pub min_height: f32,
    pub max_height: f32,
}

impl ProceduralHeightmap {
    /// Create a new procedural heightmap
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
            falloff: None,
            seed: 0,
            min_height: 0.0,
            max_height: 1.0,
        }
    }

    /// Add a noise layer
    pub fn with_layer(mut self, layer: NoiseLayer) -> Self {
        self.layers.push(layer);
        self
    }

    /// Set falloff function
    pub fn with_falloff(mut self, falloff: FalloffType) -> Self {
        self.falloff = Some(falloff);
        self
    }

    /// Set seed
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Set output range
    pub fn with_range(mut self, min: f32, max: f32) -> Self {
        self.min_height = min;
        self.max_height = max;
        self
    }

    /// Sample height at position (returns 0-1 normalized)
    pub fn sample(&self, x: f32, z: f32) -> f32 {
        if self.layers.is_empty() {
            return 0.0;
        }

        // Sum all noise layers
        let mut height = 0.0;
        let mut total_weight = 0.0;

        for layer in &self.layers {
            let value = layer.sample(x, z, self.seed);
            height += value * layer.amplitude;
            total_weight += layer.amplitude;
        }

        // Normalize
        if total_weight > 0.0 {
            height /= total_weight;
        }

        // Apply falloff
        if let Some(ref falloff) = self.falloff {
            let falloff_value = falloff.sample(x, z);
            height *= falloff_value;
        }

        // Clamp to 0-1
        height.clamp(0.0, 1.0)
    }

    /// Generate heightmap data
    pub fn generate(&self, width: u32, height: u32, scale: f32) -> Vec<f32> {
        let mut data = Vec::with_capacity((width * height) as usize);

        for z in 0..height {
            for x in 0..width {
                let wx = (x as f32 - width as f32 / 2.0) * scale;
                let wz = (z as f32 - height as f32 / 2.0) * scale;
                data.push(self.sample(wx, wz));
            }
        }

        data
    }

    // ========== Presets ==========

    /// Rolling hills preset
    pub fn rolling_hills() -> Self {
        Self::new()
            .with_layer(NoiseLayer::perlin(0.005, 0.5))
            .with_layer(NoiseLayer::perlin(0.02, 0.3))
            .with_layer(NoiseLayer::perlin(0.08, 0.15))
            .with_layer(NoiseLayer::perlin(0.3, 0.05))
    }

    /// Mountain range preset
    pub fn mountains() -> Self {
        Self::new()
            .with_layer(NoiseLayer::ridged(0.002, 0.6))
            .with_layer(NoiseLayer::fbm(0.01, 0.25))
            .with_layer(NoiseLayer::perlin(0.05, 0.1))
            .with_layer(NoiseLayer::perlin(0.2, 0.05))
    }

    /// Flat plains preset
    pub fn plains() -> Self {
        Self::new()
            .with_layer(NoiseLayer::perlin(0.002, 0.3))
            .with_layer(NoiseLayer::perlin(0.01, 0.5))
            .with_layer(NoiseLayer::perlin(0.05, 0.2))
    }

    /// Canyon terrain preset
    pub fn canyons() -> Self {
        Self::new()
            .with_layer(NoiseLayer::ridged(0.003, 0.8))
            .with_layer(NoiseLayer::billow(0.01, 0.15))
            .with_layer(NoiseLayer::perlin(0.1, 0.05))
    }

    /// Island preset
    pub fn island(radius: f32) -> Self {
        Self::new()
            .with_layer(NoiseLayer::perlin(0.003, 0.4))
            .with_layer(NoiseLayer::ridged(0.01, 0.35))
            .with_layer(NoiseLayer::perlin(0.05, 0.15))
            .with_layer(NoiseLayer::perlin(0.2, 0.1))
            .with_falloff(FalloffType::Island { radius })
    }
}

impl Default for ProceduralHeightmap {
    fn default() -> Self {
        Self::rolling_hills()
    }
}

/// Falloff types for terrain boundaries
#[derive(Clone)]
pub enum FalloffType {
    /// Circular island falloff
    Island {
        /// Island radius
        radius: f32,
    },
    /// Square falloff
    Square {
        /// Half-size
        half_size: f32,
    },
    /// Radial gradient
    Radial {
        /// Inner radius (full height)
        inner_radius: f32,
        /// Outer radius (zero height)
        outer_radius: f32,
    },
    /// One-sided cliff
    Cliff {
        /// Cliff direction (normalized)
        direction: Vec3,
        /// Cliff offset from origin
        offset: f32,
        /// Cliff falloff distance
        falloff: f32,
    },
    /// Custom function
    Custom {
        /// Falloff function (takes x, z, returns 0-1)
        func: std::sync::Arc<dyn Fn(f32, f32) -> f32 + Send + Sync>,
    },
}

impl std::fmt::Debug for FalloffType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FalloffType::Island { radius } => f.debug_struct("Island").field("radius", radius).finish(),
            FalloffType::Square { half_size } => f.debug_struct("Square").field("half_size", half_size).finish(),
            FalloffType::Radial { inner_radius, outer_radius } => f.debug_struct("Radial")
                .field("inner_radius", inner_radius)
                .field("outer_radius", outer_radius)
                .finish(),
            FalloffType::Cliff { direction, offset, falloff } => f.debug_struct("Cliff")
                .field("direction", direction)
                .field("offset", offset)
                .field("falloff", falloff)
                .finish(),
            FalloffType::Custom { .. } => f.debug_struct("Custom").finish_non_exhaustive(),
        }
    }
}

impl FalloffType {
    /// Sample falloff value at position (0 = no height, 1 = full height)
    pub fn sample(&self, x: f32, z: f32) -> f32 {
        match self {
            FalloffType::Island { radius } => {
                let dist = (x * x + z * z).sqrt();
                let t = dist / *radius;
                if t >= 1.0 {
                    0.0
                } else {
                    // Smooth falloff using smoothstep
                    let t = t.clamp(0.0, 1.0);
                    1.0 - (t * t * (3.0 - 2.0 * t))
                }
            }
            FalloffType::Square { half_size } => {
                let dx = x.abs() / *half_size;
                let dz = z.abs() / *half_size;
                let t = dx.max(dz);
                if t >= 1.0 {
                    0.0
                } else {
                    let t = t.clamp(0.0, 1.0);
                    1.0 - (t * t * (3.0 - 2.0 * t))
                }
            }
            FalloffType::Radial { inner_radius, outer_radius } => {
                let dist = (x * x + z * z).sqrt();
                if dist <= *inner_radius {
                    1.0
                } else if dist >= *outer_radius {
                    0.0
                } else {
                    let t = (dist - inner_radius) / (outer_radius - inner_radius);
                    1.0 - t
                }
            }
            FalloffType::Cliff { direction, offset, falloff } => {
                let dot = x * direction.x + z * direction.z;
                let dist = dot - *offset;
                if dist <= 0.0 {
                    1.0
                } else if dist >= *falloff {
                    0.0
                } else {
                    1.0 - (dist / *falloff)
                }
            }
            FalloffType::Custom { func } => {
                func(x, z).clamp(0.0, 1.0)
            }
        }
    }
}

/// Heightmap operations and utilities
pub struct HeightmapOps;

impl HeightmapOps {
    /// Smooth heightmap data using box blur
    pub fn smooth(data: &mut [f32], width: u32, height: u32, iterations: u32) {
        let mut temp = data.to_vec();

        for _ in 0..iterations {
            for z in 1..(height - 1) {
                for x in 1..(width - 1) {
                    let idx = (z * width + x) as usize;

                    // 3x3 box average
                    let mut sum = 0.0;
                    for dz in -1i32..=1 {
                        for dx in -1i32..=1 {
                            let nidx = ((z as i32 + dz) as u32 * width + (x as i32 + dx) as u32) as usize;
                            sum += data[nidx];
                        }
                    }
                    temp[idx] = sum / 9.0;
                }
            }

            // Copy back
            data.copy_from_slice(&temp);
        }
    }

    /// Apply erosion simulation (simplified thermal erosion)
    pub fn erode(data: &mut [f32], width: u32, height: u32, iterations: u32, talus: f32) {
        for _ in 0..iterations {
            for z in 1..(height - 1) {
                for x in 1..(width - 1) {
                    let idx = (z * width + x) as usize;
                    let h = data[idx];

                    // Find steepest downhill neighbor
                    let mut max_slope = 0.0;
                    let mut target_idx = idx;

                    for dz in -1i32..=1 {
                        for dx in -1i32..=1 {
                            if dx == 0 && dz == 0 {
                                continue;
                            }
                            let nidx = ((z as i32 + dz) as u32 * width + (x as i32 + dx) as u32) as usize;
                            let nh = data[nidx];
                            let slope = h - nh;
                            if slope > max_slope && slope > talus {
                                max_slope = slope;
                                target_idx = nidx;
                            }
                        }
                    }

                    // Move material downhill
                    if target_idx != idx {
                        let transfer = (max_slope - talus) * 0.5;
                        data[idx] -= transfer;
                        data[target_idx] += transfer;
                    }
                }
            }
        }
    }

    /// Normalize heightmap to 0-1 range
    pub fn normalize(data: &mut [f32]) {
        if data.is_empty() {
            return;
        }

        let min = data.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        if (max - min).abs() < 0.0001 {
            data.fill(0.5);
            return;
        }

        let range = max - min;
        for h in data.iter_mut() {
            *h = (*h - min) / range;
        }
    }

    /// Apply power curve (increases contrast)
    pub fn apply_curve(data: &mut [f32], power: f32) {
        for h in data.iter_mut() {
            *h = h.powf(power);
        }
    }

    /// Clamp heights to range
    pub fn clamp(data: &mut [f32], min: f32, max: f32) {
        for h in data.iter_mut() {
            *h = h.clamp(min, max);
        }
    }

    /// Add detail noise to heightmap
    pub fn add_detail(data: &mut [f32], width: u32, height: u32, frequency: f32, amplitude: f32, seed: u64) {
        let noise = NoiseLayer::perlin(frequency, amplitude);
        let scale = 1.0; // World scale per pixel

        for z in 0..height {
            for x in 0..width {
                let idx = (z * width + x) as usize;
                let wx = x as f32 * scale;
                let wz = z as f32 * scale;
                data[idx] += noise.sample(wx, wz, seed) * amplitude;
            }
        }
    }

    /// Generate normals from heightmap
    pub fn generate_normals(data: &[f32], width: u32, height: u32, scale: f32) -> Vec<Vec3> {
        let mut normals = Vec::with_capacity(data.len());

        for z in 0..height {
            for x in 0..width {
                let idx = |px: u32, pz: u32| -> usize {
                    (pz.min(height - 1) * width + px.min(width - 1)) as usize
                };

                let h_l = if x > 0 { data[idx(x - 1, z)] } else { data[idx(x, z)] };
                let h_r = if x < width - 1 { data[idx(x + 1, z)] } else { data[idx(x, z)] };
                let h_d = if z > 0 { data[idx(x, z - 1)] } else { data[idx(x, z)] };
                let h_u = if z < height - 1 { data[idx(x, z + 1)] } else { data[idx(x, z)] };

                let dx = (h_l - h_r) * scale;
                let dz = (h_d - h_u) * scale;

                // Normal from finite differences
                let normal = Vec3::new(dx, 2.0, dz);
                let len = (normal.x * normal.x + normal.y * normal.y + normal.z * normal.z).sqrt();
                normals.push(Vec3::new(normal.x / len, normal.y / len, normal.z / len));
            }
        }

        normals
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_procedural_heightmap() {
        let heightmap = ProceduralHeightmap::new()
            .with_layer(NoiseLayer::perlin(0.01, 1.0));

        let h = heightmap.sample(0.0, 0.0);
        assert!(h >= 0.0 && h <= 1.0);
    }

    #[test]
    fn test_falloff() {
        let falloff = FalloffType::Island { radius: 100.0 };
        assert!((falloff.sample(0.0, 0.0) - 1.0).abs() < 0.001);
        assert!((falloff.sample(200.0, 0.0) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_heightmap_generation() {
        let heightmap = ProceduralHeightmap::plains();
        let data = heightmap.generate(64, 64, 1.0);
        assert_eq!(data.len(), 64 * 64);
    }
}
