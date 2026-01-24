//! Camera Controls Demo
//!
//! Demonstrates blinc_3d camera controller utilities:
//! - OrbitController - Orbits around a target (Blender-style)
//! - FlyController - Free-flight WASD + mouse look
//! - FollowController - Follows an entity with offset and damping
//! - DroneController - Smooth cinematic camera paths
//! - CameraShake - Trauma-based screen shake effect
//!
//! Run with: cargo run -p blinc_3d --example camera_demo --features "sdf"

use blinc_3d::prelude::*;
use blinc_3d::sdf::{SdfCamera, SdfScene};
use blinc_3d::utils::camera::*;
use blinc_animation::SpringConfig;
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::events::event_types;
use blinc_core::Transform;
use blinc_layout::stateful::ButtonState;
use std::f32::consts::PI;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Blinc 3D - Camera Controls Demo".to_string(),
        width: 1200,
        height: 800,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

// ============================================================================
// Camera Mode Definitions
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum CameraMode {
    #[default]
    Orbit,
    Fly,
    Follow,
    Drone,
}

impl CameraMode {
    fn name(&self) -> &'static str {
        match self {
            CameraMode::Orbit => "Orbit",
            CameraMode::Fly => "Fly",
            CameraMode::Follow => "Follow",
            CameraMode::Drone => "Drone",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            CameraMode::Orbit => "Right-drag to rotate, scroll to zoom, middle-drag to pan",
            CameraMode::Fly => "WASD to move, mouse to look, Shift for speed",
            CameraMode::Follow => "Camera follows target with smooth damping",
            CameraMode::Drone => "Smooth cinematic path between waypoints",
        }
    }
}

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
                    text("Camera Controls Demo")
                        .size(24.0)
                        .weight(FontWeight::Bold)
                        .color(Color::WHITE),
                )
                .child(
                    text("Orbit, Fly, Follow, and Drone camera modes with shake effects")
                        .size(14.0)
                        .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
                ),
        )
        .child(shake_button())
}

fn main_content(width: f32, height: f32) -> impl ElementBuilder {
    div()
        .w(width)
        .h(height)
        .flex_row()
        .child(viewport_area(width - 300.0, height))
        .child(control_panel())
}

