//! Physics Demo
//!
//! This example demonstrates blinc_3d's physics utilities:
//! - PhysicsWorld configuration and presets
//! - RigidBody types (Dynamic, Kinematic, Static) and presets
//! - Collider shapes (Sphere, Box, Capsule, Cylinder, Cone, etc.)
//! - Material presets (bouncy, slippery, metal, wood, stone)
//! - Joint types (Fixed, Ball, Revolute, Prismatic, Distance, Rope)
//! - Physics queries (Ray, QueryFilter)
//!
//! Run with: cargo run -p blinc_3d --example physics_demo --features utils-rapier

use blinc_3d::ecs::{Entity, World};
use blinc_3d::geometry::{BoxGeometry, CylinderGeometry, SphereGeometry};
use blinc_3d::integration::render_scene;
use blinc_3d::lights::{AmbientLight, DirectionalLight, ShadowConfig};
use blinc_3d::materials::StandardMaterial;
use blinc_3d::prelude::*;
use blinc_3d::scene::{Mesh, Object3D, PerspectiveCamera};
use blinc_3d::utils::physics::*;
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::events::event_types;
use blinc_core::State;
use blinc_layout::stateful::ButtonState;
use blinc_layout::widgets::elapsed_ms;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Blinc 3D - Physics Demo".to_string(),
        width: 1400,
        height: 900,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

