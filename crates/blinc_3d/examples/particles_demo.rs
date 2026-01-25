//! Particle Effects Demo
//!
//! Demonstrates blinc_3d particle system utilities:
//! - Preset effects: fire, smoke, sparks, rain, snow, explosion, magic, confetti
//! - Emitter shapes: Point, Sphere, Cone, Box, Circle
//! - Force affectors: Gravity, Wind, Vortex, Drag, Turbulence
//! - Blend modes: Alpha, Additive, Multiply
//!
//! Particles are rendered as ECS entities using the GPU pipeline.
//!
//! Run with: cargo run -p blinc_3d --example particles_demo --features "sdf"

use blinc_3d::lights::AmbientLight;
use blinc_3d::prelude::*;
use blinc_3d::sdf::SdfScene;
use blinc_3d::utils::particles::*;
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_cn::prelude::*;
use blinc_core::events::event_types;
use blinc_layout::stateful::ButtonState;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU32, Ordering};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Blinc 3D - Particle Effects Demo".to_string(),
        width: 1200,
        height: 800,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

// ============================================================================
// Particle Effect Definitions
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum ParticleEffect {
    #[default]
    Fire,        // Bonfire (default)
    FireCandle,  // Small candle flame
    FireInferno, // Large intense flames
    Smoke,
    Sparks,
    Rain,
    Snow,
    Explosion,
    Magic,
    Confetti,
    Custom,
}

impl ParticleEffect {
    fn name(&self) -> &'static str {
        match self {
            ParticleEffect::Fire => "Fire - Bonfire",
            ParticleEffect::FireCandle => "Fire - Candle",
            ParticleEffect::FireInferno => "Fire - Inferno",
            ParticleEffect::Smoke => "Smoke",
            ParticleEffect::Sparks => "Sparks",
            ParticleEffect::Rain => "Rain",
            ParticleEffect::Snow => "Snow",
            ParticleEffect::Explosion => "Explosion",
            ParticleEffect::Magic => "Magic",
            ParticleEffect::Confetti => "Confetti",
            ParticleEffect::Custom => "Custom",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            ParticleEffect::Fire => "Large campfire/bonfire style flames",
            ParticleEffect::FireCandle => "Small gentle candle flame",
            ParticleEffect::FireInferno => "Intense raging inferno",
            ParticleEffect::Smoke => "Billowing smoke with wind drift",
            ParticleEffect::Sparks => "Fast streaking spark particles",
            ParticleEffect::Rain => "Heavy rainfall with stretched drops",
            ParticleEffect::Snow => "Gentle falling snowflakes",
            ParticleEffect::Explosion => "Burst of particles outward",
            ParticleEffect::Magic => "Swirling magical sparkles",
            ParticleEffect::Confetti => "Celebratory paper confetti",
            ParticleEffect::Custom => "Build your own particle system",
        }
    }

    fn to_key(&self) -> &'static str {
        match self {
            ParticleEffect::Fire => "fire",
            ParticleEffect::FireCandle => "fire_candle",
            ParticleEffect::FireInferno => "fire_inferno",
            ParticleEffect::Smoke => "smoke",
            ParticleEffect::Sparks => "sparks",
            ParticleEffect::Rain => "rain",
            ParticleEffect::Snow => "snow",
            ParticleEffect::Explosion => "explosion",
            ParticleEffect::Magic => "magic",
            ParticleEffect::Confetti => "confetti",
            ParticleEffect::Custom => "custom",
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "fire" => ParticleEffect::Fire,
            "fire_candle" => ParticleEffect::FireCandle,
            "fire_inferno" => ParticleEffect::FireInferno,
            "smoke" => ParticleEffect::Smoke,
            "sparks" => ParticleEffect::Sparks,
            "rain" => ParticleEffect::Rain,
            "snow" => ParticleEffect::Snow,
            "explosion" => ParticleEffect::Explosion,
            "magic" => ParticleEffect::Magic,
            "confetti" => ParticleEffect::Confetti,
            "custom" => ParticleEffect::Custom,
            _ => ParticleEffect::Fire,
        }
    }

    /// Get the actual ParticleSystem from blinc_3d utilities
    fn to_system(&self) -> ParticleSystem {
        match self {
            ParticleEffect::Fire => ParticleSystem::fire_bonfire(),
            ParticleEffect::FireCandle => ParticleSystem::fire_candle(),
            ParticleEffect::FireInferno => ParticleSystem::fire_inferno(),
            ParticleEffect::Smoke => ParticleSystem::smoke(),
            ParticleEffect::Sparks => ParticleSystem::sparks(),
            ParticleEffect::Rain => ParticleSystem::rain(),
            ParticleEffect::Snow => ParticleSystem::snow(),
            ParticleEffect::Explosion => ParticleSystem::explosion(),
            ParticleEffect::Magic => ParticleSystem::magic(),
            ParticleEffect::Confetti => ParticleSystem::confetti(),
            ParticleEffect::Custom => ParticleSystem::new()
                .with_max_particles(5000)
                .with_emitter(EmitterShape::Sphere { radius: 0.5 })
                .with_emission_rate(100.0)
                .with_lifetime(1.0, 3.0)
                .with_speed(1.0, 3.0)
                .with_size(0.1, 0.2, 0.0, 0.05)
                .with_colors(Color::WHITE,Color::WHITE, Color::rgba(1.0, 1.0, 1.0, 0.0)),
        }
    }
}

