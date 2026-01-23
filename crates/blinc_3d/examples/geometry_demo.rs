//! Geometry Primitives Demo
//!
//! This example demonstrates blinc_3d's geometry primitive generators:
//! - BoxGeometry (cubes and boxes)
//! - SphereGeometry
//! - PlaneGeometry
//! - CylinderGeometry
//! - TorusGeometry
//!
//! Uses proper ECS with Mesh entities and render_scene() integration.
//! Features animated rotation and a dynamic stats panel.
//!
//! Run with: cargo run -p blinc_3d --example geometry_demo

use blinc_3d::prelude::*;
use blinc_3d::ecs::{System, SystemContext, SystemStage};
use blinc_3d::integration::render_scene;
use blinc_3d::materials::BasicMaterial;
use blinc_3d::lights::{AmbientLight, DirectionalLight};
use blinc_3d::scene::{Object3D, Mesh, PerspectiveCamera};
use blinc_3d::geometry::{BoxGeometry, SphereGeometry, PlaneGeometry, CylinderGeometry, TorusGeometry};
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::{Brush, CornerRadius, DrawContext, Rect};
use std::f32::consts::PI;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU32, Ordering};

// ============================================================================
// Shared State for Dynamic Stats Display
// ============================================================================

/// Elapsed time in milliseconds (for display)
static ELAPSED_MS: AtomicU32 = AtomicU32::new(0);
/// Frame counter
static FRAME_COUNT: AtomicU32 = AtomicU32::new(0);

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Blinc 3D - Geometry Primitives Demo".to_string(),
        width: 1100,
        height: 750,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

// ============================================================================
// Geometry Metadata for Display
// ============================================================================
#[derive(Clone, Debug)]
struct GeometryInfo {
    name: &'static str,
    description: &'static str,
    vertex_count: usize,
    triangle_count: usize,
}

// ============================================================================
// Marker Component for Rotating Objects
// ============================================================================

/// Marker component for objects that should rotate
#[derive(Clone, Debug)]
struct Rotating {
    speed: f32,  // radians per second
    axis: Vec3,  // rotation axis
}

impl Component for Rotating {
    const STORAGE: blinc_3d::ecs::StorageType = blinc_3d::ecs::StorageType::Dense;
}

// ============================================================================
// Rotation System
// ============================================================================

struct RotationSystem;

impl System for RotationSystem {
    fn run(&mut self, ctx: &mut SystemContext) {
        // Collect entities with Rotating and Object3D
        let entities: Vec<_> = ctx.world.query::<(&Rotating, &Object3D)>()
            .iter()
            .map(|(e, _)| e)
            .collect();

        for entity in entities {
            let rotation_data = ctx.world.get::<Rotating>(entity)
                .map(|r| (r.speed, r.axis));

            if let Some((speed, axis)) = rotation_data {
                if let Some(transform) = ctx.world.get_mut::<Object3D>(entity) {
                    // Create incremental rotation quaternion
                    let delta_angle = speed * ctx.delta_time;
                    let delta_rotation = Quat::from_axis_angle(axis, delta_angle);
                    // Apply rotation
                    transform.rotation = delta_rotation * transform.rotation;
                }
            }
        }
    }

    fn name(&self) -> &'static str {
        "RotationSystem"
    }

    fn stage(&self) -> SystemStage {
        SystemStage::Update
    }
}

// ============================================================================
// Scene Setup
// ============================================================================

