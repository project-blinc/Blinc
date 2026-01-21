//! glTF 2.0 mesh loader
//!
//! Loads glTF and GLB files using the `gltf` crate.

use super::{
    AlphaMode, AnimationChannel, AnimationTarget, AnimationValues, Interpolation, LoadError,
    LoadedAnimation, LoadedMaterial, LoadedMesh, LoadedNode, LoadedScene, LoadedTexture,
    LoadedTransform, LoadedVertex, MeshLoader, TextureData, TextureFilter, TextureSampler,
    WrapMode,
};
use crate::math::Quat;
use blinc_core::{Color, Vec2, Vec3};
use gltf::Gltf;
use std::path::Path;

/// glTF 2.0 mesh loader
pub struct GltfLoader {
    /// Whether to load animations
    pub load_animations: bool,
    /// Whether to load materials
    pub load_materials: bool,
    /// Whether to load textures
    pub load_textures: bool,
}

impl GltfLoader {
    /// Create a new glTF loader with default settings
    pub fn new() -> Self {
        Self {
            load_animations: true,
            load_materials: true,
            load_textures: true,
        }
    }

    /// Set whether to load animations
    pub fn with_animations(mut self, load: bool) -> Self {
        self.load_animations = load;
        self
    }

    /// Set whether to load materials
    pub fn with_materials(mut self, load: bool) -> Self {
        self.load_materials = load;
        self
    }

    /// Set whether to load textures
    pub fn with_textures(mut self, load: bool) -> Self {
        self.load_textures = load;
        self
    }
}

impl Default for GltfLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl MeshLoader for GltfLoader {
    fn load(&self, path: &Path) -> Result<LoadedScene, LoadError> {
        let gltf = Gltf::open(path).map_err(|e| LoadError::Parse(e.to_string()))?;

        let base_path = path.parent().unwrap_or(Path::new("."));

        // Load buffer data
        let buffers = load_buffers(&gltf, base_path)?;

        let scene_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("scene")
            .to_string();

        let mut scene = LoadedScene::new(&scene_name);
        scene.source_path = path.to_path_buf();

        // Load materials
        if self.load_materials {
            for material in gltf.materials() {
                scene.materials.push(load_material(&material));
            }
        }

        // Load textures
        if self.load_textures {
            for texture in gltf.textures() {
                if let Some(loaded) = load_texture(&texture, base_path) {
                    scene.textures.push(loaded);
                }
            }
        }

        // Load meshes from all nodes
        for gltf_mesh in gltf.meshes() {
            for primitive in gltf_mesh.primitives() {
                if let Some(loaded) = load_primitive(&primitive, &buffers, &gltf_mesh) {
                    scene.meshes.push(loaded);
                }
            }
        }

        // Load scene hierarchy
        if let Some(gltf_scene) = gltf.default_scene().or_else(|| gltf.scenes().next()) {
            for node in gltf_scene.nodes() {
                let node_index = load_node_hierarchy(&node, &mut scene.nodes);
                scene.root_nodes.push(node_index);
            }
        }

        // Load animations
        if self.load_animations {
            for animation in gltf.animations() {
                if let Some(loaded) = load_animation(&animation, &buffers) {
                    scene.animations.push(loaded);
                }
            }
        }

        Ok(scene)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["gltf", "glb"]
    }

    fn name(&self) -> &'static str {
        "glTF 2.0 Loader"
    }
}

/// Load buffer data from the glTF file
fn load_buffers(gltf: &Gltf, base_path: &Path) -> Result<Vec<Vec<u8>>, LoadError> {
    let mut buffers = Vec::new();

    for buffer in gltf.buffers() {
        match buffer.source() {
            gltf::buffer::Source::Bin => {
                // Embedded binary data in GLB
                if let Some(blob) = gltf.blob.as_ref() {
                    buffers.push(blob.clone());
                } else {
                    return Err(LoadError::InvalidData("Missing embedded buffer".into()));
                }
            }
            gltf::buffer::Source::Uri(uri) => {
                if uri.starts_with("data:") {
                    // Base64 encoded data
                    let data = decode_data_uri(uri)?;
                    buffers.push(data);
                } else {
                    // External file
                    let buffer_path = base_path.join(uri);
                    let data = std::fs::read(&buffer_path)?;
                    buffers.push(data);
                }
            }
        }
    }

    Ok(buffers)
}

