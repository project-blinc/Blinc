# KHR_texture_transform

Loads Poly Haven's `marble_cliff_02` asset (CC0) — a displaced
rock chunk with a tiling PBR material — and showcases the
`KHR_texture_transform` glTF extension support added in
`blinc_core::TextureTransform` + `blinc_gpu::mesh_pipeline` +
`blinc_game_kit::gltf::parse_material`.

The asset's glTF JSON was patched to include
`"extensions": { "KHR_texture_transform": { "scale": [3, 3] } }`
on every texture binding, so `parse_material` reads a 3× tile
transform and the shader multiplies UVs accordingly before every
sample. Press **T** to toggle the transform off for a side-by-side
comparison — toggling swaps between the parsed `Material` and a
clone with `texture_transform: None`, exercising the shader's
identity path.

# License

Asset: `marble_cliff_02` by Rob Tuytel, published under CC0
(<https://polyhaven.com/a/marble_cliff_02>).

<div class="blinc-example" data-example="texture_transform_demo" data-title="Blinc texture_transform_demo example"></div>

> **Tip:** Some demos are best viewed in a full browser window. Click "Open in a new tab" below for the full experience.

[Open in a new tab](../../examples/texture_transform_demo/index.html) · [View source on GitHub](https://github.com/project-blinc/Blinc/blob/main/examples/blinc_app_examples/examples/texture_transform_demo.rs)
