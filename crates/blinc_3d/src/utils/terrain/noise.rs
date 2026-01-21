//! Noise functions for procedural terrain generation

/// Noise layer configuration
#[derive(Clone, Debug)]
pub struct NoiseLayer {
    /// Noise type
    pub noise_type: NoiseType,
    /// Frequency (higher = more detail)
    pub frequency: f32,
    /// Amplitude (output scale)
    pub amplitude: f32,
    /// Octaves for fractal noise
    pub octaves: u32,
    /// Lacunarity (frequency multiplier per octave)
    pub lacunarity: f32,
    /// Persistence (amplitude multiplier per octave)
    pub persistence: f32,
    /// Layer-specific seed offset
    pub seed_offset: u64,
}

impl NoiseLayer {
    /// Create a new noise layer
    pub fn new(noise_type: NoiseType, frequency: f32, amplitude: f32) -> Self {
        Self {
            noise_type,
            frequency,
            amplitude,
            octaves: 1,
            lacunarity: 2.0,
            persistence: 0.5,
            seed_offset: 0,
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
        Self::new(NoiseType::Ridged, frequency, amplitude)
            .with_octaves(6)
    }

    /// Create billow noise layer (abs of Perlin)
    pub fn billow(frequency: f32, amplitude: f32) -> Self {
        Self::new(NoiseType::Billow, frequency, amplitude)
            .with_octaves(4)
    }

    /// Create FBM (fractal Brownian motion) noise layer
    pub fn fbm(frequency: f32, amplitude: f32) -> Self {
        Self::new(NoiseType::Perlin, frequency, amplitude)
            .with_octaves(6)
    }

    /// Set number of octaves
    pub fn with_octaves(mut self, octaves: u32) -> Self {
        self.octaves = octaves.clamp(1, 16);
        self
    }

    /// Set lacunarity
    pub fn with_lacunarity(mut self, lacunarity: f32) -> Self {
        self.lacunarity = lacunarity;
        self
    }

    /// Set persistence
    pub fn with_persistence(mut self, persistence: f32) -> Self {
        self.persistence = persistence;
        self
    }

    /// Set seed offset
    pub fn with_seed_offset(mut self, offset: u64) -> Self {
        self.seed_offset = offset;
        self
    }

    /// Sample noise at position
    pub fn sample(&self, x: f32, z: f32, seed: u64) -> f32 {
        let combined_seed = seed.wrapping_add(self.seed_offset);

        match self.noise_type {
            NoiseType::Perlin => {
                if self.octaves == 1 {
                    perlin_2d(x * self.frequency, z * self.frequency, combined_seed)
                } else {
                    fbm_perlin(x, z, self.frequency, self.octaves, self.lacunarity, self.persistence, combined_seed)
                }
            }
            NoiseType::Simplex => {
                if self.octaves == 1 {
                    simplex_2d(x * self.frequency, z * self.frequency, combined_seed)
                } else {
                    fbm_simplex(x, z, self.frequency, self.octaves, self.lacunarity, self.persistence, combined_seed)
                }
            }
            NoiseType::Worley => {
                worley_2d(x * self.frequency, z * self.frequency, combined_seed)
            }
            NoiseType::Ridged => {
                ridged_multifractal(x, z, self.frequency, self.octaves, self.lacunarity, self.persistence, combined_seed)
            }
            NoiseType::Billow => {
                billow_noise(x, z, self.frequency, self.octaves, self.lacunarity, self.persistence, combined_seed)
            }
            NoiseType::Value => {
                value_noise_2d(x * self.frequency, z * self.frequency, combined_seed)
            }
        }
    }
}

/// Noise function types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseType {
    /// Classic Perlin noise
    Perlin,
    /// Simplex noise (faster, fewer artifacts)
    Simplex,
    /// Worley/cellular noise
    Worley,
    /// Ridged multifractal
    Ridged,
    /// Billow (absolute value Perlin)
    Billow,
    /// Simple value noise
    Value,
}

// ========== Noise Implementations ==========

/// Hash function for noise
fn hash(x: i32, y: i32, seed: u64) -> u32 {
    let mut h = seed as u32;
    h = h.wrapping_mul(374761393);
    h = h.wrapping_add((x as u32).wrapping_mul(668265263));
    h = h.wrapping_add((y as u32).wrapping_mul(2654435761));
    h ^= h >> 13;
    h = h.wrapping_mul(1274126177);
    h ^= h >> 16;
    h
}

/// Convert hash to float in [-1, 1]
fn hash_to_float(h: u32) -> f32 {
    (h as f32) / (u32::MAX as f32) * 2.0 - 1.0
}

