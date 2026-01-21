//! Canvas integration for 3D rendering
//!
//! Provides utilities for rendering 3D scenes through Blinc's canvas element.

use crate::ecs::{Entity, World};
use crate::scene::{Object3D, OrthographicCamera, PerspectiveCamera};
use blinc_core::{Camera, CameraProjection, DrawContext, Light, Mat4, Vec3, Color};

/// Bounds of the canvas viewport
#[derive(Clone, Copy, Debug)]
pub struct CanvasBounds {
    /// X position
    pub x: f32,
    /// Y position
    pub y: f32,
    /// Width
    pub width: f32,
    /// Height
    pub height: f32,
}

impl CanvasBounds {
    /// Create new canvas bounds
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Get aspect ratio
    pub fn aspect_ratio(&self) -> f32 {
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

    // Note: Actual mesh rendering would be done here
    // The DrawContext API may need mesh/material registration first
    render_meshes(ctx, world);
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

/// Render all visible meshes
fn render_meshes(ctx: &mut dyn DrawContext, world: &World) {
    use crate::scene::Mesh;

    // Query all entities with Mesh and Object3D
    for (_entity, (mesh, transform)) in world.query::<(&Mesh, &Object3D)>().iter() {
        // Skip invisible objects
        if !transform.visible {
            continue;
        }

        // Get local transform matrix (world matrix would need hierarchy traversal)
        let _world_matrix = transform.local_matrix();

        // Note: The actual drawing requires mesh/material registration with DrawContext
        // and proper MeshId/MaterialId handles. For now, this is a placeholder.
        // The integration would typically involve:
        // 1. Registering geometry as a GPU mesh
        // 2. Registering material as a GPU material
        // 3. Using the returned IDs with draw_mesh
        let _ = (mesh.geometry, mesh.material);
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
