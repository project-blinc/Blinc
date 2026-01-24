//! Particle Effects Demo
//!
//! Demonstrates blinc_3d particle system utilities:
//! - Preset effects: fire, smoke, sparks, rain, snow, explosion, magic, confetti
//! - Emitter shapes: Point, Sphere, Cone, Box, Circle
//! - Force affectors: Gravity, Wind, Vortex, Drag, Turbulence
//! - Blend modes: Alpha, Additive, Multiply
//!
//! This demo shows how to configure ParticleSystem components.
//! In a full application, attach ParticleSystem to entities for GPU rendering.
//!
//! Run with: cargo run -p blinc_3d --example particles_demo --features "sdf"

use blinc_3d::prelude::*;
use blinc_3d::sdf::{SdfCamera, SdfScene};
use blinc_3d::utils::particles::*;
use blinc_animation::SpringConfig;
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::events::event_types;
use blinc_core::Transform;
use blinc_layout::stateful::ButtonState;
use blinc_layout::widgets::elapsed_ms;
use std::f32::consts::PI;

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
    Fire,
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
            ParticleEffect::Fire => "Fire",
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
            ParticleEffect::Fire => "Rising flames with additive blending",
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

    fn color(&self) -> Color {
        match self {
            ParticleEffect::Fire => Color::rgb(1.0, 0.4, 0.1),
            ParticleEffect::Smoke => Color::rgb(0.5, 0.5, 0.5),
            ParticleEffect::Sparks => Color::rgb(1.0, 0.8, 0.3),
            ParticleEffect::Rain => Color::rgb(0.5, 0.6, 0.8),
            ParticleEffect::Snow => Color::rgb(0.9, 0.95, 1.0),
            ParticleEffect::Explosion => Color::rgb(1.0, 0.6, 0.2),
            ParticleEffect::Magic => Color::rgb(0.6, 0.4, 1.0),
            ParticleEffect::Confetti => Color::rgb(1.0, 0.5, 0.8),
            ParticleEffect::Custom => Color::rgb(0.4, 0.8, 0.4),
        }
    }

    /// Get the actual ParticleSystem from blinc_3d utilities
    fn to_system(&self) -> ParticleSystem {
        match self {
            ParticleEffect::Fire => ParticleSystem::fire(),
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
                .with_colors(Color::WHITE, Color::rgba(1.0, 1.0, 1.0, 0.0)),
        }
    }
}

const ALL_EFFECTS: [ParticleEffect; 9] = [
    ParticleEffect::Fire,
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
// UI Building
// ============================================================================

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.06, 0.06, 0.1, 1.0))
        .flex_col()
        .child(header())
        .child(main_content(ctx.width, ctx.height - 80.0))
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
                    text("ParticleSystem presets and configuration")
                        .size(14.0)
                        .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
                ),
        )
        .child(burst_button())
}

fn main_content(width: f32, height: f32) -> impl ElementBuilder {
    div()
        .w(width)
        .h(height)
        .flex_row()
        .child(viewport_area(width - 360.0, height))
        .child(control_panel())
}

