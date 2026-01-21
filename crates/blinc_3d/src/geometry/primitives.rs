//! Geometry primitive generators

use super::{Geometry, Vertex};
use std::f32::consts::PI;

/// Box geometry generator (like Three.js BoxGeometry)
pub struct BoxGeometry;

impl BoxGeometry {
    /// Create a box with given dimensions
    pub fn new(width: f32, height: f32, depth: f32) -> Geometry {
        Self::with_segments(width, height, depth, 1, 1, 1)
    }

    /// Create a cube
    pub fn cube(size: f32) -> Geometry {
        Self::new(size, size, size)
    }

    /// Create a box with segments
    pub fn with_segments(
        width: f32,
        height: f32,
        depth: f32,
        width_segments: u32,
        height_segments: u32,
        depth_segments: u32,
    ) -> Geometry {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let hw = width / 2.0;
        let hh = height / 2.0;
        let hd = depth / 2.0;

        // Helper to build a plane
        let mut build_plane = |u: usize,
                               v: usize,
                               w: usize,
                               udir: f32,
                               vdir: f32,
                               width: f32,
                               height: f32,
                               depth: f32,
                               grid_x: u32,
                               grid_y: u32| {
            let segment_width = width / grid_x as f32;
            let segment_height = height / grid_y as f32;
            let width_half = width / 2.0;
            let height_half = height / 2.0;
            let depth_half = depth / 2.0;

            let grid_x1 = grid_x + 1;
            let grid_y1 = grid_y + 1;

            let vertex_offset = vertices.len() as u32;

            for iy in 0..grid_y1 {
                let y = iy as f32 * segment_height - height_half;
                for ix in 0..grid_x1 {
                    let x = ix as f32 * segment_width - width_half;

                    let mut position = [0.0f32; 3];
                    position[u] = x * udir;
                    position[v] = y * vdir;
                    position[w] = depth_half;

                    let mut normal = [0.0f32; 3];
                    normal[w] = if depth > 0.0 { 1.0 } else { -1.0 };

                    let uv = [
                        ix as f32 / grid_x as f32,
                        1.0 - iy as f32 / grid_y as f32,
                    ];

                    vertices.push(Vertex::new(position, normal, uv));
                }
            }

            for iy in 0..grid_y {
                for ix in 0..grid_x {
                    let a = vertex_offset + ix + grid_x1 * iy;
                    let b = vertex_offset + ix + grid_x1 * (iy + 1);
                    let c = vertex_offset + (ix + 1) + grid_x1 * (iy + 1);
                    let d = vertex_offset + (ix + 1) + grid_x1 * iy;

                    indices.extend_from_slice(&[a, b, d, b, c, d]);
                }
            }
        };

        // +Z face
        build_plane(0, 1, 2, 1.0, 1.0, width, height, hd, width_segments, height_segments);
        // -Z face
        build_plane(0, 1, 2, -1.0, 1.0, width, height, -hd, width_segments, height_segments);
        // +Y face
        build_plane(0, 2, 1, 1.0, 1.0, width, depth, hh, width_segments, depth_segments);
        // -Y face
        build_plane(0, 2, 1, 1.0, -1.0, width, depth, -hh, width_segments, depth_segments);
        // +X face
        build_plane(2, 1, 0, 1.0, 1.0, depth, height, hw, depth_segments, height_segments);
        // -X face
        build_plane(2, 1, 0, -1.0, 1.0, depth, height, -hw, depth_segments, height_segments);

        Geometry::from_data(vertices, indices)
    }
}

/// Sphere geometry generator (like Three.js SphereGeometry)
pub struct SphereGeometry;

impl SphereGeometry {
    /// Create a sphere with given radius
    pub fn new(radius: f32) -> Geometry {
        Self::with_detail(radius, 32, 16)
    }

    /// Create a sphere with custom segment count
    pub fn with_detail(radius: f32, width_segments: u32, height_segments: u32) -> Geometry {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let width_segments = width_segments.max(3);
        let height_segments = height_segments.max(2);

        for iy in 0..=height_segments {
            let v = iy as f32 / height_segments as f32;
            let phi = v * PI;

            for ix in 0..=width_segments {
                let u = ix as f32 / width_segments as f32;
                let theta = u * 2.0 * PI;

                let x = -radius * phi.sin() * theta.cos();
                let y = radius * phi.cos();
                let z = radius * phi.sin() * theta.sin();

                let nx = x / radius;
                let ny = y / radius;
                let nz = z / radius;

                vertices.push(Vertex::new([x, y, z], [nx, ny, nz], [u, v]));
            }
        }

        for iy in 0..height_segments {
            for ix in 0..width_segments {
                let a = iy * (width_segments + 1) + ix;
                let b = a + width_segments + 1;
                let c = b + 1;
                let d = a + 1;

                if iy != 0 {
                    indices.extend_from_slice(&[a, b, d]);
                }
                if iy != height_segments - 1 {
                    indices.extend_from_slice(&[b, c, d]);
                }
            }
        }

        Geometry::from_data(vertices, indices)
    }
}

