//! Canvas integration for 3D rendering
//!
//! Provides utilities for rendering 3D scenes through Blinc's canvas element.

use crate::ecs::{Entity, World};
use crate::math::Mat4Ext;
use crate::scene::{Object3D, OrthographicCamera, PerspectiveCamera, SdfMesh};
use crate::sdf::{SdfCamera, SdfCodegen, SdfScene};
use blinc_core::{Camera, CameraProjection, DrawContext, Light, Mat4, Rect, Sdf3DViewport, Vec3, Color};

// Re-export CanvasBounds from blinc_layout to avoid type duplication
pub use blinc_layout::CanvasBounds;

/// Extension trait for CanvasBounds to add 3D-specific helpers
pub trait CanvasBoundsExt {
    /// Get aspect ratio (width / height)
    fn aspect_ratio(&self) -> f32;
}

impl CanvasBoundsExt for CanvasBounds {
    fn aspect_ratio(&self) -> f32 {
        if self.height > 0.0 {
            self.width / self.height
        } else {
            1.0
        }
    }
}

/// Render configuration
#[derive(Clone, Debug)]
pub struct RenderConfig {
    /// Clear color
    pub clear_color: Color,
    /// Enable shadows
    pub shadows_enabled: bool,
    /// Shadow map resolution
    pub shadow_map_size: u32,
    /// Enable post-processing
    pub post_processing: bool,
    /// Enable anti-aliasing
    pub antialiasing: bool,
    /// Gamma correction
    pub gamma: f32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            clear_color: Color::rgb(0.1, 0.1, 0.15),
            shadows_enabled: true,
            shadow_map_size: 1024,
            post_processing: true,
            antialiasing: true,
            gamma: 2.2,
        }
    }
}

/// Render a 3D scene to the canvas
///
/// This function renders all visible meshes in the world using the specified
/// camera entity. It integrates with Blinc's DrawContext for rendering.
///
/// # Arguments
///
/// * `ctx` - The draw context from the canvas
/// * `world` - The ECS world containing entities and components
/// * `camera_entity` - The entity with camera and Object3D components
/// * `bounds` - The canvas viewport bounds
pub fn render_scene(
    ctx: &mut dyn DrawContext,
    world: &World,
    camera_entity: Entity,
    bounds: CanvasBounds,
) {
    render_scene_with_time(ctx, world, camera_entity, bounds, 0.0)
}

/// Render a 3D scene to the canvas with time for SDF animations
///
/// # Arguments
///
/// * `ctx` - The draw context from the canvas
/// * `world` - The ECS world containing entities and components
/// * `camera_entity` - The entity with camera and Object3D components
/// * `bounds` - The canvas viewport bounds
/// * `time` - Animation time in seconds (used for SDF animations)
pub fn render_scene_with_time(
    ctx: &mut dyn DrawContext,
    world: &World,
    camera_entity: Entity,
    bounds: CanvasBounds,
    time: f32,
) {
    render_scene_full(ctx, world, camera_entity, bounds, time, &RenderConfig::default())
}

/// Render a 3D scene with custom configuration
pub fn render_scene_with_config(
    ctx: &mut dyn DrawContext,
    world: &World,
    camera_entity: Entity,
    bounds: CanvasBounds,
    config: &RenderConfig,
) {
    render_scene_full(ctx, world, camera_entity, bounds, 0.0, config)
}

