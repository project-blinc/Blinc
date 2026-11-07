# Skeleton animation with glTF + `blinc_canvas_kit`.

Loads Sketchfab's buster_drone (39 meshes, 92 nodes, one 25-second
`Start_Liftoff` clip), runs the clip through `blinc_game_kit::skeleton` each
frame, and renders the result with `SceneKit3D`. Asset load is
non-blocking: the UI paints a loading overlay while a background
thread parses the glTF, then flips a `scene_ready` signal that
the overlay's `Stateful` subtree dismisses itself on.

The model is "Buster Drone" by LaVADraGoN
(<https://sketchfab.com/3d-models/buster-drone-294e79652f494130ad2ab00a13fdbafd>),
licensed CC-BY-4.0 (<http://creativecommons.org/licenses/by/4.0/>).
Full attribution alongside the asset in `assets/3d/buster_drone/license.txt`.

Controls:
- **Drag**: orbit
- **Scroll**: zoom
- **Space**: pause / resume
- **R**: reset clip time
- **Left / Right**: scrub ±1 frame

```sh
cargo run -p blinc_app_examples --example gltf_animation_demo \
    --features gltf --release
```

<div class="blinc-example" data-example="gltf_animation_demo" data-title="Blinc gltf_animation_demo example"></div>

> **Tip:** Some demos are best viewed in a full browser window. Click "Open in a new tab" below for the full experience.

[Open in a new tab](../../examples/gltf_animation_demo/index.html) · [View source on GitHub](https://github.com/project-blinc/Blinc/blob/main/examples/blinc_app_examples/examples/gltf_animation_demo.rs)
