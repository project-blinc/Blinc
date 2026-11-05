# SceneKit3D

`SceneKit3D` is the 3D counterpart to [`CanvasKit`](./interactive.md). It wraps an orbit camera, a set of lights, an environment map, and a mesh list into a single handle you can mount as a `Div`. Ideal for model viewers, glTF playback, and any 3D content where you don't want to hand-wire matrix math and event plumbing.

For a lower-level intro (raw `MeshData`, shaders, materials), see [3D Rendering](../advanced/3d-rendering.md).

## Minimal example

```rust
use blinc_canvas_kit::prelude::*;
use blinc::core::{Material, MeshData};
use std::sync::Arc;

fn build_ui() -> impl ElementBuilder {
    let kit = SceneKit3D::new("viewer")
        .with_environment(generate_studio_environment(256))
        .with_camera(OrbitCamera::default()
            .with_distance(5.0)
            .with_elevation(0.2));

    // Load a mesh — replace with glTF loading in real apps.
    let mesh: Arc<MeshData> = load_my_mesh();
    kit.add_mesh(mesh);

    div()
        .w_full()
        .h_full()
        .child(kit.element_auto())
}
```

`element_auto()` returns a `Div` that draws every registered mesh each frame, wires orbit (drag) + zoom (scroll) to the camera, and redraws continuously. `element(|ctx, bounds| ...)` is the manual equivalent if you want to mix in custom primitives around the scene.

## Orbit camera

`OrbitCamera` is a spherical-coordinate camera around a target point:

```rust
let cam = OrbitCamera::default()
    .with_distance(5.0)      // Radius from target
    .with_azimuth(0.5)       // Horizontal angle, radians
    .with_elevation(0.3)     // Vertical angle, radians
    .with_target(Vec3::ZERO) // Look-at point
    .with_fov_y(60f32.to_radians());
```

Runtime mutation via the kit:

```rust
kit.update_camera(|cam| {
    cam.orbit(dx_rad, dy_rad);  // Mouse delta in radians
    cam.zoom(1.1);              // > 1 zooms out, < 1 zooms in
});
```

`kit.camera()` snapshots the current camera; `kit.camera_signal()` gives a signal id you can subscribe to for external UI synced to camera state.

## Lights

Lights are added via `with_light(...)` (builder) or `set_lights(vec)` (runtime):

```rust
use blinc::core::Light;

let kit = SceneKit3D::new("viewer")
    .with_light(Light::directional([0.5, -1.0, 0.3], [1.0, 0.95, 0.9], 1.2))
    .with_light(Light::point([2.0, 1.0, 2.0], [0.4, 0.7, 1.0], 5.0));
```

See [3D Rendering](../advanced/3d-rendering.md) for the full `Light` API (directional, point, spot, with shadow toggles).

## Environment maps

Two helpers produce cubemaps ready to feed into `with_environment`:

```rust
// Procedural studio lighting (gradient sky + soft ground)
let env = generate_studio_environment(256);

// HDRI (Radiance `.hdr` file) → cubemap + irradiance + specular IBL
let hdr_bytes = std::fs::read("studio.hdr")?;
let env = generate_hdri_environment(&hdr_bytes, 512);

let kit = SceneKit3D::new("viewer").with_environment(env);
```

Shortcut: `with_hdri(hdr_bytes, face_size)` does the read + decode in one call. Both `set_environment` and `set_hdri` are available for async loading — spawn a background task, build the `EnvironmentData`, then apply.

## Mesh management

| Method | Purpose |
|--------|---------|
| `kit.add(geometry, material)` | Add from `(Vec<Vertex>, Vec<u32>)` + material. Returns a `MeshHandle`. |
| `kit.add_mesh(Arc<MeshData>)` | Add a pre-built mesh (glTF load, procedural generator). Returns a `MeshHandle`. |
| `kit.set_position(handle, pos)` | Update mesh world-space translation. |
| `kit.set_rotation(handle, euler)` | Update Euler rotation (radians). |
| `kit.set_scale(handle, scale)` | Update scale. |
| `kit.set_visible(handle, visible)` | Toggle without removing. |

All mutations are `&self` — the kit is `Clone` and `Send`, so background loader threads can push meshes into it as assets resolve. The render closure reads the latest state each frame.

## Input wiring

For key-driven camera moves (WASD fly-through, etc.), pair the kit with `blinc_input::InputState`:

```rust
use blinc_input::InputState;

let input = InputState::new();
let kit = SceneKit3D::new("viewer").with_input(&input);
```

`with_input` automates the two error-prone pieces users tend to forget:
- `capture_input` on the outer viewport `Div` so key/pointer/scroll feed the state
- `InputState::frame_end()` at the end of every paint pass so edge-triggered queries (`is_key_just_pressed`) stay one-frame-scoped

## Gallery

- [3D Mesh Demo](../web/example-gallery/mesh_3d_demo.md) — Khronos `DamagedHelmet` with orbit + HDRI
- [Skeleton animation with glTF](../web/example-gallery/gltf_animation_demo.md) — Sketchfab `buster_drone`, 39 meshes, 92 nodes, 25s skeletal clip
- [End-to-end 3D demo](../web/example-gallery/strangler_demo.md) — SceneKit3D wired into a Blinc app