const ALL_EFFECTS: [ParticleEffect; 11] = [
    ParticleEffect::Fire,
    ParticleEffect::FireCandle,
    ParticleEffect::FireInferno,
    ParticleEffect::Smoke,
    ParticleEffect::Sparks,
    ParticleEffect::Rain,
    ParticleEffect::Snow,
    ParticleEffect::Explosion,
    ParticleEffect::Magic,
    ParticleEffect::Confetti,
    ParticleEffect::Custom,
];

// ============================================================================
// Shared State for Animation Thread
// ============================================================================

/// Shared elapsed time (in milliseconds)
static ELAPSED_TIME_MS: AtomicU32 = AtomicU32::new(0);

/// Camera angle (stored as fixed-point, multiply by 0.001)
static CAMERA_ANGLE: AtomicU32 = AtomicU32::new(300); // 0.3 initial

/// Helper to compute hash for change detection
fn compute_effect_hash(effect_key: &str, cam_angle: f32) -> u32 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    effect_key.hash(&mut hasher);
    // Don't include cam_angle in hash - we update camera position without recreating world
    hasher.finish() as u32
}

// ============================================================================
// ECS World Creation
// ============================================================================

/// Create an ECS world with the particle system and scene entities
fn create_particle_world(effect: ParticleEffect, cam_angle: f32) -> (World, Entity) {
    let mut world = World::new();

    // Add ambient light
    world.spawn().insert(AmbientLight {
        color: Color::WHITE,
        intensity: 0.4,
    });

    // Create SDF scene for background (ground + pedestal)
    let sdf_scene = create_sdf_scene();
    world
        .spawn()
        .insert(SdfMesh {
            scene: sdf_scene,
            cast_shadows: false,
            receive_shadows: false,
        })
        .insert(Object3D::default());

    // Create particle system entity at emitter position
    let mut particle_system = effect.to_system();

    // For burst effects (explosion, confetti), trigger an initial burst
    match effect {
        ParticleEffect::Explosion => particle_system.burst(200),
        ParticleEffect::Confetti => particle_system.burst(500),
        _ => {}
    }

    world.spawn().insert(particle_system).insert(Object3D {
        position: Vec3::new(0.0, 0.7, 0.0), // On top of pedestal
        ..Default::default()
    });

    // Create camera - position it and look at the center
    let cam_x = cam_angle.sin() * 6.0;
    let cam_z = cam_angle.cos() * 6.0;
    let camera_pos = Vec3::new(cam_x, 3.0, cam_z);

    let mut camera_transform = Object3D::default();
    camera_transform.position = camera_pos;
    camera_transform.look_at(Vec3::new(0.0, 1.0, 0.0));

    let camera = world
        .spawn()
        .insert(camera_transform)
        .insert(PerspectiveCamera::new(0.8, 16.0 / 9.0, 0.1, 100.0))
        .id();

    (world, camera)
}

