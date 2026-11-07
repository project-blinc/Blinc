//! 3D Mesh Demo — renders the Khronos glTF `DamagedHelmet` sample model
//! through Blinc's mesh pipeline.
//!
//! Demonstrates:
//! - `blinc_canvas_kit::SceneKit3D` — orbit camera + light rig wrapped
//!   around a `canvas` element, with drag/scroll input wired for free.
//! - `DrawContext::draw_mesh_data` — the direct-render mesh path. The
//!   canvas closure just calls `ctx.draw_mesh_data(&mesh, transform)`;
//!   everything behind that (camera capture, pending-mesh queue,
//!   GpuPaintContext → GpuRenderer dispatch, PBR shading) is plumbing.
//! - Inline glTF loading — no external `gltf` crate dep. The sample
//!   model has a fixed layout (single mesh, single primitive, packed
//!   f32 attributes at known bufferView offsets, u16 indices), so
//!   parsing is a handful of offset reads plus a `blinc_image::ImageData`
//!   call for the albedo texture.
//! - Non-blocking asset loading. On desktop the mesh + HDR decode is
//!   cheap and runs synchronously; on wasm the `WebAssetLoader`
//!   preload is background-spawned by the wrapper, so `build_ui`
//!   returns before any asset is cached. A `spawn_local` polling loop
//!   waits for the preload, then populates a shared slot that the
//!   Stateful viewport wrapper swaps the loading overlay out for.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p blinc_app_examples --example mesh_3d_demo --features canvas
//! ```
//!
//! Controls: drag the viewport to orbit, scroll to zoom.
//!
//! # Damaged Helmet
//!
//! <https://github.com/KhronosGroup/glTF-Sample-Models/tree/master/2.0/DamagedHelmet>
//!
//! ## License Information
//!
//! Battle Damaged Sci-fi Helmet - PBR by
//! [theblueturtle_](https://sketchfab.com/theblueturtle_), published
//! under a Creative Commons Attribution-NonCommercial license.
//!
//! <https://sketchfab.com/models/b81008d513954189a063ff901f7abfe4>

use std::sync::{Arc, OnceLock};

use blinc_animation::get_scheduler;
use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;
use blinc_canvas_kit::InputState;
use blinc_canvas_kit::prelude::*;
use blinc_core::{
    AlphaMode, Color, Light, Mat4, Material, MeshData, State, TextureData, Vec3, Vertex,
};

const HELMET_GLTF_DIR: &str = "examples/blinc_app_examples/examples/assets/3d/DamagedHelmet";
const HDR_PATH: &str = "examples/blinc_app_examples/examples/assets/3d/rogland_clear_night_2k.hdr";

// glTF bufferView offsets — fixed in the Khronos sample repo.
const IDX_OFFSET: usize = 0;
const IDX_COUNT: usize = 46356;
const POS_OFFSET: usize = 92712;
const NRM_OFFSET: usize = 267384;
const UV_OFFSET: usize = 442056;
const VTX_COUNT: usize = 14556;

// ─────────────────────────────────────────────────────────────────────────────
// Asset loading — `None` on any failure so callers can retry.
// ─────────────────────────────────────────────────────────────────────────────

fn try_load_texture(path: &str) -> Option<TextureData> {
    let img = blinc_image::ImageData::load(blinc_image::ImageSource::File(path.into())).ok()?;
    let (width, height) = img.dimensions();
    Some(TextureData::new(img.into_pixels(), width, height))
}

