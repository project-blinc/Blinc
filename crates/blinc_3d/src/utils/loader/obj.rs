//! Wavefront OBJ mesh loader
//!
//! Loads OBJ files using the `tobj` crate.

use super::{
    AlphaMode, LoadError, LoadedMaterial, LoadedMesh, LoadedNode, LoadedScene, LoadedTexture,
    LoadedTransform, LoadedVertex, MeshLoader, TextureData, TextureSampler,
};
use blinc_core::{Color, Vec2, Vec3};
use std::path::Path;
use tobj;

/// Wavefront OBJ mesh loader
pub struct ObjLoader {
    /// Whether to triangulate faces
    pub triangulate: bool,
    /// Whether to generate normals if missing
    pub generate_normals: bool,
    /// Whether to load materials from MTL files
    pub load_materials: bool,
}

impl ObjLoader {
    /// Create a new OBJ loader with default settings
    pub fn new() -> Self {
        Self {
            triangulate: true,
            generate_normals: true,
            load_materials: true,
        }
    }

    /// Set whether to triangulate non-triangle faces
    pub fn with_triangulate(mut self, triangulate: bool) -> Self {
        self.triangulate = triangulate;
        self
    }

    /// Set whether to generate normals if missing
    pub fn with_generate_normals(mut self, generate: bool) -> Self {
        self.generate_normals = generate;
        self
    }

    /// Set whether to load materials
    pub fn with_materials(mut self, load: bool) -> Self {
        self.load_materials = load;
        self
    }
}

impl Default for ObjLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl MeshLoader for ObjLoader {
    fn load(&self, path: &Path) -> Result<LoadedScene, LoadError> {
        let load_options = tobj::LoadOptions {
            triangulate: self.triangulate,
            single_index: true,
            ..Default::default()
        };

        let (models, materials_result) = tobj::load_obj(path, &load_options)
            .map_err(|e| LoadError::Parse(e.to_string()))?;

        let scene_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("scene")
            .to_string();

        let mut scene = LoadedScene::new(&scene_name);
        scene.source_path = path.to_path_buf();

        // Load materials
        if self.load_materials {
            if let Ok(materials) = materials_result {
                for material in materials {
                    scene.materials.push(convert_material(&material, path));

                    // Add textures from material
                    add_material_textures(&material, path, &mut scene.textures);
                }
            }
        }

        // Load meshes
        for model in models {
            if let Some(mesh) = convert_model(&model, self.generate_normals) {
                let node = LoadedNode {
                    name: model.name.clone(),
                    transform: LoadedTransform::default(),
                    mesh_index: Some(scene.meshes.len()),
                    children: Vec::new(),
                };

                scene.nodes.push(node);
                scene.root_nodes.push(scene.nodes.len() - 1);
                scene.meshes.push(mesh);
            }
        }

        Ok(scene)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["obj"]
    }

    fn name(&self) -> &'static str {
        "Wavefront OBJ Loader"
    }
}

/// Convert a tobj model to a LoadedMesh
fn convert_model(model: &tobj::Model, generate_normals: bool) -> Option<LoadedMesh> {
    let mesh = &model.mesh;

    if mesh.positions.is_empty() {
        return None;
    }

    let vertex_count = mesh.positions.len() / 3;
    let has_normals = !mesh.normals.is_empty();
    let has_uvs = !mesh.texcoords.is_empty();

    let mut vertices: Vec<LoadedVertex> = Vec::with_capacity(vertex_count);

    for i in 0..vertex_count {
        let position = Vec3::new(
            mesh.positions[i * 3],
            mesh.positions[i * 3 + 1],
            mesh.positions[i * 3 + 2],
        );

        let normal = if has_normals && mesh.normals.len() > i * 3 + 2 {
            Some(Vec3::new(
                mesh.normals[i * 3],
                mesh.normals[i * 3 + 1],
                mesh.normals[i * 3 + 2],
            ))
        } else {
            None
        };

        let uv = if has_uvs && mesh.texcoords.len() > i * 2 + 1 {
            Some(Vec2::new(
                mesh.texcoords[i * 2],
                1.0 - mesh.texcoords[i * 2 + 1], // Flip V coordinate
            ))
        } else {
            None
        };

        vertices.push(LoadedVertex {
            position,
            normal,
            uv,
            uv2: None,
            tangent: None,
            color: None,
            joints: None,
            weights: None,
        });
    }

    let indices: Vec<u32> = mesh.indices.iter().map(|&i| i as u32).collect();

    let mut loaded_mesh = LoadedMesh {
        name: model.name.clone(),
        vertices,
        indices,
        material_index: mesh.material_id,
        transform: LoadedTransform::default(),
    };

    // Generate normals if missing and requested
    if generate_normals && !has_normals {
        loaded_mesh.compute_flat_normals();
    }

    Some(loaded_mesh)
}