/// Create SDF scene for background visualization
fn create_sdf_scene() -> SdfScene {
    let mut scene = SdfScene::new();

    // Ground plane
    let floor = SdfScene::box_node(Vec3::new(5.0, 0.05, 5.0)).at(Vec3::new(0.0, -0.5, 0.0));

    // Simple pedestal
    let pedestal = SdfScene::cylinder(0.5, 0.3).at(Vec3::new(0.0, 0.0, 0.0));

    // Emitter marker sphere
    let emitter = SdfScene::sphere(0.15).at(Vec3::new(0.0, 0.5, 0.0));

    let combined = SdfScene::union(floor, pedestal);
    let combined = SdfScene::union(combined, emitter);

    scene.set_root(combined);
    scene
}

// ============================================================================
// UI Building
// ============================================================================

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    // Create shared state at the top level
    let effect_state = ctx.use_state_keyed("particle_effect", || "fire".to_string());

    // Get current effect for initial world creation
    let effect_key = effect_state.get();
    let cam_angle = CAMERA_ANGLE.load(Ordering::Relaxed) as f32 * 0.001;
    let current_hash = compute_effect_hash(&effect_key, cam_angle);

    // Create persisted world state (survives UI rebuilds)
    let world_state = ctx.use_state_keyed("particle_world", || {
        let effect = ParticleEffect::from_key(&effect_key);
        let (world, camera) = create_particle_world(effect, cam_angle);
        Arc::new(Mutex::new((world, camera, current_hash)))
    });

    // Clone world for the tick callback
    let world_for_tick = world_state.get();

    // Register tick callback to update particle systems at 120fps
    ctx.use_tick_callback(move |dt| {
        // Update elapsed time
        let current = ELAPSED_TIME_MS.load(Ordering::Relaxed);
        let delta_ms = (dt * 1000.0) as u32;
        ELAPSED_TIME_MS.store(current.wrapping_add(delta_ms), Ordering::Relaxed);

        // Update camera angle with slow rotation
        let angle = CAMERA_ANGLE.load(Ordering::Relaxed);
        let new_angle = angle.wrapping_add((dt * 50.0) as u32); // Slow rotation
        CAMERA_ANGLE.store(new_angle, Ordering::Relaxed);

        // Lock world and update camera position
        if let Ok(mut world_data) = world_for_tick.lock() {
            let (ref mut world, camera_entity, _) = *world_data;

            // Update camera position based on angle
            let cam_angle = new_angle as f32 * 0.001;
            let cam_x = cam_angle.sin() * 6.0;
            let cam_z = cam_angle.cos() * 6.0;
            let camera_pos = Vec3::new(cam_x, 3.0, cam_z);

            if let Some(camera_transform) = world.get_mut::<Object3D>(camera_entity) {
                camera_transform.position = camera_pos;
                camera_transform.look_at(Vec3::new(0.0, 1.0, 0.0));
            }

            // For burst effects (explosion, confetti), periodically re-trigger bursts
            // so the effect can be seen repeatedly
            let time_secs = current.wrapping_add(delta_ms) as f32 / 1000.0;
            let burst_interval = 3.0; // Re-burst every 3 seconds

            // Check if we crossed a burst interval boundary
            let prev_time = current as f32 / 1000.0;
            let prev_interval = (prev_time / burst_interval) as u32;
            let curr_interval = (time_secs / burst_interval) as u32;

            if curr_interval != prev_interval {
                // Query particle systems and re-trigger bursts for non-looping effects
                let entities: Vec<_> = world.query::<(&ParticleSystem,)>()
                    .iter()
                    .map(|(e, _)| e)
                    .collect();

                for entity in entities {
                    if let Some(ps) = world.get_mut::<ParticleSystem>(entity) {
                        if !ps.looping && ps.emission_rate == 0.0 {
                            // This is a burst effect - trigger a burst
                            ps.burst(200);
                            ps.play(); // Ensure it's playing
                        }
                    }
                }
            }

            // Note: ParticleSystem animation is handled by the GPU shader via time parameter
            // passed to render_scene_with_time - no CPU update needed
        }
    });

    let world_for_content = world_state.get();

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.06, 0.06, 0.1, 1.0))
        .flex_col()
        .child(header())
        .child(main_content(
            ctx.width,
            ctx.height - 80.0,
            effect_state,
            world_for_content,
        ))
}

