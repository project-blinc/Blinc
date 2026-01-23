//! Materials Demo
//!
//! This example demonstrates blinc_3d's material system:
//! - BasicMaterial (unlit, solid color)
//! - StandardMaterial (PBR with metalness/roughness)
//! - PhongMaterial (classic specular highlights)
//! - Material properties visualization
//!
//! Run with: cargo run -p blinc_3d --example materials_demo

use blinc_3d::prelude::*;
use blinc_3d::integration::{render_scene, CanvasBounds, CanvasBoundsExt, RenderConfig};
use blinc_3d::materials::{BasicMaterial, PhongMaterial, StandardMaterial, Side};
use blinc_3d::lights::{AmbientLight, DirectionalLight, PointLight};
use blinc_3d::scene::{Object3D, Mesh, PerspectiveCamera};
use blinc_3d::geometry::SphereGeometry;
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use std::f32::consts::PI;
use std::sync::{Arc, Mutex};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Blinc 3D - Materials Demo".to_string(),
        width: 1100,
        height: 750,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

// ============================================================================
// Material Definitions
// ============================================================================

struct MaterialDemo {
    name: &'static str,
    mat_type: &'static str,
    description: &'static str,
}

fn get_material_demos() -> Vec<MaterialDemo> {
    vec![
        MaterialDemo {
            name: "Basic Red",
            mat_type: "BasicMaterial",
            description: "Unlit solid color",
        },
        MaterialDemo {
            name: "Polished Metal",
            mat_type: "StandardMaterial",
            description: "metalness=1.0, roughness=0.1",
        },
        MaterialDemo {
            name: "Brushed Metal",
            mat_type: "StandardMaterial",
            description: "metalness=0.9, roughness=0.4",
        },
        MaterialDemo {
            name: "Copper",
            mat_type: "StandardMaterial",
            description: "Warm metal tones",
        },
        MaterialDemo {
            name: "Gold",
            mat_type: "StandardMaterial",
            description: "Rich metallic gold",
        },
        MaterialDemo {
            name: "Plastic",
            mat_type: "StandardMaterial",
            description: "metalness=0.0, roughness=0.3",
        },
        MaterialDemo {
            name: "Rubber",
            mat_type: "StandardMaterial",
            description: "metalness=0.0, roughness=0.9",
        },
        MaterialDemo {
            name: "Phong Shiny",
            mat_type: "PhongMaterial",
            description: "shininess=64",
        },
        MaterialDemo {
            name: "Emissive Glow",
            mat_type: "StandardMaterial",
            description: "Self-illuminating material",
        },
        MaterialDemo {
            name: "Glass",
            mat_type: "StandardMaterial",
            description: "Transparent, low roughness",
        },
        MaterialDemo {
            name: "Wood",
            mat_type: "StandardMaterial",
            description: "Dielectric, medium rough",
        },
        MaterialDemo {
            name: "Basic Wireframe",
            mat_type: "BasicMaterial",
            description: "Wireframe rendering mode",
        },
    ]
}

// ============================================================================
// Scene Setup - Creates world with spheres using different materials
// ============================================================================

