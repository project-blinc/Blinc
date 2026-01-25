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

use blinc_3d::ecs::{Entity, System, SystemContext, SystemStage, World};
use blinc_3d::geometry::{BoxGeometry, CylinderGeometry, SphereGeometry};
use blinc_3d::integration::render_scene_with_time;
use blinc_3d::lights::{AmbientLight, DirectionalLight, ShadowConfig};
use blinc_3d::materials::StandardMaterial;
use blinc_3d::prelude::*;
use blinc_3d::scene::{Mesh, Object3D, PerspectiveCamera};
use blinc_3d::utils::physics::*;
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_cn::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

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

impl PhysicsCategory {
    fn to_key(&self) -> &'static str {
        match self {
            PhysicsCategory::Config => "config",
            PhysicsCategory::RigidBodies => "bodies",
            PhysicsCategory::Colliders => "colliders",
            PhysicsCategory::Joints => "joints",
            PhysicsCategory::Queries => "queries",
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "config" => PhysicsCategory::Config,
            "bodies" => PhysicsCategory::RigidBodies,
            "colliders" => PhysicsCategory::Colliders,
            "joints" => PhysicsCategory::Joints,
            "queries" => PhysicsCategory::Queries,
            _ => PhysicsCategory::Config,
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            PhysicsCategory::Config => "Configuration",
            PhysicsCategory::RigidBodies => "Rigid Bodies",
            PhysicsCategory::Colliders => "Colliders",
            PhysicsCategory::Joints => "Joints",
            PhysicsCategory::Queries => "Queries",
        }
    }
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

    fn to_key(&self) -> &'static str {
        match self {
            ConfigPreset::Default => "default",
            ConfigPreset::ZeroGravity => "zero_gravity",
            ConfigPreset::Sidescroller => "sidescroller",
            ConfigPreset::LowGravity => "low_gravity",
            ConfigPreset::Custom => "custom",
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "default" => ConfigPreset::Default,
            "zero_gravity" => ConfigPreset::ZeroGravity,
            "sidescroller" => ConfigPreset::Sidescroller,
            "low_gravity" => ConfigPreset::LowGravity,
            "custom" => ConfigPreset::Custom,
            _ => ConfigPreset::Default,
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

    fn to_key(&self) -> &'static str {
        match self {
            RigidBodyPreset::Dynamic => "dynamic",
            RigidBodyPreset::Kinematic => "kinematic",
            RigidBodyPreset::Static => "static",
            RigidBodyPreset::PlayerCharacter => "player",
            RigidBodyPreset::Projectile => "projectile",
            RigidBodyPreset::Vehicle => "vehicle",
            RigidBodyPreset::Debris => "debris",
            RigidBodyPreset::Floating => "floating",
            RigidBodyPreset::Platform => "platform",
            RigidBodyPreset::Environment => "environment",
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "dynamic" => RigidBodyPreset::Dynamic,
            "kinematic" => RigidBodyPreset::Kinematic,
            "static" => RigidBodyPreset::Static,
            "player" => RigidBodyPreset::PlayerCharacter,
            "projectile" => RigidBodyPreset::Projectile,
            "vehicle" => RigidBodyPreset::Vehicle,
            "debris" => RigidBodyPreset::Debris,
            "floating" => RigidBodyPreset::Floating,
            "platform" => RigidBodyPreset::Platform,
            "environment" => RigidBodyPreset::Environment,
            _ => RigidBodyPreset::Dynamic,
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

    fn to_key(&self) -> &'static str {
        match self {
            ColliderPreset::Sphere => "sphere",
            ColliderPreset::Cube => "cube",
            ColliderPreset::Cuboid => "cuboid",
            ColliderPreset::Capsule => "capsule",
            ColliderPreset::Cylinder => "cylinder",
            ColliderPreset::Bouncy => "bouncy",
            ColliderPreset::Slippery => "slippery",
            ColliderPreset::Metal => "metal",
            ColliderPreset::Wood => "wood",
            ColliderPreset::Stone => "stone",
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "sphere" => ColliderPreset::Sphere,
            "cube" => ColliderPreset::Cube,
            "cuboid" => ColliderPreset::Cuboid,
            "capsule" => ColliderPreset::Capsule,
            "cylinder" => ColliderPreset::Cylinder,
            "bouncy" => ColliderPreset::Bouncy,
            "slippery" => ColliderPreset::Slippery,
            "metal" => ColliderPreset::Metal,
            "wood" => ColliderPreset::Wood,
            "stone" => ColliderPreset::Stone,
            _ => ColliderPreset::Sphere,
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

    fn to_key(&self) -> &'static str {
        match self {
            JointPreset::Fixed => "fixed",
            JointPreset::Ball => "ball",
            JointPreset::Revolute => "revolute",
            JointPreset::Prismatic => "prismatic",
            JointPreset::Spring => "spring",
            JointPreset::Rope => "rope",
            JointPreset::DoorHinge => "door_hinge",
            JointPreset::WheelAxle => "wheel_axle",
            JointPreset::Suspension => "suspension",
            JointPreset::ChainLink => "chain_link",
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "fixed" => JointPreset::Fixed,
            "ball" => JointPreset::Ball,
            "revolute" => JointPreset::Revolute,
            "prismatic" => JointPreset::Prismatic,
            "spring" => JointPreset::Spring,
            "rope" => JointPreset::Rope,
            "door_hinge" => JointPreset::DoorHinge,
            "wheel_axle" => JointPreset::WheelAxle,
            "suspension" => JointPreset::Suspension,
            "chain_link" => JointPreset::ChainLink,
            _ => JointPreset::Fixed,
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

    fn to_key(&self) -> &'static str {
        match self {
            QueryPreset::GroundCheck => "ground",
            QueryPreset::PlayerInteraction => "player_interact",
            QueryPreset::ProjectileHit => "projectile",
            QueryPreset::AllPhysics => "all",
            QueryPreset::TriggersOnly => "triggers",
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "ground" => QueryPreset::GroundCheck,
            "player_interact" => QueryPreset::PlayerInteraction,
            "projectile" => QueryPreset::ProjectileHit,
            "all" => QueryPreset::AllPhysics,
            "triggers" => QueryPreset::TriggersOnly,
            _ => QueryPreset::GroundCheck,
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

    // Create entities based on category (static positions - no time dependency)
    match category {
        PhysicsCategory::Config => {
            create_config_entities(&mut world, &physics_config);
        }
        PhysicsCategory::RigidBodies => {
            create_body_entities(&mut world, body_preset);
        }
        PhysicsCategory::Colliders => {
            create_collider_entities(&mut world, collider_preset);
        }
        PhysicsCategory::Joints => {
            create_joint_entities(&mut world, joint_preset);
        }
        PhysicsCategory::Queries => {
            create_query_entities(&mut world);
        }
    }

    // Create camera with proper look_at
    let mut camera_transform = Object3D::default();
    camera_transform.position = Vec3::new(0.0, 2.0, 6.0);
    camera_transform.look_at(Vec3::new(0.0, 0.0, 0.0));

    let camera = world
        .spawn()
        .insert(camera_transform)
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

fn create_config_entities(world: &mut World, config: &PhysicsConfig) {
    // Visualize gravity with spheres at different heights
    let _gravity_strength = (config.gravity.y.abs() / 9.81).min(2.0);

    let colors = [
        Color::rgba(0.8, 0.4, 0.2, 1.0),
        Color::rgba(0.2, 0.6, 0.8, 1.0),
        Color::rgba(0.6, 0.8, 0.2, 1.0),
    ];

    for (i, color) in colors.iter().enumerate() {
        let x_pos = (i as f32 - 1.0) * 0.8;
        let y_pos = 1.5 - (i as f32 * 0.5); // Static staggered heights

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

fn create_body_entities(world: &mut World, preset: RigidBodyPreset) {
    let body = preset.to_body();

    let color = match body.body_type {
        RigidBodyType::Dynamic => Color::rgba(0.2, 0.6, 0.9, 1.0),
        RigidBodyType::Kinematic => Color::rgba(0.9, 0.6, 0.2, 1.0),
        RigidBodyType::Static => Color::rgba(0.5, 0.5, 0.5, 1.0),
    };

    // Static position based on body type
    let y_offset = match body.body_type {
        RigidBodyType::Dynamic => 0.5,
        RigidBodyType::Kinematic => 0.3,
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
            rotation: Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 0.3), // Static rotation
            ..Default::default()
        })
        .insert(Mesh {
            geometry: geo_handle,
            material: mat_handle,
        })
        .insert(body)
        .insert(Collider::cube(0.4));
}

fn create_collider_entities(world: &mut World, preset: ColliderPreset) {
    let collider = preset.to_collider();
    let rotation = 0.3; // Static rotation angle

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

fn create_joint_entities(world: &mut World, preset: JointPreset) {
    // Static positions for each joint type - no time dependency to avoid jitter
    let (pos_a, pos_b) = match preset {
        JointPreset::Fixed | JointPreset::ChainLink => (
            Vec3::new(-0.5, 0.5, 0.0),
            Vec3::new(0.5, 0.5, 0.0),
        ),
        JointPreset::Ball
        | JointPreset::Revolute
        | JointPreset::DoorHinge
        | JointPreset::WheelAxle => (
            Vec3::new(-0.6, 0.5, 0.0),
            Vec3::new(0.6, 0.3, 0.0),
        ),
        JointPreset::Prismatic | JointPreset::Suspension => (
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
        JointPreset::Spring | JointPreset::Rope => (
            Vec3::new(-0.5, 1.0, 0.0),
            Vec3::new(0.8, 0.5, 0.0),
        ),
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

fn create_query_entities(world: &mut World) {
    // Ray visualization (thin elongated box) - static length, no time dependency
    let ray_length = 3.0;
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
// Physics Animation System
// ============================================================================

/// Simple velocity component for physics simulation
#[derive(Clone, Debug, Default)]
struct Velocity {
    linear: Vec3,
}

impl blinc_3d::ecs::Component for Velocity {
    const STORAGE: blinc_3d::ecs::StorageType = blinc_3d::ecs::StorageType::Dense;
}

/// System that simulates physics with gravity for dynamic bodies
struct PhysicsAnimationSystem {
    gravity: Vec3,
    ground_y: f32,
    elapsed: f32,
}

impl System for PhysicsAnimationSystem {
    fn run(&mut self, ctx: &mut SystemContext) {
        self.elapsed += ctx.delta_time;

        // Query all entities with Object3D and RigidBody - collect body info
        let entities: Vec<_> = ctx
            .world
            .query::<(&Object3D, &RigidBody)>()
            .iter()
            .map(|(e, (_, body))| (e, body.body_type, body.gravity_scale))
            .collect();

        for (entity, body_type, gravity_scale) in entities {
            // Static bodies never move
            if body_type == RigidBodyType::Static {
                continue;
            }

            // Kinematic bodies only rotate (moved by user, not physics)
            if body_type == RigidBodyType::Kinematic {
                if let Some(obj) = ctx.world.get_mut::<Object3D>(entity) {
                    // Just rotate kinematic bodies slowly
                    obj.rotation = Quat::from_axis_angle(
                        Vec3::new(0.0, 1.0, 0.0),
                        self.elapsed * 0.3,
                    );
                }
                continue;
            }

            // Dynamic bodies: full physics simulation
            let velocity = ctx
                .world
                .get::<Velocity>(entity)
                .map(|v| v.linear)
                .unwrap_or(Vec3::ZERO);

            // Apply gravity to velocity (using gravity_scale from body)
            let mut new_velocity = Vec3::new(
                velocity.x,
                velocity.y + self.gravity.y * gravity_scale * ctx.delta_time,
                velocity.z,
            );

            // Update position with velocity
            if let Some(obj) = ctx.world.get_mut::<Object3D>(entity) {
                obj.position.x += new_velocity.x * ctx.delta_time;
                obj.position.y += new_velocity.y * ctx.delta_time;
                obj.position.z += new_velocity.z * ctx.delta_time;

                // Ground collision - bounce with damping
                if obj.position.y < self.ground_y {
                    obj.position.y = self.ground_y;
                    // Bounce with energy loss (60% retained)
                    new_velocity.y = -new_velocity.y * 0.6;
                    new_velocity.x *= 0.9;
                    new_velocity.z *= 0.9;

                    // Stop bouncing when velocity is very low
                    if new_velocity.y.abs() < 0.1 {
                        new_velocity.y = 0.0;
                    }
                }

                // Rotate slowly for visual interest
                obj.rotation = Quat::from_axis_angle(
                    Vec3::new(0.0, 1.0, 0.0),
                    self.elapsed * 0.5,
                );
            }

            // Store velocity for next frame
            if ctx.world.get::<Velocity>(entity).is_some() {
                if let Some(vel) = ctx.world.get_mut::<Velocity>(entity) {
                    vel.linear = new_velocity;
                }
            } else {
                ctx.world.insert(entity, Velocity { linear: new_velocity });
            }
        }
    }

    fn name(&self) -> &'static str {
        "PhysicsAnimationSystem"
    }

    fn stage(&self) -> SystemStage {
        SystemStage::Update
    }
}

// ============================================================================
// Shared State for Animation Thread
// ============================================================================

/// Shared elapsed time (in milliseconds as u32, divide by 1000.0 for seconds)
static ELAPSED_TIME_MS: AtomicU32 = AtomicU32::new(0);

/// World configuration - stored as hash for change detection
static WORLD_CONFIG_HASH: AtomicU32 = AtomicU32::new(0);

/// Helper to compute a simple hash of current settings
fn compute_config_hash(
    category: &str,
    config: &str,
    body: &str,
    collider: &str,
    joint: &str,
) -> u32 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    category.hash(&mut hasher);
    config.hash(&mut hasher);
    body.hash(&mut hasher);
    collider.hash(&mut hasher);
    joint.hash(&mut hasher);
    hasher.finish() as u32
}

// ============================================================================
// UI Building
// ============================================================================

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    // Create all shared state at the top level
    let category_state = ctx.use_state_keyed("physics_category", || "config".to_string());
    let config_state = ctx.use_state_keyed("config_preset", || "default".to_string());
    let body_state = ctx.use_state_keyed("body_preset", || "dynamic".to_string());
    let collider_state = ctx.use_state_keyed("collider_preset", || "sphere".to_string());
    let joint_state = ctx.use_state_keyed("joint_preset", || "fixed".to_string());
    let query_state = ctx.use_state_keyed("query_preset", || "ground".to_string());

    // Create the world using persisted state (survives UI rebuilds)
    // The world is recreated when settings change via the config hash
    let cat = category_state.get();
    let cfg = config_state.get();
    let bod = body_state.get();
    let col = collider_state.get();
    let jnt = joint_state.get();

    let current_hash = compute_config_hash(&cat, &cfg, &bod, &col, &jnt);

    // Create world state - uses current settings (initial creation only)
    // The center_panel handles updates when signals change via deps()
    let world_state = ctx.use_state_keyed("physics_world", || {
        let category = PhysicsCategory::from_key(&cat);
        let config = ConfigPreset::from_key(&cfg);
        let body = RigidBodyPreset::from_key(&bod);
        let collider = ColliderPreset::from_key(&col);
        let joint = JointPreset::from_key(&jnt);

        let (world, camera) = create_physics_world(category, config, body, collider, joint);
        WORLD_CONFIG_HASH.store(current_hash, Ordering::Relaxed);
        Arc::new(Mutex::new((world, camera, current_hash)))
    });

    // Clone world for the tick callback (runs ECS systems to animate entities)
    let world_for_tick = world_state.get();

    // Register tick callback to run ECS systems at 120fps
    // This runs on the animation scheduler's background thread and triggers redraws
    ctx.use_tick_callback(move |dt| {
        // Update elapsed time (accumulate delta time in milliseconds)
        let current = ELAPSED_TIME_MS.load(Ordering::Relaxed);
        let delta_ms = (dt * 1000.0) as u32;
        ELAPSED_TIME_MS.store(current.wrapping_add(delta_ms), Ordering::Relaxed);

        // Lock world and run animation systems
        if let Ok(mut world_data) = world_for_tick.lock() {
            let (ref mut world, _, _) = *world_data;

            // Read gravity from PhysicsWorld resource (respects config preset)
            let gravity = world
                .resource::<PhysicsWorld>()
                .map(|pw| pw.config.gravity)
                .unwrap_or(Vec3::new(0.0, -9.81, 0.0));

            // Create and run the physics animation system with actual gravity from config
            let mut animation_system = PhysicsAnimationSystem {
                gravity,
                ground_y: -1.3, // Ground plane Y position (slightly above visual ground)
                elapsed: current as f32 / 1000.0,
            };

            let mut sys_ctx = SystemContext {
                world,
                delta_time: dt.min(0.1), // Cap to avoid large jumps
                elapsed_time: current as f32 / 1000.0,
                frame: 0,
            };

            animation_system.run(&mut sys_ctx);
        }
    });

    let world_for_canvas = world_state.get();

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
        .child(main_content(
            ctx.width,
            ctx.height - 80.0,
            category_state,
            config_state,
            body_state,
            collider_state,
            joint_state,
            query_state,
            world_for_canvas,
        ))
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

fn main_content(
    width: f32,
    height: f32,
    category_state: blinc_core::State<String>,
    config_state: blinc_core::State<String>,
    body_state: blinc_core::State<String>,
    collider_state: blinc_core::State<String>,
    joint_state: blinc_core::State<String>,
    query_state: blinc_core::State<String>,
    world: Arc<Mutex<(World, Entity, u32)>>,
) -> impl ElementBuilder {
    div()
        .w(width)
        .h(height)
        .flex_row()
        .gap(16.0)
        // Left: Category tabs and selection
        .child(left_panel(
            category_state.clone(),
            config_state.clone(),
            body_state.clone(),
            collider_state.clone(),
            joint_state.clone(),
            query_state.clone(),
        ))
        // Center: Visualization - depends on state signals for incremental updates
        .child(center_panel(
            width - 620.0,
            height,
            world,
            category_state.clone(),
            config_state.clone(),
            body_state.clone(),
            collider_state.clone(),
            joint_state.clone(),
        ))
        // Right: Details
        .child(right_panel(
            category_state,
            config_state,
            body_state,
            collider_state,
            joint_state,
            query_state,
        ))
}

fn left_panel(
    category_state: blinc_core::State<String>,
    config_state: blinc_core::State<String>,
    body_state: blinc_core::State<String>,
    collider_state: blinc_core::State<String>,
    joint_state: blinc_core::State<String>,
    query_state: blinc_core::State<String>,
) -> impl ElementBuilder {
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
                // Category selection
                .child(
                    text("Category")
                        .size(14.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(category_radio_group(category_state.clone()))
                .child(divider())
                // Selection panel based on category
                .child(selection_panel(
                    category_state,
                    config_state,
                    body_state,
                    collider_state,
                    joint_state,
                    query_state,
                )),
        )
}

fn category_radio_group(category_state: blinc_core::State<String>) -> impl ElementBuilder {
    cn::radio_group(&category_state)
        .horizontal()
        .option("config", "Config")
        .option("bodies", "Bodies")
        .option("colliders", "Colliders")
        .option("joints", "Joints")
        .option("queries", "Queries")
}

fn selection_panel(
    category_state: blinc_core::State<String>,
    config_state: blinc_core::State<String>,
    body_state: blinc_core::State<String>,
    collider_state: blinc_core::State<String>,
    joint_state: blinc_core::State<String>,
    query_state: blinc_core::State<String>,
) -> impl ElementBuilder {
    let category = PhysicsCategory::from_key(&category_state.get());

    match category {
        PhysicsCategory::Config => {
            div()
                .flex_col()
                .gap(8.0)
                .child(
                    text("Configuration Presets")
                        .size(14.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(config_radio_group(config_state))
        }
        PhysicsCategory::RigidBodies => {
            div()
                .flex_col()
                .gap(8.0)
                .child(
                    text("RigidBody Presets")
                        .size(14.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(body_radio_group(body_state))
        }
        PhysicsCategory::Colliders => {
            div()
                .flex_col()
                .gap(8.0)
                .child(
                    text("Collider Presets")
                        .size(14.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(collider_radio_group(collider_state))
        }
        PhysicsCategory::Joints => {
            div()
                .flex_col()
                .gap(8.0)
                .child(
                    text("Joint Presets")
                        .size(14.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(joint_radio_group(joint_state))
        }
        PhysicsCategory::Queries => {
            div()
                .flex_col()
                .gap(8.0)
                .child(
                    text("Query Presets")
                        .size(14.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(query_radio_group(query_state))
        }
    }
}

fn config_radio_group(config_state: blinc_core::State<String>) -> impl ElementBuilder {
    cn::radio_group(&config_state)
        .option("default", "Default - Earth gravity")
        .option("zero_gravity", "Zero Gravity - Space sim")
        .option("sidescroller", "Sidescroller - High gravity")
        .option("low_gravity", "Low Gravity - Moon")
        .option("custom", "Custom - Manual config")
}

fn body_radio_group(body_state: blinc_core::State<String>) -> impl ElementBuilder {
    cn::radio_group(&body_state)
        .option("dynamic", "Dynamic - Physics driven")
        .option("kinematic", "Kinematic - User controlled")
        .option("static", "Static - Never moves")
        .option("player", "Player Character - Fixed rotation")
        .option("projectile", "Projectile - Fast with CCD")
        .option("vehicle", "Vehicle - Heavy, damped")
        .option("debris", "Debris - Lightweight")
        .option("floating", "Floating - Reduced gravity")
        .option("platform", "Platform - Moving kinematic")
        .option("environment", "Environment - Static world")
}

fn collider_radio_group(collider_state: blinc_core::State<String>) -> impl ElementBuilder {
    cn::radio_group(&collider_state)
        .option("sphere", "Sphere - Round shape")
        .option("cube", "Cube - Uniform box")
        .option("cuboid", "Cuboid - Non-uniform box")
        .option("capsule", "Capsule - Cylinder + hemispheres")
        .option("cylinder", "Cylinder - Cylindrical")
        .option("bouncy", "Bouncy - High restitution")
        .option("slippery", "Slippery - Low friction")
        .option("metal", "Metal - Dense")
        .option("wood", "Wood - Medium friction")
        .option("stone", "Stone - Heavy, high friction")
}

fn joint_radio_group(joint_state: blinc_core::State<String>) -> impl ElementBuilder {
    cn::radio_group(&joint_state)
        .option("fixed", "Fixed - Bodies move together")
        .option("ball", "Ball - Rotate around point")
        .option("revolute", "Revolute - Single axis rotation")
        .option("prismatic", "Prismatic - Single axis slide")
        .option("spring", "Spring - Distance with stiffness")
        .option("rope", "Rope - Max distance constraint")
        .option("door_hinge", "Door Hinge - Limited rotation")
        .option("wheel_axle", "Wheel Axle - Free rotation")
        .option("suspension", "Suspension - Vertical spring")
        .option("chain_link", "Chain Link - Rigid distance")
}

fn query_radio_group(query_state: blinc_core::State<String>) -> impl ElementBuilder {
    cn::radio_group(&query_state)
        .option("ground", "Ground Check - Environment only")
        .option("player_interact", "Player Interaction - Players/enemies")
        .option("projectile", "Projectile Hit - Damageable objects")
        .option("all", "All Physics - No filtering")
        .option("triggers", "Triggers Only - Sensor colliders")
}

fn center_panel(
    width: f32,
    height: f32,
    world: Arc<Mutex<(World, Entity, u32)>>,
    category_state: blinc_core::State<String>,
    config_state: blinc_core::State<String>,
    body_state: blinc_core::State<String>,
    collider_state: blinc_core::State<String>,
    joint_state: blinc_core::State<String>,
) -> impl ElementBuilder {
    use blinc_layout::stateful::{stateful_with_key, NoState};

    // Use stateful container with deps to subscribe to state changes
    // This ensures the canvas re-renders when any setting changes
    stateful_with_key::<NoState>("physics_center_panel")
        .deps([
            category_state.signal_id(),
            config_state.signal_id(),
            body_state.signal_id(),
            collider_state.signal_id(),
            joint_state.signal_id(),
        ])
        .on_state(move |_ctx| {
            // Read current state values - this runs when signals change
            let cat = category_state.get();
            let cfg = config_state.get();
            let bod = body_state.get();
            let col = collider_state.get();
            let jnt = joint_state.get();

            // Check if we need to recreate the world (settings changed)
            let current_hash = compute_config_hash(&cat, &cfg, &bod, &col, &jnt);
            {
                let mut world_data = world.lock().unwrap();
                let stored_hash = world_data.2;
                if stored_hash != current_hash {
                    // Settings changed - recreate world with new configuration
                    let category = PhysicsCategory::from_key(&cat);
                    let config = ConfigPreset::from_key(&cfg);
                    let body = RigidBodyPreset::from_key(&bod);
                    let collider = ColliderPreset::from_key(&col);
                    let joint = JointPreset::from_key(&jnt);

                    let (new_world, camera) = create_physics_world(category, config, body, collider, joint);
                    *world_data = (new_world, camera, current_hash);
                    WORLD_CONFIG_HASH.store(current_hash, Ordering::Relaxed);
                }
            }

            // Clone the Arc for the canvas closure
            let world_for_canvas = world.clone();

            div()
                .w(width)
                .h(height)
                .bg(Color::rgba(0.08, 0.08, 0.1, 1.0))
                .rounded(8.0)
                .overflow_clip()
                .child(
                    canvas(move |draw_ctx, bounds| {
                        // Get current time from the animation scheduler (updated by tick callback)
                        let t = ELAPSED_TIME_MS.load(Ordering::Relaxed) as f32 / 1000.0;

                        // Lock the world for rendering
                        if let Ok(world_data) = world_for_canvas.lock() {
                            let (ref world, camera_entity, _) = *world_data;
                            // Render using proper ECS pipeline with time for any time-based effects
                            render_scene_with_time(draw_ctx, world, camera_entity, bounds, t);
                        }
                    })
                    .w_full()
                    .h_full(),
                )
        })
}

fn right_panel(
    category_state: blinc_core::State<String>,
    config_state: blinc_core::State<String>,
    body_state: blinc_core::State<String>,
    collider_state: blinc_core::State<String>,
    joint_state: blinc_core::State<String>,
    query_state: blinc_core::State<String>,
) -> impl ElementBuilder {
    let category = PhysicsCategory::from_key(&category_state.get());

    let content = match category {
        PhysicsCategory::Config => {
            config_details(ConfigPreset::from_key(&config_state.get()))
        }
        PhysicsCategory::RigidBodies => {
            body_details(RigidBodyPreset::from_key(&body_state.get()))
        }
        PhysicsCategory::Colliders => {
            collider_details(ColliderPreset::from_key(&collider_state.get()))
        }
        PhysicsCategory::Joints => {
            joint_details(JointPreset::from_key(&joint_state.get()))
        }
        PhysicsCategory::Queries => {
            query_details(QueryPreset::from_key(&query_state.get()))
        }
    };

    div().w(320.0).h_full().child(
        scroll()
            .w_full()
            .h_full()
            .bg(Color::rgba(0.1, 0.1, 0.12, 1.0))
            .rounded(8.0)
            .p(12.0)
            .child(content),
    )
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

fn property_row(prop_label: &str, value: &str) -> Div {
    div()
        .flex_row()
        .justify_between()
        .child(
            text(prop_label)
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