/// Plane geometry generator (like Three.js PlaneGeometry)
pub struct PlaneGeometry;

impl PlaneGeometry {
    /// Create a plane with given dimensions
    pub fn new(width: f32, height: f32) -> Geometry {
        Self::with_segments(width, height, 1, 1)
    }

    /// Create a plane with segments
    pub fn with_segments(width: f32, height: f32, width_segments: u32, height_segments: u32) -> Geometry {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let hw = width / 2.0;
        let hh = height / 2.0;

        let grid_x = width_segments;
        let grid_y = height_segments;
        let grid_x1 = grid_x + 1;
        let grid_y1 = grid_y + 1;

        let segment_width = width / grid_x as f32;
        let segment_height = height / grid_y as f32;

        for iy in 0..grid_y1 {
            let y = iy as f32 * segment_height - hh;
            for ix in 0..grid_x1 {
                let x = ix as f32 * segment_width - hw;

                vertices.push(Vertex::new(
                    [x, 0.0, y],
                    [0.0, 1.0, 0.0],
                    [ix as f32 / grid_x as f32, 1.0 - iy as f32 / grid_y as f32],
                ));
            }
        }

        for iy in 0..grid_y {
            for ix in 0..grid_x {
                let a = ix + grid_x1 * iy;
                let b = ix + grid_x1 * (iy + 1);
                let c = (ix + 1) + grid_x1 * (iy + 1);
                let d = (ix + 1) + grid_x1 * iy;

                indices.extend_from_slice(&[a, b, d, b, c, d]);
            }
        }

        Geometry::from_data(vertices, indices)
    }
}

/// Cylinder geometry generator
pub struct CylinderGeometry;

impl CylinderGeometry {
    /// Create a cylinder
    pub fn new(radius_top: f32, radius_bottom: f32, height: f32) -> Geometry {
        Self::with_detail(radius_top, radius_bottom, height, 32, 1, false)
    }

    /// Create with custom detail
    pub fn with_detail(
        radius_top: f32,
        radius_bottom: f32,
        height: f32,
        radial_segments: u32,
        height_segments: u32,
        open_ended: bool,
    ) -> Geometry {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let half_height = height / 2.0;
        let radial_segments = radial_segments.max(3);

        // Generate side vertices
        for y in 0..=height_segments {
            let v = y as f32 / height_segments as f32;
            let radius = v * (radius_bottom - radius_top) + radius_top;

            for x in 0..=radial_segments {
                let u = x as f32 / radial_segments as f32;
                let theta = u * 2.0 * PI;

                let sin_theta = theta.sin();
                let cos_theta = theta.cos();

                let px = radius * sin_theta;
                let py = half_height - v * height;
                let pz = radius * cos_theta;

                // Normal (pointing outward)
                let slope = (radius_bottom - radius_top) / height;
                let ny = 1.0 / (1.0 + slope * slope).sqrt();
                let nxz = slope * ny;
                let nx = nxz * sin_theta;
                let nz = nxz * cos_theta;

                vertices.push(Vertex::new([px, py, pz], [nx, ny, nz], [u, v]));
            }
        }

        // Generate side indices
        for y in 0..height_segments {
            for x in 0..radial_segments {
                let a = y * (radial_segments + 1) + x;
                let b = a + radial_segments + 1;
                let c = b + 1;
                let d = a + 1;

                indices.extend_from_slice(&[a, b, d, b, c, d]);
            }
        }

        // Generate caps if not open-ended
        if !open_ended {
            // Top cap
            if radius_top > 0.0 {
                let center_index = vertices.len() as u32;
                vertices.push(Vertex::new([0.0, half_height, 0.0], [0.0, 1.0, 0.0], [0.5, 0.5]));

                for x in 0..=radial_segments {
                    let u = x as f32 / radial_segments as f32;
                    let theta = u * 2.0 * PI;
                    let px = radius_top * theta.sin();
                    let pz = radius_top * theta.cos();
                    vertices.push(Vertex::new(
                        [px, half_height, pz],
                        [0.0, 1.0, 0.0],
                        [0.5 + 0.5 * theta.sin(), 0.5 + 0.5 * theta.cos()],
                    ));
                }

                for x in 0..radial_segments {
                    indices.extend_from_slice(&[center_index, center_index + x + 1, center_index + x + 2]);
                }
            }

            // Bottom cap
            if radius_bottom > 0.0 {
                let center_index = vertices.len() as u32;
                vertices.push(Vertex::new([0.0, -half_height, 0.0], [0.0, -1.0, 0.0], [0.5, 0.5]));

                for x in 0..=radial_segments {
                    let u = x as f32 / radial_segments as f32;
                    let theta = u * 2.0 * PI;
                    let px = radius_bottom * theta.sin();
                    let pz = radius_bottom * theta.cos();
                    vertices.push(Vertex::new(
                        [px, -half_height, pz],
                        [0.0, -1.0, 0.0],
                        [0.5 + 0.5 * theta.sin(), 0.5 - 0.5 * theta.cos()],
                    ));
                }

                for x in 0..radial_segments {
                    indices.extend_from_slice(&[center_index, center_index + x + 2, center_index + x + 1]);
                }
            }
        }

        Geometry::from_data(vertices, indices)
    }