/// Convert a tobj material to a LoadedMaterial
fn convert_material(material: &tobj::Material, _obj_path: &Path) -> LoadedMaterial {
    let base_color = if let Some(diffuse) = material.diffuse {
        Color::rgb(diffuse[0], diffuse[1], diffuse[2])
    } else {
        Color::WHITE
    };

    // Estimate metallic/roughness from specular/shininess
    let metallic = if let Some(specular) = material.specular {
        // High specular = more metallic
        (specular[0] + specular[1] + specular[2]) / 3.0
    } else {
        0.0
    };

    let roughness = if let Some(shininess) = material.shininess {
        // Higher shininess = lower roughness
        1.0 - (shininess / 1000.0).clamp(0.0, 1.0)
    } else {
        0.5
    };

    let emissive = if let Some(ambient) = material.ambient {
        // Use ambient as emissive approximation
        Color::rgb(ambient[0] * 0.1, ambient[1] * 0.1, ambient[2] * 0.1)
    } else {
        Color::BLACK
    };

    let dissolve = material.dissolve.unwrap_or(1.0);
    let alpha_mode = if dissolve < 1.0 {
        AlphaMode::Blend
    } else {
        AlphaMode::Opaque
    };

    LoadedMaterial {
        name: material.name.clone(),
        base_color: Color::rgba(base_color.r, base_color.g, base_color.b, dissolve),
        base_color_texture: None, // Set by add_material_textures
        metallic,
        roughness,
        metallic_roughness_texture: None,
        normal_texture: None,
        normal_scale: 1.0,
        occlusion_texture: None,
        occlusion_strength: 1.0,
        emissive,
        emissive_texture: None,
        alpha_mode,
        alpha_cutoff: 0.5,
        double_sided: false,
    }
}

/// Add textures from a material to the scene
fn add_material_textures(material: &tobj::Material, obj_path: &Path, textures: &mut Vec<LoadedTexture>) {
    let base_path = obj_path.parent().unwrap_or(Path::new("."));

    // Diffuse texture
    if let Some(ref diffuse_texture) = material.diffuse_texture {
        textures.push(LoadedTexture {
            name: diffuse_texture.clone(),
            data: TextureData::Path(base_path.join(diffuse_texture)),
            sampler: TextureSampler::default(),
        });
    }

    // Normal texture
    if let Some(ref normal_texture) = material.normal_texture {
        textures.push(LoadedTexture {
            name: normal_texture.clone(),
            data: TextureData::Path(base_path.join(normal_texture)),
            sampler: TextureSampler::default(),
        });
    }

    // Specular texture (can be used for metallic-roughness)
    if let Some(ref specular_texture) = material.specular_texture {
        textures.push(LoadedTexture {
            name: specular_texture.clone(),
            data: TextureData::Path(base_path.join(specular_texture)),
            sampler: TextureSampler::default(),
        });
    }

    // Ambient texture (can be used for occlusion)
    if let Some(ref ambient_texture) = material.ambient_texture {
        textures.push(LoadedTexture {
            name: ambient_texture.clone(),
            data: TextureData::Path(base_path.join(ambient_texture)),
            sampler: TextureSampler::default(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_obj_loader_creation() {
        let loader = ObjLoader::new();
        assert!(loader.triangulate);
        assert!(loader.generate_normals);
        assert!(loader.load_materials);
    }

    #[test]
    fn test_supported_extensions() {
        let loader = ObjLoader::new();
        assert!(loader.can_load("obj"));
        assert!(loader.can_load("OBJ"));
        assert!(!loader.can_load("gltf"));
    }
}