fn create_materials_world() -> (World, Entity) {
    let mut world = World::new();

    // Add ambient light for base illumination
    world.spawn()
        .insert(AmbientLight {
            color: Color::WHITE,
            intensity: 0.2,
        });

    // Add main directional light (sun-like)
    let mut sun_transform = Object3D::default().with_position(5.0, 10.0, 5.0);
    sun_transform.look_at(Vec3::ZERO);
    world.spawn()
        .insert(sun_transform)
        .insert(DirectionalLight::sun().intensity(1.0));

    // Add fill light from the side
    world.spawn()
        .insert(Object3D::default().with_position(-5.0, 3.0, 2.0))
        .insert(PointLight::white(0.5).distance(20.0));

    // Create sphere geometry once, reuse for all materials
    let sphere_geom = world.add_geometry(SphereGeometry::with_detail(0.5, 32, 24));

    // Material 1: Basic Red (unlit)
    let basic_red = world.add_material(BasicMaterial::with_color(Color::rgb(0.9, 0.2, 0.2)));
    world.spawn()
        .insert(Object3D::default().with_position(-3.0, 2.0, 0.0))
        .insert(Mesh::new(sphere_geom, basic_red));

    // Material 2: Polished Metal
    let polished_metal = world.add_material(StandardMaterial::metal(
        Color::rgb(0.8, 0.8, 0.85),
        0.1, // Very smooth
    ));
    world.spawn()
        .insert(Object3D::default().with_position(-1.5, 2.0, 0.0))
        .insert(Mesh::new(sphere_geom, polished_metal));

    // Material 3: Brushed Metal
    let brushed_metal = world.add_material(StandardMaterial::metal(
        Color::rgb(0.7, 0.7, 0.75),
        0.4, // Rougher surface
    ));
    world.spawn()
        .insert(Object3D::default().with_position(0.0, 2.0, 0.0))
        .insert(Mesh::new(sphere_geom, brushed_metal));

    // Material 4: Copper
    let copper = world.add_material(StandardMaterial::metal(
        Color::rgb(0.95, 0.64, 0.54),
        0.3,
    ));
    world.spawn()
        .insert(Object3D::default().with_position(1.5, 2.0, 0.0))
        .insert(Mesh::new(sphere_geom, copper));

    // Material 5: Gold
    let gold = world.add_material(StandardMaterial::metal(
        Color::rgb(1.0, 0.84, 0.0),
        0.2,
    ));
    world.spawn()
        .insert(Object3D::default().with_position(3.0, 2.0, 0.0))
        .insert(Mesh::new(sphere_geom, gold));

    // Material 6: Plastic (dielectric)
    let plastic = world.add_material(StandardMaterial::plastic(
        Color::rgb(0.2, 0.5, 0.9),
        0.3,
    ));
    world.spawn()
        .insert(Object3D::default().with_position(-3.0, 0.0, 0.0))
        .insert(Mesh::new(sphere_geom, plastic));

    // Material 7: Rubber (rough dielectric)
    let rubber = world.add_material(StandardMaterial::plastic(
        Color::rgb(0.1, 0.1, 0.1),
        0.9, // Very rough
    ));
    world.spawn()
        .insert(Object3D::default().with_position(-1.5, 0.0, 0.0))
        .insert(Mesh::new(sphere_geom, rubber));

    // Material 8: Phong Shiny
    let phong_shiny = world.add_material(PhongMaterial::with_color(Color::rgb(0.9, 0.3, 0.6))
        .specular(Color::WHITE)
        .shininess(64.0));
    world.spawn()
        .insert(Object3D::default().with_position(0.0, 0.0, 0.0))
        .insert(Mesh::new(sphere_geom, phong_shiny));

    // Material 9: Emissive
    let emissive = world.add_material(StandardMaterial {
        color: Color::rgb(0.1, 0.1, 0.1),
        metalness: 0.0,
        roughness: 0.5,
        emissive: Color::rgb(0.0, 1.0, 0.5),
        emissive_intensity: 2.0,
        ..Default::default()
    });
    world.spawn()
        .insert(Object3D::default().with_position(1.5, 0.0, 0.0))
        .insert(Mesh::new(sphere_geom, emissive));

    // Material 10: Glass-like (transparent)
    let glass = world.add_material(StandardMaterial {
        color: Color::rgba(0.9, 0.95, 1.0, 0.3),
        metalness: 0.0,
        roughness: 0.05,
        transparent: true,
        ..Default::default()
    });
    world.spawn()
        .insert(Object3D::default().with_position(3.0, 0.0, 0.0))
        .insert(Mesh::new(sphere_geom, glass));

    // Material 11: Wood-like
    let wood = world.add_material(StandardMaterial::plastic(
        Color::rgb(0.55, 0.35, 0.2),
        0.7,
    ));
    world.spawn()
        .insert(Object3D::default().with_position(-1.5, -2.0, 0.0))
        .insert(Mesh::new(sphere_geom, wood));

    // Material 12: Wireframe
    let wireframe = world.add_material(BasicMaterial::with_color(Color::rgb(0.3, 1.0, 0.8))
        .wireframe());
    world.spawn()
        .insert(Object3D::default().with_position(1.5, -2.0, 0.0))
        .insert(Mesh::new(sphere_geom, wireframe));

    // Create camera
    let mut camera_transform = Object3D::default().with_position(0.0, 0.0, 8.0);
    camera_transform.look_at(Vec3::ZERO);
    let camera = world.spawn()
        .insert(camera_transform)
        .insert(PerspectiveCamera::new(PI / 4.0, 1.5, 0.1, 100.0))
        .id();

    (world, camera)
}