/// Decode a data URI (base64 encoded)
fn decode_data_uri(uri: &str) -> Result<Vec<u8>, LoadError> {
    // Format: data:[<mediatype>][;base64],<data>
    let parts: Vec<&str> = uri.splitn(2, ',').collect();
    if parts.len() != 2 {
        return Err(LoadError::InvalidData("Invalid data URI format".into()));
    }

    let header = parts[0];
    let data = parts[1];

    if header.contains(";base64") {
        // Base64 decode
        // Simple base64 decoder (for production, use a proper crate)
        decode_base64(data)
    } else {
        // URL encoded
        Ok(urlencoding_decode(data))
    }
}

/// Simple base64 decoder
fn decode_base64(input: &str) -> Result<Vec<u8>, LoadError> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn decode_char(c: u8) -> Option<u8> {
        ALPHABET.iter().position(|&b| b == c).map(|p| p as u8)
    }

    let input: Vec<u8> = input.bytes().filter(|&b| b != b'\n' && b != b'\r' && b != b' ').collect();
    let mut output = Vec::with_capacity(input.len() * 3 / 4);

    for chunk in input.chunks(4) {
        let mut bits: u32 = 0;
        let mut valid_bytes = 0;

        for (i, &byte) in chunk.iter().enumerate() {
            if byte == b'=' {
                break;
            }
            if let Some(value) = decode_char(byte) {
                bits |= (value as u32) << (18 - i * 6);
                valid_bytes += 1;
            }
        }

        if valid_bytes >= 2 {
            output.push((bits >> 16) as u8);
        }
        if valid_bytes >= 3 {
            output.push((bits >> 8) as u8);
        }
        if valid_bytes >= 4 {
            output.push(bits as u8);
        }
    }

    Ok(output)
}

/// Simple URL decoding
fn urlencoding_decode(input: &str) -> Vec<u8> {
    let mut output = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                output.push(byte);
            }
        } else {
            output.push(c as u8);
        }
    }

    output
}

/// Load a single primitive from a mesh
fn load_primitive(
    primitive: &gltf::Primitive,
    buffers: &[Vec<u8>],
    mesh: &gltf::Mesh,
) -> Option<LoadedMesh> {
    let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(|d| d.as_slice()));

    // Read positions (required)
    let positions: Vec<Vec3> = reader
        .read_positions()?
        .map(|p| Vec3::new(p[0], p[1], p[2]))
        .collect();

    if positions.is_empty() {
        return None;
    }

    // Read optional attributes
    let normals: Option<Vec<Vec3>> = reader
        .read_normals()
        .map(|iter| iter.map(|n| Vec3::new(n[0], n[1], n[2])).collect());

    let uvs: Option<Vec<Vec2>> = reader
        .read_tex_coords(0)
        .map(|iter| iter.into_f32().map(|uv| Vec2::new(uv[0], uv[1])).collect());

    let uvs2: Option<Vec<Vec2>> = reader
        .read_tex_coords(1)
        .map(|iter| iter.into_f32().map(|uv| Vec2::new(uv[0], uv[1])).collect());

    let tangents: Option<Vec<[f32; 4]>> = reader
        .read_tangents()
        .map(|iter| iter.collect());

    let colors: Option<Vec<Color>> = reader.read_colors(0).map(|iter| {
        iter.into_rgba_f32()
            .map(|c| Color::rgba(c[0], c[1], c[2], c[3]))
            .collect()
    });

    let joints: Option<Vec<[u16; 4]>> = reader
        .read_joints(0)
        .map(|iter| iter.into_u16().collect());

    let weights: Option<Vec<[f32; 4]>> = reader
        .read_weights(0)
        .map(|iter| iter.into_f32().collect());

    // Read indices
    let indices: Vec<u32> = reader
        .read_indices()
        .map(|iter| iter.into_u32().collect())
        .unwrap_or_else(|| (0..positions.len() as u32).collect());

    // Build vertices
    let vertices: Vec<LoadedVertex> = positions
        .into_iter()
        .enumerate()
        .map(|(i, position)| LoadedVertex {
            position,
            normal: normals.as_ref().and_then(|n| n.get(i).copied()),
            uv: uvs.as_ref().and_then(|u| u.get(i).copied()),
            uv2: uvs2.as_ref().and_then(|u| u.get(i).copied()),
            tangent: tangents.as_ref().and_then(|t| t.get(i).copied()),
            color: colors.as_ref().and_then(|c| c.get(i).copied()),
            joints: joints.as_ref().and_then(|j| j.get(i).copied()),
            weights: weights.as_ref().and_then(|w| w.get(i).copied()),
        })
        .collect();

    let name = mesh.name().unwrap_or("mesh").to_string();

    Some(LoadedMesh {
        name,
        vertices,
        indices,
        material_index: primitive.material().index(),
        transform: LoadedTransform::default(),
    })
}

