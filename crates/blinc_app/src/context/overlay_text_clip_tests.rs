use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::{create_test_texture, read_rendered_pixels};
use blinc_core::{Affine2D, Transform};
use blinc_core::{ClipLength, ClipPath};
const W: u32 = 240;
const H: u32 = 180;
const BLUE: [u8; 4] = [0, 0, 255, 255];
fn app() -> BlincApp {
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })
    .expect("offscreen GPU required")
}
fn tree(clip: bool, polygon: bool) -> RenderTree {
    let mut parent = div()
        .id("clip-owner")
        .absolute()
        .left(20.0)
        .top(20.0)
        .w(140.0)
        .h(90.0);
    if clip {
        parent = parent.clip_path(if polygon {
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
        });
    }
    let ui = div().w(W as f32).h(H as f32).child(
        parent.child(
            div()
                .id("text-frame")
                .child(text("MMMMMMMM").size(32.0).color(Color::RED).no_wrap()),
        ),
    );
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(W as f32, H as f32);
    tree
}
fn render(c: &mut RenderContext, tree: &RenderTree) -> image::RgbaImage {
    let state = blinc_layout::RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    let (t, v) = create_test_texture(c.device(), c.texture_format(), W, H);
    c.renderer.clear_target(&v, wgpu::Color::BLUE);
    c.render_overlay_tree_with_motion(tree, &state, W, H, &v)
        .unwrap();
    read_rendered_pixels(c.device(), c.queue(), &t, W, H)
}
#[test]
#[ignore] // Requires GPU
fn overlay_text_clip_public_entry_observes_inherited_polygon_and_path() {
    let mut app = app();
    let c = app.context();
    let baseline = render(c, &tree(false, false));
    for polygon in [false, true] {
        let page = render(c, &tree(true, polygon));
        let mut counts = [0; 2];
        for y in 5..H - 5 {
            for x in 5..W - 5 {
                let u = x as f32 + 0.5 - 20.0;
                let v = y as f32 + 0.5 - 20.0;
                let edge = u / 75.0 + v / 80.0;
                if u.abs() < 2.0 || v.abs() < 2.0 || (edge - 1.0).abs() < 0.05 {
                    continue;
                }
                let keep = u > 0.0 && v > 0.0 && edge < 1.0;
                if baseline.get_pixel(x, y).0[0] > 20 {
                    counts[usize::from(keep)] += 1;
                }
                let expected = if keep {
                    baseline.get_pixel(x, y).0
                } else {
                    BLUE
                };
                assert_eq!(
                    page.get_pixel(x, y).0,
                    expected,
                    "({x},{y}) polygon={polygon}"
                );
            }
        }
        assert!(counts.into_iter().all(|n| n > 10), "{counts:?}");
    }
}

