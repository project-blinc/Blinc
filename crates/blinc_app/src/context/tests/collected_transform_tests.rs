use super::main_text_clip_tests::context;
use super::opacity_group_tests::pixel;
use super::*;
use crate::tests::read_rendered_pixels;
use blinc_animation::AnimationScheduler;
use blinc_core::{Affine2D, Transform};
use blinc_layout::svg;

#[test]
#[ignore] // Requires GPU
fn collected_rotation_stays_inside_sliding_and_scaling_ancestors() {
    let mut context = context(true, 1);
    let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
    for dpi in [1.0, 2.0] {
        for compositor in [false, true] {
            let ui = div().w(240.0).h(180.0).bg(Color::BLUE).child(
                div()
                    .id("panel")
                    .absolute()
                    .left(70.0)
                    .top(40.0)
                    .w(100.0)
                    .h(100.0)
                    .child(
                        div()
                            .id("rotated")
                            .absolute()
                            .left(20.0)
                            .top(20.0)
                            .w(24.0)
                            .h(24.0)
                            .bg(Color::RED)
                            .child(svg(r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><rect width="24" height="24" fill="#00ff00"/></svg>"##).size(24.0, 24.0)),
                    ),
            );
            let mut tree = RenderTree::from_element(&ui);
            tree.compute_layout(240.0, 180.0);
            tree.set_scale_factor(dpi);
            let panel = tree.query_by_id("panel").unwrap();
            let rotated = tree.query_by_id("rotated").unwrap();
            tree.update_render_props(rotated, |p| {
                p.transform = Some(Transform::Affine2D(Affine2D {
                    elements: [0.0, 1.0, -1.0, 0.0, 0.0, 0.0],
                }));
            });
            let width = (240.0 * dpi) as u32;
            let height = (180.0 * dpi) as u32;
            let texture = context.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("Collected transform target"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: context.texture_format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            // Sample entry, settled and exit positions in both slide directions.
            for (tx, sx, sy) in [
                (48.0, 1.0, 1.0),
                (24.0, 0.85, 1.1),
                (0.0, 1.0, 1.0),
                (-24.0, 0.85, 1.1),
                (-48.0, 1.0, 1.0),
            ] {
                tree.update_render_props(panel, |p| {
                    p.transform_origin = Some([0.0, 0.0]);
                    p.transform = Some(Transform::Affine2D(Affine2D {
                        elements: [sx, 0.0, 0.0, sy, tx, 0.0],
                    }));
                });
                context.invalidate_render_cache();
                context
                    .render_tree_with_motion_opt(
                        &tree,
                        &state,
                        width,
                        height,
                        &view,
                        compositor.then_some(&texture),
                        false,
                    )
                    .unwrap();
                let page = read_rendered_pixels(
                    context.device(),
                    context.queue(),
                    &texture,
                    width,
                    height,
                );
                let center = ((70.0 + 32.0 * sx + tx) * dpi, (40.0 + 32.0 * sy) * dpi);
                for dx in [-6.0, 0.0, 6.0] {
                    for dy in [-6.0, 0.0, 6.0] {
                        pixel(
                            &page,
                            ((center.0 + dx * dpi) as u32, (center.1 + dy * dpi) as u32),
                            [0, 255, 0, 255],
                        );
                    }
                }
                let green = page
                    .enumerate_pixels()
                    .filter(|(_, _, p)| p[1] > 200 && p[0] < 20 && p[2] < 20);
                for (x, y, _) in green {
                    assert!(
                        (x as f32 + 0.5 - center.0).abs() <= 12.0 * sx * dpi + 1.0
                            && (y as f32 + 0.5 - center.1).abs() <= 12.0 * sy * dpi + 1.0,
                        "SVG escaped its native background: dpi={dpi} compositor={compositor} tx={tx} scale=({sx}, {sy}) pixel=({x}, {y})"
                    );
                }
            }
        }
    }
}