fn header() -> impl ElementBuilder {
    div()
        .w_full()
        .h(80.0)
        .px(24.0)
        .flex_row()
        .items_center()
        .justify_between()
        .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(
                    text("Particle Effects Demo")
                        .size(24.0)
                        .weight(FontWeight::Bold)
                        .color(Color::WHITE),
                )
                .child(
                    text("GPU-rendered ParticleSystem components via ECS")
                        .size(14.0)
                        .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
                ),
        )
}

fn main_content(
    width: f32,
    height: f32,
    effect_state: blinc_core::State<String>,
    world: Arc<Mutex<(World, Entity, u32)>>,
) -> impl ElementBuilder {
    div()
        .w(width)
        .h(height)
        .flex_row()
        .child(viewport_area(
            width - 360.0,
            height,
            effect_state.clone(),
            world,
        ))
        .child(control_panel(effect_state))
}

fn viewport_area(
    width: f32,
    height: f32,
    effect_state: blinc_core::State<String>,
    world: Arc<Mutex<(World, Entity, u32)>>,
) -> impl ElementBuilder {
    stateful::<ButtonState>().deps([effect_state.signal_id()]).on_state(move |ctx| {
        // Handle mouse drag for camera rotation
        if let Some(event) = ctx.event() {
            if event.event_type == event_types::POINTER_MOVE && ctx.state() == ButtonState::Pressed
            {
                // Update camera angle via atomic
                let current = CAMERA_ANGLE.load(Ordering::Relaxed);
                let delta = (event.local_x * 0.5) as i32;
                CAMERA_ANGLE.store((current as i32 + delta).max(0) as u32, Ordering::Relaxed);
            }
        }

        // Read current effect for code example display
        let effect_key = effect_state.get();
        let current_effect = ParticleEffect::from_key(&effect_key);

        // Check if effect changed and recreate world if needed
        let current_hash = compute_effect_hash(&effect_key, 0.0);
        {
            let mut world_data = world.lock().unwrap();
            let stored_hash = world_data.2;
            if stored_hash != current_hash {
                // Effect changed - recreate world with new particle system
                let cam_angle = CAMERA_ANGLE.load(Ordering::Relaxed) as f32 * 0.001;
                let (new_world, camera) = create_particle_world(current_effect, cam_angle);
                *world_data = (new_world, camera, current_hash);
            }
        }

        // Clone world for canvas closure
        let world_for_canvas = world.clone();

        div()
            .w(width)
            .h(height)
            .bg(Color::rgba(0.02, 0.02, 0.05, 1.0))
            .cursor_pointer()
            .child(
                canvas(move |draw_ctx, bounds| {
                    // Get current time from animation scheduler
                    let time = ELAPSED_TIME_MS.load(Ordering::Relaxed) as f32 / 1000.0;

                    // Lock world for rendering (systems run in tick callback)
                    if let Ok(world_data) = world_for_canvas.lock() {
                        let (ref world, camera_entity, _) = *world_data;
                        render_scene_with_time(draw_ctx, world, camera_entity, bounds, time);
                    }
                })
                .w_full()
                .h_full(),
            )
            .child(
                // Overlay: usage example
                div()
                    .absolute()
                    .left(16.0)
                    .bottom(16.0)
                    .max_w(width - 32.0)
                    .p(12.0)
                    .bg(Color::rgba(0.0, 0.0, 0.0, 0.85))
                    .rounded(8.0)
                    .flex_col()
                    .gap(8.0)
                    .child(
                        text("Usage Example")
                            .size(14.0)
                            .weight(FontWeight::SemiBold)
                            .color(Color::WHITE),
                    )
                    .child(code_example(current_effect)),
            )
    })
}

