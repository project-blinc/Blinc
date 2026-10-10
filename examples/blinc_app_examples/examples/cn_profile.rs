//! cn_demo's UI built offscreen, and what a frame costs with it: the build,
//! a full paint through the real renderer, and the frame after a tab switch,
//! stage by stage. Run it under a sampling profiler to see where the time
//! goes:
//!
//! ```sh
//! cargo run -p blinc_app_examples --example cn_profile --features cn --release
//! samply record target/release/examples/cn_profile
//! ```
//!
//! `CN_PROFILE_ROUNDS` sets how many tab switches it times (default 10).

#[path = "cn_demo.rs"]
#[allow(dead_code)]
mod cn_demo;

use std::sync::{Arc, Mutex};
use std::time::Instant;

use blinc_app::BlincApp;
use blinc_app::windowed::WindowedContext;
use blinc_layout::render_state::RenderState;
use blinc_layout::renderer::RenderTree;

const W: u32 = 900;
const H: u32 = 900;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let bundle = cn_demo::theme_bundle();
    let css = bundle.css_sources.clone();
    blinc_theme::ThemeState::init(bundle, blinc_theme::ColorScheme::Dark);
    blinc_layout::init_text_measurer();

    let mut ctx = WindowedContext::headless(W as f32, H as f32);
    let handle = ctx.animations.lock().unwrap().handle();
    blinc_animation::set_global_scheduler(handle.clone());
    blinc_layout::render_state::set_global_scheduler(handle);
    if !blinc_core::BlincContextState::is_initialized() {
        blinc_core::BlincContextState::init(
            blinc_core::reactive::global_graph(),
            Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
    }
    for source in &css {
        ctx.add_css(source);
    }

    let t = Instant::now();
    let ui = cn_demo::build_ui(&mut ctx);
    let built = ms(t);
    let t = Instant::now();
    let mut tree = RenderTree::from_element(&ui);
    let tree_built = ms(t);
    let t = Instant::now();
    if let Some(sheet) = ctx.stylesheet.clone() {
        tree.set_stylesheet_arc(sheet);
    }
    tree.apply_all_stylesheet_styles();
    tree.compute_layout(W as f32, H as f32);
    tree.start_all_css_animations();
    let styled = ms(t);
    println!("build_ui {built:.1}ms, tree {tree_built:.1}ms, style+layout {styled:.1}ms");

    let mut app = BlincApp::new().expect("GPU");
    let texture = app.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("cn_profile surface"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: app.texture_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut rs = RenderState::new(Arc::clone(&ctx.animations));
    rs.set_viewport(0.0, 0.0, W as f32, H as f32);

    let paint = |app: &mut BlincApp, tree: &RenderTree, rs: &RenderState| {
        let t = Instant::now();
        app.render_tree_with_motion_opt(tree, rs, &view, Some(&texture), W, H, false)
            .expect("render");
        let _ = app.read_texture_to_rgba8(&texture, W, H);
        ms(t)
    };
    for i in 0..3 {
        println!("full paint {i}: {:.1}ms", paint(&mut app, &tree, &rs));
    }

    // The state the first tabs strip is bound to; a click on a trigger
    // writes it.
    let selected = ctx.use_state_keyed("simple_tab", || "tab1".to_string());
    let rounds: usize = std::env::var("CN_PROFILE_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    for round in 0..rounds {
        let label = if round % 2 == 0 { "tab2" } else { "tab1" };
        let t = Instant::now();
        selected.set(label.to_string());
        let events = ms(t);
        let t = Instant::now();
        let updates = blinc_layout::take_pending_partial_prop_updates();
        let n_updates = updates.len();
        tree.apply_partial_property_updates(updates);
        let drain = ms(t);
        let t = Instant::now();
        tree.process_pending_subtree_rebuilds();
        let rebuilds = ms(t);
        // What the windowed loop does when an update needs layout.
        let t = Instant::now();
        tree.apply_stylesheet_layout_overrides();
        let overrides = ms(t);
        let t = Instant::now();
        tree.compute_layout(W as f32, H as f32);
        let layout = ms(t);
        let t = Instant::now();
        tree.apply_flip_transitions();
        tree.update_flip_bounds();
        rs.begin_stable_motion_frame();
        tree.initialize_motion_animations(&mut rs);
        rs.end_stable_motion_frame();
        tree.start_all_css_animations();
        let after_layout = ms(t);
        let painted = paint(&mut app, &tree, &rs);
        println!(
            "select {label}: set {events:6.1}ms  drain {drain:6.1}ms ({n_updates} updates)  rebuilds {rebuilds:6.1}ms  overrides {overrides:6.1}ms  layout {layout:6.1}ms  motion+css {after_layout:6.1}ms  paint {painted:6.1}ms"
        );
    }
}