fn compare(
    page: &image::RgbaImage,
    baseline: &image::RgbaImage,
    member: impl Fn(f32, f32) -> Option<bool>,
) {
    let mut counts = [0; 2];
    for y in 5..H - 5 {
        for x in 5..W - 5 {
            let Some(keep) = member(x as f32 + 0.5, y as f32 + 0.5) else {
                continue;
            };
            let base = baseline.get_pixel(x, y).0;
            if base[0] > 20 {
                counts[usize::from(keep)] += 1;
            }
            let expected = if keep { base } else { BLUE };
            let actual = page.get_pixel(x, y).0;
            assert!(
                actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3),
                "({x},{y}) {actual:?} expected {expected:?}"
            );
        }
    }
    assert!(counts.into_iter().all(|n| n > 10), "{counts:?}");
}
fn triangle(u: f32, v: f32) -> Option<bool> {
    let edge = u / 75.0 + v / 80.0;
    if u.abs() < 3.0 || v.abs() < 3.0 || (edge - 1.0).abs() < 0.06 {
        None
    } else {
        Some(u > 0.0 && v > 0.0 && edge < 1.0)
    }
}
#[test]
#[ignore] // Requires GPU
fn overlay_text_clip_owner_frame_survives_descendant_transforms_and_dpi() {
    let mut app = app();
    let c = app.context();
    for scale in [1.0, 1.5] {
        for m in [
            [1.0, 0.2, 0.3, 1.0, 0.0, 0.0],
            [0.0, 1.0, -1.0, 0.0, 95.0, 0.0],
            [-1.0, 0.0, 0.0, 1.0, 120.0, 0.0],
        ] {
            let make = |clip| {
                let mut t = tree(clip, false);
                t.set_scale_factor(scale);
                let owner = t.query_by_id("clip-owner").unwrap();
                t.update_render_props(owner, |p| {
                    p.transform = Some(Transform::Affine2D(Affine2D { elements: m }));
                    p.transform_origin = Some([0.0, 0.0]);
                });
                let child = t.query_by_id("text-frame").unwrap();
                t.update_render_props(child, |p| {
                    p.transform = Some(Transform::translate(8.0, 4.0));
                    p.transform_origin = Some([0.0, 0.0]);
                });
                t
            };
            let baseline = render(c, &make(false));
            let page = render(c, &make(true));
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

#[test]
#[ignore] // Requires GPU
fn overlay_text_clip_styled_spans_scissors_and_changed_frames() {
    use blinc_layout::rich_text::rich_text;
    let mut app = app();
    let c = app.context();
    let build = || {
        let ui = div().w(W as f32).h(H as f32).child(
            div()
                .id("owner")
                .absolute()
                .left(20.0)
                .top(20.0)
                .w(140.0)
                .h(90.0)
                .child(
                    div()
                        .absolute()
                        .left(10.0)
                        .top(5.0)
                        .w(90.0)
                        .h(60.0)
                        .overflow_clip()
                        .child(
                            rich_text("<b>MMMM</b>MMMM")
                                .size(32.0)
                                .default_color(Color::RED)
                                .no_wrap(),
                        ),
                ),
        );
        let mut t = RenderTree::from_element(&ui);
        t.compute_layout(W as f32, H as f32);
        t
    };
    let mut t = build();
    let baseline = render(c, &t);
    let owner = t.query_by_id("owner").unwrap();
    for vertices in [
        vec![(0.0, 0.0), (75.0, 0.0), (0.0, 80.0)],
        vec![(25.0, 0.0), (100.0, 0.0), (25.0, 80.0)],
    ] {
        let shift = vertices[0].0;
        t.update_render_props(owner, |p| {
            p.clip_path = Some(ClipPath::Path {
                vertices: vertices.clone(),
            })
        });
        let collected = c.collect_render_elements(&t).0;
        assert!(collected.len() >= 2);
        let first = collected[0].path_clip.as_ref().unwrap();
        assert!(
            collected
                .iter()
                .all(|text| Arc::ptr_eq(first, text.path_clip.as_ref().unwrap()))
        );
        let page = render(c, &t);
        compare(&page, &baseline, |x, y| {
            if [30.0, 120.0].into_iter().any(|e| (x - e).abs() < 3.0)
                || [25.0, 85.0].into_iter().any(|e| (y - e).abs() < 3.0)
            {
                return None;
            }
            triangle(x - 20.0 - shift, y - 20.0)
                .map(|keep| keep && x > 30.0 && x < 120.0 && y > 25.0 && y < 85.0)
        });
    }
    // Fewer than three points and non-finite coordinates cover no text.
    for vertices in [vec![], vec![(0.0, 0.0), (f32::NAN, 30.0), (70.0, 80.0)]] {
        t.update_render_props(owner, |p| p.clip_path = Some(ClipPath::Path { vertices }));
        assert!(render(c, &t).pixels().all(|p| p.0 == BLUE));
    }
    t.update_render_props(owner, |p| p.clip_path = None);
    assert_eq!(render(c, &t), baseline);
}

#[test]
fn overlay_text_clip_owned_payload_survives_clones_and_batch_rebase() {
    let clip = ClipPath::Polygon {
        points: vec![
            (ClipLength::Percent(0.0), ClipLength::Percent(0.0)),
            (ClipLength::Percent(100.0), ClipLength::Percent(0.0)),
            (ClipLength::Percent(0.0), ClipLength::Percent(100.0)),
        ],
    };
    let owned = text_clip::CollectedTextPath::capture(
        &clip,
        [10.0, 20.0, 40.0, 50.0],
        Some([1.0, 0.0, 0.5, 1.0, 3.0, 4.0]),
        (2.0, 0.5),
        (5.0, 6.0),
        Some((30.0, 40.0)),
        2.0,
        None,
    )
    .unwrap();
    let clone = owned.clone();
    drop(owned);
    let glyph = GpuGlyph {
        bounds: [1.0, 2.0, 10.0, 10.0],
        ..Default::default()
    };
    let batch = clone.prepare(&[glyph, glyph], Some([40.0, 20.0, 80.0, 60.0]), (W, H));
    assert_eq!(batch.glyphs[0].bounds, glyph.bounds);
    assert_eq!(batch.glyphs[0].clip_bounds, [40.0, 20.0, 80.0, 60.0]);
    assert_eq!(batch.glyphs[0].flags[1], batch.glyphs[1].flags[1]);
    // Local (0,0): CSS gives (23,24), then motion gives (21,38), then DPI.
    assert_eq!(&batch.aux_data[3][..2], [42.0, 76.0]);
    let mut out = PrimitiveBatch::new();
    out.aux_data = vec![[999.0; 4]; 2048];
    out.merge(batch);
    assert_eq!(out.glyphs[0].flags[1], 2049.0);
    assert_eq!(&out.aux_data[2051][..2], [42.0, 76.0]);
}

#[test]
#[ignore] // Requires GPU
fn overlay_text_clip_layers_restore_walker_and_text_auxiliary_owners() {
    use blinc_core::{ClipShape, Point};
    let mut app = app();
    let c = app.context();
    let build = |clip| {
        let mut root = div().w(W as f32).h(H as f32).child(
            canvas(|ctx, _| {
                ctx.push_clip(ClipShape::Polygon(vec![
                    Point::new(400.0, 400.0),
                    Point::new(450.0, 400.0),
                    Point::new(450.0, 450.0),
                    Point::new(400.0, 450.0),
                ]));
                ctx.fill_rect(
                    Rect::new(400.0, 400.0, 50.0, 50.0),
                    CornerRadius::ZERO,
                    Color::RED.into(),
                );
                ctx.pop_clip();
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
            root =
                root.child(parent.child(text("MMMMMMMM").size(32.0).color(Color::RED).no_wrap()));
        }
        let mut t = RenderTree::from_element(&root);
        t.compute_layout(W as f32, H as f32);
        t
    };
    let baseline = render(c, &build(false));
    let page = render(c, &build(true));
    compare(&page, &baseline, |x, y| {
        if y < 90.0 {
            triangle(x - 20.0, y - 20.0)
        } else {
            triangle(x - 24.0, y - 95.0)
        }
    });
}