fn viewport_area(width: f32, height: f32) -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(move |ctx| {
        // Effect selection signal
        let effect = ctx.use_signal("particle_effect", || ParticleEffect::Fire);

        // Use elapsed_ms for animation - doesn't trigger re-renders
        let time = elapsed_ms() as f32 / 1000.0;

        // Camera angle offset from mouse drag
        let angle = ctx.use_signal("angle", || 0.3f32);

        // Handle mouse drag for camera rotation
        if let Some(event) = ctx.event() {
            if event.event_type == event_types::POINTER_MOVE && ctx.state() == ButtonState::Pressed
            {
                angle.update(|a| a + event.local_x * 0.0005);
            }
        }

        // Calculate camera position
        let cam_angle = angle.get() + time * 0.05;
        let cam_x = cam_angle.sin() * 6.0;
        let cam_z = cam_angle.cos() * 6.0;

        // Get current particle system configuration
        let system = effect.get().to_system();

        // Create simple scene for context
        let scene = create_demo_scene();
        let camera = SdfCamera {
            position: Vec3::new(cam_x, 3.0, cam_z),
            target: Vec3::new(0.0, 1.0, 0.0),
            up: Vec3::new(0.0, 1.0, 0.0),
            fov: 0.8,
        };

        div()
            .w(width)
            .h(height)
            .bg(Color::rgba(0.02, 0.02, 0.05, 1.0))
            .cursor_pointer()
            .child(
                canvas(move |draw_ctx, bounds| {
                    scene.render(draw_ctx, &camera, bounds, time);
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
                    .child(code_example(effect.get())),
            )
    })
}

fn code_example(effect: ParticleEffect) -> impl ElementBuilder {
    let code = match effect {
        ParticleEffect::Fire => "let particles = ParticleSystem::fire();\nworld.spawn().insert(particles);",
        ParticleEffect::Smoke => "let particles = ParticleSystem::smoke();\nworld.spawn().insert(particles);",
        ParticleEffect::Sparks => "let particles = ParticleSystem::sparks();\nworld.spawn().insert(particles);",
        ParticleEffect::Rain => "let particles = ParticleSystem::rain();\nworld.spawn().insert(particles);",
        ParticleEffect::Snow => "let particles = ParticleSystem::snow();\nworld.spawn().insert(particles);",
        ParticleEffect::Explosion => {
            "let mut particles = ParticleSystem::explosion();\nparticles.burst(100); // Trigger burst\nworld.spawn().insert(particles);"
        }
        ParticleEffect::Magic => "let particles = ParticleSystem::magic();\nworld.spawn().insert(particles);",
        ParticleEffect::Confetti => {
            "let mut particles = ParticleSystem::confetti();\nparticles.burst(500); // Launch confetti\nworld.spawn().insert(particles);"
        }
        ParticleEffect::Custom => {
            "let particles = ParticleSystem::new()\n    .with_emitter(EmitterShape::Sphere { radius: 0.5 })\n    .with_emission_rate(100.0)\n    .with_lifetime(1.0, 3.0)\n    .with_force(Force::gravity(Vec3::new(0.0, -9.8, 0.0)));\nworld.spawn().insert(particles);"
        }
    };

    div()
        .px(8.0)
        .py(6.0)
        .bg(Color::rgba(0.1, 0.12, 0.15, 1.0))
        .rounded(4.0)
        .child(
            text(code)
                .size(11.0)
                .color(Color::rgba(0.7, 0.9, 0.7, 1.0)),
        )
}

fn control_panel() -> impl ElementBuilder {
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
                .child(effect_selector())
                .child(divider())
                .child(
                    text("System Configuration")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(system_properties())
                .child(divider())
                .child(
                    text("Emitter Details")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(emitter_info())
                .child(divider())
                .child(
                    text("Force Affectors")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(forces_info()),
        )
}

fn effect_selector() -> impl ElementBuilder {
    div().flex_col().gap(6.0).children(
        ALL_EFFECTS
            .iter()
            .map(|&effect| effect_button(effect))
            .collect::<Vec<_>>(),
    )
}

fn effect_button(effect: ParticleEffect) -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(move |ctx| {
        let current_effect = ctx.use_signal("particle_effect", || ParticleEffect::Fire);
        let is_selected = current_effect.get() == effect;

        let (bg, text_color) = match (ctx.state(), is_selected) {
            (_, true) => (effect.color(), Color::WHITE),
            (ButtonState::Hovered, false) => (Color::rgba(0.2, 0.2, 0.25, 1.0), Color::WHITE),
            _ => (
                Color::rgba(0.12, 0.12, 0.16, 1.0),
                Color::rgba(0.8, 0.8, 0.8, 1.0),
            ),
        };

        if let Some(event) = ctx.event() {
            if event.event_type == event_types::POINTER_UP {
                current_effect.set(effect);
            }
        }

        let scale = ctx.use_spring(
            "scale",
            if ctx.state() == ButtonState::Pressed {
                0.97
            } else {
                1.0
            },
            SpringConfig::snappy(),
        );

        div()
            .w_full()
            .h(52.0)
            .bg(bg)
            .rounded(8.0)
            .px(12.0)
            .flex_row()
            .items_center()
            .gap(12.0)
            .cursor_pointer()
            .transform(Transform::scale(scale, scale))
            .child(
                div()
                    .w(28.0)
                    .h(28.0)
                    .rounded(6.0)
                    .bg(if is_selected {
                        Color::rgba(1.0, 1.0, 1.0, 0.2)
                    } else {
                        effect.color().with_alpha(0.3)
                    })
                    .justify_center().items_center()
                    .child(effect_icon(effect)),
            )
            .child(
                div()
                    .flex_col()
                    .gap(2.0)
                    .child(
                        text(effect.name())
                            .size(14.0)
                            .weight(FontWeight::Medium)
                            .color(text_color),
                    )
                    .child(
                        text(effect.description())
                            .size(10.0)
                            .color(Color::rgba(0.6, 0.6, 0.6, 1.0)),
                    ),
            )
    })
}

fn effect_icon(effect: ParticleEffect) -> impl ElementBuilder {
    let color = effect.color();
    div().w(12.0).h(12.0).rounded(6.0).bg(color)
}

fn system_properties() -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(|ctx| {
        let effect = ctx.use_signal("particle_effect", || ParticleEffect::Fire);
        let system = effect.get().to_system();

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
                &format!(
                    "{:.1} - {:.1}",
                    system.start_speed.0, system.start_speed.1
                ),
            ))
            .child(property_row(
                "Start Size",
                &format!(
                    "{:.2} - {:.2}",
                    system.start_size.0, system.start_size.1
                ),
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
    })
}

fn emitter_info() -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(|ctx| {
        let effect = ctx.use_signal("particle_effect", || ParticleEffect::Fire);
        let system = effect.get().to_system();

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
    })
}

