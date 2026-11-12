//! 3D viewport and particle viewport uniforms.

/// Uniform data for 3D SDF raymarching
///
/// Must match the shader layout exactly:
/// ```wgsl
/// struct SdfUniform {
///     camera_pos: vec4<f32>,
///     camera_dir: vec4<f32>,
///     camera_up: vec4<f32>,
///     camera_right: vec4<f32>,
///     resolution: vec2<f32>,
///     time: f32,
///     fov: f32,
///     max_steps: u32,
///     max_distance: f32,
///     epsilon: f32,
///     _padding: f32,
///     uv_offset: vec2<f32>,
///     uv_scale: vec2<f32>,
/// }
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Sdf3DUniform {
    /// Camera position (vec4, w unused)
    pub camera_pos: [f32; 4],
    /// Camera direction (normalized, vec4, w unused)
    pub camera_dir: [f32; 4],
    /// Camera up vector (normalized, vec4, w unused)
    pub camera_up: [f32; 4],
    /// Camera right vector (normalized, vec4, w unused)
    pub camera_right: [f32; 4],
    /// Resolution of the original (unclipped) viewport (width, height)
    pub resolution: [f32; 2],
    /// Time for animation
    pub time: f32,
    /// Field of view in radians
    pub fov: f32,
    /// Maximum raymarch steps
    pub max_steps: u32,
    /// Maximum ray distance
    pub max_distance: f32,
    /// Surface hit epsilon
    pub epsilon: f32,
    /// Padding for alignment
    pub _padding: f32,
    /// UV offset for clipped viewports (how much is clipped from top-left, normalized 0-1)
    pub uv_offset: [f32; 2],
    /// UV scale for clipped viewports (visible portion size / original size)
    pub uv_scale: [f32; 2],
}

impl Default for Sdf3DUniform {
    fn default() -> Self {
        Self {
            camera_pos: [0.0, 2.0, 5.0, 1.0],
            camera_dir: [0.0, 0.0, -1.0, 0.0],
            camera_up: [0.0, 1.0, 0.0, 0.0],
            camera_right: [1.0, 0.0, 0.0, 0.0],
            resolution: [800.0, 600.0],
            time: 0.0,
            fov: 0.8,
            max_steps: 128,
            max_distance: 100.0,
            epsilon: 0.001,
            _padding: 0.0,
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
        }
    }
}

/// A 3D viewport to be rendered via raymarching
#[derive(Clone, Debug)]
pub struct Viewport3D {
    /// The generated WGSL shader code for this scene
    pub shader_wgsl: String,
    /// Uniform data for the shader
    pub uniforms: Sdf3DUniform,
    /// Viewport bounds in screen coordinates
    pub bounds: [f32; 4],
    /// Lights in the scene
    pub lights: Vec<blinc_core::Light>,
}

/// GPU particle viewport for rendering particle systems
#[derive(Clone, Debug)]
pub struct ParticleViewport3D {
    /// Emitter configuration
    pub emitter: crate::particles::GpuEmitter,
    /// Force affectors
    pub forces: Vec<crate::particles::GpuForce>,
    /// Maximum particles in this system
    pub max_particles: u32,
    /// Viewport bounds in screen coordinates (x, y, width, height)
    pub bounds: [f32; 4],
    /// Camera position
    pub camera_pos: [f32; 3],
    /// Camera target
    pub camera_target: [f32; 3],
    /// Camera up vector
    pub camera_up: [f32; 3],
    /// Field of view
    pub fov: f32,
    /// Current time
    pub time: f32,
    /// Delta time for this frame
    pub delta_time: f32,
    /// Blend mode (0=alpha, 1=additive)
    pub blend_mode: u32,
    /// Whether system is playing
    pub playing: bool,
}

impl Default for ParticleViewport3D {
    fn default() -> Self {
        Self {
            emitter: crate::particles::GpuEmitter::default(),
            forces: Vec::new(),
            max_particles: 10000,
            bounds: [0.0, 0.0, 800.0, 600.0],
            camera_pos: [0.0, 2.0, 5.0],
            camera_target: [0.0, 0.0, 0.0],
            camera_up: [0.0, 1.0, 0.0],
            fov: 0.8,
            time: 0.0,
            delta_time: 0.016,
            blend_mode: 0,
            playing: true,
        }
    }
}

// Keep the old GpuRect for backwards compatibility during transition
/// Legacy rectangle primitive (deprecated - use GpuPrimitive instead)
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[deprecated(note = "Use GpuPrimitive instead")]
pub struct GpuRect {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub corner_radius: [f32; 4],
    pub border_width: f32,
    pub border_color: [f32; 4],
    pub _padding: [f32; 3],
}