fn viewport_area(width: f32, height: f32) -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(move |ctx| {
        // Camera mode signal
        let mode = ctx.use_signal("camera_mode", || CameraMode::Orbit);

        // Time tracking
        let time = ctx.use_signal("time", || 0.0f32);
        let dt = 0.016f32; // ~60fps
        time.update(|t| t + dt);

        // Mouse tracking for camera input
        let mouse_x = ctx.use_signal("mouse_x", || 0.0f32);
        let mouse_y = ctx.use_signal("mouse_y", || 0.0f32);
        let prev_mouse_x = ctx.use_signal("prev_mouse_x", || 0.0f32);
        let prev_mouse_y = ctx.use_signal("prev_mouse_y", || 0.0f32);
        let is_dragging = ctx.use_signal("dragging", || false);

        // Controller state signals (stored as tuples/primitives for serialization)
        let orbit_azimuth = ctx.use_signal("orbit_azimuth", || 0.0f32);
        let orbit_elevation = ctx.use_signal("orbit_elevation", || 0.3f32);
        let orbit_distance = ctx.use_signal("orbit_distance", || 8.0f32);
        let orbit_target_x = ctx.use_signal("orbit_target_x", || 0.0f32);
        let orbit_target_z = ctx.use_signal("orbit_target_z", || 0.0f32);

        let fly_pos_x = ctx.use_signal("fly_pos_x", || 0.0f32);
        let fly_pos_y = ctx.use_signal("fly_pos_y", || 3.0f32);
        let fly_pos_z = ctx.use_signal("fly_pos_z", || 8.0f32);
        let fly_yaw = ctx.use_signal("fly_yaw", || 0.0f32);
        let fly_pitch = ctx.use_signal("fly_pitch", || 0.0f32);

        let follow_offset_x = ctx.use_signal("follow_offset_x", || -3.0f32);
        let follow_offset_y = ctx.use_signal("follow_offset_y", || 4.0f32);
        let follow_offset_z = ctx.use_signal("follow_offset_z", || 5.0f32);

        let drone_time = ctx.use_signal("drone_time", || 0.0f32);

        // Shake state
        let shake_trauma = ctx.use_signal("shake_trauma", || 0.0f32);
        let shake_noise_time = ctx.use_signal("shake_noise_time", || 0.0f32);

        // Handle mouse events
        if let Some(event) = ctx.event() {
            match event.event_type {
                event_types::POINTER_DOWN => {
                    is_dragging.set(true);
                    prev_mouse_x.set(event.local_x);
                    prev_mouse_y.set(event.local_y);
                }
                event_types::POINTER_UP => {
                    is_dragging.set(false);
                }
                event_types::POINTER_MOVE => {
                    mouse_x.set(event.local_x);
                    mouse_y.set(event.local_y);
                }
                _ => {}
            }
        }

        // Calculate mouse delta
        let mouse_delta_x = if is_dragging.get() {
            mouse_x.get() - prev_mouse_x.get()
        } else {
            0.0
        };
        let mouse_delta_y = if is_dragging.get() {
            mouse_y.get() - prev_mouse_y.get()
        } else {
            0.0
        };

        // Update prev mouse for next frame
        if is_dragging.get() {
            prev_mouse_x.set(mouse_x.get());
            prev_mouse_y.set(mouse_y.get());
        }

        // Build CameraInput
        let camera_input = CameraInput {
            mouse_delta: Vec2::new(mouse_delta_x, mouse_delta_y),
            scroll_delta: 0.0,
            keys: CameraKeys::default(),
            primary_pressed: false,
            secondary_pressed: is_dragging.get(),
            middle_pressed: false,
        };

        // Update camera based on mode using actual controllers
        let (cam_pos, cam_target) = match mode.get() {
            CameraMode::Orbit => {
                // Create and update OrbitController
                let mut orbit = OrbitController::new(
                    Vec3::new(orbit_target_x.get(), 0.0, orbit_target_z.get()),
                    orbit_distance.get(),
                );
                orbit.azimuth = orbit_azimuth.get();
                orbit.elevation = orbit_elevation.get();
                orbit.target_azimuth = orbit_azimuth.get();
                orbit.target_elevation = orbit_elevation.get();
                orbit.target_distance = orbit_distance.get();
                orbit.rotation_speed = 0.005;
                orbit.damping = 0.0; // No damping for immediate response

                let current = CameraTransform::default();
                let update_ctx = CameraUpdateContext {
                    dt,
                    elapsed: time.get(),
                    current: &current,
                };

                let transform = orbit.update(&update_ctx, &camera_input);

                // Store updated state
                orbit_azimuth.set(orbit.azimuth);
                orbit_elevation.set(orbit.elevation);
                orbit_distance.set(orbit.distance);

                (transform.position, orbit.target)
            }

            CameraMode::Fly => {
                // Create and update FlyController
                let mut fly = FlyController::new(Vec3::new(
                    fly_pos_x.get(),
                    fly_pos_y.get(),
                    fly_pos_z.get(),
                ));
                fly.yaw = fly_yaw.get();
                fly.pitch = fly_pitch.get();
                fly.look_speed = 0.003;

                let current = CameraTransform::default();
                let update_ctx = CameraUpdateContext {
                    dt,
                    elapsed: time.get(),
                    current: &current,
                };

                let transform = fly.update(&update_ctx, &camera_input);

                // Store updated state
                fly_pos_x.set(fly.position.x);
                fly_pos_y.set(fly.position.y);
                fly_pos_z.set(fly.position.z);
                fly_yaw.set(fly.yaw);
                fly_pitch.set(fly.pitch);

                let forward = fly.forward();
                let target = Vec3::new(
                    fly.position.x + forward.x * 5.0,
                    fly.position.y + forward.y * 5.0,
                    fly.position.z + forward.z * 5.0,
                );
                (transform.position, target)
            }

            CameraMode::Follow => {
                // Create and update FollowController
                let mut follow = FollowController::new();
                follow.offset = Vec3::new(
                    follow_offset_x.get(),
                    follow_offset_y.get(),
                    follow_offset_z.get(),
                );
                follow.position_damping = 0.05;

                // Animate target moving in a circle
                let t = time.get() * 0.5;
                let target_pos = Vec3::new(t.sin() * 2.0, 0.0, t.cos() * 2.0);
                follow.set_target(target_pos, None);

                let current = CameraTransform {
                    position: Vec3::new(
                        target_pos.x + follow_offset_x.get(),
                        target_pos.y + follow_offset_y.get(),
                        target_pos.z + follow_offset_z.get(),
                    ),
                    ..Default::default()
                };
                let update_ctx = CameraUpdateContext {
                    dt,
                    elapsed: time.get(),
                    current: &current,
                };

                let transform = follow.update(&update_ctx, &camera_input);
                (transform.position, target_pos)
            }

            CameraMode::Drone => {
                // Create DroneController with waypoints
                let mut drone = DroneController::new();
                drone.looping = true;
                drone.speed = 1.0;

                // Add cinematic waypoints
                drone.add_waypoint(
                    CameraWaypoint::at(Vec3::new(8.0, 3.0, 8.0), 0.0)
                        .looking_at(Vec3::ZERO)
                        .with_ease(0.5),
                );
                drone.add_waypoint(
                    CameraWaypoint::at(Vec3::new(-8.0, 5.0, 4.0), 4.0)
                        .looking_at(Vec3::ZERO)
                        .with_ease(0.7),
                );
                drone.add_waypoint(
                    CameraWaypoint::at(Vec3::new(0.0, 8.0, -8.0), 3.0)
                        .looking_at(Vec3::ZERO)
                        .with_ease(0.6),
                );
                drone.add_waypoint(
                    CameraWaypoint::at(Vec3::new(8.0, 3.0, 8.0), 3.0)
                        .looking_at(Vec3::ZERO)
                        .with_ease(0.5),
                );

                // Manually advance time
                drone.seek(drone_time.get());
                drone.resume();

                let current = CameraTransform::default();
                let update_ctx = CameraUpdateContext {
                    dt,
                    elapsed: time.get(),
                    current: &current,
                };

                let transform = drone.update(&update_ctx, &camera_input);

                // Update drone time for next frame
                let new_time = (drone_time.get() + dt) % 10.0; // Total duration ~10s
                drone_time.set(new_time);

                (transform.position, Vec3::ZERO)
            }
        };

        // Apply camera shake using CameraShake utility
        let mut shake = CameraShake::medium();
        shake.set_trauma(shake_trauma.get());

        // Decay and update shake
        let decay_rate = 1.5;
        let new_trauma = (shake_trauma.get() - decay_rate * dt).max(0.0);
        shake_trauma.set(new_trauma);

        // Get shake offset
        let shake_offset = if new_trauma > 0.001 {
            shake_noise_time.update(|t| t + dt * shake.frequency);
            let intensity = new_trauma * new_trauma;
            let noise_t = shake_noise_time.get();

            Vec3::new(
                simple_noise(noise_t, 0) * shake.max_offset.x * intensity,
                simple_noise(noise_t, 1) * shake.max_offset.y * intensity,
                simple_noise(noise_t, 2) * shake.max_offset.z * intensity,
            )
        } else {
            Vec3::ZERO
        };

        let final_pos = Vec3::new(
            cam_pos.x + shake_offset.x,
            cam_pos.y + shake_offset.y,
            cam_pos.z + shake_offset.z,
        );

        // Create SDF scene
        let scene = create_demo_scene();
        let camera = SdfCamera {
            position: final_pos,
            target: cam_target,
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
                    scene.render(draw_ctx, &camera, bounds, time.get());
                })
                .w_full()
                .h_full(),
            )
            .child(
                // Overlay: current mode indicator
                div()
                    .absolute()
                    .left(16.0)
                    .bottom(16.0)
                    .px(12.0)
                    .py(8.0)
                    .bg(Color::rgba(0.0, 0.0, 0.0, 0.7))
                    .rounded(8.0)
                    .child(
                        text(mode.get().description())
                            .size(12.0)
                            .color(Color::rgba(1.0, 1.0, 1.0, 0.8)),
                    ),
            )
    })
}