fn try_load_helmet() -> Option<MeshData> {
    let bin_path = format!("{HELMET_GLTF_DIR}/DamagedHelmet.bin");
    let bin = blinc_platform::assets::load_asset(&bin_path).ok()?;

    let read_f32 =
        |off: usize| -> f32 { f32::from_le_bytes(bin[off..off + 4].try_into().unwrap()) };
    let read_u16 =
        |off: usize| -> u16 { u16::from_le_bytes(bin[off..off + 2].try_into().unwrap()) };

    // The sample model's glTF node applies +90° around X at runtime
    // (quaternion `(0.7071, 0, 0, 0.7071)`). We bake the rotation into
    // vertices here so the demo transform stays `Mat4::IDENTITY` and
    // the orbit camera rotates around the origin cleanly.
    let rotate_x90 = |p: [f32; 3]| -> [f32; 3] { [p[0], -p[2], p[1]] };

    let mut vertices = Vec::with_capacity(VTX_COUNT);
    for i in 0..VTX_COUNT {
        let pos = rotate_x90([
            read_f32(POS_OFFSET + i * 12),
            read_f32(POS_OFFSET + i * 12 + 4),
            read_f32(POS_OFFSET + i * 12 + 8),
        ]);
        let nrm = rotate_x90([
            read_f32(NRM_OFFSET + i * 12),
            read_f32(NRM_OFFSET + i * 12 + 4),
            read_f32(NRM_OFFSET + i * 12 + 8),
        ]);
        // DamagedHelmet's V is in [1, 2] (legal glTF, wrap expected).
        // Blinc's sampler is clamp-to-edge, so normalise via `fract`.
        let raw_u = read_f32(UV_OFFSET + i * 8);
        let raw_v = read_f32(UV_OFFSET + i * 8 + 4);
        let uv = [raw_u - raw_u.floor(), raw_v - raw_v.floor()];
        vertices.push(Vertex::new(pos).with_normal(nrm).with_uv(uv));
    }

    let mut indices = Vec::with_capacity(IDX_COUNT);
    for i in 0..IDX_COUNT {
        indices.push(read_u16(IDX_OFFSET + i * 2) as u32);
    }

    let albedo_tex = try_load_texture(&format!("{HELMET_GLTF_DIR}/Default_albedo.jpg"))?;
    let normal_tex = try_load_texture(&format!("{HELMET_GLTF_DIR}/Default_normal.jpg"))?;
    let metal_rough_tex =
        try_load_texture(&format!("{HELMET_GLTF_DIR}/Default_metalRoughness.jpg"))?;
    let emissive_tex = try_load_texture(&format!("{HELMET_GLTF_DIR}/Default_emissive.jpg"))?;
    let ao_tex = try_load_texture(&format!("{HELMET_GLTF_DIR}/Default_AO.jpg"))?;

    let material = Material {
        base_color: [1.0, 1.0, 1.0, 1.0],
        metallic: 1.0,
        roughness: 1.0,
        emissive: [1.0, 1.0, 1.0],
        base_color_texture: Some(albedo_tex),
        normal_map: Some(normal_tex),
        normal_scale: 1.0,
        metallic_roughness_texture: Some(metal_rough_tex),
        emissive_texture: Some(emissive_tex),
        occlusion_texture: Some(ao_tex),
        occlusion_strength: 1.0,
        displacement_map: None,
        displacement_scale: 0.0,
        unlit: false,
        alpha_mode: AlphaMode::Opaque,
        alpha_cutoff: 0.5,
        receives_shadows: false,
        casts_shadows: false,
        texture_transform: None,
    };

    Some(MeshData {
        vertices: std::sync::Arc::new(vertices),
        indices: std::sync::Arc::new(indices),
        material,
        skin: None,
        morph_targets: std::sync::Arc::new(Vec::new()),
        morph_weights: Vec::new(),
    })
}

/// Load the helmet mesh + HDR bytes. Both go through the platform
/// asset loader, so the wasm polling loop retries on a tick until
/// preload's cache populates. Returns `None` as soon as any fetch
/// misses. HDR is handed back as raw bytes — `SceneKit3D::set_hdri`
/// does the cubemap prefilter on the loader thread.
fn try_load_assets() -> Option<(Arc<MeshData>, Vec<u8>)> {
    let helmet = try_load_helmet()?;
    let hdr_bytes = blinc_platform::assets::load_asset(HDR_PATH).ok()?;
    Some((Arc::new(helmet), hdr_bytes))
}