/// Render a 3D scene with time and custom configuration
///
/// This is the full render function that includes all parameters for both
/// mesh rendering and SDF raymarching.
pub fn render_scene_full(
    ctx: &mut dyn DrawContext,
    world: &World,
    camera_entity: Entity,
    bounds: CanvasBounds,
    time: f32,
    _config: &RenderConfig,
) {
    use blinc_core::{ClipShape, Rect};

    // Push clip to constrain rendering to canvas bounds
    ctx.push_clip(ClipShape::Rect(Rect::new(0.0, 0.0, bounds.width, bounds.height)));

    // Get camera transform
    let camera_transform = world
        .get_component::<Object3D>(camera_entity)
        .cloned()
        .unwrap_or_default();

    // Try to get perspective camera first, then orthographic
    let (core_camera, fov) = if let Some(perspective) = world.get_component::<PerspectiveCamera>(camera_entity) {
        (perspective.to_core_camera(&camera_transform, bounds.aspect_ratio()), perspective.effective_fov())
    } else if let Some(ortho) = world.get_component::<OrthographicCamera>(camera_entity) {
        (ortho.to_core_camera(&camera_transform), 0.8) // Ortho doesn't have FOV, use default
    } else {
        // Default perspective camera
        let target = Vec3::new(
            camera_transform.position.x + camera_transform.forward().x,
            camera_transform.position.y + camera_transform.forward().y,
            camera_transform.position.z + camera_transform.forward().z,
        );
        (Camera {
            position: camera_transform.position,
            target,
            up: Vec3::UP,
            projection: CameraProjection::Perspective {
                fov_y: 0.8,
                aspect: bounds.aspect_ratio(),
                near: 0.1,
                far: 100.0,
            },
        }, 0.8)
    };

    // Set camera on context
    ctx.set_camera(&core_camera);

    // Collect and add lights
    add_lights_to_context(ctx, world);

    // Render meshes with CPU wireframe projection
    render_meshes(ctx, world, bounds, &core_camera);

    // Render SDF meshes using the same camera
    render_sdf_meshes_with_fov(ctx, world, &camera_transform, bounds, time, fov);

    // Pop the clip region
    ctx.pop_clip();
}

/// Add all lights from the world to the draw context
fn add_lights_to_context(ctx: &mut dyn DrawContext, world: &World) {
    use crate::lights::{AmbientLight, DirectionalLight, PointLight, SpotLight};

    // Query ambient lights
    for (_entity, ambient) in world.query::<&AmbientLight>().iter() {
        ctx.add_light(Light::Ambient {
            color: ambient.color,
            intensity: ambient.intensity,
        });
    }

    // Query directional lights
    for (_entity, (light, transform)) in world.query::<(&DirectionalLight, &Object3D)>().iter() {
        ctx.add_light(Light::Directional {
            direction: transform.forward(),
            color: light.color,
            intensity: light.intensity,
            cast_shadows: light.cast_shadows,
        });
    }

    // Query point lights
    for (_entity, (light, transform)) in world.query::<(&PointLight, &Object3D)>().iter() {
        ctx.add_light(Light::Point {
            position: transform.position,
            color: light.color,
            intensity: light.intensity,
            range: light.distance,
        });
    }

    // Query spot lights
    for (_entity, (light, transform)) in world.query::<(&SpotLight, &Object3D)>().iter() {
        ctx.add_light(Light::Spot {
            position: transform.position,
            direction: transform.forward(),
            color: light.color,
            intensity: light.intensity,
            range: light.distance,
            inner_angle: light.angle * (1.0 - light.penumbra),
            outer_angle: light.angle,
        });
    }
}

/// Triangle data for sorting and rendering
struct ProjectedTriangle {
    /// Screen coordinates of vertices
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    /// Average depth for sorting (larger = further)
    depth: f32,
    /// Shaded color
    color: Color,
}

/// Collect lights from the world for shading
struct SceneLights {
    ambient: Color,
    ambient_intensity: f32,
    /// Main directional light direction (normalized)
    dir_light_dir: Vec3,
    dir_light_color: Color,
    dir_light_intensity: f32,
}

impl Default for SceneLights {
    fn default() -> Self {
        Self {
            ambient: Color::WHITE,
            ambient_intensity: 0.2,
            dir_light_dir: Vec3::new(0.5, 0.8, 0.3).normalize(),
            dir_light_color: Color::WHITE,
            dir_light_intensity: 0.8,
        }
    }
}