/// Simple noise function for shake
fn simple_noise(t: f32, seed: u32) -> f32 {
    let s = seed as f32;
    let a = (t * 1.0 + s * 12.9898).sin() * 43758.5453;
    let b = (t * 2.3 + s * 78.233).sin() * 24634.6345;
    let c = (t * 0.7 + s * 45.164).sin() * 83456.2345;
    (a + b + c).fract() * 2.0 - 1.0
}

fn control_panel() -> impl ElementBuilder {
    div()
        .w(300.0)
        .h_full()
        .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
        .p(16.0)
        .flex_col()
        .gap(16.0)
        .child(
            text("Camera Mode")
                .size(18.0)
                .weight(FontWeight::SemiBold)
                .color(Color::WHITE),
        )
        .child(camera_mode_selector())
        .child(divider())
        .child(
            text("Controller Settings")
                .size(18.0)
                .weight(FontWeight::SemiBold)
                .color(Color::WHITE),
        )
        .child(camera_settings())
        .child(divider())
        .child(
            text("Shake Presets")
                .size(18.0)
                .weight(FontWeight::SemiBold)
                .color(Color::WHITE),
        )
        .child(shake_presets())
}

fn camera_mode_selector() -> impl ElementBuilder {
    div()
        .flex_col()
        .gap(8.0)
        .child(camera_mode_button(CameraMode::Orbit))
        .child(camera_mode_button(CameraMode::Fly))
        .child(camera_mode_button(CameraMode::Follow))
        .child(camera_mode_button(CameraMode::Drone))
}

