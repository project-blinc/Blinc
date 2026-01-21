//! Loaded scene data structures
//!
//! Contains the intermediate representation for loaded 3D assets.

use blinc_core::{Color, Vec2, Vec3};
use crate::math::Quat;
use std::path::PathBuf;

/// A fully loaded 3D scene
#[derive(Clone, Debug)]
pub struct LoadedScene {
    /// Scene name (usually from file name)
    pub name: String,
    /// Source file path
    pub source_path: PathBuf,
    /// All meshes in the scene
    pub meshes: Vec<LoadedMesh>,
    /// All materials used
    pub materials: Vec<LoadedMaterial>,
    /// All textures referenced
    pub textures: Vec<LoadedTexture>,
    /// Animation clips (if any)
    pub animations: Vec<LoadedAnimation>,
    /// Scene hierarchy nodes
    pub nodes: Vec<LoadedNode>,
    /// Root node indices
    pub root_nodes: Vec<usize>,
}

impl LoadedScene {
    /// Create an empty scene
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source_path: PathBuf::new(),
            meshes: Vec::new(),
            materials: Vec::new(),
            textures: Vec::new(),
            animations: Vec::new(),
            nodes: Vec::new(),
            root_nodes: Vec::new(),
        }
    }

    /// Get a mesh by index
    pub fn mesh(&self, index: usize) -> Option<&LoadedMesh> {
        self.meshes.get(index)
    }

    /// Get a material by index
    pub fn material(&self, index: usize) -> Option<&LoadedMaterial> {
        self.materials.get(index)
    }

    /// Get a texture by index
    pub fn texture(&self, index: usize) -> Option<&LoadedTexture> {
        self.textures.get(index)
    }

    /// Get total vertex count across all meshes
    pub fn total_vertices(&self) -> usize {
        self.meshes.iter().map(|m| m.vertices.len()).sum()
    }

    /// Get total triangle count across all meshes
    pub fn total_triangles(&self) -> usize {
        self.meshes.iter().map(|m| m.indices.len() / 3).sum()
    }

    /// Check if the scene has animations
    pub fn has_animations(&self) -> bool {
        !self.animations.is_empty()
    }

    /// Get the bounding box of all meshes
    pub fn bounding_box(&self) -> Option<(Vec3, Vec3)> {
        if self.meshes.is_empty() {
            return None;
        }

        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);

        for mesh in &self.meshes {
            for vertex in &mesh.vertices {
                min.x = min.x.min(vertex.position.x);
                min.y = min.y.min(vertex.position.y);
                min.z = min.z.min(vertex.position.z);
                max.x = max.x.max(vertex.position.x);
                max.y = max.y.max(vertex.position.y);
                max.z = max.z.max(vertex.position.z);
            }
        }

        Some((min, max))
    }
}

/// A loaded mesh with vertex data
#[derive(Clone, Debug)]
pub struct LoadedMesh {
    /// Mesh name
    pub name: String,
    /// Vertex positions, normals, UVs, etc.
    pub vertices: Vec<LoadedVertex>,
    /// Triangle indices
    pub indices: Vec<u32>,
    /// Material index (into LoadedScene.materials)
    pub material_index: Option<usize>,
    /// Local transform relative to parent node
    pub transform: LoadedTransform,
}