fn forces_info() -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(|ctx| {
        let effect = ctx.use_signal("particle_effect", || ParticleEffect::Fire);
        let system = effect.get().to_system();

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
                div()
                    .mt(8.0)
                    .child(
                        text("Particle Colors")
                            .size(14.0)
                            .weight(FontWeight::Medium)
                            .color(Color::WHITE),
                    ),
            )
            .child(color_swatch("Start", system.start_color))
            .child(color_swatch("End", system.end_color));

        content
    })
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

fn property_row(label: &'static str, value: &str) -> impl ElementBuilder {
    let value = value.to_string();
    div()
        .flex_row()
        .justify_between()
        .items_center()
        .child(
            text(label)
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

fn force_badge(label: &str) -> impl ElementBuilder {
    let label = label.to_string();
    div()
        .px(8.0)
        .py(4.0)
        .bg(Color::rgba(0.2, 0.25, 0.35, 1.0))
        .rounded(4.0)
        .child(
            text(&label)
                .size(10.0)
                .color(Color::rgba(0.8, 0.9, 1.0, 1.0)),
        )
}

fn color_swatch(label: &'static str, color: Color) -> impl ElementBuilder {
    div()
        .flex_row()
        .justify_between()
        .items_center()
        .mt(4.0)
        .child(
            text(label)
                .size(12.0)
                .color(Color::rgba(0.7, 0.7, 0.7, 1.0)),
        )
        .child(
            div().flex_row().gap(8.0).items_center().child(
                div()
                    .w(24.0)
                    .h(24.0)
                    .rounded(4.0)
                    .bg(color)
                    .border(1.0, Color::rgba(0.4, 0.4, 0.4, 1.0)),
            ).child(
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

fn burst_button() -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(|ctx| {
        let effect = ctx.use_signal("particle_effect", || ParticleEffect::Fire);
        let system = effect.get().to_system();
        let is_burst_effect = !system.looping;

        let bg = match ctx.state() {
            ButtonState::Hovered => Color::rgba(0.5, 0.8, 0.4, 1.0),
            ButtonState::Pressed => Color::rgba(0.3, 0.6, 0.2, 1.0),
            _ => Color::rgba(0.4, 0.7, 0.3, 1.0),
        };

        if let Some(event) = ctx.event() {
            if event.event_type == event_types::POINTER_UP {
                // In a real app: system.burst(100);
                tracing::info!("Burst triggered for {}", effect.get().name());
            }
        }

        let scale = ctx.use_spring(
            "scale",
            if ctx.state() == ButtonState::Pressed {
                0.95
            } else {
                1.0
            },
            SpringConfig::snappy(),
        );

        div()
            .px(16.0)
            .py(10.0)
            .bg(bg)
            .rounded(8.0)
            .cursor_pointer()
            .transform(Transform::scale(scale, scale))
            .child(
                div()
                    .flex_col()
                    .items_center()
                    .child(
                        text("Burst!")
                            .size(14.0)
                            .weight(FontWeight::SemiBold)
                            .color(Color::WHITE),
                    )
                    .child(if is_burst_effect {
                        text("(one-shot effect)")
                            .size(10.0)
                            .color(Color::rgba(1.0, 1.0, 1.0, 0.7))
                    } else {
                        text("system.burst(100)")
                            .size(10.0)
                            .color(Color::rgba(1.0, 1.0, 1.0, 0.7))
                    }),
            )
    })
}

fn divider() -> impl ElementBuilder {
    div()
        .w_full()
        .h(1.0)
        .bg(Color::rgba(0.2, 0.2, 0.25, 1.0))
}

// ============================================================================
// Scene Creation (simple scene for visual context)
// ============================================================================

fn create_demo_scene() -> SdfScene {
    let mut scene = SdfScene::new();

    // Ground plane (thin box: 10 wide x 0.1 tall x 10 deep)
    let floor = SdfScene::box_node(Vec3::new(5.0, 0.05, 5.0))
        .at(Vec3::new(0.0, -0.5, 0.0));

    // Emitter marker sphere
    let emitter = SdfScene::sphere(0.2).at(Vec3::new(0.0, 0.5, 0.0));

    // Simple pedestal
    let pedestal = SdfScene::cylinder(0.5, 0.3).at(Vec3::new(0.0, 0.0, 0.0));

    let combined = SdfScene::union(floor, pedestal);
    let combined = SdfScene::union(combined, emitter);

    scene.set_root(combined);
    scene
}
