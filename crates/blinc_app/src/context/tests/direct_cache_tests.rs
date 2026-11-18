use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::read_rendered_pixels;
use blinc_animation::AnimationScheduler;
use blinc_layout::canvas::{Canvas, CanvasRenderFn};
use blinc_layout::div::{ElementBuilder, ElementTypeId};
use blinc_layout::{LayoutNodeId, LayoutTree};
use std::cell::Cell;
use std::rc::Rc;

struct PaintMarker(Canvas);

impl ElementBuilder for PaintMarker {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.0.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.0.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[]
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Canvas
    }

    fn canvas_render_info(&self) -> Option<CanvasRenderFn> {
        self.0.canvas_render_info()
    }

    fn is_static_canvas(&self) -> bool {
        true
    }
}

#[test]
#[ignore] // Requires GPU
fn direct_cache_keeps_static_groups_without_canvas_replay() {
    check_direct_cache(false);
}

#[test]
#[ignore] // Requires GPU
fn direct_cache_refreshes_canvas_after_order_resolution() {
    check_direct_cache(true);
}

#[test]
#[ignore] // Requires GPU
fn direct_cache_keeps_motion_text_rows_across_successive_frames() {
    use blinc_core::reactive::signal;
    for node_table in [false, true] {
        blinc_theme::ThemeState::init(
            blinc_theme::HybridTheme::bundle(),
            blinc_theme::ColorScheme::Light,
        );
        let mut app = BlincApp::with_config(BlincConfig {
            node_table,
            motion_promotion: false,
            sample_count: 1,
            ..Default::default()
        })
        .expect("offscreen GPU required");
        let paints = Rc::new(Cell::new(0));
        let count = paints.clone();
        let marker = PaintMarker(
            blinc_layout::canvas(move |ctx, _| {
                count.set(count.get() + 1);
                ctx.fill_rect(
                    Rect::new(0.0, 0.0, 8.0, 8.0),
                    0.0.into(),
                    Color::WHITE.into(),
                );
            })
            .w(8.0)
            .h(8.0),
        );
        let offset = signal(0.0f32);
        let ui = div().w(120.0).h(80.0).bg(Color::BLUE).child(marker).child(
            div()
                .absolute()
                .left(12.0)
                .top(12.0)
                .w(60.0)
                .h(32.0)
                .bg(Color::RED)
                .bind_transform_from(offset, |x| blinc_core::Transform::translate(x, 0.0))
                .child(text("Rows").size(14.0)),
        );
        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(120.0, 80.0);
        let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
        let target = app.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("retained direct motion text"),
            size: wgpu::Extent3d {
                width: 120,
                height: 80,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: app.texture_format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        // Register the reactive row and prime geometry and glyphs together.
        offset.set(1.0);
        let updates = tree.take_pending_property_updates();
        tree.apply_partial_property_updates(updates);
        app.render_tree_with_motion_opt(&tree, &state, &view, None, 120, 80, false)
            .unwrap();
        let before = paints.get();
        let mut frames = Vec::new();
        for x in [2.25, 4.5, 6.75, 9.0] {
            offset.set(x);
            let updates = tree.take_pending_property_updates();
            tree.apply_partial_property_updates(updates);
            app.render_tree_with_motion_opt(&tree, &state, &view, None, 120, 80, true)
                .unwrap();
            assert_eq!(
                paints.get(),
                before,
                "motion text lost its row (GPU table {node_table})"
            );
            frames.push((
                x,
                read_rendered_pixels(app.device(), app.queue(), &target, 120, 80),
            ));
        }
        for (x, cached) in frames {
            offset.set(x);
            let updates = tree.take_pending_property_updates();
            tree.apply_partial_property_updates(updates);
            app.render_tree_with_motion_opt(&tree, &state, &view, None, 120, 80, false)
                .unwrap();
            let fresh = read_rendered_pixels(app.device(), app.queue(), &target, 120, 80);
            assert!(
                cached
                    .as_raw()
                    .iter()
                    .zip(fresh.as_raw())
                    .all(|(a, b)| a.abs_diff(*b) <= 2),
                "retained text differs from fresh paint at {x} (GPU table {node_table})"
            );
        }
    }
}

fn check_direct_cache(independent_canvas: bool) {
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    let mut app = BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })
    .expect("offscreen GPU required");
    let full_paints = Rc::new(Cell::new(0));
    let count = full_paints.clone();
    let marker = PaintMarker(
        blinc_layout::canvas(move |ctx, _| {
            count.set(count.get() + 1);
            ctx.fill_rect(
                Rect::new(0.0, 0.0, 8.0, 8.0),
                0.0.into(),
                Color::WHITE.into(),
            );
        })
        .w(8.0)
        .h(8.0),
    );
    let value = Rc::new(Cell::new(false));
    let mut ui = div().w(80.0).h(80.0).bg(Color::BLUE).child(marker);
    if independent_canvas {
        let value = value.clone();
        ui = ui.child(
            blinc_layout::canvas(move |ctx, _| {
                let color = if value.get() {
                    Color::RED
                } else {
                    Color::GREEN
                };
                ctx.fill_rect(Rect::new(0.0, 0.0, 24.0, 24.0), 0.0.into(), color.into());
            })
            .w(24.0)
            .h(24.0),
        );
    } else {
        ui = ui.child(
            div()
                .w(24.0)
                .h(24.0)
                .opacity(0.5)
                .bg(Color::RED)
                .child(div().w(12.0).h(12.0).bg(Color::GREEN)),
        );
    }
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(80.0, 80.0);
    let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
    let texture = app.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("direct retained frame"),
        size: wgpu::Extent3d {
            width: 80,
            height: 80,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: app.texture_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    app.render_tree_with_motion_opt(&tree, &state, &view, None, 80, 80, false)
        .unwrap();
    let paints = full_paints.get();
    assert!(paints > 0);
    app.context()
        .cached_bg_batch
        .as_mut()
        .unwrap()
        .cross_stream_groups
        .resolved = true;
    value.set(true);
    app.render_tree_with_motion_opt(&tree, &state, &view, None, 80, 80, true)
        .unwrap();
    assert_eq!(
        full_paints.get(),
        paints,
        "retained frame repainted the tree"
    );
    let cached = read_rendered_pixels(app.device(), app.queue(), &texture, 80, 80);
    app.render_tree_with_motion_opt(&tree, &state, &view, None, 80, 80, false)
        .unwrap();
    let fresh = read_rendered_pixels(app.device(), app.queue(), &texture, 80, 80);
    assert_eq!(cached, fresh, "retained frame differs from fresh paint");
}
