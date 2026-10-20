use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::{create_test_texture, read_rendered_pixels};

fn app() -> BlincApp {
    BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })
    .expect("offscreen GPU required")
}

fn render(context: &mut RenderContext, expected: [u8; 4]) {
    let (texture, view) = create_test_texture(context.device(), context.texture_format(), 160, 120);
    context.renderer.resize(160, 120);
    context
        .renderer
        .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
    context.dispatch_dynamic_batch_overlay(&view);
    let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
    let actual = page.get_pixel(55, 55).0;
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
        "{actual:?}, expected {expected:?}"
    );
    assert_eq!(page.get_pixel(5, 5).0, [0, 0, 255, 255]);
}

#[test]
#[ignore] // Requires GPU
fn motion_layers_dispatch_preserves_group_overlap() {
    let mut app = app();
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.push_motion_subtree();
    paint.push_layer(blinc_core::LayerConfig::new().opacity(0.5));
    paint.fill_rect(
        Rect::new(20.0, 20.0, 60.0, 60.0),
        0.0.into(),
        Color::RED.into(),
    );
    paint.fill_rect(
        Rect::new(40.0, 40.0, 60.0, 60.0),
        0.0.into(),
        Color::GREEN.into(),
    );
    paint.pop_layer();
    paint.pop_motion_subtree();
    app.context().cached_dynamic_batch = Some(paint.take_dynamic_batch());
    render(app.context(), [0, 128, 127, 255]);
    let context = app.context();
    let blinc_gpu::primitives::LayerCommand::Push { config } = &mut context
        .cached_dynamic_batch
        .as_mut()
        .unwrap()
        .layer_commands[0]
        .command
    else {
        panic!("motion group lost")
    };
    config.opacity = 1.0;
    render(context, [0, 255, 0, 255]);
}

#[test]
#[ignore] // Requires GPU
fn motion_layers_path_only_nested_groups_reach_the_overlay() {
    use blinc_core::{LayerConfig, Path};
    use blinc_gpu::primitives::LayerCommand;
    let mut app = app();
    let mut batch = PrimitiveBatch::new();
    for _ in 0..2 {
        batch.push_layer_command(LayerCommand::Push {
            config: LayerConfig::new().opacity(0.5),
        });
    }
    batch.push_path(blinc_gpu::path::tessellate_fill(
        &Path::rect(Rect::new(20.0, 20.0, 60.0, 60.0)),
        &Color::RED.into(),
    ));
    for _ in 0..2 {
        batch.push_layer_command(LayerCommand::Pop);
    }
    app.context().cached_dynamic_batch = Some(batch);
    render(app.context(), [64, 0, 191, 255]);
}

#[test]
#[ignore] // Requires GPU
fn motion_layers_css_record_in_dynamic_batch_requests_full_paint() {
    use blinc_layout::renderer::CssAnimPaintMeta;
    let mut app = app();
    let tree = blinc_layout::RenderTree::from_element(&div().id("dynamic-css-record"));
    let root = tree.root().unwrap();
    tree.css_anim_paint_records_mut().insert(
        root,
        CssAnimPaintMeta {
            dynamic_batch: true,
            stable_id: tree.stable_id(root).unwrap(),
            primitive_range: 0..1,
            layer_push_index: Some(0),
            last_opacity: 0.5,
            last_translate: (0.0, 0.0),
            last_scale: (1.0, 1.0),
            last_rotation_rad: 0.0,
            last_rotate_x_rad: 0.0,
            last_rotate_y_rad: 0.0,
            last_background_color: None,
            last_border_color: None,
            last_corner_radius: [0.0; 4],
            last_border_width: 0.0,
            last_shadow_params: [0.0; 4],
            last_shadow_color: [0.0; 4],
            last_filter_a: [0.0; 4],
            last_filter_b: [1.0, 1.0, 1.0, 0.0],
            centre: (50.0, 50.0),
            last_screen_aabb: Some([0.0, 0.0, 100.0, 100.0]),
        },
    );
    let context = app.context();
    let mut sentinel = PrimitiveBatch::new();
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.fill_rect(
        Rect::new(20.0, 20.0, 60.0, 60.0),
        0.0.into(),
        Color::RED.into(),
    );
    sentinel.primitives = paint.take_batch().primitives;
    let original = sentinel.primitives[0].color;
    context.cached_bg_batch = Some(sentinel);
    assert!(matches!(
        context.apply_css_deltas(&tree, 1.0),
        CssDeltaResult::Bail
    ));
    assert_eq!(
        context.cached_bg_batch.as_ref().unwrap().primitives[0].color,
        original
    );
}