fn collect_scene_lights(world: &World) -> SceneLights {
    use crate::lights::{AmbientLight, DirectionalLight};

    let mut lights = SceneLights::default();

    // Get ambient light
    for (_entity, ambient) in world.query::<&AmbientLight>().iter() {
        lights.ambient = ambient.color;
        lights.ambient_intensity = ambient.intensity;
        break;
    }

    // Get first directional light
    for (_entity, (light, transform)) in world.query::<(&DirectionalLight, &Object3D)>().iter() {
        lights.dir_light_dir = transform.forward().normalize();
        lights.dir_light_color = light.color;
        lights.dir_light_intensity = light.intensity;
        break;
    }

    lights
}

/// Get material color supporting all material types
fn get_material_color(world: &World, material_handle: crate::materials::MaterialHandle) -> (Color, f32, f32) {
    use crate::materials::{BasicMaterial, PhongMaterial, StandardMaterial};

    // Try StandardMaterial first (PBR)
    if let Some(mat) = world.get_material_as::<StandardMaterial>(material_handle) {
        return (mat.color, mat.metalness, mat.roughness);
    }

    // Try PhongMaterial
    if let Some(mat) = world.get_material_as::<PhongMaterial>(material_handle) {
        return (mat.color, 0.0, 1.0 / (mat.shininess + 1.0));
    }

    // Try BasicMaterial
    if let Some(mat) = world.get_material_as::<BasicMaterial>(material_handle) {
        return (mat.color, 0.0, 1.0);
    }

    // Default
    (Color::WHITE, 0.0, 0.5)
}

/// Apply simple lighting to a base color
fn shade_color(base_color: Color, normal: Vec3, lights: &SceneLights, metalness: f32, roughness: f32) -> Color {
    // Ambient contribution
    let ambient = Color::rgb(
        base_color.r * lights.ambient.r * lights.ambient_intensity,
        base_color.g * lights.ambient.g * lights.ambient_intensity,
        base_color.b * lights.ambient.b * lights.ambient_intensity,
    );

    // Diffuse lighting (Lambertian)
    let n_dot_l = normal.dot(lights.dir_light_dir).max(0.0);
    let diffuse_strength = n_dot_l * lights.dir_light_intensity * (1.0 - metalness * 0.5);

    let diffuse = Color::rgb(
        base_color.r * lights.dir_light_color.r * diffuse_strength,
        base_color.g * lights.dir_light_color.g * diffuse_strength,
        base_color.b * lights.dir_light_color.b * diffuse_strength,
    );

    // Simple specular highlight for metallic surfaces
    let spec_strength = if metalness > 0.5 {
        (1.0 - roughness) * metalness * n_dot_l.powf(2.0 + (1.0 - roughness) * 30.0) * 0.5
    } else {
        0.0
    };

    let specular = Color::rgb(
        lights.dir_light_color.r * spec_strength,
        lights.dir_light_color.g * spec_strength,
        lights.dir_light_color.b * spec_strength,
    );

    // Combine and clamp
    Color::rgba(
        (ambient.r + diffuse.r + specular.r).min(1.0),
        (ambient.g + diffuse.g + specular.g).min(1.0),
        (ambient.b + diffuse.b + specular.b).min(1.0),
        base_color.a,
    )
}