// ============================================================================
// Physics Demo Types
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum PhysicsCategory {
    #[default]
    Config,
    RigidBodies,
    Colliders,
    Joints,
    Queries,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum ConfigPreset {
    #[default]
    Default,
    ZeroGravity,
    Sidescroller,
    LowGravity,
    Custom,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum RigidBodyPreset {
    #[default]
    Dynamic,
    Kinematic,
    Static,
    PlayerCharacter,
    Projectile,
    Vehicle,
    Debris,
    Floating,
    Platform,
    Environment,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum ColliderPreset {
    #[default]
    Sphere,
    Cube,
    Cuboid,
    Capsule,
    Cylinder,
    Bouncy,
    Slippery,
    Metal,
    Wood,
    Stone,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum JointPreset {
    #[default]
    Fixed,
    Ball,
    Revolute,
    Prismatic,
    Spring,
    Rope,
    DoorHinge,
    WheelAxle,
    Suspension,
    ChainLink,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum QueryPreset {
    #[default]
    GroundCheck,
    PlayerInteraction,
    ProjectileHit,
    AllPhysics,
    TriggersOnly,
}

// ============================================================================
// Conversion Functions
// ============================================================================

impl ConfigPreset {
    fn to_config(&self) -> PhysicsConfig {
        match self {
            ConfigPreset::Default => PhysicsConfig::default(),
            ConfigPreset::ZeroGravity => PhysicsConfig::zero_gravity(),
            ConfigPreset::Sidescroller => PhysicsConfig::sidescroller(),
            ConfigPreset::LowGravity => PhysicsConfig::low_gravity(),
            ConfigPreset::Custom => PhysicsConfig::default()
                .with_gravity(Vec3::new(0.0, -5.0, 0.0))
                .with_timestep(1.0 / 120.0)
                .with_ccd(true)
                .with_solver_iterations(8),
        }
    }

    fn name(&self) -> &'static str {
        match self {
            ConfigPreset::Default => "Default",
            ConfigPreset::ZeroGravity => "Zero Gravity",
            ConfigPreset::Sidescroller => "Sidescroller",
            ConfigPreset::LowGravity => "Low Gravity (Moon)",
            ConfigPreset::Custom => "Custom",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            ConfigPreset::Default => "Earth gravity (-9.81 m/s²)",
            ConfigPreset::ZeroGravity => "Space simulation, no gravity",
            ConfigPreset::Sidescroller => "Increased gravity for platformers",
            ConfigPreset::LowGravity => "Moon gravity (-1.62 m/s²)",
            ConfigPreset::Custom => "Custom configuration",
        }
    }
}

impl RigidBodyPreset {
    fn to_body(&self) -> RigidBody {
        match self {
            RigidBodyPreset::Dynamic => RigidBody::dynamic(),
            RigidBodyPreset::Kinematic => RigidBody::kinematic(),
            RigidBodyPreset::Static => RigidBody::static_body(),
            RigidBodyPreset::PlayerCharacter => RigidBody::player_character(),
            RigidBodyPreset::Projectile => RigidBody::projectile(),
            RigidBodyPreset::Vehicle => RigidBody::vehicle(),
            RigidBodyPreset::Debris => RigidBody::debris(),
            RigidBodyPreset::Floating => RigidBody::floating(),
            RigidBodyPreset::Platform => RigidBody::platform(),
            RigidBodyPreset::Environment => RigidBody::environment(),
        }
    }

    fn name(&self) -> &'static str {
        match self {
            RigidBodyPreset::Dynamic => "Dynamic",
            RigidBodyPreset::Kinematic => "Kinematic",
            RigidBodyPreset::Static => "Static",
            RigidBodyPreset::PlayerCharacter => "Player Character",
            RigidBodyPreset::Projectile => "Projectile",
            RigidBodyPreset::Vehicle => "Vehicle",
            RigidBodyPreset::Debris => "Debris",
            RigidBodyPreset::Floating => "Floating",
            RigidBodyPreset::Platform => "Platform",
            RigidBodyPreset::Environment => "Environment",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            RigidBodyPreset::Dynamic => "Moves according to physics forces",
            RigidBodyPreset::Kinematic => "User-controlled, affects dynamic bodies",
            RigidBodyPreset::Static => "Never moves, infinite mass",
            RigidBodyPreset::PlayerCharacter => "Fixed rotation, responsive controls",
            RigidBodyPreset::Projectile => "Fast moving with CCD",
            RigidBodyPreset::Vehicle => "Heavy with damping",
            RigidBodyPreset::Debris => "Lightweight physics objects",
            RigidBodyPreset::Floating => "Reduced gravity effect",
            RigidBodyPreset::Platform => "Moving platform (kinematic)",
            RigidBodyPreset::Environment => "Static world geometry",
        }
    }
}

impl ColliderPreset {
    fn to_collider(&self) -> Collider {
        match self {
            ColliderPreset::Sphere => Collider::sphere(0.5),
            ColliderPreset::Cube => Collider::cube(0.5),
            ColliderPreset::Cuboid => Collider::cuboid(0.5, 0.25, 0.75),
            ColliderPreset::Capsule => Collider::capsule(0.5, 0.25),
            ColliderPreset::Cylinder => Collider::cylinder(0.5, 0.3),
            ColliderPreset::Bouncy => Collider::sphere(0.5).bouncy(),
            ColliderPreset::Slippery => Collider::cube(0.5).slippery(),
            ColliderPreset::Metal => Collider::sphere(0.5).metal(),
            ColliderPreset::Wood => Collider::cube(0.5).wood(),
            ColliderPreset::Stone => Collider::cube(0.5).stone(),
        }
    }

    fn name(&self) -> &'static str {
        match self {
            ColliderPreset::Sphere => "Sphere",
            ColliderPreset::Cube => "Cube",
            ColliderPreset::Cuboid => "Cuboid",
            ColliderPreset::Capsule => "Capsule",
            ColliderPreset::Cylinder => "Cylinder",
            ColliderPreset::Bouncy => "Bouncy (Rubber)",
            ColliderPreset::Slippery => "Slippery (Ice)",
            ColliderPreset::Metal => "Metal",
            ColliderPreset::Wood => "Wood",
            ColliderPreset::Stone => "Stone",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            ColliderPreset::Sphere => "Spherical collision shape",
            ColliderPreset::Cube => "Uniform box collision",
            ColliderPreset::Cuboid => "Non-uniform box shape",
            ColliderPreset::Capsule => "Cylinder with hemispheres",
            ColliderPreset::Cylinder => "Cylindrical collision",
            ColliderPreset::Bouncy => "High restitution (0.9)",
            ColliderPreset::Slippery => "Very low friction (0.05)",
            ColliderPreset::Metal => "Dense, moderate bounce",
            ColliderPreset::Wood => "Medium friction, light",
            ColliderPreset::Stone => "High friction, heavy",
        }
    }
}

impl JointPreset {
    fn name(&self) -> &'static str {
        match self {
            JointPreset::Fixed => "Fixed",
            JointPreset::Ball => "Ball",
            JointPreset::Revolute => "Revolute",
            JointPreset::Prismatic => "Prismatic",
            JointPreset::Spring => "Spring",
            JointPreset::Rope => "Rope",
            JointPreset::DoorHinge => "Door Hinge",
            JointPreset::WheelAxle => "Wheel Axle",
            JointPreset::Suspension => "Suspension",
            JointPreset::ChainLink => "Chain Link",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            JointPreset::Fixed => "Bodies move together",
            JointPreset::Ball => "Rotation around a point",
            JointPreset::Revolute => "Rotation around one axis",
            JointPreset::Prismatic => "Translation along one axis",
            JointPreset::Spring => "Distance with stiffness",
            JointPreset::Rope => "Maximum distance constraint",
            JointPreset::DoorHinge => "Limited rotation hinge",
            JointPreset::WheelAxle => "Free rotation axle",
            JointPreset::Suspension => "Vertical travel spring",
            JointPreset::ChainLink => "Rigid distance connection",
        }
    }
}

impl QueryPreset {
    fn name(&self) -> &'static str {
        match self {
            QueryPreset::GroundCheck => "Ground Check",
            QueryPreset::PlayerInteraction => "Player Interaction",
            QueryPreset::ProjectileHit => "Projectile Hit",
            QueryPreset::AllPhysics => "All Physics",
            QueryPreset::TriggersOnly => "Triggers Only",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            QueryPreset::GroundCheck => "Environment collision only",
            QueryPreset::PlayerInteraction => "Players and enemies",
            QueryPreset::ProjectileHit => "All damageable objects",
            QueryPreset::AllPhysics => "All physics bodies",
            QueryPreset::TriggersOnly => "Only sensor colliders",
        }
    }
}

// ============================================================================
// ECS World Creation
// ============================================================================