fn code_example(effect: ParticleEffect) -> impl ElementBuilder {
    let code_str = match effect {
        ParticleEffect::Fire => "world.spawn()\n    .insert(ParticleSystem::fire())\n    .insert(Object3D { position: Vec3::new(0.0, 1.0, 0.0), ..Default::default() });",
        ParticleEffect::Smoke => "world.spawn()\n    .insert(ParticleSystem::smoke())\n    .insert(Object3D::default());",
        ParticleEffect::Sparks => "world.spawn()\n    .insert(ParticleSystem::sparks())\n    .insert(Object3D::default());",
        ParticleEffect::Rain => "world.spawn()\n    .insert(ParticleSystem::rain())\n    .insert(Object3D { position: Vec3::new(0.0, 10.0, 0.0), ..Default::default() });",
        ParticleEffect::Snow => "world.spawn()\n    .insert(ParticleSystem::snow())\n    .insert(Object3D { position: Vec3::new(0.0, 10.0, 0.0), ..Default::default() });",
        ParticleEffect::Explosion => {
            "let mut system = ParticleSystem::explosion();\nsystem.burst(100); // Trigger burst\nworld.spawn()\n    .insert(system)\n    .insert(Object3D::default());"
        }
        ParticleEffect::Magic => "world.spawn()\n    .insert(ParticleSystem::magic())\n    .insert(Object3D::default());",
        ParticleEffect::Confetti => {
            "let mut system = ParticleSystem::confetti();\nsystem.burst(500); // Launch confetti\nworld.spawn()\n    .insert(system)\n    .insert(Object3D::default());"
        }
        ParticleEffect::Custom => {
            "let particles = ParticleSystem::new()\n    .with_emitter(EmitterShape::Sphere { radius: 0.5 })\n    .with_emission_rate(100.0)\n    .with_lifetime(1.0, 3.0)\n    .with_force(Force::gravity(Vec3::new(0.0, -9.8, 0.0)));\nworld.spawn()\n    .insert(particles)\n    .insert(Object3D::default());"
        }
        ParticleEffect::FireCandle => "world.spawn()\n    .insert(ParticleSystem::fire_candle())\n    .insert(Object3D::default());",
        ParticleEffect::FireInferno => "world.spawn()\n    .insert(ParticleSystem::fire_inferno())\n    .insert(Object3D::default());",
    };

    div()
        .px(8.0)
        .py(6.0)
        .bg(Color::rgba(0.1, 0.12, 0.15, 1.0))
        .rounded(4.0)
        .child(code(code_str))
}

