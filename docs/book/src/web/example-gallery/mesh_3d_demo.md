# 3D Mesh Demo — renders the Khronos glTF `DamagedHelmet` sample model

Demonstrates:
- `blinc_canvas_kit::SceneKit3D` — orbit camera + light rig wrapped
  around a `canvas` element, with drag/scroll input wired for free.
- `DrawContext::draw_mesh_data` — the direct-render mesh path. The
  canvas closure just calls `ctx.draw_mesh_data(&mesh, transform)`;
  everything behind that (camera capture, pending-mesh queue,
  GpuPaintContext → GpuRenderer dispatch, PBR shading) is plumbing.
- Inline glTF loading — no external `gltf` crate dep. The sample
  model has a fixed layout (single mesh, single primitive, packed
  f32 attributes at known bufferView offsets, u16 indices), so
  parsing is a handful of offset reads plus a `blinc_image::ImageData`
  call for the albedo texture.
- Non-blocking asset loading. On desktop the mesh + HDR decode is
  cheap and runs synchronously; on wasm the `WebAssetLoader`
  preload is background-spawned by the wrapper, so `build_ui`
  returns before any asset is cached. A `spawn_local` polling loop
  waits for the preload, then populates a shared slot that the
  Stateful viewport wrapper swaps the loading overlay out for.

Controls: drag the viewport to orbit, scroll to zoom.

# Damaged Helmet

<https://github.com/KhronosGroup/glTF-Sample-Models/tree/master/2.0/DamagedHelmet>

## License Information

Battle Damaged Sci-fi Helmet - PBR by
[theblueturtle_](https://sketchfab.com/theblueturtle_), published
under a Creative Commons Attribution-NonCommercial license.

<https://sketchfab.com/models/b81008d513954189a063ff901f7abfe4>

<div class="blinc-example" data-example="mesh_3d_demo" data-title="Blinc mesh_3d_demo example"></div>

> **Tip:** Some demos are best viewed in a full browser window. Click "Open in a new tab" below for the full experience.

[Open in a new tab](../../examples/mesh_3d_demo/index.html) · [View source on GitHub](https://github.com/project-blinc/Blinc/blob/main/examples/blinc_app_examples/examples/mesh_3d_demo.rs)