/// Creates a physics world with entities based on the current settings
fn create_physics_world(
    category: PhysicsCategory,
    config_preset: ConfigPreset,
    body_preset: RigidBodyPreset,
    collider_preset: ColliderPreset,
    joint_preset: JointPreset,
    time: f32,
) -> (World, Entity) {
    let mut world = World::new();

    // Insert physics resource
    let physics_config = config_preset.to_config();
    world.insert_resource(PhysicsWorld::new(physics_config.clone()));

    // Create ground entity
    let ground_geometry = BoxGeometry::new(6.0, 0.2, 6.0);
    let ground_material = StandardMaterial {
        color: Color::rgba(0.25, 0.25, 0.3, 1.0),
        metalness: 0.1,
        roughness: 0.9,
        ..Default::default()
    };

    let ground_geom_handle = world.add_geometry(ground_geometry);
    let ground_mat_handle = world.add_material(ground_material);

    let _ground = world
        .spawn()
        .insert(Object3D {
            position: Vec3::new(0.0, -1.5, 0.0),
            ..Default::default()
        })
        .insert(Mesh {
            geometry: ground_geom_handle,
            material: ground_mat_handle,
        })
        .insert(RigidBody::static_body())
        .insert(Collider::cuboid(3.0, 0.1, 3.0))
        .id();

    // Create entities based on category
    match category {
        PhysicsCategory::Config => {
            create_config_entities(&mut world, &physics_config, time);
        }
        PhysicsCategory::RigidBodies => {
            create_body_entities(&mut world, body_preset, time);
        }
        PhysicsCategory::Colliders => {
            create_collider_entities(&mut world, collider_preset, time);
        }
        PhysicsCategory::Joints => {
            create_joint_entities(&mut world, joint_preset, time);
        }
        PhysicsCategory::Queries => {
            create_query_entities(&mut world, time);
        }
    }

    // Create camera
    let camera = world
        .spawn()
        .insert(Object3D {
            position: Vec3::new(0.0, 2.0, 6.0),
            ..Default::default()
        })
        .insert(PerspectiveCamera::new(0.8, 16.0 / 9.0, 0.1, 100.0))
        .id();

    // Add lighting
    world.spawn().insert(AmbientLight {
        color: Color::WHITE,
        intensity: 0.3,
    });

    world
        .spawn()
        .insert(Object3D {
            position: Vec3::new(5.0, 8.0, 5.0),
            ..Default::default()
        })
        .insert(DirectionalLight {
            color: Color::WHITE,
            intensity: 1.0,
            cast_shadows: true,
            shadow: ShadowConfig::default(),
            shadow_camera_size: 20.0,
        });

    (world, camera)
}

fn create_config_entities(world: &mut World, config: &PhysicsConfig, time: f32) {
    // Visualize gravity with falling spheres
    let gravity_strength = (config.gravity.y.abs() / 9.81).min(2.0);
    let fall_offset = (time * gravity_strength) % 4.0;

    let colors = [
        Color::rgba(0.8, 0.4, 0.2, 1.0),
        Color::rgba(0.2, 0.6, 0.8, 1.0),
        Color::rgba(0.6, 0.8, 0.2, 1.0),
    ];

    for (i, color) in colors.iter().enumerate() {
        let x_pos = (i as f32 - 1.0) * 0.8;
        let y_pos = 2.0 - ((fall_offset + i as f32) % 4.0);

        let sphere_geo = SphereGeometry::new(0.3);
        let sphere_mat = StandardMaterial {
            color: *color,
            metalness: 0.3,
            roughness: 0.6,
            ..Default::default()
        };

        let geo_handle = world.add_geometry(sphere_geo);
        let mat_handle = world.add_material(sphere_mat);

        world
            .spawn()
            .insert(Object3D {
                position: Vec3::new(x_pos, y_pos, 0.0),
                ..Default::default()
            })
            .insert(Mesh {
                geometry: geo_handle,
                material: mat_handle,
            })
            .insert(RigidBody::dynamic())
            .insert(Collider::sphere(0.3));
    }
}

fn create_body_entities(world: &mut World, preset: RigidBodyPreset, time: f32) {
    let body = preset.to_body();

    let color = match body.body_type {
        RigidBodyType::Dynamic => Color::rgba(0.2, 0.6, 0.9, 1.0),
        RigidBodyType::Kinematic => Color::rgba(0.9, 0.6, 0.2, 1.0),
        RigidBodyType::Static => Color::rgba(0.5, 0.5, 0.5, 1.0),
    };

    let y_offset = match body.body_type {
        RigidBodyType::Dynamic => (time * 2.0).sin() * 0.5,
        RigidBodyType::Kinematic => (time * 1.5).sin() * 0.3,
        RigidBodyType::Static => 0.0,
    };

    let box_geo = BoxGeometry::new(0.8, 0.8, 0.8);
    let box_mat = StandardMaterial {
        color,
        metalness: 0.5,
        roughness: 0.4,
        ..Default::default()
    };

    let geo_handle = world.add_geometry(box_geo);
    let mat_handle = world.add_material(box_mat);

    world
        .spawn()
        .insert(Object3D {
            position: Vec3::new(0.0, y_offset, 0.0),
            rotation: Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), time * 0.5),
            ..Default::default()
        })
        .insert(Mesh {
            geometry: geo_handle,
            material: mat_handle,
        })
        .insert(body)
        .insert(Collider::cube(0.4));
}

