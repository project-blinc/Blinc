//! Canvas integration for 3D rendering
//!
//! Provides utilities for rendering 3D scenes through Blinc's canvas element.

use crate::ecs::{Entity, World};
use crate::math::Mat4Ext;
use crate::scene::{Object3D, OrthographicCamera, PerspectiveCamera};
use blinc_core::{Camera, CameraProjection, DrawContext, Light, Mat4, Vec3, Color};

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
    render_scene_with_config(ctx, world, camera_entity, bounds, &RenderConfig::default())
}

/// Render a 3D scene with custom configuration
pub fn render_scene_with_config(
    ctx: &mut dyn DrawContext,
    world: &World,
    camera_entity: Entity,
    bounds: CanvasBounds,
    _config: &RenderConfig,
) {
    // Get camera transform
    let camera_transform = world
        .get_component::<Object3D>(camera_entity)
        .cloned()
        .unwrap_or_default();

    // Try to get perspective camera first, then orthographic
    let core_camera = if let Some(perspective) = world.get_component::<PerspectiveCamera>(camera_entity) {
        perspective.to_core_camera(&camera_transform, bounds.aspect_ratio())
    } else if let Some(ortho) = world.get_component::<OrthographicCamera>(camera_entity) {
        ortho.to_core_camera(&camera_transform)
    } else {
        // Default perspective camera
        let target = Vec3::new(
            camera_transform.position.x + camera_transform.forward().x,
            camera_transform.position.y + camera_transform.forward().y,
            camera_transform.position.z + camera_transform.forward().z,
        );
        Camera {
            position: camera_transform.position,
            target,
            up: Vec3::UP,
            projection: CameraProjection::Perspective {
                fov_y: 0.8,
                aspect: bounds.aspect_ratio(),
                near: 0.1,
                far: 100.0,
            },
        }
    };

    // Set camera on context
    ctx.set_camera(&core_camera);

    // Collect and add lights
    add_lights_to_context(ctx, world);

    // Render meshes with CPU wireframe projection
    render_meshes(ctx, world, bounds, &core_camera);
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

/// Render all visible meshes with CPU wireframe projection
fn render_meshes(ctx: &mut dyn DrawContext, world: &World, bounds: CanvasBounds, camera: &Camera) {
    use crate::materials::BasicMaterial;
    use crate::scene::Mesh;

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

        // Get material color (default to white if not BasicMaterial)
        let color = world
            .get_material_as::<BasicMaterial>(mesh.material)
            .map(|m| m.color)
            .unwrap_or(Color::WHITE);

        // Get model matrix
        let model_matrix = transform.local_matrix();
        let mvp = view_proj.mul(&model_matrix);

        // Project and draw wireframe edges
        let vertices = &geometry.vertices;
        let indices = &geometry.indices;

        // Helper to project a 3D point to screen coordinates with perspective divide
        let project = |v: &crate::geometry::Vertex| -> Option<(f32, f32)> {
            let px = v.position[0];
            let py = v.position[1];
            let pz = v.position[2];

            // Manual 4x4 matrix * vec4 multiplication to get clip coordinates
            let clip_x = mvp.cols[0][0] * px + mvp.cols[1][0] * py + mvp.cols[2][0] * pz + mvp.cols[3][0];
            let clip_y = mvp.cols[0][1] * px + mvp.cols[1][1] * py + mvp.cols[2][1] * pz + mvp.cols[3][1];
            let clip_z = mvp.cols[0][2] * px + mvp.cols[1][2] * py + mvp.cols[2][2] * pz + mvp.cols[3][2];
            let clip_w = mvp.cols[0][3] * px + mvp.cols[1][3] * py + mvp.cols[2][3] * pz + mvp.cols[3][3];

            // Clip if behind camera (w <= 0)
            if clip_w <= 0.0001 {
                return None;
            }

            // Perspective divide to get NDC
            let ndc_x = clip_x / clip_w;
            let ndc_y = clip_y / clip_w;
            let ndc_z = clip_z / clip_w;

            // Cull if outside NDC bounds
            if ndc_z < 0.0 || ndc_z > 1.0 {
                return None;
            }

            // NDC to screen coordinates
            // NDC x/y is -1 to 1, we map to 0 to viewport size
            let screen_x = (ndc_x + 1.0) * 0.5 * bounds.width;
            let screen_y = (1.0 - ndc_y) * 0.5 * bounds.height; // Flip Y

            Some((screen_x, screen_y))
        };

        // Draw triangles as wireframe
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

            let p0 = project(&vertices[i0]);
            let p1 = project(&vertices[i1]);
            let p2 = project(&vertices[i2]);

            // Draw edges if both endpoints are visible
            if let (Some((x0, y0)), Some((x1, y1))) = (p0, p1) {
                draw_line(ctx, x0, y0, x1, y1, color);
            }
            if let (Some((x1, y1)), Some((x2, y2))) = (p1, p2) {
                draw_line(ctx, x1, y1, x2, y2, color);
            }
            if let (Some((x2, y2)), Some((x0, y0))) = (p2, p0) {
                draw_line(ctx, x2, y2, x0, y0, color);
            }
        }
    }
}

/// Draw a line using small rectangles (since DrawContext doesn't have line primitives)
fn draw_line(ctx: &mut dyn DrawContext, x0: f32, y0: f32, x1: f32, y1: f32, color: Color) {
    use blinc_core::{Brush, CornerRadius, Rect};

    let dx = x1 - x0;
    let dy = y1 - y0;
    let length = (dx * dx + dy * dy).sqrt();

    if length < 0.5 {
        return;
    }

    // Draw line as a series of small squares
    let steps = (length / 2.0).max(1.0) as i32;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let x = x0 + dx * t;
        let y = y0 + dy * t;
        ctx.fill_rect(
            Rect::new(x - 0.5, y - 0.5, 1.5, 1.5),
            CornerRadius::ZERO,
            Brush::Solid(color),
        );
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