/// Render all visible meshes with filled triangles and basic lighting
fn render_meshes(ctx: &mut dyn DrawContext, world: &World, bounds: CanvasBounds, camera: &Camera) {
    use crate::scene::Mesh;
    use blinc_core::{Brush, Path};

    let view_matrix = Mat4::look_at_rh(camera.position, camera.target, camera.up);
    let proj_matrix = match camera.projection {
        CameraProjection::Perspective { fov_y, aspect, near, far } => {
            Mat4::perspective_rh(fov_y, aspect, near, far)
        }
        CameraProjection::Orthographic { left, right, bottom, top, near, far } => {
            Mat4::orthographic_rh(left, right, bottom, top, near, far)
        }
    };
    let view_proj = proj_matrix.mul(&view_matrix);

    // Collect scene lights
    let lights = collect_scene_lights(world);

    // Collect all triangles for depth sorting
    let mut triangles: Vec<ProjectedTriangle> = Vec::new();

    // Query all entities with Mesh and Object3D
    for (_entity, (mesh, transform)) in world.query::<(&Mesh, &Object3D)>().iter() {
        // Skip invisible objects
        if !transform.visible {
            continue;
        }

        // Get the geometry
        let Some(geometry) = world.get_geometry(mesh.geometry) else {
            continue;
        };

        // Get material properties
        let (base_color, metalness, roughness) = get_material_color(world, mesh.material);

        // Get model matrix
        let model_matrix = transform.local_matrix();
        let mvp = view_proj.mul(&model_matrix);
        let normal_matrix = model_matrix.inverse().transpose();

        let vertices = &geometry.vertices;
        let indices = &geometry.indices;

        // Process triangles
        for chunk in indices.chunks(3) {
            if chunk.len() < 3 {
                continue;
            }
            let i0 = chunk[0] as usize;
            let i1 = chunk[1] as usize;
            let i2 = chunk[2] as usize;

            if i0 >= vertices.len() || i1 >= vertices.len() || i2 >= vertices.len() {
                continue;
            }

            let v0 = &vertices[i0];
            let v1 = &vertices[i1];
            let v2 = &vertices[i2];

            // Project vertices to screen
            let project_with_depth = |v: &crate::geometry::Vertex| -> Option<(f32, f32, f32)> {
                let px = v.position[0];
                let py = v.position[1];
                let pz = v.position[2];

                let clip_x = mvp.cols[0][0] * px + mvp.cols[1][0] * py + mvp.cols[2][0] * pz + mvp.cols[3][0];
                let clip_y = mvp.cols[0][1] * px + mvp.cols[1][1] * py + mvp.cols[2][1] * pz + mvp.cols[3][1];
                let clip_z = mvp.cols[0][2] * px + mvp.cols[1][2] * py + mvp.cols[2][2] * pz + mvp.cols[3][2];
                let clip_w = mvp.cols[0][3] * px + mvp.cols[1][3] * py + mvp.cols[2][3] * pz + mvp.cols[3][3];

                if clip_w <= 0.0001 {
                    return None;
                }

                let ndc_x = clip_x / clip_w;
                let ndc_y = clip_y / clip_w;
                let ndc_z = clip_z / clip_w;

                if ndc_z < -0.1 || ndc_z > 1.1 {
                    return None;
                }

                let screen_x = (ndc_x + 1.0) * 0.5 * bounds.width;
                let screen_y = (1.0 - ndc_y) * 0.5 * bounds.height;

                Some((screen_x, screen_y, ndc_z))
            };

            let p0 = project_with_depth(v0);
            let p1 = project_with_depth(v1);
            let p2 = project_with_depth(v2);

            // All three vertices must be visible
            let (Some((x0, y0, z0)), Some((x1, y1, z1)), Some((x2, y2, z2))) = (p0, p1, p2) else {
                continue;
            };

            // Back-face culling using cross product in screen space
            let edge1_x = x1 - x0;
            let edge1_y = y1 - y0;
            let edge2_x = x2 - x0;
            let edge2_y = y2 - y0;
            let cross = edge1_x * edge2_y - edge1_y * edge2_x;
            if cross < 0.0 {
                continue; // Back-facing
            }

            // Calculate face normal in world space for shading
            let n0 = Vec3::new(v0.normal[0], v0.normal[1], v0.normal[2]);
            let n1 = Vec3::new(v1.normal[0], v1.normal[1], v1.normal[2]);
            let n2 = Vec3::new(v2.normal[0], v2.normal[1], v2.normal[2]);
            let avg_normal = Vec3::new(
                (n0.x + n1.x + n2.x) / 3.0,
                (n0.y + n1.y + n2.y) / 3.0,
                (n0.z + n1.z + n2.z) / 3.0,
            );

            // Transform normal by normal matrix
            let world_normal = Vec3::new(
                normal_matrix.cols[0][0] * avg_normal.x + normal_matrix.cols[1][0] * avg_normal.y + normal_matrix.cols[2][0] * avg_normal.z,
                normal_matrix.cols[0][1] * avg_normal.x + normal_matrix.cols[1][1] * avg_normal.y + normal_matrix.cols[2][1] * avg_normal.z,
                normal_matrix.cols[0][2] * avg_normal.x + normal_matrix.cols[1][2] * avg_normal.y + normal_matrix.cols[2][2] * avg_normal.z,
            ).normalize();

            // Apply shading
            let shaded_color = shade_color(base_color, world_normal, &lights, metalness, roughness);

            // Average depth for sorting
            let avg_depth = (z0 + z1 + z2) / 3.0;

            triangles.push(ProjectedTriangle {
                p0: (x0, y0),
                p1: (x1, y1),
                p2: (x2, y2),
                depth: avg_depth,
                color: shaded_color,
            });
        }
    }

    // Sort triangles back-to-front (painter's algorithm)
    triangles.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal));

    // Draw filled triangles
    for tri in &triangles {
        let path = Path::new()
            .move_to(tri.p0.0, tri.p0.1)
            .line_to(tri.p1.0, tri.p1.1)
            .line_to(tri.p2.0, tri.p2.1)
            .close();
        ctx.fill_path(&path, Brush::Solid(tri.color));
    }
}