fn register_scheduler_tick() {
    let scheduler_for_redraw = get_scheduler();
    get_scheduler().register_tick_callback(move |_dt: f32| {
        scheduler_for_redraw.request_redraw();
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Async asset handle — slot populated by the loader, `scene_ready`
// flipped once `slot.set` has succeeded.
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AsyncAssets {
    slot: Arc<OnceLock<Arc<MeshData>>>,
    input: InputState,
}

impl AsyncAssets {
    fn new() -> Self {
        Self {
            slot: Arc::new(OnceLock::new()),
            input: InputState::new(),
        }
    }

    /// Spawn the loader. `kit` is cloned in so `set_hdri` can be
    /// called once HDR bytes resolve — `SceneKit3D` is now `Send`,
    /// so the clone moves into the worker cleanly.
    fn spawn_load(&self, kit: SceneKit3D, scene_ready: State<bool>) {
        let slot = self.slot.clone();

        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            let Some((helmet, hdr_bytes)) = try_load_assets() else {
                tracing::error!("mesh_3d_demo: asset load failed");
                return;
            };
            register_scheduler_tick();
            tracing::info!("mesh_3d_demo: applying HDRI ({} bytes)", hdr_bytes.len());
            kit.set_hdri(&hdr_bytes, 256);
            let _ = slot.set(helmet);
            scene_ready.set(true);
            get_scheduler().request_redraw();
        });

        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                register_scheduler_tick();
                loop {
                    if let Some((helmet, hdr_bytes)) = try_load_assets() {
                        tracing::info!("mesh_3d_demo: applying HDRI ({} bytes)", hdr_bytes.len());
                        kit.set_hdri(&hdr_bytes, 256);
                        let _ = slot.set(helmet);
                        scene_ready.set(true);
                        get_scheduler().request_redraw();
                        break;
                    }
                    // Exit the retry loop once the platform loader
                    // declares every preload fetch has landed (success
                    // or failure). Without this the wasm task would
                    // spin forever if any asset permanently 404s —
                    // the overlay would never dismiss and the user
                    // would have no feedback that something is wrong.
                    if blinc_platform::assets::preload_settled() {
                        tracing::error!(
                            "mesh_3d_demo: preload settled but assets still missing — \
                             check that DamagedHelmet/ and rogland_clear_night_2k.hdr \
                             are served under examples/blinc_app_examples/examples/assets/"
                        );
                        break;
                    }
                    sleep_ms(100).await;
                }
            });
        }
    }

    fn get(&self) -> Option<&Arc<MeshData>> {
        self.slot.get()
    }
}

#[cfg(target_arch = "wasm32")]
async fn sleep_ms(ms: u32) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;
    // `setTimeout` exists on a page and on a worker's global scope; `window()` is `None` in a
    // worker, where a promise that never resolves would stall the load loop for good.
    let global = js_sys::global();
    let set_timeout = js_sys::Reflect::get(&global, &"setTimeout".into())
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    let promise = js_sys::Promise::new(
        &mut |resolve: js_sys::Function, _reject| match &set_timeout {
            Some(set_timeout) => {
                let _ = set_timeout.call2(&global, &resolve, &JsValue::from(ms as i32));
            }
            None => {
                let _ = resolve.call0(&JsValue::NULL);
            }
        },
    );
    let _ = JsFuture::from(promise).await;
}