fn camera_mode_button(mode: CameraMode) -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(move |ctx| {
        let current_mode = ctx.use_signal("camera_mode", || CameraMode::Orbit);
        let is_selected = current_mode.get() == mode;

        let (bg, text_color) = match (ctx.state(), is_selected) {
            (_, true) => (Color::rgba(0.3, 0.5, 0.9, 1.0), Color::WHITE),
            (ButtonState::Hovered, false) => (Color::rgba(0.2, 0.2, 0.25, 1.0), Color::WHITE),
            _ => (
                Color::rgba(0.12, 0.12, 0.16, 1.0),
                Color::rgba(0.8, 0.8, 0.8, 1.0),
            ),
        };

        if let Some(event) = ctx.event() {
            if event.event_type == event_types::POINTER_UP {
                current_mode.set(mode);
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
            .w_full()
            .h(48.0)
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
                    .w(8.0)
                    .h(8.0)
                    .rounded(4.0)
                    .bg(if is_selected {
                        Color::WHITE
                    } else {
                        Color::rgba(0.4, 0.4, 0.4, 1.0)
                    }),
            )
            .child(
                div()
                    .flex_col()
                    .gap(2.0)
                    .child(
                        text(mode.name())
                            .size(14.0)
                            .weight(FontWeight::Medium)
                            .color(text_color),
                    )
                    .child(
                        text(mode.description())
                            .size(10.0)
                            .color(Color::rgba(0.6, 0.6, 0.6, 1.0)),
                    ),
            )
    })
}

fn camera_settings() -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(|ctx| {
        let mode = ctx.use_signal("camera_mode", || CameraMode::Orbit);

        let content = match mode.get() {
            CameraMode::Orbit => {
                let distance = ctx.use_signal("orbit_distance", || 8.0f32);
                let azimuth = ctx.use_signal("orbit_azimuth", || 0.0f32);
                let elevation = ctx.use_signal("orbit_elevation", || 0.3f32);

                div()
                    .flex_col()
                    .gap(8.0)
                    .child(setting_row("Distance", &format!("{:.1}", distance.get())))
                    .child(setting_row(
                        "Azimuth",
                        &format!("{:.2} rad", azimuth.get()),
                    ))
                    .child(setting_row(
                        "Elevation",
                        &format!("{:.2} rad", elevation.get()),
                    ))
                    .child(setting_row("Damping", "0.1"))
            }
            CameraMode::Fly => {
                let yaw = ctx.use_signal("fly_yaw", || 0.0f32);
                let pitch = ctx.use_signal("fly_pitch", || 0.0f32);
                let pos_y = ctx.use_signal("fly_pos_y", || 3.0f32);

                div()
                    .flex_col()
                    .gap(8.0)
                    .child(setting_row("Yaw", &format!("{:.2} rad", yaw.get())))
                    .child(setting_row("Pitch", &format!("{:.2} rad", pitch.get())))
                    .child(setting_row("Height", &format!("{:.1}", pos_y.get())))
                    .child(setting_row("Move Speed", "5.0"))
            }
            CameraMode::Follow => {
                let offset_y = ctx.use_signal("follow_offset_y", || 4.0f32);
                let offset_z = ctx.use_signal("follow_offset_z", || 5.0f32);

                div()
                    .flex_col()
                    .gap(8.0)
                    .child(setting_row(
                        "Height Offset",
                        &format!("{:.1}", offset_y.get()),
                    ))
                    .child(setting_row(
                        "Distance Offset",
                        &format!("{:.1}", offset_z.get()),
                    ))
                    .child(setting_row("Position Damping", "0.05"))
                    .child(setting_row("Rotation Damping", "0.08"))
            }
            CameraMode::Drone => {
                let drone_time = ctx.use_signal("drone_time", || 0.0f32);
                let progress = (drone_time.get() / 10.0 * 100.0) as u32;

                div()
                    .flex_col()
                    .gap(8.0)
                    .child(setting_row("Waypoints", "4"))
                    .child(setting_row("Total Duration", "10.0s"))
                    .child(setting_row("Progress", &format!("{}%", progress)))
                    .child(setting_row("Looping", "Yes"))
            }
        };

        content
    })
}