/// Render all SdfMesh entities in the world
fn render_sdf_meshes_with_fov(
    ctx: &mut dyn DrawContext,
    world: &World,
    camera_transform: &Object3D,
    bounds: CanvasBounds,
    time: f32,
    fov: f32,
) {
    // Calculate camera vectors from transform
    let camera_pos = camera_transform.position;
    let camera_dir = camera_transform.forward().normalize();
    let up = camera_transform.up();

    // Calculate right vector (cross product of direction and up)
    let right = Vec3::new(
        camera_dir.z * up.y - camera_dir.y * up.z,
        camera_dir.x * up.z - camera_dir.z * up.x,
        camera_dir.y * up.x - camera_dir.x * up.y,
    );
    let right_len = (right.x * right.x + right.y * right.y + right.z * right.z).sqrt();
    let camera_right = if right_len > 0.0001 {
        Vec3::new(right.x / right_len, right.y / right_len, right.z / right_len)
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    };

    // Recalculate up (cross product of right and direction)
    let camera_up = Vec3::new(
        camera_right.y * camera_dir.z - camera_right.z * camera_dir.y,
        camera_right.z * camera_dir.x - camera_right.x * camera_dir.z,
        camera_right.x * camera_dir.y - camera_right.y * camera_dir.x,
    );

    // Query all SdfMesh entities
    for (_entity, (sdf_mesh, transform)) in world.query::<(&SdfMesh, &Object3D)>().iter() {
        if !transform.visible {
            continue;
        }

        // Generate shader code for this SDF scene
        let shader_wgsl = SdfCodegen::generate_full_shader(&sdf_mesh.scene);

        // Create the SDF 3D viewport
        let viewport = Sdf3DViewport {
            shader_wgsl,
            camera_pos,
            camera_dir,
            camera_up,
            camera_right,
            fov,
            time,
            max_steps: 128,
            max_distance: 100.0,
            epsilon: 0.001,
            lights: Vec::new(),
        };

        // Draw the SDF viewport
        let rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
        ctx.draw_sdf_viewport(rect, &viewport);
    }
}

/// Scene renderer that can be used with canvas callbacks
pub struct SceneRenderer {
    /// The ECS world
    world: World,
    /// Camera entity
    camera: Entity,
    /// Render configuration
    config: RenderConfig,
}

impl SceneRenderer {
    /// Create a new scene renderer
    pub fn new(world: World, camera: Entity) -> Self {
        Self {
            world,
            camera,
            config: RenderConfig::default(),
        }
    }

    /// Set render configuration
    pub fn with_config(mut self, config: RenderConfig) -> Self {
        self.config = config;
        self
    }

    /// Get mutable reference to the world
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// Get reference to the world
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Set the active camera
    pub fn set_camera(&mut self, camera: Entity) {
        self.camera = camera;
    }