// ─────────────────────────────────────────────────────────────────────────────
// Main
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "3D Mesh Demo — DamagedHelmet".to_string(),
        width: 960,
        height: 720,
        animation_fps_cap: Some(60),
        ..Default::default()
    };

    blinc_app::windowed::WindowedApp::run(config, build_ui)
}

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    // Scene-ready signal — flipped by the loader once helmet + HDR
    // are resident. The overlay's Stateful subtree watches it.
    let scene_ready = ctx.use_state_keyed("mesh_3d_scene_ready", || false);

    // Kit + viewport built ONCE — same pattern as `gltf_animation_demo`.
    // Default procedural studio cubemap until the loader thread calls
    // `kit.set_hdri(bytes, 256)` with the asset-loaded HDR (the kit's
    // `Arc<RwLock<…>>` env field picks the swap up on the next frame).
    let kit = SceneKit3D::new("mesh_3d_demo")
        .with_camera(
            OrbitCamera::default()
                .with_distance(3.2)
                .with_elevation(0.2)
                .with_azimuth(0.4)
                .with_target(Vec3::new(0.0, 0.0, 0.187)),
        )
        .with_light(Light::Directional {
            direction: Vec3::new(-0.4, -1.0, -0.3).normalize(),
            color: Color::WHITE,
            intensity: 2.5,
            cast_shadows: false,
        });

    // Shared assets handle — spawn_load runs exactly once thanks to
    // `use_state_keyed`'s single-init contract. The kit is cloned in
    // so the loader can call `set_hdri` once the HDR bytes resolve.
    let assets = ctx
        .use_state_keyed("mesh_3d_assets", {
            let scene_ready = scene_ready.clone();
            let kit = kit.clone();
            move || {
                let a = AsyncAssets::new();
                a.spawn_load(kit, scene_ready);
                a
            }
        })
        .try_get()
        .expect("assets handle should exist after use_state_keyed init");

    let assets_ren = assets.clone();
    let viewport = kit
        .with_input(&assets.input)
        .element(move |ctx, _bounds| {
            let Some(helmet) = assets_ren.get() else {
                return;
            };
            ctx.draw_mesh_data(helmet.clone(), Mat4::default());
        })
        .id("mesh_3d_viewport");

    // Loading overlay — same structure every refresh, `.hidden()`
    // toggled on ready. See `gltf_animation_demo` for why the shape
    // is identical between branches.
    let scene_ready_ov = scene_ready.clone();
    let overlay = stateful::<NoState>()
        .deps([scene_ready.signal_id()])
        .on_state(move |_ctx| {
            let mut d = div()
                .absolute()
                .top(0.0)
                .left(0.0)
                .w_full()
                .h_full()
                .bg(Color::rgba(0.0, 0.0, 0.0, 0.6))
                .flex_col()
                .items_center()
                .justify_center()
                .child(
                    text("Loading helmet…")
                        .size(18.0)
                        .color(Color::rgba(0.95, 0.95, 0.95, 1.0)),
                );
            if scene_ready_ov.get() {
                d = d.hidden();
            }
            d
        });

    let viewport_stack = div()
        .relative()
        .flex_grow()
        .w_full()
        .overflow_clip()
        .child(viewport)
        .child(overlay);

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.05, 0.06, 0.10, 1.0))
        .flex_col()
        .child(header_bar())
        .child(viewport_stack)
        .child(hint_bar())
}

fn header_bar() -> Div {
    div()
        .w_full()
        .h(48.0)
        .bg(Color::rgba(0.09, 0.10, 0.14, 1.0))
        .flex_row()
        .items_center()
        .justify_center()
        .gap(12.0)
        .child(
            text("DamagedHelmet — glTF 2.0 PBR sample")
                .size(16.0)
                .weight(FontWeight::SemiBold)
                .color(Color::rgba(0.95, 0.95, 1.0, 1.0)),
        )
}

fn hint_bar() -> Div {
    div()
        .w_full()
        .h(32.0)
        .bg(Color::rgba(0.07, 0.08, 0.12, 1.0))
        .flex_row()
        .items_center()
        .justify_center()
        .child(
            text("Drag to orbit · Scroll to zoom")
                .size(12.0)
                .color(Color::rgba(0.55, 0.58, 0.70, 1.0)),
        )
}