#[test]
#[ignore] // Requires GPU
fn motion_layers_binding_patch_uses_dynamic_index_and_recovers_from_zero() {
    use blinc_animation::{AnimatedValue, AnimationScheduler};
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    let mut app = app();
    let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
    let opacity = Arc::new(Mutex::new(AnimatedValue::with_default(
        scheduler.lock().unwrap().handle(),
        1.0,
    )));
    let ui = div()
        .w(160.0)
        .h(120.0)
        .bg(Color::BLUE)
        .child(
            div()
                .absolute()
                .left(140.0)
                .top(100.0)
                .w(5.0)
                .h(5.0)
                .bg(Color::WHITE)
                .blur(1.0),
        )
        .child(
            div()
                .w(120.0)
                .h(120.0)
                .child(
                    div()
                        .absolute()
                        .left(20.0)
                        .top(20.0)
                        .w(60.0)
                        .h(60.0)
                        .bg(Color::RED),
                )
                .child(
                    div()
                        .absolute()
                        .left(40.0)
                        .top(40.0)
                        .w(60.0)
                        .h(60.0)
                        .bg(Color::GREEN),
                )
                .motion()
                .opacity(opacity.clone()),
        );
    let mut tree = blinc_layout::RenderTree::from_element(&ui);
    tree.compute_layout(160.0, 120.0);
    let state = blinc_layout::RenderState::new(scheduler);
    // Exercise the public non-compositor dispatch as well as the cached helper.
    let (texture, view) = create_test_texture(app.device(), app.texture_format(), 160, 120);
    app.render_tree_with_motion(&tree, &state, &view, 160, 120)
        .unwrap();
    let page = read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120);
    assert!(
        page.get_pixel(55, 55)
            .0
            .iter()
            .zip([0u8, 255, 0, 255])
            .all(|(a, b)| a.abs_diff(b) <= 2)
    );
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    tree.render_with_motion(&mut paint, &state);
    let background = paint.take_batch();
    let dynamic = paint.take_dynamic_batch();
    assert_eq!(background.layer_commands.len(), 2);
    let records = tree.composite_bindings();
    assert_eq!(records.len(), 1);
    let index = records.values().next().unwrap().layer_push_index.unwrap();
    assert!(matches!(
        dynamic.layer_commands.get(index).map(|e| &e.command),
        Some(blinc_gpu::primitives::LayerCommand::Push { .. })
    ));
    drop(records);
    let alphas: Vec<_> = dynamic.primitives.iter().map(|p| p.color[3]).collect();
    let context = app.context();
    context.cached_bg_batch = Some(background);
    context.cached_dynamic_batch = Some(dynamic);
    render(context, [0, 255, 0, 255]);
    for (value, expected) in [
        (0.5, [0, 128, 127, 255]),
        (0.0, [0, 0, 255, 255]),
        (0.75, [0, 191, 64, 255]),
        (0.25, [0, 64, 191, 255]),
    ] {
        opacity.lock().unwrap().set_immediate(value);
        assert!(context.apply_binding_deltas(&tree, 1.0));
        assert_eq!(
            context
                .cached_dynamic_batch
                .as_ref()
                .unwrap()
                .primitives
                .iter()
                .map(|p| p.color[3])
                .collect::<Vec<_>>(),
            alphas
        );
        render(context, expected);
    }
    let blinc_gpu::primitives::LayerCommand::Push { config } =
        &context.cached_bg_batch.as_ref().unwrap().layer_commands[0].command
    else {
        panic!("static group lost")
    };
    assert_eq!(config.opacity, 1.0);
}