fn create_collider_entities(world: &mut World, preset: ColliderPreset, time: f32) {
    let collider = preset.to_collider();
    let rotation = time * 0.5;

    // Create geometry based on shape
    let (geo_handle, color) = match &collider.shape {
        ColliderShape::Sphere { radius } => {
            let geo = SphereGeometry::new(*radius * 2.0);
            (
                world.add_geometry(geo),
                Color::rgba(0.3, 0.7, 0.9, 1.0),
            )
        }
        ColliderShape::Box { half_extents } => {
            let geo = BoxGeometry::new(
                half_extents.x * 2.0,
                half_extents.y * 2.0,
                half_extents.z * 2.0,
            );
            (
                world.add_geometry(geo),
                Color::rgba(0.9, 0.5, 0.3, 1.0),
            )
        }
        ColliderShape::Capsule { half_height, radius } => {
            // Approximate capsule with cylinder
            let geo = CylinderGeometry::cylinder(*radius * 2.0, *half_height * 2.0 + *radius * 2.0);
            (
                world.add_geometry(geo),
                Color::rgba(0.5, 0.8, 0.4, 1.0),
            )
        }
        ColliderShape::Cylinder { half_height, radius } => {
            let geo = CylinderGeometry::cylinder(*radius * 2.0, *half_height * 2.0);
            (
                world.add_geometry(geo),
                Color::rgba(0.8, 0.4, 0.7, 1.0),
            )
        }
        _ => {
            let geo = SphereGeometry::new(0.5);
            (
                world.add_geometry(geo),
                Color::rgba(0.5, 0.5, 0.5, 1.0),
            )
        }
    };

    let material = StandardMaterial {
        color,
        metalness: 0.4,
        roughness: 0.5,
        ..Default::default()
    };
    let mat_handle = world.add_material(material);

    world
        .spawn()
        .insert(Object3D {
            position: Vec3::ZERO,
            rotation: Quat::from_euler(rotation * 0.3, rotation, rotation * 0.2),
            ..Default::default()
        })
        .insert(Mesh {
            geometry: geo_handle,
            material: mat_handle,
        })
        .insert(RigidBody::dynamic())
        .insert(collider);
}

fn create_joint_entities(world: &mut World, preset: JointPreset, time: f32) {
    let (pos_a, pos_b) = match preset {
        JointPreset::Fixed | JointPreset::ChainLink => {
            let offset = (time * 0.5).sin() * 0.5;
            (
                Vec3::new(offset, 0.0, 0.0),
                Vec3::new(offset + 1.0, 0.0, 0.0),
            )
        }
        JointPreset::Ball
        | JointPreset::Revolute
        | JointPreset::DoorHinge
        | JointPreset::WheelAxle => {
            let angle = time * 1.5;
            (
                Vec3::ZERO,
                Vec3::new(angle.cos() * 1.2, angle.sin() * 0.5, 0.0),
            )
        }
        JointPreset::Prismatic | JointPreset::Suspension => {
            let offset = (time * 2.0).sin() * 0.8;
            (Vec3::ZERO, Vec3::new(0.0, offset, 0.0))
        }
        JointPreset::Spring | JointPreset::Rope => {
            let dist = 1.0 + (time * 3.0).sin() * 0.3;
            (
                Vec3::ZERO,
                Vec3::new(dist, (time * 2.0).sin() * 0.3, 0.0),
            )
        }
    };

    // Body A (red)
    let box_geo_a = BoxGeometry::new(0.5, 0.5, 0.5);
    let mat_a = StandardMaterial {
        color: Color::rgba(0.8, 0.3, 0.3, 1.0),
        metalness: 0.4,
        roughness: 0.5,
        ..Default::default()
    };

    let geo_a_handle = world.add_geometry(box_geo_a);
    let mat_a_handle = world.add_material(mat_a);

    let _body_a = world
        .spawn()
        .insert(Object3D {
            position: pos_a,
            ..Default::default()
        })
        .insert(Mesh {
            geometry: geo_a_handle,
            material: mat_a_handle,
        })
        .insert(RigidBody::static_body())
        .insert(Collider::cube(0.25))
        .id();

    // Body B (green)
    let box_geo_b = BoxGeometry::new(0.5, 0.5, 0.5);
    let mat_b = StandardMaterial {
        color: Color::rgba(0.3, 0.8, 0.3, 1.0),
        metalness: 0.4,
        roughness: 0.5,
        ..Default::default()
    };

    let geo_b_handle = world.add_geometry(box_geo_b);
    let mat_b_handle = world.add_material(mat_b);

    let _body_b = world
        .spawn()
        .insert(Object3D {
            position: pos_b,
            ..Default::default()
        })
        .insert(Mesh {
            geometry: geo_b_handle,
            material: mat_b_handle,
        })
        .insert(RigidBody::dynamic())
        .insert(Collider::cube(0.25))
        .id();
}