impl LoadedMesh {
    /// Create a new empty mesh
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            vertices: Vec::new(),
            indices: Vec::new(),
            material_index: None,
            transform: LoadedTransform::default(),
        }
    }

    /// Check if the mesh has normals
    pub fn has_normals(&self) -> bool {
        self.vertices.first().map(|v| v.normal.is_some()).unwrap_or(false)
    }

    /// Check if the mesh has UVs
    pub fn has_uvs(&self) -> bool {
        self.vertices.first().map(|v| v.uv.is_some()).unwrap_or(false)
    }

    /// Check if the mesh has tangents
    pub fn has_tangents(&self) -> bool {
        self.vertices.first().map(|v| v.tangent.is_some()).unwrap_or(false)
    }

    /// Check if the mesh has vertex colors
    pub fn has_colors(&self) -> bool {
        self.vertices.first().map(|v| v.color.is_some()).unwrap_or(false)
    }

    /// Get triangle count
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Compute normals if not present (flat shading)
    pub fn compute_flat_normals(&mut self) {
        if self.has_normals() {
            return;
        }

        for i in (0..self.indices.len()).step_by(3) {
            let i0 = self.indices[i] as usize;
            let i1 = self.indices[i + 1] as usize;
            let i2 = self.indices[i + 2] as usize;

            let p0 = self.vertices[i0].position;
            let p1 = self.vertices[i1].position;
            let p2 = self.vertices[i2].position;

            let edge1 = Vec3::new(p1.x - p0.x, p1.y - p0.y, p1.z - p0.z);
            let edge2 = Vec3::new(p2.x - p0.x, p2.y - p0.y, p2.z - p0.z);

            let normal = Vec3::new(
                edge1.y * edge2.z - edge1.z * edge2.y,
                edge1.z * edge2.x - edge1.x * edge2.z,
                edge1.x * edge2.y - edge1.y * edge2.x,
            );
            let len = (normal.x * normal.x + normal.y * normal.y + normal.z * normal.z).sqrt();
            let normal = if len > 0.0 {
                Vec3::new(normal.x / len, normal.y / len, normal.z / len)
            } else {
                Vec3::new(0.0, 1.0, 0.0)
            };

            self.vertices[i0].normal = Some(normal);
            self.vertices[i1].normal = Some(normal);
            self.vertices[i2].normal = Some(normal);
        }
    }
}

/// A vertex from a loaded mesh
#[derive(Clone, Debug)]
pub struct LoadedVertex {
    /// Position in local space
    pub position: Vec3,
    /// Normal vector (optional)
    pub normal: Option<Vec3>,
    /// Texture coordinates (optional)
    pub uv: Option<Vec2>,
    /// Secondary UV set (optional)
    pub uv2: Option<Vec2>,
    /// Tangent vector with handedness in w (optional)
    pub tangent: Option<[f32; 4]>,
    /// Vertex color (optional)
    pub color: Option<Color>,
    /// Joint indices for skinning (optional)
    pub joints: Option<[u16; 4]>,
    /// Joint weights for skinning (optional)
    pub weights: Option<[f32; 4]>,
}

impl LoadedVertex {
    /// Create a vertex with just position
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            normal: None,
            uv: None,
            uv2: None,
            tangent: None,
            color: None,
            joints: None,
            weights: None,
        }
    }

    /// Create with position and normal
    pub fn with_normal(position: Vec3, normal: Vec3) -> Self {
        Self {
            position,
            normal: Some(normal),
            ..Self::new(position)
        }
    }
}

/// Transform data
#[derive(Clone, Debug)]
pub struct LoadedTransform {
    /// Position
    pub position: Vec3,
    /// Rotation as quaternion
    pub rotation: Quat,
    /// Scale
    pub scale: Vec3,
}

impl Default for LoadedTransform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::new(1.0, 1.0, 1.0),
        }
    }
}

/// A loaded material
#[derive(Clone, Debug)]
pub struct LoadedMaterial {
    /// Material name
    pub name: String,
    /// Base/albedo color
    pub base_color: Color,
    /// Base color texture index
    pub base_color_texture: Option<usize>,
    /// Metallic factor (0.0 = dielectric, 1.0 = metal)
    pub metallic: f32,
    /// Roughness factor (0.0 = smooth, 1.0 = rough)
    pub roughness: f32,
    /// Metallic-roughness texture index
    pub metallic_roughness_texture: Option<usize>,
    /// Normal map texture index
    pub normal_texture: Option<usize>,
    /// Normal map scale
    pub normal_scale: f32,
    /// Occlusion texture index
    pub occlusion_texture: Option<usize>,
    /// Occlusion strength
    pub occlusion_strength: f32,
    /// Emissive color
    pub emissive: Color,
    /// Emissive texture index
    pub emissive_texture: Option<usize>,
    /// Alpha mode
    pub alpha_mode: AlphaMode,
    /// Alpha cutoff (for Mask mode)
    pub alpha_cutoff: f32,
    /// Whether the material is double-sided
    pub double_sided: bool,
}

impl Default for LoadedMaterial {
    fn default() -> Self {
        Self {
            name: String::new(),
            base_color: Color::WHITE,
            base_color_texture: None,
            metallic: 0.0,
            roughness: 0.5,
            metallic_roughness_texture: None,
            normal_texture: None,
            normal_scale: 1.0,
            occlusion_texture: None,
            occlusion_strength: 1.0,
            emissive: Color::BLACK,
            emissive_texture: None,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
        }
    }
}