fn create_geometry_world() -> (World, Entity, Vec<GeometryInfo>) {
    let mut world = World::new();
    let mut geometry_info = Vec::new();

    // Add ambient light
    world.spawn()
        .insert(AmbientLight {
            color: Color::WHITE,
            intensity: 0.3,
        });

    // Add directional light
    let mut light_transform = Object3D::default().with_position(5.0, 8.0, 5.0);
    light_transform.look_at(Vec3::ZERO);
    world.spawn()
        .insert(light_transform)
        .insert(DirectionalLight::sun().intensity(1.0));

    // Create wireframe materials with different colors
    let colors = [
        Color::rgb(0.3, 0.7, 1.0),  // Blue - Box
        Color::rgb(1.0, 0.5, 0.3),  // Orange - Sphere
        Color::rgb(0.5, 1.0, 0.5),  // Green - Plane
        Color::rgb(1.0, 0.8, 0.3),  // Yellow - Cylinder
        Color::rgb(0.8, 0.4, 1.0),  // Purple - Torus
        Color::rgb(0.3, 1.0, 0.8),  // Cyan - Tall Box
    ];

    // Row 1: Box, Sphere, Plane
    // Row 2: Cylinder, Torus, Tall Box
    let positions = [
        Vec3::new(-3.0, 1.5, 0.0),
        Vec3::new(0.0, 1.5, 0.0),
        Vec3::new(3.0, 1.5, 0.0),
        Vec3::new(-3.0, -1.5, 0.0),
        Vec3::new(0.0, -1.5, 0.0),
        Vec3::new(3.0, -1.5, 0.0),
    ];

    // 1. Box Geometry (rotating)
    let box_geom = BoxGeometry::cube(1.2);
    geometry_info.push(GeometryInfo {
        name: "BoxGeometry",
        description: "Rectangular prism / cube",
        vertex_count: box_geom.vertex_count(),
        triangle_count: box_geom.triangle_count(),
    });
    let box_handle = world.add_geometry(box_geom);
    let box_mat = world.add_material(BasicMaterial::with_color(colors[0]).wireframe());
    world.spawn()
        .insert(Object3D::default().with_position(positions[0].x, positions[0].y, positions[0].z))
        .insert(Mesh::new(box_handle, box_mat))
        .insert(Rotating { speed: 0.8, axis: Vec3::new(0.0, 1.0, 0.0) });

    // 2. Sphere Geometry (rotating)
    let sphere_geom = SphereGeometry::with_detail(0.7, 24, 18);
    geometry_info.push(GeometryInfo {
        name: "SphereGeometry",
        description: "UV sphere with configurable segments",
        vertex_count: sphere_geom.vertex_count(),
        triangle_count: sphere_geom.triangle_count(),
    });
    let sphere_handle = world.add_geometry(sphere_geom);
    let sphere_mat = world.add_material(BasicMaterial::with_color(colors[1]).wireframe());
    world.spawn()
        .insert(Object3D::default().with_position(positions[1].x, positions[1].y, positions[1].z))
        .insert(Mesh::new(sphere_handle, sphere_mat))
        .insert(Rotating { speed: 0.5, axis: Vec3::new(0.0, 1.0, 0.2).normalize() });

    // 3. Plane Geometry (rotating slowly)
    let plane_geom = PlaneGeometry::with_segments(1.8, 1.8, 6, 6);
    geometry_info.push(GeometryInfo {
        name: "PlaneGeometry",
        description: "Flat quad grid",
        vertex_count: plane_geom.vertex_count(),
        triangle_count: plane_geom.triangle_count(),
    });
    let plane_handle = world.add_geometry(plane_geom);
    let plane_mat = world.add_material(BasicMaterial::with_color(colors[2]).wireframe());
    let mut plane_transform = Object3D::default().with_position(positions[2].x, positions[2].y, positions[2].z);
    plane_transform.rotation = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), -0.3);
    world.spawn()
        .insert(plane_transform)
        .insert(Mesh::new(plane_handle, plane_mat))
        .insert(Rotating { speed: 0.3, axis: Vec3::new(0.0, 0.0, 1.0) });

    // 4. Cylinder Geometry (rotating)
    let cylinder_geom = CylinderGeometry::with_detail(0.5, 0.5, 1.2, 20, 1, false);
    geometry_info.push(GeometryInfo {
        name: "CylinderGeometry",
        description: "Radial cylinder with caps",
        vertex_count: cylinder_geom.vertex_count(),
        triangle_count: cylinder_geom.triangle_count(),
    });
    let cylinder_handle = world.add_geometry(cylinder_geom);
    let cylinder_mat = world.add_material(BasicMaterial::with_color(colors[3]).wireframe());
    world.spawn()
        .insert(Object3D::default().with_position(positions[3].x, positions[3].y, positions[3].z))
        .insert(Mesh::new(cylinder_handle, cylinder_mat))
        .insert(Rotating { speed: 0.6, axis: Vec3::new(0.0, 1.0, 0.0) });

    // 5. Torus Geometry (rotating)
    let torus_geom = TorusGeometry::with_detail(0.6, 0.25, 20, 14);
    geometry_info.push(GeometryInfo {
        name: "TorusGeometry",
        description: "Donut shape (ring torus)",
        vertex_count: torus_geom.vertex_count(),
        triangle_count: torus_geom.triangle_count(),
    });
    let torus_handle = world.add_geometry(torus_geom);
    let torus_mat = world.add_material(BasicMaterial::with_color(colors[4]).wireframe());
    let mut torus_transform = Object3D::default().with_position(positions[4].x, positions[4].y, positions[4].z);
    torus_transform.rotation = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), 0.4);
    world.spawn()
        .insert(torus_transform)
        .insert(Mesh::new(torus_handle, torus_mat))
        .insert(Rotating { speed: 0.7, axis: Vec3::new(1.0, 0.5, 0.0).normalize() });

    // 6. Tall Box (rotating)
    let tall_box_geom = BoxGeometry::new(0.6, 1.8, 0.6);
    geometry_info.push(GeometryInfo {
        name: "BoxGeometry",
        description: "Non-uniform dimensions",
        vertex_count: tall_box_geom.vertex_count(),
        triangle_count: tall_box_geom.triangle_count(),
    });
    let tall_box_handle = world.add_geometry(tall_box_geom);
    let tall_box_mat = world.add_material(BasicMaterial::with_color(colors[5]).wireframe());
    world.spawn()
        .insert(Object3D::default().with_position(positions[5].x, positions[5].y, positions[5].z))
        .insert(Mesh::new(tall_box_handle, tall_box_mat))
        .insert(Rotating { speed: 0.4, axis: Vec3::new(0.0, 1.0, 0.0) });

    // Create camera
    let mut camera_transform = Object3D::default().with_position(0.0, 1.0, 9.0);
    camera_transform.look_at(Vec3::new(0.0, 0.0, 0.0));
    let camera = world.spawn()
        .insert(camera_transform)
        .insert(PerspectiveCamera::new(PI / 4.0, 1.5, 0.1, 100.0))
        .id();

    (world, camera, geometry_info)
}