fn create_query_entities(world: &mut World, time: f32) {
    // Ray visualization (thin elongated box)
    let ray_length = 3.0 + (time * 0.5).sin() * 0.5;
    let ray_geo = BoxGeometry::new(ray_length, 0.02, 0.02);
    let ray_mat = StandardMaterial {
        color: Color::rgba(1.0, 0.8, 0.2, 1.0),
        emissive: Color::rgba(1.0, 0.8, 0.2, 0.5),
        metalness: 0.0,
        roughness: 1.0,
        ..Default::default()
    };

    let ray_geo_handle = world.add_geometry(ray_geo);
    let ray_mat_handle = world.add_material(ray_mat);

    world
        .spawn()
        .insert(Object3D {
            position: Vec3::new(-2.0 + ray_length * 0.5, 1.0, 0.0),
            rotation: Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), -0.3),
            ..Default::default()
        })
        .insert(Mesh {
            geometry: ray_geo_handle,
            material: ray_mat_handle,
        });

    // Target sphere
    let sphere_geo = SphereGeometry::new(0.4);
    let sphere_mat = StandardMaterial {
        color: Color::rgba(0.3, 0.7, 0.9, 1.0),
        metalness: 0.5,
        roughness: 0.4,
        ..Default::default()
    };

    let sphere_geo_handle = world.add_geometry(sphere_geo);
    let sphere_mat_handle = world.add_material(sphere_mat);

    world
        .spawn()
        .insert(Object3D {
            position: Vec3::new(1.0, 0.5, 0.0),
            ..Default::default()
        })
        .insert(Mesh {
            geometry: sphere_geo_handle,
            material: sphere_mat_handle,
        })
        .insert(RigidBody::dynamic())
        .insert(Collider::sphere(0.4));

    // Target box
    let box_geo = BoxGeometry::new(0.5, 0.5, 0.5);
    let box_mat = StandardMaterial {
        color: Color::rgba(0.9, 0.5, 0.3, 1.0),
        metalness: 0.4,
        roughness: 0.5,
        ..Default::default()
    };

    let box_geo_handle = world.add_geometry(box_geo);
    let box_mat_handle = world.add_material(box_mat);

    world
        .spawn()
        .insert(Object3D {
            position: Vec3::new(0.0, 0.0, 1.0),
            ..Default::default()
        })
        .insert(Mesh {
            geometry: box_geo_handle,
            material: box_mat_handle,
        })
        .insert(RigidBody::dynamic())
        .insert(Collider::cube(0.25));
}

// ============================================================================
// UI Building
// ============================================================================

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.05, 0.05, 0.08, 1.0))
        .flex_col()
        .p(16.0)
        .gap(16.0)
        // Header
        .child(header())
        // Main content
        .child(main_content(ctx.width, ctx.height - 80.0))
}

fn header() -> Div {
    div()
        .flex_col()
        .gap(4.0)
        .child(
            text("Blinc 3D - Physics Demo")
                .size(28.0)
                .color(Color::WHITE),
        )
        .child(
            text("RigidBody, Collider, Joint, and Query utilities")
                .size(14.0)
                .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
        )
}

fn main_content(width: f32, height: f32) -> impl ElementBuilder {
    div()
        .w(width)
        .h(height)
        .flex_row()
        .gap(16.0)
        // Left: Category tabs and selection
        .child(left_panel())
        // Center: Visualization
        .child(center_panel(width - 620.0, height))
        // Right: Details
        .child(right_panel())
}

fn left_panel() -> impl ElementBuilder {
    scroll()
        .w(280.0)
        .h_full()
        .bg(Color::rgba(0.1, 0.1, 0.12, 1.0))
        .rounded(8.0)
        .p(8.0)
        .child(
            div()
                .flex_col()
                .gap(12.0)
                // Category tabs
                .child(category_tabs())
                // Selection panel
                .child(selection_panel()),
        )
}

fn category_tabs() -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(|ctx| {
        let category = ctx.use_signal("category", || PhysicsCategory::Config);

        let categories = [
            (PhysicsCategory::Config, "Config"),
            (PhysicsCategory::RigidBodies, "Bodies"),
            (PhysicsCategory::Colliders, "Colliders"),
            (PhysicsCategory::Joints, "Joints"),
            (PhysicsCategory::Queries, "Queries"),
        ];

        let mut row = div().flex_row().flex_wrap().gap(4.0);

        for (cat, name) in categories {
            let is_selected = category.get() == cat;
            let bg = if is_selected {
                Color::rgba(0.3, 0.5, 0.8, 1.0)
            } else {
                Color::rgba(0.15, 0.15, 0.2, 1.0)
            };

            row = row.child(category_button(cat, name, bg, category.clone()));
        }

        row
    })
}

fn category_button(
    cat: PhysicsCategory,
    name: &'static str,
    bg: Color,
    category: State<PhysicsCategory>,
) -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(move |ctx| {
        let bg = if ctx.state() == ButtonState::Hovered {
            Color::rgba(0.25, 0.4, 0.7, 1.0)
        } else {
            bg
        };

        if let Some(event) = ctx.event() {
            if event.event_type == event_types::POINTER_UP {
                category.set(cat);
            }
        }

        div()
            .px(12.0)
            .py(8.0)
            .bg(bg)
            .rounded(6.0)
            .cursor_pointer()
            .child(text(name).size(12.0).color(Color::WHITE))
    })
}

fn selection_panel() -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(|ctx| {
        let category = ctx.use_signal("category", || PhysicsCategory::Config);
        let config_preset = ctx.use_signal("config_preset", || ConfigPreset::Default);
        let body_preset = ctx.use_signal("body_preset", || RigidBodyPreset::Dynamic);
        let collider_preset = ctx.use_signal("collider_preset", || ColliderPreset::Sphere);
        let joint_preset = ctx.use_signal("joint_preset", || JointPreset::Fixed);
        let query_preset = ctx.use_signal("query_preset", || QueryPreset::GroundCheck);

        match category.get() {
            PhysicsCategory::Config => preset_list_config(config_preset),
            PhysicsCategory::RigidBodies => preset_list_body(body_preset),
            PhysicsCategory::Colliders => preset_list_collider(collider_preset),
            PhysicsCategory::Joints => preset_list_joint(joint_preset),
            PhysicsCategory::Queries => preset_list_query(query_preset),
        }
    })
}

