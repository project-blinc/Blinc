//! Shader registry

use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Shader identifiers
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShaderId {
    /// Basic unlit mesh shader
    MeshBasic,
    /// PBR mesh shader
    MeshPbr,
    /// Phong mesh shader
    MeshPhong,
    /// SDF raymarching shader
    SdfRaymarch,
    /// SDF 3D text shader
    SdfText3d,
    /// SDF particles shader
    SdfParticles,
    /// Skybox shader
    Skybox,
    /// Shadow map shader
    ShadowMap,
    /// Post-process shader
    PostProcess,
    /// Custom shader with ID
    Custom(u32),
}

/// Shader registry for managing shader modules
pub struct ShaderRegistry {
    /// Compiled shader modules
    shaders: FxHashMap<ShaderId, wgpu::ShaderModule>,
    /// Custom shader counter
    next_custom_id: u32,
}

impl ShaderRegistry {
    /// Create a new shader registry
    pub fn new() -> Self {
        Self {
            shaders: FxHashMap::default(),
            next_custom_id: 0,
        }
    }

    /// Register all default shaders
    pub fn register_defaults(&mut self, device: &wgpu::Device) {
        self.register(
            device,
            ShaderId::MeshBasic,
            include_str!("shaders/mesh_basic.wgsl"),
        );
        self.register(
            device,
            ShaderId::MeshPbr,
            include_str!("shaders/mesh_pbr.wgsl"),
        );
        self.register(
            device,
            ShaderId::MeshPhong,
            include_str!("shaders/mesh_phong.wgsl"),
        );
        self.register(
            device,
            ShaderId::SdfRaymarch,
            include_str!("shaders/sdf_raymarch.wgsl"),
        );
        self.register(
            device,
            ShaderId::SdfText3d,
            include_str!("shaders/sdf_text_3d.wgsl"),
        );
        self.register(
            device,
            ShaderId::SdfParticles,
            include_str!("shaders/sdf_particles.wgsl"),
        );
        self.register(
            device,
            ShaderId::Skybox,
            include_str!("shaders/skybox.wgsl"),
        );
        self.register(
            device,
            ShaderId::ShadowMap,
            include_str!("shaders/shadow_map.wgsl"),
        );
    }

    /// Register a shader from WGSL source
    pub fn register(&mut self, device: &wgpu::Device, id: ShaderId, wgsl: &str) {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&format!("shader_{:?}", id)),
            source: wgpu::ShaderSource::Wgsl(wgsl.into()),
        });
        self.shaders.insert(id, module);
    }

    /// Register a custom shader
    pub fn register_custom(&mut self, device: &wgpu::Device, wgsl: &str) -> ShaderId {
        let id = ShaderId::Custom(self.next_custom_id);
        self.next_custom_id += 1;
        self.register(device, id, wgsl);
        id
    }

    /// Get a shader module
    pub fn get(&self, id: ShaderId) -> Option<&wgpu::ShaderModule> {
        self.shaders.get(&id)
    }

    /// Check if a shader is registered
    pub fn contains(&self, id: ShaderId) -> bool {
        self.shaders.contains_key(&id)
    }

    /// Remove a shader
    pub fn remove(&mut self, id: ShaderId) -> Option<wgpu::ShaderModule> {
        self.shaders.remove(&id)
    }
}

impl Default for ShaderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