// ============================================================================
// UI Building
// ============================================================================

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    // Create world with persisted state (survives UI rebuilds)
    let world = ctx.use_state_keyed("geometry_world", || {
        let (world, camera, _) = create_geometry_world();
        Arc::new(Mutex::new((world, camera)))
    });

    // Get geometry info (static, computed once)
    let geometry_info = ctx.use_state_keyed("geometry_info", || {
        let (world, _, info) = create_geometry_world();
        drop(world);
        info
    });

    // Register tick callback for animation
    let world_for_tick = world.get();
    ctx.use_tick_callback(move |dt| {
        if let Ok(mut guard) = world_for_tick.lock() {
            let (ref mut world, _) = *guard;

            // Update stats
            let elapsed = ELAPSED_MS.load(Ordering::Relaxed);
            ELAPSED_MS.store(elapsed.wrapping_add((dt * 1000.0) as u32), Ordering::Relaxed);
            FRAME_COUNT.fetch_add(1, Ordering::Relaxed);

            // Run rotation system
            let mut rotation_system = RotationSystem;
            let mut sys_ctx = SystemContext {
                world,
                delta_time: dt.min(0.1),
                elapsed_time: elapsed as f32 / 1000.0,
                frame: FRAME_COUNT.load(Ordering::Relaxed) as u64,
            };
            rotation_system.run(&mut sys_ctx);
        }
    });

    let world_for_canvas = world.get();

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.06, 0.06, 0.1, 1.0))
        .flex_col()
        .p(16.0)
        .gap(16.0)
        // Title
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(
                    text("Blinc 3D - Geometry Primitives")
                        .size(28.0)
                        .color(Color::WHITE),
                )
                .child(
                    text("3D geometry generators with ECS integration and render_scene()")
                        .size(14.0)
                        .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
                ),
        )
        // Main content
        .child(
            div()
                .flex_1()
                .flex_row()
                .gap(16.0)
                // 3D Viewport
                .child(geometry_viewport(world_for_canvas))
                // Info panel with dynamic stats
                .child(geometry_info_panel(geometry_info.get())),
        )
}

fn geometry_viewport(world: Arc<Mutex<(World, Entity)>>) -> Canvas {
    canvas(move |ctx, bounds| {
        let guard = world.lock().unwrap();
        let (ref world, camera) = *guard;

        // Render using blinc_3d's render_scene integration
        render_scene(
            ctx,
            world,
            camera,
            bounds,
        );
    })
    .flex_grow()
    .h_full()
}