fn control_panel(effect_state: blinc_core::State<String>) -> impl ElementBuilder {
    scroll()
        .w(360.0)
        .h_full()
        .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
        .child(
            div()
                .p(16.0)
                .flex_col()
                .gap(16.0)
                .child(
                    text("Particle Presets")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(
                    div()
                        .bg(Color::GRAY)
                        .rounded_md()
                        .child(effect_radio_group(effect_state.clone())),
                )
                .child(divider())
                .child(
                    text("System Configuration")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(system_properties(effect_state.clone()))
                .child(divider())
                .child(
                    text("Emitter Details")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(emitter_info(effect_state.clone()))
                .child(divider())
                .child(
                    text("Force Affectors")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(forces_info(effect_state)),
        )
}

fn effect_radio_group(effect_state: blinc_core::State<String>) -> impl ElementBuilder {
    cn::radio_group(&effect_state)
        .label("Select Effect")
        .option("fire", "Fire - Rising flames")
        .option("smoke", "Smoke - Billowing clouds")
        .option("sparks", "Sparks - Fast streaks")
        .option("rain", "Rain - Falling drops")
        .option("snow", "Snow - Gentle flakes")
        .option("explosion", "Explosion - Burst outward")
        .option("magic", "Magic - Swirling sparkles")
        .option("confetti", "Confetti - Celebration")
        .option("custom", "Custom - Build your own")
}

fn system_properties(effect_state: blinc_core::State<String>) -> impl ElementBuilder {
    let effect_key = effect_state.get();
    let effect = ParticleEffect::from_key(&effect_key);
    let system = effect.to_system();

    div()
        .flex_col()
        .gap(8.0)
        .child(property_row(
            "Max Particles",
            &format!("{}", system.max_particles),
        ))
        .child(property_row(
            "Emission Rate",
            &format!("{:.0}/s", system.emission_rate),
        ))
        .child(property_row(
            "Lifetime",
            &format!("{:.1} - {:.1}s", system.lifetime.0, system.lifetime.1),
        ))
        .child(property_row(
            "Speed",
            &format!("{:.1} - {:.1}", system.start_speed.0, system.start_speed.1),
        ))
        .child(property_row(
            "Start Size",
            &format!("{:.2} - {:.2}", system.start_size.0, system.start_size.1),
        ))
        .child(property_row(
            "End Size",
            &format!("{:.2} - {:.2}", system.end_size.0, system.end_size.1),
        ))
        .child(property_row(
            "Blend Mode",
            match system.blend_mode {
                BlendMode::Alpha => "Alpha",
                BlendMode::Additive => "Additive",
                BlendMode::Multiply => "Multiply",
                BlendMode::Premultiplied => "Premultiplied",
            },
        ))
        .child(property_row(
            "Render Mode",
            match system.render_mode {
                RenderMode::Billboard => "Billboard",
                RenderMode::Stretched => "Stretched",
                RenderMode::Horizontal => "Horizontal",
                RenderMode::Vertical => "Vertical",
            },
        ))
        .child(property_row(
            "Looping",
            if system.looping { "Yes" } else { "No" },
        ))
}

fn emitter_info(effect_state: blinc_core::State<String>) -> impl ElementBuilder {
    let effect_key = effect_state.get();
    let effect = ParticleEffect::from_key(&effect_key);
    let system = effect.to_system();

    let emitter_desc = match &system.emitter {
        EmitterShape::Point => "Point".to_string(),
        EmitterShape::Sphere { radius } => format!("Sphere (r={:.2})", radius),
        EmitterShape::Hemisphere { radius } => format!("Hemisphere (r={:.2})", radius),
        EmitterShape::Cone { angle, radius } => {
            format!("Cone (angle={:.2}, r={:.2})", angle, radius)
        }
        EmitterShape::Box { half_extents } => format!(
            "Box ({:.1} x {:.1} x {:.1})",
            half_extents.x * 2.0,
            half_extents.y * 2.0,
            half_extents.z * 2.0
        ),
        EmitterShape::Circle { radius } => format!("Circle (r={:.2})", radius),
    };

    div()
        .flex_col()
        .gap(8.0)
        .child(property_row("Shape", &emitter_desc))
        .child(property_row(
            "Direction",
            &format!(
                "({:.1}, {:.1}, {:.1})",
                system.direction.x, system.direction.y, system.direction.z
            ),
        ))
        .child(property_row(
            "Randomness",
            &format!("{:.0}%", system.direction_randomness * 100.0),
        ))
        .child(property_row(
            "Gravity Scale",
            &format!("{:.1}", system.gravity_scale),
        ))
}

fn forces_info(effect_state: blinc_core::State<String>) -> impl ElementBuilder {
    let effect_key = effect_state.get();
    let effect = ParticleEffect::from_key(&effect_key);
    let system = effect.to_system();

    let mut content = div().flex_col().gap(6.0);

    if system.forces.is_empty() {
        content = content.child(
            text("No forces applied")
                .size(12.0)
                .color(Color::rgba(0.5, 0.5, 0.5, 1.0)),
        );
    } else {
        for (i, force) in system.forces.iter().enumerate() {
            let force_desc = describe_force(force);
            content = content.child(force_badge(&format!("{}. {}", i + 1, force_desc)));
        }
    }

    // Colors section
    content = content
        .child(
            div()
                .h(1.0)
                .w_full()
                .bg(Color::rgba(0.2, 0.2, 0.25, 1.0))
                .mt(8.0),
        )
        .child(
            div().mt(8.0).child(
                text("Particle Colors")
                    .size(14.0)
                    .weight(FontWeight::Medium)
                    .color(Color::WHITE),
            ),
        )
        .child(color_swatch("Start", system.start_color))
        .child(color_swatch("End", system.end_color));

    content
}

fn describe_force(force: &Force) -> String {
    match force {
        Force::Gravity(v) => format!("Gravity ({:.1}, {:.1}, {:.1})", v.x, v.y, v.z),
        Force::Wind {
            direction,
            strength,
            turbulence,
        } => format!(
            "Wind dir=({:.1},{:.1},{:.1}) s={:.1} t={:.1}",
            direction.x, direction.y, direction.z, strength, turbulence
        ),
        Force::Vortex {
            axis,
            center: _,
            strength,
        } => format!(
            "Vortex axis=({:.1},{:.1},{:.1}) s={:.1}",
            axis.x, axis.y, axis.z, strength
        ),
        Force::Drag(c) => format!("Drag (coeff={:.1})", c),
        Force::Turbulence {
            strength,
            frequency,
        } => format!("Turbulence s={:.1} f={:.1}", strength, frequency),
        Force::Attractor { position, strength } => {
            format!(
                "Attractor pos=({:.1},{:.1},{:.1}) s={:.1}",
                position.x, position.y, position.z, strength
            )
        }
        Force::Radial { center, strength } => {
            format!(
                "Radial c=({:.1},{:.1},{:.1}) s={:.1}",
                center.x, center.y, center.z, strength
            )
        }
    }
}

fn property_row(prop_label: &'static str, value: &str) -> impl ElementBuilder {
    let value = value.to_string();
    div()
        .flex_row()
        .justify_between()
        .items_center()
        .child(
            text(prop_label)
                .size(12.0)
                .color(Color::rgba(0.7, 0.7, 0.7, 1.0)),
        )
        .child(
            div()
                .px(8.0)
                .py(3.0)
                .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
                .rounded(4.0)
                .child(text(&value).size(11.0).color(Color::WHITE)),
        )
}

fn force_badge(badge_label: &str) -> impl ElementBuilder {
    let badge_label = badge_label.to_string();
    div()
        .px(8.0)
        .py(4.0)
        .bg(Color::rgba(0.2, 0.25, 0.35, 1.0))
        .rounded(4.0)
        .child(
            text(&badge_label)
                .size(10.0)
                .color(Color::rgba(0.8, 0.9, 1.0, 1.0)),
        )
}

fn color_swatch(swatch_label: &'static str, color: Color) -> impl ElementBuilder {
    div()
        .flex_row()
        .justify_between()
        .items_center()
        .mt(4.0)
        .child(
            text(swatch_label)
                .size(12.0)
                .color(Color::rgba(0.7, 0.7, 0.7, 1.0)),
        )
        .child(
            div()
                .flex_row()
                .gap(8.0)
                .items_center()
                .child(
                    div()
                        .w(24.0)
                        .h(24.0)
                        .rounded(4.0)
                        .bg(color)
                        .border(1.0, Color::rgba(0.4, 0.4, 0.4, 1.0)),
                )
                .child(
                    text(&format!(
                        "rgba({:.0}, {:.0}, {:.0}, {:.2})",
                        color.r * 255.0,
                        color.g * 255.0,
                        color.b * 255.0,
                        color.a
                    ))
                    .size(10.0)
                    .color(Color::rgba(0.6, 0.6, 0.6, 1.0)),
                ),
        )
}

fn divider() -> impl ElementBuilder {
    div().w_full().h(1.0).bg(Color::rgba(0.2, 0.2, 0.25, 1.0))
}