fn preset_list_config(selected: State<ConfigPreset>) -> Div {
    let presets = [
        ConfigPreset::Default,
        ConfigPreset::ZeroGravity,
        ConfigPreset::Sidescroller,
        ConfigPreset::LowGravity,
        ConfigPreset::Custom,
    ];
    preset_list(presets.to_vec(), selected, |p| p.name(), |p| p.description())
}

fn preset_list_body(selected: State<RigidBodyPreset>) -> Div {
    let presets = [
        RigidBodyPreset::Dynamic,
        RigidBodyPreset::Kinematic,
        RigidBodyPreset::Static,
        RigidBodyPreset::PlayerCharacter,
        RigidBodyPreset::Projectile,
        RigidBodyPreset::Vehicle,
        RigidBodyPreset::Debris,
        RigidBodyPreset::Floating,
        RigidBodyPreset::Platform,
        RigidBodyPreset::Environment,
    ];
    preset_list(presets.to_vec(), selected, |p| p.name(), |p| p.description())
}

fn preset_list_collider(selected: State<ColliderPreset>) -> Div {
    let presets = [
        ColliderPreset::Sphere,
        ColliderPreset::Cube,
        ColliderPreset::Cuboid,
        ColliderPreset::Capsule,
        ColliderPreset::Cylinder,
        ColliderPreset::Bouncy,
        ColliderPreset::Slippery,
        ColliderPreset::Metal,
        ColliderPreset::Wood,
        ColliderPreset::Stone,
    ];
    preset_list(presets.to_vec(), selected, |p| p.name(), |p| p.description())
}

fn preset_list_joint(selected: State<JointPreset>) -> Div {
    let presets = [
        JointPreset::Fixed,
        JointPreset::Ball,
        JointPreset::Revolute,
        JointPreset::Prismatic,
        JointPreset::Spring,
        JointPreset::Rope,
        JointPreset::DoorHinge,
        JointPreset::WheelAxle,
        JointPreset::Suspension,
        JointPreset::ChainLink,
    ];
    preset_list(presets.to_vec(), selected, |p| p.name(), |p| p.description())
}

fn preset_list_query(selected: State<QueryPreset>) -> Div {
    let presets = [
        QueryPreset::GroundCheck,
        QueryPreset::PlayerInteraction,
        QueryPreset::ProjectileHit,
        QueryPreset::AllPhysics,
        QueryPreset::TriggersOnly,
    ];
    preset_list(presets.to_vec(), selected, |p| p.name(), |p| p.description())
}

fn preset_list<T: Copy + PartialEq + Send + Sync + Default + 'static>(
    presets: Vec<T>,
    selected: State<T>,
    name_fn: fn(&T) -> &'static str,
    desc_fn: fn(&T) -> &'static str,
) -> Div {
    let mut col = div().flex_col().gap(4.0);

    for preset in presets {
        col = col.child(preset_button(preset, selected.clone(), name_fn, desc_fn));
    }

    col
}

fn preset_button<T: Copy + PartialEq + Send + Sync + Default + 'static>(
    preset: T,
    selected: State<T>,
    name_fn: fn(&T) -> &'static str,
    desc_fn: fn(&T) -> &'static str,
) -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(move |ctx| {
        let is_selected = selected.get() == preset;
        let bg = if is_selected {
            Color::rgba(0.2, 0.4, 0.6, 1.0)
        } else if ctx.state() == ButtonState::Hovered {
            Color::rgba(0.18, 0.18, 0.22, 1.0)
        } else {
            Color::rgba(0.12, 0.12, 0.15, 1.0)
        };

        if let Some(event) = ctx.event() {
            if event.event_type == event_types::POINTER_UP {
                selected.set(preset);
            }
        }

        div()
            .w_full()
            .p(8.0)
            .bg(bg)
            .rounded(6.0)
            .cursor_pointer()
            .flex_col()
            .gap(2.0)
            .child(text(name_fn(&preset)).size(13.0).color(Color::WHITE))
            .child(
                text(desc_fn(&preset))
                    .size(10.0)
                    .color(Color::rgba(0.5, 0.5, 0.6, 1.0)),
            )
    })
}

fn center_panel(width: f32, height: f32) -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(move |ctx| {
        // Get current selections from signals
        let category = ctx.use_signal("category", || PhysicsCategory::Config);
        let config_preset = ctx.use_signal("config_preset", || ConfigPreset::Default);
        let body_preset = ctx.use_signal("body_preset", || RigidBodyPreset::Dynamic);
        let collider_preset = ctx.use_signal("collider_preset", || ColliderPreset::Sphere);
        let joint_preset = ctx.use_signal("joint_preset", || JointPreset::Fixed);

        // Get current values
        let cat = category.get();
        // Use elapsed_ms for animation - doesn't trigger re-renders
        let t = elapsed_ms() as f32 / 1000.0;

        // Create ECS World with physics entities
        let (world, camera_entity) = create_physics_world(
            cat,
            config_preset.get(),
            body_preset.get(),
            collider_preset.get(),
            joint_preset.get(),
            t,
        );

        div()
            .w(width)
            .h(height)
            .bg(Color::rgba(0.08, 0.08, 0.1, 1.0))
            .rounded(8.0)
            .overflow_clip()
            .child(
                canvas(move |draw_ctx, bounds| {
                    // Render using proper ECS pipeline
                    render_scene(draw_ctx, &world, camera_entity, bounds);
                })
                .w_full()
                .h_full(),
            )
    })
}