// ============================================================================
// UI Building
// ============================================================================

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    let (world, camera) = create_materials_world();
    let world = Arc::new(Mutex::new(world));
    let demos = get_material_demos();

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.06, 0.06, 0.1, 1.0))
        .flex_col()
        .p(16.0)
        .gap(16.0)
        // Header
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(
                    text("Blinc 3D - Materials System")
                        .size(28.0)
                        .color(Color::WHITE),
                )
                .child(
                    text("PBR materials with metalness, roughness, and emissive properties")
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
                .child(materials_viewport(world.clone(), camera))
                // Properties panel
                .child(properties_panel(demos)),
        )
}

fn materials_viewport(world: Arc<Mutex<World>>, camera: Entity) -> Canvas {
    canvas(move |ctx, bounds| {
        let world = world.lock().unwrap();

        // Use the actual render_scene integration
        render_scene(
            ctx,
            &world,
            camera,
            bounds,
        );
    })
    .flex_grow()
    .h_full()
}

fn properties_panel(demos: Vec<MaterialDemo>) -> Div {
    let panel = div()
        .w(280.0)
        .h_full()
        .bg(Color::rgba(0.1, 0.1, 0.14, 1.0))
        .rounded(8.0)
        .p(16.0)
        .flex_col()
        .gap(16.0)
        // Material Types header
        .child(text("Material Types").size(16.0).color(Color::WHITE))
        .child(
            div()
                .flex_col()
                .gap(8.0)
                .child(material_type_info(
                    "BasicMaterial",
                    "Unlit, solid color only",
                    Color::rgb(0.3, 0.6, 0.9),
                ))
                .child(material_type_info(
                    "StandardMaterial",
                    "PBR with metalness/roughness",
                    Color::rgb(0.9, 0.6, 0.3),
                ))
                .child(material_type_info(
                    "PhongMaterial",
                    "Classic specular highlights",
                    Color::rgb(0.9, 0.3, 0.6),
                )),
        )
        // PBR Properties header
        .child(text("PBR Properties").size(16.0).color(Color::WHITE))
        .child(
            div()
                .flex_col()
                .gap(8.0)
                .child(property_info("metalness", "0.0 - 1.0", "Metal vs dielectric"))
                .child(property_info("roughness", "0.0 - 1.0", "Surface smoothness"))
                .child(property_info("emissive", "Color", "Self-illumination")),
        )
        // Rendering info
        .child(text("GPU Rendering").size(16.0).color(Color::WHITE))
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(render_info("Cook-Torrance BRDF"))
                .child(render_info("GGX distribution"))
                .child(render_info("Schlick Fresnel"))
                .child(render_info("Real-time GPU shaders")),
        );

    panel
}

fn material_type_info(name: &'static str, desc: &'static str, color: Color) -> Div {
    div()
        .flex_row()
        .gap(10.0)
        .items_center()
        .child(div().w(12.0).h(12.0).rounded(6.0).bg(color))
        .child(
            div()
                .flex_col()
                .gap(2.0)
                .child(text(name).size(12.0).color(Color::WHITE))
                .child(
                    text(desc)
                        .size(10.0)
                        .color(Color::rgba(0.5, 0.5, 0.6, 1.0)),
                ),
        )
}

fn property_info(name: &'static str, range: &'static str, desc: &'static str) -> Div {
    div()
        .flex_col()
        .gap(2.0)
        .child(
            div()
                .flex_row()
                .justify_between()
                .child(text(name).size(11.0).color(Color::rgba(0.8, 0.8, 0.9, 1.0)))
                .child(
                    text(range)
                        .size(10.0)
                        .color(Color::rgba(0.5, 0.7, 1.0, 0.8)),
                ),
        )
        .child(
            text(desc)
                .size(10.0)
                .color(Color::rgba(0.5, 0.5, 0.6, 1.0)),
        )
}

fn render_info(info: &'static str) -> Div {
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
            text(info)
                .size(11.0)
                .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
        )
}