    /// Create a simple cylinder
    pub fn cylinder(radius: f32, height: f32) -> Geometry {
        Self::new(radius, radius, height)
    }

    /// Create a cone
    pub fn cone(radius: f32, height: f32) -> Geometry {
        Self::new(0.0, radius, height)
    }
}

/// Torus geometry generator
pub struct TorusGeometry;

impl TorusGeometry {
    /// Create a torus
    pub fn new(radius: f32, tube: f32) -> Geometry {
        Self::with_detail(radius, tube, 32, 16)
    }

    /// Create with custom detail
    pub fn with_detail(radius: f32, tube: f32, radial_segments: u32, tubular_segments: u32) -> Geometry {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let radial_segments = radial_segments.max(3);
        let tubular_segments = tubular_segments.max(3);

        for j in 0..=radial_segments {
            for i in 0..=tubular_segments {
                let u = i as f32 / tubular_segments as f32 * 2.0 * PI;
                let v = j as f32 / radial_segments as f32 * 2.0 * PI;

                let x = (radius + tube * v.cos()) * u.cos();
                let y = tube * v.sin();
                let z = (radius + tube * v.cos()) * u.sin();

                // Center of tube at this position
                let cx = radius * u.cos();
                let cz = radius * u.sin();

                // Normal pointing from center of tube to vertex
                let nx = x - cx;
                let ny = y;
                let nz = z - cz;
                let len = (nx * nx + ny * ny + nz * nz).sqrt();
                let nx = nx / len;
                let ny = ny / len;
                let nz = nz / len;

                vertices.push(Vertex::new(
                    [x, y, z],
                    [nx, ny, nz],
                    [i as f32 / tubular_segments as f32, j as f32 / radial_segments as f32],
                ));
            }
        }

        for j in 1..=radial_segments {
            for i in 1..=tubular_segments {
                let a = (tubular_segments + 1) * j + i - 1;
                let b = (tubular_segments + 1) * (j - 1) + i - 1;
                let c = (tubular_segments + 1) * (j - 1) + i;
                let d = (tubular_segments + 1) * j + i;

                indices.extend_from_slice(&[a, b, d, b, c, d]);
            }
        }

        Geometry::from_data(vertices, indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_box_geometry() {
        let geom = BoxGeometry::new(1.0, 1.0, 1.0);
        assert_eq!(geom.vertex_count(), 24); // 6 faces * 4 vertices
        assert_eq!(geom.triangle_count(), 12); // 6 faces * 2 triangles
    }

    #[test]
    fn test_sphere_geometry() {
        let geom = SphereGeometry::with_detail(1.0, 8, 6);
        assert!(geom.vertex_count() > 0);
        assert!(geom.triangle_count() > 0);
    }

    #[test]
    fn test_plane_geometry() {
        let geom = PlaneGeometry::new(1.0, 1.0);
        assert_eq!(geom.vertex_count(), 4);
        assert_eq!(geom.triangle_count(), 2);
    }
}
