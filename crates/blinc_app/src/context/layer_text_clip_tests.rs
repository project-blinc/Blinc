use super::main_text_clip_tests::{compare, context, triangle};
use super::*;
use crate::tests::read_rendered_pixels;
const W: u32 = 240;
const H: u32 = 180;
fn tree(clip: bool, inside: bool, polygon: bool, angle: f32, dpi: f32) -> RenderTree {
    use blinc_core::{ClipLength, ClipPath};
    let path = if polygon {
        ClipPath::Polygon {
            points: vec![
                (ClipLength::Px(0.0), ClipLength::Px(0.0)),
                (ClipLength::Px(75.0), ClipLength::Px(0.0)),
                (ClipLength::Px(0.0), ClipLength::Px(80.0)),
            ],
        }
    } else {
        ClipPath::Path {
            vertices: vec![(0.0, 0.0), (75.0, 0.0), (0.0, 80.0)],
        }
    };
    let mut panel = div()
        .id("panel")
        .w(140.0)
        .h(90.0)
        .child(text("MMMMMMMM").size(32.0).color(Color::RED).no_wrap());
    let mut owner = div()
        .id("owner")
        .absolute()
        .left(20.0)
        .top(20.0)
        .w(140.0)
        .h(90.0);
    if clip {
        if inside {
            panel = panel.clip_path(path);
        } else {
            owner = owner.clip_path(path);
        }
    }
    let ui = div()
        .w(W as f32)
        .h(H as f32)
        .bg(Color::BLUE)
        .child(owner.child(panel));
    let mut t = RenderTree::from_element(&ui);
    t.compute_layout(W as f32, H as f32);
    t.set_scale_factor(dpi);
    let panel = t.query_by_id("panel").unwrap();
    t.update_render_props(panel, |p| {
        p.rotate_y = Some(angle);
        p.perspective = Some(500.0);
    });
    t
}
fn render(c: &mut RenderContext, t: &RenderTree, compositor: bool, fast: bool) -> image::RgbaImage {
    let state = RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    let texture = c.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("3D text clip surface"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: c.texture_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    c.render_tree_with_motion_opt(t, &state, W, H, &view, compositor.then_some(&texture), fast)
        .unwrap();
    read_rendered_pixels(c.device(), c.queue(), &texture, W, H)
}
fn local(x: f32, y: f32, angle: f32, dpi: f32) -> (f32, f32) {
    let a = angle.to_radians();
    let u = x / dpi - 90.0;
    let v = y / dpi - 65.0;
    let x = u / (a.cos() - u * a.sin() / 500.0);
    (70.0 + x, 45.0 + v * (1.0 + x * a.sin() / 500.0))
}
#[test]
#[ignore] // Requires GPU
fn layer_text_clip_public_bakes_keep_inside_and_outside_owners() {
    for compositor in [false, true] {
        for inside in [true, false] {
            let mut c = context(true, 1);
            for (angle, dpi) in [(0.0, 1.0), (35.0, 1.0), (-25.0, 1.5)] {
                c.invalidate_render_cache();
                let baseline = render(
                    &mut c,
                    &tree(false, inside, false, angle, dpi),
                    compositor,
                    false,
                );
                for polygon in [false, true] {
                    c.invalidate_render_cache();
                    let t = tree(true, inside, polygon, angle, dpi);
                    for fast in [false, true] {
                        eprintln!(
                            "compositor={compositor} inside={inside} angle={angle} dpi={dpi} polygon={polygon} fast={fast}"
                        );
                        let page = render(&mut c, &t, compositor, fast);
                        compare(&page, &baseline, |x, y| {
                            let (x, y) = if inside {
                                local(x, y, angle, dpi)
                            } else {
                                (x / dpi - 20.0, y / dpi - 20.0)
                            };
                            triangle(x, y)
                        });
                    }
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn layer_text_clip_affines_and_changing_contours_keep_owner_frames() {
    use blinc_core::{Affine2D, ClipPath, Transform};
    let mut c = context(true, 1);
    for m in [
        [1.0, 0.2, 0.3, 1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, 1.0, 120.0, 0.0],
    ] {
        let make = |clip| {
            let mut t = tree(clip, true, false, 25.0, 1.0);
            let panel = t.query_by_id("panel").unwrap();
            t.update_render_props(panel, |p| {
                p.transform = Some(Transform::Affine2D(Affine2D { elements: m }));
                p.transform_origin = Some([0.0, 0.0]);
            });
            let child = t.layout().children(panel)[0];
            t.update_render_props(child, |p| {
                p.transform = Some(Transform::translate(8.0, 4.0))
            });
            t
        };
        c.invalidate_render_cache();
        let baseline = render(&mut c, &make(false), false, false);
        c.invalidate_render_cache();
        let mut t = make(true);
        let panel = t.query_by_id("panel").unwrap();
        for shift in [0.0, 20.0, 0.0] {
            t.update_render_props(panel, |p| {
                p.clip_path = Some(ClipPath::Path {
                    vertices: vec![(shift, 0.0), (shift + 75.0, 0.0), (shift, 80.0)],
                })
            });
            c.invalidate_render_cache();
            let page = render(&mut c, &t, false, false);
            compare(&page, &baseline, |x, y| {
                let (x, y) = local(x, y, 25.0, 1.0);
                let [a, b, cc, d, tx, ty] = m;
                let (x, y) = (x - tx, y - ty);
                let det = a * d - b * cc;
                triangle((d * x - cc * y) / det - shift, (-b * x + a * y) / det)
            });
        }
        for vertices in [vec![], vec![(0.0, 0.0), (f32::NAN, 30.0), (70.0, 80.0)]] {
            t.update_render_props(panel, |p| p.clip_path = Some(ClipPath::Path { vertices }));
            c.invalidate_render_cache();
            assert!(
                render(&mut c, &t, false, false)
                    .pixels()
                    .all(|p| p.0 == [0, 0, 255, 255])
            );
        }
        t.update_render_props(panel, |p| p.clip_path = None);
        c.invalidate_render_cache();
        assert_eq!(render(&mut c, &t, false, false), baseline);
    }
}

#[test]
#[ignore] // Requires GPU
fn layer_text_clip_mixed_runs_rebase_each_owner() {
    use blinc_core::{ClipPath, Transform};
    let mut c = context(true, 1);
    let make = |clip| {
        let mut panel = div().id("panel").w(140.0).h(140.0);
        for y in [0.0, 70.0] {
            let mut owner = div().absolute().top(y).w(140.0).h(70.0);
            if y > 0.0 {
                owner = owner.transform(Transform::translate(4.0, 0.0));
            }
            if clip {
                owner = owner.clip_path(ClipPath::Path {
                    vertices: vec![(0.0, 0.0), (75.0, 0.0), (0.0, 80.0)],
                });
            }
            panel =
                panel.child(owner.child(text("MMMMMMMM").size(32.0).color(Color::RED).no_wrap()));
        }
        let ui = div().w(W as f32).h(H as f32).bg(Color::BLUE).child(
            div()
                .absolute()
                .left(20.0)
                .top(20.0)
                .w(140.0)
                .h(140.0)
                .child(panel),
        );
        let mut t = RenderTree::from_element(&ui);
        t.compute_layout(W as f32, H as f32);
        let panel = t.query_by_id("panel").unwrap();
        t.update_render_props(panel, |p| {
            p.rotate_y = Some(25.0);
            p.perspective = Some(500.0);
        });
        t
    };
    let baseline = render(&mut c, &make(false), false, false);
    c.invalidate_render_cache();
    let t = make(true);
    let state = RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    let (texts, _, _, _) = c.collect_render_elements_with_state(&t, Some(&state));
    assert_eq!(texts.len(), 2);
    assert!(texts[0].css_affine.is_none());
    assert!(texts[1].css_affine.is_some());
    c.renderer.update_aux_data_slice(&vec![[0.0; 4]; 4096]);
    let page = render(&mut c, &t, false, false);
    compare(&page, &baseline, |x, y| {
        let a = 25.0_f32.to_radians();
        let u = x - 90.0;
        let v = y - 90.0;
        let x = u / (a.cos() - u * a.sin() / 500.0);
        let (x, y) = (70.0 + x, 70.0 + v * (1.0 + x * a.sin() / 500.0));
        if y < 60.0 {
            triangle(x, y)
        } else {
            triangle(x - 4.0, y - 70.0)
        }
    });
}

#[test]
#[ignore] // Requires GPU
fn layer_text_clip_overlapping_plain_and_affine_runs_keep_paint_order() {
    use blinc_core::{ClipPath, Transform};
    let mut c = context(true, 1);
    let make = |first| {
        let mut panel = div()
            .id("panel")
            .w(140.0)
            .h(90.0)
            .clip_path(ClipPath::Path {
                vertices: vec![(0.0, 0.0), (75.0, 0.0), (0.0, 80.0)],
            });
        if first {
            panel = panel.child(
                div()
                    .absolute()
                    .w(140.0)
                    .h(90.0)
                    .child(text("MMMMMMMM").size(32.0).color(Color::RED).no_wrap()),
            );
        }
        panel = panel.child(
            div()
                .absolute()
                .w(140.0)
                .h(90.0)
                .transform(Transform::translate(1.0, 0.0))
                .child(text("MMMMMMMM").size(32.0).color(Color::GREEN).no_wrap()),
        );
        let ui = div().w(W as f32).h(H as f32).bg(Color::BLUE).child(
            div()
                .absolute()
                .left(20.0)
                .top(20.0)
                .w(140.0)
                .h(90.0)
                .child(panel),
        );
        let mut t = RenderTree::from_element(&ui);
        t.compute_layout(W as f32, H as f32);
        let panel = t.query_by_id("panel").unwrap();
        t.update_render_props(panel, |p| {
            p.rotate_y = Some(25.0);
            p.perspective = Some(500.0);
        });
        t
    };
    let reference = render(&mut c, &make(false), false, false);
    c.invalidate_render_cache();
    let page = render(&mut c, &make(true), false, false);
    let mut count = 0;
    for y in 0..H {
        for x in 0..W {
            let expected = reference.get_pixel(x, y).0;
            if expected[1] > 220 && expected[0] < 5 {
                count += 1;
                let actual = page.get_pixel(x, y).0;
                assert!(
                    actual[1] > 200 && actual[0] < 35,
                    "({x},{y}) {actual:?} expected green {expected:?}"
                );
            }
        }
    }
    assert!(
        count > 30,
        "overlap control must contain opaque foreground ink"
    );
}