/// Alpha rendering mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaMode {
    /// Fully opaque
    Opaque,
    /// Alpha cutoff (discard below threshold)
    Mask,
    /// Alpha blending
    Blend,
}

/// A loaded texture
#[derive(Clone, Debug)]
pub struct LoadedTexture {
    /// Texture name
    pub name: String,
    /// Image data
    pub data: TextureData,
    /// Sampling parameters
    pub sampler: TextureSampler,
}

/// Texture image data
#[derive(Clone, Debug)]
pub enum TextureData {
    /// External file path
    Path(PathBuf),
    /// Embedded image data
    Embedded {
        /// Raw bytes
        data: Vec<u8>,
        /// MIME type (e.g., "image/png")
        mime_type: String,
    },
    /// Decoded RGBA pixels
    Pixels {
        /// RGBA pixel data
        data: Vec<u8>,
        /// Width in pixels
        width: u32,
        /// Height in pixels
        height: u32,
    },
}

/// Texture sampling parameters
#[derive(Clone, Debug)]
pub struct TextureSampler {
    /// Minification filter
    pub min_filter: TextureFilter,
    /// Magnification filter
    pub mag_filter: TextureFilter,
    /// Wrap mode for U coordinate
    pub wrap_u: WrapMode,
    /// Wrap mode for V coordinate
    pub wrap_v: WrapMode,
}

impl Default for TextureSampler {
    fn default() -> Self {
        Self {
            min_filter: TextureFilter::LinearMipmapLinear,
            mag_filter: TextureFilter::Linear,
            wrap_u: WrapMode::Repeat,
            wrap_v: WrapMode::Repeat,
        }
    }
}

/// Texture filter mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureFilter {
    /// Nearest neighbor
    Nearest,
    /// Bilinear
    Linear,
    /// Nearest with mipmaps
    NearestMipmapNearest,
    /// Linear with mipmaps
    LinearMipmapLinear,
    /// Nearest base, linear mipmap
    NearestMipmapLinear,
    /// Linear base, nearest mipmap
    LinearMipmapNearest,
}

/// Texture wrap mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrapMode {
    /// Repeat texture
    Repeat,
    /// Mirror and repeat
    MirroredRepeat,
    /// Clamp to edge
    ClampToEdge,
}

/// A scene hierarchy node
#[derive(Clone, Debug)]
pub struct LoadedNode {
    /// Node name
    pub name: String,
    /// Local transform
    pub transform: LoadedTransform,
    /// Mesh index (if this node has a mesh)
    pub mesh_index: Option<usize>,
    /// Child node indices
    pub children: Vec<usize>,
}

/// A loaded animation clip
#[derive(Clone, Debug)]
pub struct LoadedAnimation {
    /// Animation name
    pub name: String,
    /// Duration in seconds
    pub duration: f32,
    /// Animation channels (one per animated property)
    pub channels: Vec<AnimationChannel>,
}

/// An animation channel targeting a specific property
#[derive(Clone, Debug)]
pub struct AnimationChannel {
    /// Target node index
    pub node_index: usize,
    /// Target property
    pub target: AnimationTarget,
    /// Keyframe times
    pub times: Vec<f32>,
    /// Interpolation mode
    pub interpolation: Interpolation,
    /// Output values (interpretation depends on target)
    pub values: AnimationValues,
}

/// Animation target property
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationTarget {
    /// Translation (position)
    Translation,
    /// Rotation
    Rotation,
    /// Scale
    Scale,
    /// Morph target weights
    Weights,
}

/// Interpolation mode for animation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interpolation {
    /// Step interpolation (no blending)
    Step,
    /// Linear interpolation
    Linear,
    /// Cubic spline interpolation
    CubicSpline,
}

/// Animation output values
#[derive(Clone, Debug)]
pub enum AnimationValues {
    /// Vec3 values (for translation, scale)
    Vec3(Vec<Vec3>),
    /// Quaternion values (for rotation)
    Quat(Vec<Quat>),
    /// Scalar values (for weights)
    Scalar(Vec<f32>),
}