fn right_panel() -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(|ctx| {
        let category = ctx.use_signal("category", || PhysicsCategory::Config);
        let config_preset = ctx.use_signal("config_preset", || ConfigPreset::Default);
        let body_preset = ctx.use_signal("body_preset", || RigidBodyPreset::Dynamic);
        let collider_preset = ctx.use_signal("collider_preset", || ColliderPreset::Sphere);
        let joint_preset = ctx.use_signal("joint_preset", || JointPreset::Fixed);
        let query_preset = ctx.use_signal("query_preset", || QueryPreset::GroundCheck);

        let content = match category.get() {
            PhysicsCategory::Config => config_details(config_preset.get()),
            PhysicsCategory::RigidBodies => body_details(body_preset.get()),
            PhysicsCategory::Colliders => collider_details(collider_preset.get()),
            PhysicsCategory::Joints => joint_details(joint_preset.get()),
            PhysicsCategory::Queries => query_details(query_preset.get()),
        };

        div()
            .w(320.0)
            .h_full()
            .child(
                scroll()
                    .w_full()
                    .h_full()
                    .bg(Color::rgba(0.1, 0.1, 0.12, 1.0))
                    .rounded(8.0)
                    .p(12.0)
                    .child(content)
            )
    })
}

// ============================================================================
// Detail Panels
// ============================================================================

fn config_details(preset: ConfigPreset) -> Div {
    let config = preset.to_config();

    div()
        .flex_col()
        .gap(12.0)
        .child(section_header("PhysicsConfig"))
        .child(property_row("Gravity", &format!("{:?}", config.gravity)))
        .child(property_row("Timestep", &format!("{:.4}s", config.timestep)))
        .child(property_row(
            "Max Substeps",
            &config.max_substeps.to_string(),
        ))
        .child(property_row(
            "CCD Enabled",
            &config.ccd_enabled.to_string(),
        ))
        .child(property_row(
            "Solver Iterations",
            &config.solver_iterations.to_string(),
        ))
        .child(divider())
        .child(section_header("Usage"))
        .child(code_block(&format!(
            "let config = PhysicsConfig::{}();\nlet mut physics = PhysicsWorld::new(config);",
            match preset {
                ConfigPreset::Default => "default",
                ConfigPreset::ZeroGravity => "zero_gravity",
                ConfigPreset::Sidescroller => "sidescroller",
                ConfigPreset::LowGravity => "low_gravity",
                ConfigPreset::Custom => "default().with_gravity(...)",
            }
        )))
}

fn body_details(preset: RigidBodyPreset) -> Div {
    let body = preset.to_body();

    div()
        .flex_col()
        .gap(12.0)
        .child(section_header("RigidBody"))
        .child(property_row("Type", &format!("{:?}", body.body_type)))
        .child(property_row("Mass", &format!("{:.1} kg", body.mass)))
        .child(property_row(
            "Linear Damping",
            &format!("{:.2}", body.linear_damping),
        ))
        .child(property_row(
            "Angular Damping",
            &format!("{:.2}", body.angular_damping),
        ))
        .child(property_row(
            "Gravity Scale",
            &format!("{:.2}", body.gravity_scale),
        ))
        .child(property_row("Can Sleep", &body.can_sleep.to_string()))
        .child(property_row("CCD Enabled", &body.ccd_enabled.to_string()))
        .child(property_row(
            "Lock Position",
            &format!("{:?}", body.lock_position),
        ))
        .child(property_row(
            "Lock Rotation",
            &format!("{:?}", body.lock_rotation),
        ))
        .child(divider())
        .child(section_header("Usage"))
        .child(code_block(&format!(
            "let body = RigidBody::{}();",
            match preset {
                RigidBodyPreset::Dynamic => "dynamic",
                RigidBodyPreset::Kinematic => "kinematic",
                RigidBodyPreset::Static => "static_body",
                RigidBodyPreset::PlayerCharacter => "player_character",
                RigidBodyPreset::Projectile => "projectile",
                RigidBodyPreset::Vehicle => "vehicle",
                RigidBodyPreset::Debris => "debris",
                RigidBodyPreset::Floating => "floating",
                RigidBodyPreset::Platform => "platform",
                RigidBodyPreset::Environment => "environment",
            }
        )))
}

fn collider_details(preset: ColliderPreset) -> Div {
    let collider = preset.to_collider();

    let shape_desc = match &collider.shape {
        ColliderShape::Sphere { radius } => format!("Sphere(r={})", radius),
        ColliderShape::Box { half_extents } => format!("Box({:?})", half_extents),
        ColliderShape::Capsule { half_height, radius } => {
            format!("Capsule(h={}, r={})", half_height, radius)
        }
        ColliderShape::Cylinder { half_height, radius } => {
            format!("Cylinder(h={}, r={})", half_height, radius)
        }
        _ => "Other".to_string(),
    };

    div()
        .flex_col()
        .gap(12.0)
        .child(section_header("Collider"))
        .child(property_row("Shape", &shape_desc))
        .child(property_row(
            "Friction",
            &format!("{:.2}", collider.friction),
        ))
        .child(property_row(
            "Restitution",
            &format!("{:.2}", collider.restitution),
        ))
        .child(property_row("Density", &format!("{:.2}", collider.density)))
        .child(property_row("Is Sensor", &collider.is_sensor.to_string()))
        .child(divider())
        .child(section_header("Usage"))
        .child(code_block(&format!(
            "let collider = Collider::{};",
            match preset {
                ColliderPreset::Sphere => "sphere(0.5)",
                ColliderPreset::Cube => "cube(0.5)",
                ColliderPreset::Cuboid => "cuboid(0.5, 0.25, 0.75)",
                ColliderPreset::Capsule => "capsule(0.5, 0.25)",
                ColliderPreset::Cylinder => "cylinder(0.5, 0.3)",
                ColliderPreset::Bouncy => "sphere(0.5).bouncy()",
                ColliderPreset::Slippery => "cube(0.5).slippery()",
                ColliderPreset::Metal => "sphere(0.5).metal()",
                ColliderPreset::Wood => "cube(0.5).wood()",
                ColliderPreset::Stone => "cube(0.5).stone()",
            }
        )))
}