/// Load a material
fn load_material(material: &gltf::Material) -> LoadedMaterial {
    let pbr = material.pbr_metallic_roughness();
    let base_color_factor = pbr.base_color_factor();

    LoadedMaterial {
        name: material.name().unwrap_or("material").to_string(),
        base_color: Color::rgba(
            base_color_factor[0],
            base_color_factor[1],
            base_color_factor[2],
            base_color_factor[3],
        ),
        base_color_texture: pbr.base_color_texture().map(|t| t.texture().index()),
        metallic: pbr.metallic_factor(),
        roughness: pbr.roughness_factor(),
        metallic_roughness_texture: pbr
            .metallic_roughness_texture()
            .map(|t| t.texture().index()),
        normal_texture: material.normal_texture().map(|t| t.texture().index()),
        normal_scale: material.normal_texture().map(|t| t.scale()).unwrap_or(1.0),
        occlusion_texture: material.occlusion_texture().map(|t| t.texture().index()),
        occlusion_strength: material
            .occlusion_texture()
            .map(|t| t.strength())
            .unwrap_or(1.0),
        emissive: {
            let e = material.emissive_factor();
            Color::rgb(e[0], e[1], e[2])
        },
        emissive_texture: material.emissive_texture().map(|t| t.texture().index()),
        alpha_mode: match material.alpha_mode() {
            gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
            gltf::material::AlphaMode::Mask => AlphaMode::Mask,
            gltf::material::AlphaMode::Blend => AlphaMode::Blend,
        },
        alpha_cutoff: material.alpha_cutoff().unwrap_or(0.5),
        double_sided: material.double_sided(),
    }
}

/// Load a texture
fn load_texture(texture: &gltf::Texture, base_path: &Path) -> Option<LoadedTexture> {
    let image = texture.source();
    let sampler = texture.sampler();

    let data = match image.source() {
        gltf::image::Source::View { view, mime_type } => {
            // Embedded image data - we'll need the buffers to extract it
            // For now, mark as embedded
            TextureData::Embedded {
                data: Vec::new(), // Would need buffer access
                mime_type: mime_type.to_string(),
            }
        }
        gltf::image::Source::Uri { uri, mime_type: _ } => {
            if uri.starts_with("data:") {
                // Base64 encoded
                match decode_data_uri(uri) {
                    Ok(data) => TextureData::Embedded {
                        data,
                        mime_type: "image/png".to_string(), // Assume PNG
                    },
                    Err(_) => return None,
                }
            } else {
                // External file
                TextureData::Path(base_path.join(uri))
            }
        }
    };

    let sampler_info = TextureSampler {
        min_filter: sampler.min_filter().map(convert_min_filter).unwrap_or(TextureFilter::LinearMipmapLinear),
        mag_filter: sampler.mag_filter().map(convert_mag_filter).unwrap_or(TextureFilter::Linear),
        wrap_u: convert_wrap_mode(sampler.wrap_s()),
        wrap_v: convert_wrap_mode(sampler.wrap_t()),
    };

    Some(LoadedTexture {
        name: texture.name().unwrap_or("texture").to_string(),
        data,
        sampler: sampler_info,
    })
}

fn convert_min_filter(filter: gltf::texture::MinFilter) -> TextureFilter {
    match filter {
        gltf::texture::MinFilter::Nearest => TextureFilter::Nearest,
        gltf::texture::MinFilter::Linear => TextureFilter::Linear,
        gltf::texture::MinFilter::NearestMipmapNearest => TextureFilter::NearestMipmapNearest,
        gltf::texture::MinFilter::LinearMipmapNearest => TextureFilter::LinearMipmapNearest,
        gltf::texture::MinFilter::NearestMipmapLinear => TextureFilter::NearestMipmapLinear,
        gltf::texture::MinFilter::LinearMipmapLinear => TextureFilter::LinearMipmapLinear,
    }
}

