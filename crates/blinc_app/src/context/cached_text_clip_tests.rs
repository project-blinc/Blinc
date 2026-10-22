use super::main_text_clip_tests::{compare, context, tree, triangle};
use super::*;
use crate::tests::read_rendered_pixels;
const W: u32 = 240;
const H: u32 = 180;
fn render(c: &mut RenderContext, t: &RenderTree, compositor: bool, fast: bool) -> image::RgbaImage {
    let state = blinc_layout::RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    let texture = c.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("Cached text clip surface"),
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
#[test]
#[ignore] // Requires GPU
fn cached_text_clip_public_static_and_foreground_keep_rows() {
    for compositor in [false, true] {
        for foreground in [false, true] {
            let mut c = context(true, 1);
            let make = |clip| {
                let mut t = tree(clip, false);
                let owner = t.query_by_id("owner").unwrap();
                if foreground {
                    t.update_render_props(owner, |p| p.layer = RenderLayer::Foreground);
                }
                t
            };
            let baseline = render(&mut c, &make(false), compositor, false);
            let t = make(true);
            c.invalidate_render_cache();
            for fast in [false, true, true] {
                eprintln!("compositor={compositor} foreground={foreground} fast={fast}");
                let page = render(&mut c, &t, compositor, fast);
                compare(&page, &baseline, |x, y| triangle(x - 20.0, y - 20.0));
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn cached_text_clip_static_and_foreground_affines_follow_clip_owners() {
    use blinc_core::{Affine2D, Transform};
    for compositor in [false, true] {
        for foreground in [false, true] {
            let mut c = context(true, 1);
            for scale in [1.0, 1.5] {
                for m in [
                    [1.0, 0.2, 0.3, 1.0, 0.0, 0.0],
                    [0.0, 1.0, -1.0, 0.0, 95.0, 0.0],
                    [-1.0, 0.0, 0.0, 1.0, 120.0, 0.0],
                ] {
                    eprintln!(
                        "compositor={compositor} foreground={foreground} scale={scale} matrix={m:?}"
                    );
                    let make = |clip| {
                        let mut t = tree(clip, false);
                        t.set_scale_factor(scale);
                        let owner = t.query_by_id("owner").unwrap();
                        t.update_render_props(owner, |p| {
                            p.transform = Some(Transform::Affine2D(Affine2D { elements: m }));
                            p.transform_origin = Some([0.0, 0.0]);
                            if foreground {
                                p.layer = RenderLayer::Foreground;
                            }
                        });
                        let child = t.query_by_id("child").unwrap();
                        t.update_render_props(child, |p| {
                            p.transform = Some(Transform::translate(8.0, 4.0))
                        });
                        t
                    };
                    c.invalidate_render_cache();
                    let baseline = render(&mut c, &make(false), compositor, false);
                    c.invalidate_render_cache();
                    let t = make(true);
                    let mut initial_sizes = None;
                    for fast in [false, true] {
                        let page = render(&mut c, &t, compositor, fast);
                        let batch = c.cached_bg_batch.as_ref().unwrap();
                        let sizes = (batch.primitives.len(), batch.aux_data.len());
                        if let Some(previous) = initial_sizes {
                            assert_eq!(sizes, previous, "fast paint must retain one text copy");
                        } else {
                            initial_sizes = Some(sizes);
                        }
                        let [a, b, cc, d, tx, ty] = m;
                        let det = a * d - b * cc;
                        compare(&page, &baseline, |x, y| {
                            let x = x / scale - 20.0 - tx;
                            let y = y / scale - 20.0 - ty;
                            triangle((d * x - cc * y) / det, (-b * x + a * y) / det)
                        });
                    }
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn cached_text_clip_glass_layers_restore_walker_rows() {
    use blinc_core::{ClipPath, ClipShape, GlassStyle, Point, Transform};
    for glass in [false, true] {
        let mut c = context(true, 1);
        let make = |clip| {
            let mut root = div().w(W as f32).h(H as f32).bg(Color::BLUE).child(
                canvas(move |ctx, _| {
                    ctx.push_clip(ClipShape::Polygon(vec![
                        Point::new(400.0, 400.0),
                        Point::new(450.0, 400.0),
                        Point::new(450.0, 450.0),
                    ]));
                    ctx.fill_rect(
                        Rect::new(400.0, 400.0, 20.0, 20.0),
                        CornerRadius::ZERO,
                        Color::GREEN.into(),
                    );
                    ctx.pop_clip();
                    if glass {
                        ctx.fill_rect(
                            Rect::new(400.0, 400.0, 20.0, 20.0),
                            CornerRadius::ZERO,
                            Brush::Glass(GlassStyle {
                                simple: true,
                                ..Default::default()
                            }),
                        );
                    }
                })
                .absolute()
                .w(1.0)
                .h(1.0),
            );
            for (i, y) in [20.0, 95.0].into_iter().enumerate() {
                let mut parent = div()
                    .absolute()
                    .left(20.0)
                    .top(y)
                    .w(140.0)
                    .h(70.0)
                    .z_index(if i == 0 { 0 } else { 2 });
                if i == 1 {
                    parent = parent.transform(Transform::translate(4.0, 0.0));
                }
                if clip {
                    parent = parent.clip_path(ClipPath::Path {
                        vertices: vec![(0.0, 0.0), (75.0, 0.0), (0.0, 80.0)],
                    });
                }
                root = root
                    .child(parent.child(text("MMMMMMMM").size(32.0).color(Color::RED).no_wrap()));
            }
            let mut t = RenderTree::from_element(&root);
            t.compute_layout(W as f32, H as f32);
            t
        };
        let baseline = render(&mut c, &make(false), false, false);
        c.invalidate_render_cache();
        let t = make(true);
        for fast in [false, true] {
            let page = render(&mut c, &t, false, fast);
            compare(&page, &baseline, |x, y| {
                if y < 90.0 {
                    triangle(x - 20.0, y - 20.0)
                } else {
                    triangle(x - 24.0, y - 95.0)
                }
            });
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn cached_text_clip_filtered_snapshots_replay_after_poison_and_invalidation() {
    use blinc_core::Transform;
    for converted in [false, true] {
        let mut c = context(true, 1);
        let mut t = tree(true, false);
        if converted {
            let owner = t.query_by_id("owner").unwrap();
            t.update_render_props(owner, |p| {
                p.layer = RenderLayer::Foreground;
                p.transform = Some(Transform::scale(1.2, 0.7));
                p.transform_origin = Some([0.0, 0.0]);
            });
        }
        render(&mut c, &t, false, false);
        let snapshot = if converted {
            c.cached_fg_glyphs.clone().unwrap()
        } else {
            c.cached_glyphs_by_layer
                .as_ref()
                .unwrap()
                .get(&0)
                .unwrap()
                .clone()
        };
        assert!(!snapshot.is_empty());
        let mut owned = cached_text::TextBatch::default();
        owned.aux_data = Arc::new(vec![[0.0; 4]; 2048]);
        let mut source = PrimitiveBatch::new();
        source.glyphs = snapshot.glyphs.clone();
        source.primitives = snapshot.primitives.clone();
        source.aux_data = (*snapshot.aux_data).clone();
        owned.append(source);
        let selected = owned.filtered(|[x, _, w, _]| x + w > 70.0);
        assert!(Arc::ptr_eq(&selected.aux_data, &owned.aux_data));
        assert!(selected.glyphs.iter().all(|g| g.flags[1] > 2048.0));
        assert!(selected.primitives.iter().all(|p| p.clip_shape[0] > 2048.0));
        c.invalidate_render_cache();
        let draw = |c: &mut RenderContext, batch: &cached_text::TextBatch, scissor: bool| {
            let (texture, view) =
                crate::tests::create_test_texture(c.device(), c.texture_format(), W, H);
            c.renderer.clear_target(&view, wgpu::Color::BLUE);
            c.renderer.update_aux_data_slice(&vec![[0.0; 4]; 4096]);
            if scissor {
                c.renderer.set_pending_scissor((70, 0, W - 70, H));
            }
            c.render_text(&view, batch);
            c.renderer.clear_pending_scissor();
            read_rendered_pixels(c.device(), c.queue(), &texture, W, H)
        };
        let baseline = draw(&mut c, &owned, false);
        let page = draw(&mut c, &selected, true);
        assert!(baseline.pixels().filter(|p| p.0[0] > 20).count() > 30);
        for y in 0..H {
            for x in 0..W {
                assert_eq!(
                    page.get_pixel(x, y).0,
                    if x >= 70 {
                        baseline.get_pixel(x, y).0
                    } else {
                        [0, 0, 255, 255]
                    },
                    "({x},{y}) converted={converted}"
                );
            }
        }
    }
}

#[test]
fn cached_text_clip_filter_bounds_and_copy_on_write_keep_headers() {
    let g = GpuGlyph {
        bounds: [10.0, 10.0, 4.0, 6.0],
        flags: [0.0, 1.0, 0.0, 0.0],
        ..Default::default()
    };
    let mut p = GpuPrimitive::from_glyph(&g);
    p.local_affine = [2.0, 1.0, 1.0, 2.0];
    let mut source = PrimitiveBatch::new();
    source.glyphs.push(g);
    source.primitives.push(p);
    source.aux_data = vec![[1.0; 4]; 3];
    let mut cache = cached_text::TextBatch::default();
    cache.append(source.clone());
    let snapshot = cache.clone();
    let selected = cache.filtered(|[x, ..]| x < 6.0);
    assert!(selected.glyphs.is_empty());
    assert_eq!(selected.primitives.len(), 1);
    assert_eq!(selected.bounds().next().unwrap(), [5.0, 5.0, 14.0, 16.0]);
    assert!(Arc::ptr_eq(&snapshot.aux_data, &selected.aux_data));
    cache.append(source);
    assert!(!Arc::ptr_eq(&cache.aux_data, &snapshot.aux_data));
    assert_eq!(snapshot.aux_data.len(), 3);
    assert_eq!(snapshot.glyphs[0].flags[1], 1.0);
    assert_eq!(cache.glyphs[1].flags[1], 4.0);
    assert_eq!(cache.primitives[1].clip_shape[0], 4.0);
}