fn joint_details(preset: JointPreset) -> Div {
    let (joint_type, code_example) = match preset {
        JointPreset::Fixed => ("Fixed", "Joint::fixed(anchor_a, anchor_b)"),
        JointPreset::Ball => ("Ball (Spherical)", "Joint::ball(anchor_a, anchor_b)"),
        JointPreset::Revolute => {
            ("Revolute (Hinge)", "Joint::revolute(anchor_a, anchor_b, axis)")
        }
        JointPreset::Prismatic => (
            "Prismatic (Slider)",
            "Joint::prismatic(anchor_a, anchor_b, axis)",
        ),
        JointPreset::Spring => (
            "Distance/Spring",
            "Joint::spring(anchor_a, anchor_b, stiffness, damping)",
        ),
        JointPreset::Rope => ("Rope", "Joint::rope(anchor_a, anchor_b, max_length)"),
        JointPreset::DoorHinge => ("Door Hinge", "Joint::door_hinge(anchor)"),
        JointPreset::WheelAxle => ("Wheel Axle", "Joint::wheel_axle(anchor)"),
        JointPreset::Suspension => ("Suspension", "Joint::suspension(anchor, travel)"),
        JointPreset::ChainLink => ("Chain Link", "Joint::chain_link(length)"),
    };

    div()
        .flex_col()
        .gap(12.0)
        .child(section_header("Joint"))
        .child(property_row("Type", joint_type))
        .child(property_row("Description", preset.description()))
        .child(divider())
        .child(section_header("Common Properties"))
        .child(property_row("anchor_a", "Local anchor on body A"))
        .child(property_row("anchor_b", "Local anchor on body B"))
        .child(divider())
        .child(section_header("Usage"))
        .child(code_block(code_example))
}

fn query_details(preset: QueryPreset) -> Div {
    let (filter_code, description) = match preset {
        QueryPreset::GroundCheck => (
            "query_presets::ground_check()",
            "Filters for environment collision group only",
        ),
        QueryPreset::PlayerInteraction => (
            "query_presets::player_interaction()",
            "Filters for player and enemy groups",
        ),
        QueryPreset::ProjectileHit => (
            "query_presets::projectile_hit()",
            "Filters for damageable objects",
        ),
        QueryPreset::AllPhysics => (
            "query_presets::all_physics()",
            "No filtering, returns all physics bodies",
        ),
        QueryPreset::TriggersOnly => (
            "query_presets::triggers_only()",
            "Only sensor/trigger colliders",
        ),
    };

    div()
        .flex_col()
        .gap(12.0)
        .child(section_header("QueryFilter"))
        .child(property_row("Preset", preset.name()))
        .child(property_row("Description", description))
        .child(divider())
        .child(section_header("Raycast Usage"))
        .child(code_block(&format!(
            "let ray = Ray::new(origin, direction);\nlet filter = {};\nif let Some(hit) = physics.raycast_filtered(&ray, 100.0, filter) {{\n    // hit.entity, hit.position, hit.normal\n}}",
            filter_code
        )))
        .child(divider())
        .child(section_header("Collision Groups"))
        .child(property_row("DEFAULT", "1"))
        .child(property_row("ENVIRONMENT", "2"))
        .child(property_row("PLAYER", "4"))
        .child(property_row("ENEMY", "8"))
        .child(property_row("PROJECTILE", "16"))
        .child(property_row("TRIGGER", "32"))
}

// ============================================================================
// UI Helpers
// ============================================================================

fn section_header(title: &str) -> Div {
    div()
        .pb(4.0)
        .mb(4.0)
        .border_bottom(1.0, Color::rgba(0.3, 0.3, 0.35, 1.0))
        .child(text(title).size(14.0).color(Color::WHITE).bold())
}

fn property_row(label: &str, value: &str) -> Div {
    div()
        .flex_row()
        .justify_between()
        .child(
            text(label)
                .size(11.0)
                .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
        )
        .child(
            text(value)
                .size(11.0)
                .color(Color::rgba(0.8, 0.8, 0.9, 1.0)),
        )
}

fn divider() -> Div {
    div()
        .w_full()
        .h(1.0)
        .bg(Color::rgba(0.2, 0.2, 0.25, 1.0))
        .my(8.0)
}

fn code_block(code_str: &str) -> Div {
    div()
        .w_full()
        .p(8.0)
        .bg(Color::rgba(0.05, 0.05, 0.08, 1.0))
        .rounded(4.0)
        .child(
            text(code_str)
                .size(10.0)
                .color(Color::rgba(0.7, 0.9, 0.7, 1.0)),
        )
}