fn geometry_info_panel(infos: Vec<GeometryInfo>) -> Stack {
    // Use a stack with a div background and a canvas for dynamic stats
    stack()
        .w(280.0)
        .h_full()
        // Background div with static content
        .child(
            div()
                .w(280.0)
                .h_full()
                .bg(Color::rgba(0.1, 0.1, 0.14, 1.0))
                .rounded(8.0)
                .p(16.0)
                .flex_col()
                .gap(12.0)
                // Header
                .child(text("Geometry Primitives").size(16.0).color(Color::WHITE))
                .child(
                    text("Built-in 3D shape generators")
                        .size(11.0)
                        .color(Color::rgba(0.5, 0.5, 0.6, 1.0)),
                )
                // Geometry info items
                .child(geometry_info_list(infos))
                // Dynamic stats section
                .child(div().h(8.0)) // Spacer
                .child(text("Live Stats").size(14.0).color(Color::WHITE))
                .child(dynamic_stats_canvas())
                // API info
                .child(div().h(8.0)) // Spacer
                .child(text("API Usage").size(14.0).color(Color::WHITE))
                .child(
                    code("let geom = BoxGeometry::cube(1.0);\nlet handle = world.add_geometry(geom);")
                        .font_size(10.0)
                        .rounded(4.0),
                )
                .child(
                    div()
                        .flex_row()
                        .gap(6.0)
                        .items_center()
                        .child(
                            div()
                                .w(4.0)
                                .h(4.0)
                                .rounded(2.0)
                                .bg(Color::rgba(0.3, 0.8, 0.5, 0.8)),
                        )
                        .child(
                            text("Wireframe rendered via BasicMaterial")
                                .size(10.0)
                                .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
                        ),
                ),
        )
}

fn geometry_info_list(infos: Vec<GeometryInfo>) -> Div {
    let mut container = div().flex_col().gap(8.0);
    for info in infos {
        container = container.child(geometry_info_item(info));
    }
    container
}

fn geometry_info_item(info: GeometryInfo) -> Div {
    div()
        .flex_col()
        .gap(4.0)
        .child(
            div()
                .flex_row()
                .justify_between()
                .items_center()
                .child(
                    text(info.name)
                        .size(12.0)
                        .color(Color::rgba(0.9, 0.9, 1.0, 1.0)),
                )
                .child(
                    div()
                        .flex_row()
                        .gap(8.0)
                        .child(
                            text(&format!("{}v", info.vertex_count))
                                .size(10.0)
                                .color(Color::rgba(0.5, 0.7, 1.0, 0.8)),
                        )
                        .child(
                            text(&format!("{}t", info.triangle_count))
                                .size(10.0)
                                .color(Color::rgba(1.0, 0.7, 0.5, 0.8)),
                        ),
                ),
        )
        .child(
            text(info.description)
                .size(10.0)
                .color(Color::rgba(0.5, 0.5, 0.6, 1.0)),
        )
}

/// Canvas that displays dynamic stats (re-renders each frame)
fn dynamic_stats_canvas() -> Canvas {
    canvas(move |ctx: &mut dyn DrawContext, bounds| {
        // Get current stats
        let elapsed_ms = ELAPSED_MS.load(Ordering::Relaxed);
        let frame_count = FRAME_COUNT.load(Ordering::Relaxed);
        let elapsed_sec = elapsed_ms as f32 / 1000.0;
        let fps = if elapsed_sec > 0.0 {
            frame_count as f32 / elapsed_sec
        } else {
            0.0
        };

        // Draw stats with filled rectangles and text simulation
        let stat_height = 20.0;
        let y_offset = 4.0;

        // Elapsed time indicator
        draw_stat_bar(ctx, 0.0, y_offset, bounds.width, stat_height,
            "Time", &format!("{:.1}s", elapsed_sec), Color::rgba(0.3, 0.6, 0.9, 0.6));

        // Frame count indicator
        draw_stat_bar(ctx, 0.0, y_offset + stat_height + 4.0, bounds.width, stat_height,
            "Frames", &format!("{}", frame_count), Color::rgba(0.5, 0.8, 0.3, 0.6));

        // FPS indicator
        draw_stat_bar(ctx, 0.0, y_offset + (stat_height + 4.0) * 2.0, bounds.width, stat_height,
            "FPS", &format!("{:.0}", fps), Color::rgba(0.9, 0.5, 0.3, 0.6));
    })
    .w(248.0) // Panel width minus padding
    .h(80.0)
}

fn draw_stat_bar(
    ctx: &mut dyn DrawContext,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    _label: &str,
    _value: &str,
    color: Color,
) {
    // Draw background bar
    ctx.fill_rect(
        Rect::new(x, y, width, height),
        CornerRadius::uniform(4.0),
        Brush::Solid(color),
    );

    // Draw a small indicator to show it's updating
    let indicator_width = 4.0;
    let elapsed = ELAPSED_MS.load(Ordering::Relaxed) as f32 / 1000.0;
    let pulse = ((elapsed * 2.0).sin() * 0.5 + 0.5) * 0.3 + 0.7;
    ctx.fill_rect(
        Rect::new(x + 4.0, y + 4.0, indicator_width, height - 8.0),
        CornerRadius::uniform(2.0),
        Brush::Solid(Color::rgba(1.0, 1.0, 1.0, pulse)),
    );
}