fn convert_mag_filter(filter: gltf::texture::MagFilter) -> TextureFilter {
    match filter {
        gltf::texture::MagFilter::Nearest => TextureFilter::Nearest,
        gltf::texture::MagFilter::Linear => TextureFilter::Linear,
    }
}

fn convert_wrap_mode(mode: gltf::texture::WrappingMode) -> WrapMode {
    match mode {
        gltf::texture::WrappingMode::ClampToEdge => WrapMode::ClampToEdge,
        gltf::texture::WrappingMode::MirroredRepeat => WrapMode::MirroredRepeat,
        gltf::texture::WrappingMode::Repeat => WrapMode::Repeat,
    }
}

/// Load node hierarchy recursively
fn load_node_hierarchy(node: &gltf::Node, nodes: &mut Vec<LoadedNode>) -> usize {
    let transform = node.transform().decomposed();

    let loaded_node = LoadedNode {
        name: node.name().unwrap_or("node").to_string(),
        transform: LoadedTransform {
            position: Vec3::new(transform.0[0], transform.0[1], transform.0[2]),
            rotation: Quat::from_xyzw(transform.1[0], transform.1[1], transform.1[2], transform.1[3]),
            scale: Vec3::new(transform.2[0], transform.2[1], transform.2[2]),
        },
        mesh_index: node.mesh().map(|m| m.index()),
        children: Vec::new(),
    };

    let node_index = nodes.len();
    nodes.push(loaded_node);

    // Load children
    let child_indices: Vec<usize> = node
        .children()
        .map(|child| load_node_hierarchy(&child, nodes))
        .collect();

    nodes[node_index].children = child_indices;

    node_index
}

/// Load an animation
fn load_animation(animation: &gltf::Animation, buffers: &[Vec<u8>]) -> Option<LoadedAnimation> {
    let mut channels = Vec::new();
    let mut max_duration: f32 = 0.0;

    for channel in animation.channels() {
        let sampler = channel.sampler();
        let target = channel.target();
        let reader = channel.reader(|buffer| buffers.get(buffer.index()).map(|d| d.as_slice()));

        // Read input (time) values
        let times: Vec<f32> = reader.read_inputs()?.collect();

        if let Some(&last_time) = times.last() {
            max_duration = max_duration.max(last_time);
        }

        // Read output values
        let outputs = reader.read_outputs()?;

        let (anim_target, values) = match outputs {
            gltf::animation::util::ReadOutputs::Translations(iter) => {
                let values: Vec<Vec3> = iter.map(|t| Vec3::new(t[0], t[1], t[2])).collect();
                (AnimationTarget::Translation, AnimationValues::Vec3(values))
            }
            gltf::animation::util::ReadOutputs::Rotations(iter) => {
                let values: Vec<Quat> = iter
                    .into_f32()
                    .map(|r| Quat::from_xyzw(r[0], r[1], r[2], r[3]))
                    .collect();
                (AnimationTarget::Rotation, AnimationValues::Quat(values))
            }
            gltf::animation::util::ReadOutputs::Scales(iter) => {
                let values: Vec<Vec3> = iter.map(|s| Vec3::new(s[0], s[1], s[2])).collect();
                (AnimationTarget::Scale, AnimationValues::Vec3(values))
            }
            gltf::animation::util::ReadOutputs::MorphTargetWeights(iter) => {
                let values: Vec<f32> = iter.into_f32().collect();
                (AnimationTarget::Weights, AnimationValues::Scalar(values))
            }
        };

        let interpolation = match sampler.interpolation() {
            gltf::animation::Interpolation::Linear => Interpolation::Linear,
            gltf::animation::Interpolation::Step => Interpolation::Step,
            gltf::animation::Interpolation::CubicSpline => Interpolation::CubicSpline,
        };

        channels.push(AnimationChannel {
            node_index: target.node().index(),
            target: anim_target,
            times,
            interpolation,
            values,
        });
    }

    if channels.is_empty() {
        return None;
    }

    Some(LoadedAnimation {
        name: animation.name().unwrap_or("animation").to_string(),
        duration: max_duration,
        channels,
    })
}
