# End-to-end 3D demo wiring Blinc's SceneKit3D renderer up to

- [`blinc_canvas_kit::SceneKit3D`] — the camera + light + mesh
  dispatch front-end used by any Blinc app that wants to drop
  3D content into a `canvas()`. Same primitive demos use for a
  single spinning cube scale up to a full character rig
  unchanged.
- [`blinc_game_kit::gltf`] — glTF 2.0 loader. Parses the file tree once at
  startup into a `GltfScene` (meshes, nodes, skeletons,
  animation clips) that the demo holds behind an `Arc<Mutex<>>`
  and borrows per frame.
- [`blinc_game_kit::skeleton`] — runtime poser. `animate_scene_nodes`
  samples the clip's TRS channels into the live node tree;
  `scene_skinning_data` walks the posed tree to build the joint
  matrices the mesh shader consumes;
  `animate_scene_morph_weights` drives per-node blend-shape
  weights for facial expression.

The asset is "The Strangler" by Jungle Jim (CC-BY-4.0;
<https://sketchfab.com/3d-models/the-strangler-06d56efabf7445e89bb1bf41a99d08cc>),
shipped in the repo for offline reproducibility. Full
attribution lives alongside the asset in
`examples/.../assets/3d/the_strangler/license.txt`.

Per-frame flow:

1. `animate_scene_nodes(&mut scene, anim, t)` — writes sampled
   TRS onto scene nodes
2. `scene_skinning_data(&scene, &skeleton)` — returns
   `SkinningData` (joint world matrices × inverse-bind)
3. `animate_scene_morph_weights(anim, t)` — returns a
   `HashMap<node_index, Vec<f32>>` of current weights
4. For each drawable node: shallow-clone its `MeshData`
   (`Arc<Vec<_>>` inners → refcount bumps, no vertex copy),
   stamp the frame's skinning + morph_weights, dispatch via
   `DrawContext::draw_mesh_data`.

Ordering (OPAQUE before BLEND) is enforced framework-side in
`blinc_app::dispatch_pending_meshes`, so the demo submits in
scene-graph order without its own sort.

```sh
cargo run -p blinc_app_examples --example strangler_demo \
    --features gltf --release
```

[`blinc_canvas_kit::SceneKit3D`]: https://docs.rs/blinc_canvas_kit/latest/blinc_canvas_kit/struct.SceneKit3D.html
[`blinc_game_kit`]: https://github.com/project-blinc/blinc_game_kit
[`blinc_game_kit`]: https://github.com/project-blinc/blinc_game_kit

<div class="blinc-example" data-example="strangler_demo" data-title="Blinc strangler_demo example"></div>

> **Tip:** Some demos are best viewed in a full browser window. Click "Open in a new tab" below for the full experience.

[Open in a new tab](../../examples/strangler_demo/index.html) · [View source on GitHub](https://github.com/project-blinc/Blinc/blob/main/examples/blinc_app_examples/examples/strangler_demo.rs)