    /// Render the scene
    pub fn render(&self, ctx: &mut dyn DrawContext, bounds: CanvasBounds) {
        render_scene_with_config(ctx, &self.world, self.camera, bounds, &self.config);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SDF Scene Rendering
// ─────────────────────────────────────────────────────────────────────────────

/// Render an SDF scene using GPU raymarching
///
/// This function renders procedural 3D geometry defined by signed distance functions.
/// The scene is raymarched on the GPU for high-quality rendering.
///
/// # Arguments
///
/// * `ctx` - The draw context from the canvas
/// * `scene` - The SDF scene to render
/// * `camera` - The camera configuration
/// * `bounds` - The canvas viewport bounds
///
/// # Example
///
/// ```ignore
/// use blinc_3d::prelude::*;
/// use blinc_layout::prelude::*;
///
/// // Create an SDF scene
/// let scene = SdfScene::new()
///     .sphere(1.0)
///     .translate(0.0, 1.0, 0.0);
///
/// let camera = SdfCamera::default();
///
/// // Render in a canvas
/// canvas(move |ctx, bounds| {
///     render_sdf_scene(ctx, &scene, &camera, bounds, 0.0);
/// });
/// ```
pub fn render_sdf_scene(
    ctx: &mut dyn DrawContext,
    scene: &SdfScene,
    camera: &SdfCamera,
    bounds: CanvasBounds,
    time: f32,
) {
    render_sdf_scene_with_config(ctx, scene, camera, bounds, time, &SdfRenderConfig::default())
}

/// Configuration for SDF scene rendering
#[derive(Clone, Debug)]
pub struct SdfRenderConfig {
    /// Maximum raymarch steps
    pub max_steps: u32,
    /// Maximum ray distance
    pub max_distance: f32,
    /// Surface hit epsilon
    pub epsilon: f32,
    /// Lights to use for shading (if empty, uses default lighting)
    pub lights: Vec<Light>,
}

impl Default for SdfRenderConfig {
    fn default() -> Self {
        Self {
            max_steps: 128,
            max_distance: 100.0,
            epsilon: 0.001,
            lights: Vec::new(),
        }
    }
}

/// Render an SDF scene with custom configuration
pub fn render_sdf_scene_with_config(
    ctx: &mut dyn DrawContext,
    scene: &SdfScene,
    camera: &SdfCamera,
    bounds: CanvasBounds,
    time: f32,
    config: &SdfRenderConfig,
) {
    // Generate the shader code from the scene
    let shader_wgsl = SdfCodegen::generate_full_shader(scene);

    // Calculate camera vectors
    let direction = Vec3::new(
        camera.target.x - camera.position.x,
        camera.target.y - camera.position.y,
        camera.target.z - camera.position.z,
    );
    let dir_len = (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z).sqrt();
    let camera_dir = Vec3::new(
        direction.x / dir_len,
        direction.y / dir_len,
        direction.z / dir_len,
    );

    // Calculate right vector (cross product of direction and up)
    let right = Vec3::new(
        camera_dir.z * camera.up.y - camera_dir.y * camera.up.z,
        camera_dir.x * camera.up.z - camera_dir.z * camera.up.x,
        camera_dir.y * camera.up.x - camera_dir.x * camera.up.y,
    );
    let right_len = (right.x * right.x + right.y * right.y + right.z * right.z).sqrt();
    let camera_right = Vec3::new(right.x / right_len, right.y / right_len, right.z / right_len);

    // Recalculate up (cross product of right and direction)
    let camera_up = Vec3::new(
        camera_right.y * camera_dir.z - camera_right.z * camera_dir.y,
        camera_right.z * camera_dir.x - camera_right.x * camera_dir.z,
        camera_right.x * camera_dir.y - camera_right.y * camera_dir.x,
    );

    // Create the SDF 3D viewport
    let viewport = Sdf3DViewport {
        shader_wgsl,
        camera_pos: camera.position,
        camera_dir,
        camera_up,
        camera_right,
        fov: camera.fov,
        time,
        max_steps: config.max_steps,
        max_distance: config.max_distance,
        epsilon: config.epsilon,
        lights: config.lights.clone(),
    };

    // Draw the SDF viewport
    let rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
    ctx.draw_sdf_viewport(rect, &viewport);
}