/// Convert hash to float in [0, 1]
fn hash_to_float_01(h: u32) -> f32 {
    (h as f32) / (u32::MAX as f32)
}

/// Smoothstep interpolation
fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Quintic interpolation (smoother than smoothstep)
fn quintic(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Linear interpolation
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Get gradient vector from hash
fn gradient_2d(h: u32) -> (f32, f32) {
    let angle = hash_to_float_01(h) * std::f32::consts::TAU;
    (angle.cos(), angle.sin())
}

/// 2D Perlin noise
pub fn perlin_2d(x: f32, y: f32, seed: u64) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;

    let fx = x - x0 as f32;
    let fy = y - y0 as f32;

    let u = quintic(fx);
    let v = quintic(fy);

    // Get gradients at corners
    let (gx00, gy00) = gradient_2d(hash(x0, y0, seed));
    let (gx10, gy10) = gradient_2d(hash(x1, y0, seed));
    let (gx01, gy01) = gradient_2d(hash(x0, y1, seed));
    let (gx11, gy11) = gradient_2d(hash(x1, y1, seed));

    // Dot products
    let n00 = gx00 * fx + gy00 * fy;
    let n10 = gx10 * (fx - 1.0) + gy10 * fy;
    let n01 = gx01 * fx + gy01 * (fy - 1.0);
    let n11 = gx11 * (fx - 1.0) + gy11 * (fy - 1.0);

    // Interpolate
    let nx0 = lerp(n00, n10, u);
    let nx1 = lerp(n01, n11, u);
    let result = lerp(nx0, nx1, v);

    // Normalize to [0, 1]
    (result + 1.0) * 0.5
}

/// 2D Simplex noise (approximation)
pub fn simplex_2d(x: f32, y: f32, seed: u64) -> f32 {
    const F2: f32 = 0.366025403784;  // (sqrt(3) - 1) / 2
    const G2: f32 = 0.211324865405;  // (3 - sqrt(3)) / 6

    let s = (x + y) * F2;
    let i = (x + s).floor() as i32;
    let j = (y + s).floor() as i32;

    let t = (i + j) as f32 * G2;
    let x0 = x - (i as f32 - t);
    let y0 = y - (j as f32 - t);

    let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };

    let x1 = x0 - i1 as f32 + G2;
    let y1 = y0 - j1 as f32 + G2;
    let x2 = x0 - 1.0 + 2.0 * G2;
    let y2 = y0 - 1.0 + 2.0 * G2;

    let mut n0 = 0.0;
    let mut n1 = 0.0;
    let mut n2 = 0.0;

    let t0 = 0.5 - x0 * x0 - y0 * y0;
    if t0 > 0.0 {
        let t0 = t0 * t0;
        let (gx, gy) = gradient_2d(hash(i, j, seed));
        n0 = t0 * t0 * (gx * x0 + gy * y0);
    }

    let t1 = 0.5 - x1 * x1 - y1 * y1;
    if t1 > 0.0 {
        let t1 = t1 * t1;
        let (gx, gy) = gradient_2d(hash(i + i1, j + j1, seed));
        n1 = t1 * t1 * (gx * x1 + gy * y1);
    }

    let t2 = 0.5 - x2 * x2 - y2 * y2;
    if t2 > 0.0 {
        let t2 = t2 * t2;
        let (gx, gy) = gradient_2d(hash(i + 1, j + 1, seed));
        n2 = t2 * t2 * (gx * x2 + gy * y2);
    }

    // Scale to [0, 1]
    (40.0 * (n0 + n1 + n2) + 1.0) * 0.5
}

/// 2D Value noise
pub fn value_noise_2d(x: f32, y: f32, seed: u64) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;

    let fx = x - x0 as f32;
    let fy = y - y0 as f32;

    let u = smoothstep(fx);
    let v = smoothstep(fy);

    let n00 = hash_to_float_01(hash(x0, y0, seed));
    let n10 = hash_to_float_01(hash(x0 + 1, y0, seed));
    let n01 = hash_to_float_01(hash(x0, y0 + 1, seed));
    let n11 = hash_to_float_01(hash(x0 + 1, y0 + 1, seed));

    let nx0 = lerp(n00, n10, u);
    let nx1 = lerp(n01, n11, u);
    lerp(nx0, nx1, v)
}