fn setting_row(label: &'static str, value: &str) -> impl ElementBuilder {
    let value = value.to_string();
    div()
        .flex_row()
        .justify_between()
        .items_center()
        .child(
            text(label)
                .size(13.0)
                .color(Color::rgba(0.7, 0.7, 0.7, 1.0)),
        )
        .child(
            div()
                .px(8.0)
                .py(4.0)
                .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
                .rounded(4.0)
                .child(text(&value).size(13.0).color(Color::WHITE)),
        )
}

fn shake_presets() -> impl ElementBuilder {
    div()
        .flex_col()
        .gap(8.0)
        .child(shake_preset_button("Light", 0.3))
        .child(shake_preset_button("Medium", 0.6))
        .child(shake_preset_button("Heavy", 1.0))
}

fn shake_preset_button(label: &'static str, trauma_amount: f32) -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(move |ctx| {
        let shake_trauma = ctx.use_signal("shake_trauma", || 0.0f32);

        let bg = match ctx.state() {
            ButtonState::Hovered => Color::rgba(0.25, 0.25, 0.3, 1.0),
            ButtonState::Pressed => Color::rgba(0.15, 0.15, 0.2, 1.0),
            _ => Color::rgba(0.18, 0.18, 0.22, 1.0),
        };

        if let Some(event) = ctx.event() {
            if event.event_type == event_types::POINTER_UP {
                shake_trauma.set(trauma_amount);
            }
        }

        div()
            .w_full()
            .h(36.0)
            .bg(bg)
            .rounded(6.0)
            .px(12.0)
            .flex_row()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .child(
                text(label)
                    .size(13.0)
                    .color(Color::rgba(0.9, 0.9, 0.9, 1.0)),
            )
            .child(
                text(&format!("{:.0}%", trauma_amount * 100.0))
                    .size(12.0)
                    .color(Color::rgba(0.6, 0.6, 0.6, 1.0)),
            )
    })
}

fn shake_button() -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(|ctx| {
        let shake_trauma = ctx.use_signal("shake_trauma", || 0.0f32);

        let bg = match ctx.state() {
            ButtonState::Hovered => Color::rgba(0.9, 0.4, 0.4, 1.0),
            ButtonState::Pressed => Color::rgba(0.7, 0.2, 0.2, 1.0),
            _ => Color::rgba(0.8, 0.3, 0.3, 1.0),
        };

        if let Some(event) = ctx.event() {
            if event.event_type == event_types::POINTER_UP {
                shake_trauma.set(1.0);
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
                text("Shake!")
                    .size(14.0)
                    .weight(FontWeight::SemiBold)
                    .color(Color::WHITE),
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
// Scene Creation
// ============================================================================

fn create_demo_scene() -> SdfScene {
    let mut scene = SdfScene::new();

    // Create a floor plane (large flat box: 10 wide x 0.1 tall x 10 deep)
    let floor = SdfScene::box_node(Vec3::new(5.0, 0.05, 5.0))
        .at(Vec3::new(0.0, -1.0, 0.0));

    // Create some objects to demonstrate camera movement
    let sphere1 = SdfScene::sphere(0.8).at(Vec3::new(0.0, 0.5, 0.0));

    let sphere2 = SdfScene::sphere(0.5).at(Vec3::new(2.0, 0.3, 1.0));

    let cube1 = SdfScene::cube(0.6).at(Vec3::new(-2.0, 0.3, -1.0));

    let torus = SdfScene::torus(1.0, 0.3)
        .rotated(Vec3::new(PI / 4.0, 0.0, 0.0))
        .at(Vec3::new(0.0, 1.5, -2.0));

    // Combine all shapes
    let objects = SdfScene::union(sphere1, sphere2);
    let objects = SdfScene::union(objects, cube1);
    let objects = SdfScene::union(objects, torus);
    let scene_root = SdfScene::union(floor, objects);

    scene.set_root(scene_root);
    scene
}
