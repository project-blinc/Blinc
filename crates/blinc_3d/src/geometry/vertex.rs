//! Vertex format and geometry structures

use crate::math::{BoundingBox, BoundingSphere};
use blinc_core::Vec3;

/// Vertex data for 3D geometry
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Vertex {
    /// Position in local space
    pub position: [f32; 3],
    /// Normal vector
    pub normal: [f32; 3],
    /// Texture coordinates
    pub uv: [f32; 2],
    /// Tangent vector (xyz) and bitangent sign (w)
    pub tangent: [f32; 4],
}

impl Default for Vertex {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
        }
    }
}

impl Vertex {
    /// Create a new vertex
    pub fn new(position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            position,
            normal,
            uv,
            tangent: [1.0, 0.0, 0.0, 1.0],
        }
    }

    /// Create with tangent
    pub fn with_tangent(mut self, tangent: [f32; 4]) -> Self {
        self.tangent = tangent;
        self
    }
}

/// Geometry resource containing vertex and index data
#[derive(Clone, Debug)]
pub struct Geometry {
    /// Vertex data
    pub vertices: Vec<Vertex>,
    /// Index data (triangles)
    pub indices: Vec<u32>,
    /// Axis-aligned bounding box
    pub bounding_box: BoundingBox,
    /// Bounding sphere
    pub bounding_sphere: BoundingSphere,
}

impl Default for Geometry {
    fn default() -> Self {
        Self::new()
    }
}

impl Geometry {
    /// Create empty geometry
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            bounding_box: BoundingBox::empty(),
            bounding_sphere: BoundingSphere::default(),
        }
    }

    /// Create from vertices and indices
    pub fn from_data(vertices: Vec<Vertex>, indices: Vec<u32>) -> Self {
        let mut geometry = Self {
            vertices,
            indices,
            bounding_box: BoundingBox::empty(),
            bounding_sphere: BoundingSphere::default(),
        };
        geometry.compute_bounds();
        geometry
    }

    /// Compute bounding volumes from vertex data
    pub fn compute_bounds(&mut self) {
        self.bounding_box = BoundingBox::empty();
        for vertex in &self.vertices {
            self.bounding_box.expand_to_include(Vec3::new(
                vertex.position[0],
                vertex.position[1],
                vertex.position[2],
            ));
        }
        self.bounding_sphere = BoundingSphere::from_box(&self.bounding_box);
    }

    /// Compute flat normals (one normal per face)
    pub fn compute_flat_normals(&mut self) {
        for chunk in self.indices.chunks(3) {
            if chunk.len() < 3 {
                continue;
            }
            let i0 = chunk[0] as usize;
            let i1 = chunk[1] as usize;
            let i2 = chunk[2] as usize;

            let p0 = &self.vertices[i0].position;
            let p1 = &self.vertices[i1].position;
            let p2 = &self.vertices[i2].position;

            // Edge vectors
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];

            // Cross product
            let nx = e1[1] * e2[2] - e1[2] * e2[1];
            let ny = e1[2] * e2[0] - e1[0] * e2[2];
            let nz = e1[0] * e2[1] - e1[1] * e2[0];

            // Normalize
            let len = (nx * nx + ny * ny + nz * nz).sqrt();
            if len > 1e-6 {
                let inv_len = 1.0 / len;
                let normal = [nx * inv_len, ny * inv_len, nz * inv_len];
                self.vertices[i0].normal = normal;
                self.vertices[i1].normal = normal;
                self.vertices[i2].normal = normal;
            }
        }
    }

    /// Get vertex count
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Get index count
    pub fn index_count(&self) -> usize {
        self.indices.len()
    }

    /// Get triangle count
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Handle to a shared geometry resource
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GeometryHandle(pub u64);

impl Default for GeometryHandle {
    fn default() -> Self {
        Self(0)
    }
}