/// 2D Worley/cellular noise
pub fn worley_2d(x: f32, y: f32, seed: u64) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;

    let mut min_dist = f32::MAX;

    // Check 3x3 cells
    for dj in -1..=1 {
        for di in -1..=1 {
            let ci = xi + di;
            let cj = yi + dj;

            // Random point in this cell
            let h1 = hash(ci, cj, seed);
            let h2 = hash(ci, cj, seed.wrapping_add(1));
            let px = ci as f32 + hash_to_float_01(h1);
            let py = cj as f32 + hash_to_float_01(h2);

            let dx = x - px;
            let dy = y - py;
            let dist = (dx * dx + dy * dy).sqrt();

            min_dist = min_dist.min(dist);
        }
    }

    // Normalize (approximate)
    min_dist.min(1.0)
}

/// FBM with Perlin noise
pub fn fbm_perlin(x: f32, y: f32, frequency: f32, octaves: u32, lacunarity: f32, persistence: f32, seed: u64) -> f32 {
    let mut result = 0.0;
    let mut freq = frequency;
    let mut amp = 1.0;
    let mut max_amp = 0.0;

    for i in 0..octaves {
        result += perlin_2d(x * freq, y * freq, seed.wrapping_add(i as u64 * 1000)) * amp;
        max_amp += amp;
        freq *= lacunarity;
        amp *= persistence;
    }

    result / max_amp
}

/// FBM with Simplex noise
pub fn fbm_simplex(x: f32, y: f32, frequency: f32, octaves: u32, lacunarity: f32, persistence: f32, seed: u64) -> f32 {
    let mut result = 0.0;
    let mut freq = frequency;
    let mut amp = 1.0;
    let mut max_amp = 0.0;

    for i in 0..octaves {
        result += simplex_2d(x * freq, y * freq, seed.wrapping_add(i as u64 * 1000)) * amp;
        max_amp += amp;
        freq *= lacunarity;
        amp *= persistence;
    }

    result / max_amp
}

/// Ridged multifractal noise
pub fn ridged_multifractal(x: f32, y: f32, frequency: f32, octaves: u32, lacunarity: f32, persistence: f32, seed: u64) -> f32 {
    let mut result = 0.0;
    let mut freq = frequency;
    let mut amp = 1.0;
    let mut weight = 1.0;

    for i in 0..octaves {
        let signal = perlin_2d(x * freq, y * freq, seed.wrapping_add(i as u64 * 1000));
        // Ridge: 1 - |2*signal - 1|
        let ridge = 1.0 - (signal * 2.0 - 1.0).abs();
        let ridge = ridge * ridge;

        result += ridge * weight * amp;
        weight = (ridge * 2.0).clamp(0.0, 1.0);

        freq *= lacunarity;
        amp *= persistence;
    }

    result.clamp(0.0, 1.0)
}

/// Billow noise (turbulence)
pub fn billow_noise(x: f32, y: f32, frequency: f32, octaves: u32, lacunarity: f32, persistence: f32, seed: u64) -> f32 {
    let mut result = 0.0;
    let mut freq = frequency;
    let mut amp = 1.0;
    let mut max_amp = 0.0;

    for i in 0..octaves {
        let signal = perlin_2d(x * freq, y * freq, seed.wrapping_add(i as u64 * 1000));
        // Billow: |2*signal - 1|
        let billow = (signal * 2.0 - 1.0).abs();
        result += billow * amp;
        max_amp += amp;

        freq *= lacunarity;
        amp *= persistence;
    }

    result / max_amp
}

/// Domain warping - distorts input coordinates using noise
pub fn domain_warp(x: f32, y: f32, warp_frequency: f32, warp_strength: f32, seed: u64) -> (f32, f32) {
    let wx = perlin_2d(x * warp_frequency, y * warp_frequency, seed) * warp_strength;
    let wy = perlin_2d(x * warp_frequency + 5.2, y * warp_frequency + 1.3, seed) * warp_strength;
    (x + wx, y + wy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perlin_range() {
        for i in 0..100 {
            let x = i as f32 * 0.1;
            let y = i as f32 * 0.13;
            let v = perlin_2d(x, y, 42);
            assert!(v >= 0.0 && v <= 1.0, "Perlin out of range: {}", v);
        }
    }

    #[test]
    fn test_simplex_range() {
        for i in 0..100 {
            let x = i as f32 * 0.1;
            let y = i as f32 * 0.13;
            let v = simplex_2d(x, y, 42);
            assert!(v >= 0.0 && v <= 1.0, "Simplex out of range: {}", v);
        }
    }

    #[test]
    fn test_noise_layer() {
        let layer = NoiseLayer::perlin(0.01, 1.0);
        let v = layer.sample(100.0, 200.0, 42);
        assert!(v >= 0.0 && v <= 1.0);
    }

    #[test]
    fn test_fbm() {
        let v = fbm_perlin(50.0, 50.0, 0.01, 4, 2.0, 0.5, 42);
        assert!(v >= 0.0 && v <= 1.0);
    }
}
