use super::*;
use crate::tests::{create_test_texture, read_rendered_pixels};
use blinc_core::{ClipLength, ClipPath};
const W: u32 = 240;
const H: u32 = 180;
const BLUE: [u8; 4] = [0, 0, 255, 255];
fn context(unified: bool, samples: u32) -> RenderContext {
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    let renderer = pollster::block_on(GpuRenderer::new(blinc_gpu::RendererConfig {
        sample_count: 1,
        unified_text_rendering: unified,
        ..Default::default()
    }))
    .expect("offscreen GPU required");
    let device = renderer.device_arc();
    let queue = renderer.queue_arc();
    let mut text = TextRenderingContext::new(device.clone(), queue.clone());
    let mut loaded = false;
    for path in crate::system_font_paths() {
        if let Ok(data) = std::fs::read(path) {
            if text.load_font_data(data).is_ok() {
                loaded = true;
                break;
            }
        }
    }
    assert!(loaded, "test requires a system font");
    text.preload_generic_styles(GpuGenericFont::SansSerif, &[400, 700], false);
    RenderContext::new(renderer, text, device, queue, samples)
}
fn tree(clip: bool, polygon: bool) -> RenderTree {
    let mut parent = div()
        .id("owner")
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
    let ui = div().w(W as f32).h(H as f32).bg(Color::BLUE).child(
        parent.child(
            div()
                .id("child")
                .child(text("MMMMMMMM").size(32.0).color(Color::RED).no_wrap()),
        ),
    );
    let mut t = RenderTree::from_element(&ui);
    t.compute_layout(W as f32, H as f32);
    t
}
fn render(c: &mut RenderContext, t: &RenderTree) -> image::RgbaImage {
    let (texture, view) = create_test_texture(c.device(), c.texture_format(), W, H);
    c.render_tree(t, W, H, &view).unwrap();
    read_rendered_pixels(c.device(), c.queue(), &texture, W, H)
}
fn triangle(u: f32, v: f32) -> Option<bool> {
    let edge = u / 75.0 + v / 80.0;
    if u.abs() < 3.0 || v.abs() < 3.0 || (edge - 1.0).abs() < 0.06 {
        None
    } else {
        Some(u > 0.0 && v > 0.0 && edge < 1.0)
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
#[test]
#[ignore] // Requires GPU
fn main_text_clip_public_direct_and_unified_observe_path_and_polygon() {
    for unified in [false, true] {
        for samples in [1, 4] {
            eprintln!("unified={unified} samples={samples}");
            let mut c = context(unified, samples);
            let baseline = render(&mut c, &tree(false, false));
            for polygon in [false, true] {
                let page = render(&mut c, &tree(true, polygon));
                compare(&page, &baseline, |x, y| triangle(x - 20.0, y - 20.0));
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn main_text_clip_owner_transforms_and_dpi_preserve_headers() {
    use blinc_core::{Affine2D, Transform};
    for unified in [false, true] {
        let mut c = context(unified, 1);
        for scale in [1.0, 1.5] {
            for m in [
                [1.0, 0.2, 0.3, 1.0, 0.0, 0.0],
                [0.0, 1.0, -1.0, 0.0, 95.0, 0.0],
                [-1.0, 0.0, 0.0, 1.0, 120.0, 0.0],
            ] {
                let make = |clip| {
                    let mut t = tree(clip, false);
                    t.set_scale_factor(scale);
                    let owner = t.query_by_id("owner").unwrap();
                    t.update_render_props(owner, |p| {
                        p.transform = Some(Transform::Affine2D(Affine2D { elements: m }));
                        p.transform_origin = Some([0.0, 0.0]);
                    });
                    let child = t.query_by_id("child").unwrap();
                    t.update_render_props(child, |p| {
                        p.transform = Some(Transform::translate(8.0, 4.0))
                    });
                    t
                };
                let baseline = render(&mut c, &make(false));
                let page = render(&mut c, &make(true));
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

#[test]
#[ignore] // Requires GPU
fn main_text_clip_glass_and_foreground_effects_restore_owned_rows() {
    use blinc_core::{ClipShape, GlassStyle, LayerConfig, Point};
    for unified in [false, true] {
        let mut c = context(unified, 1);
        for glass in [false, true] {
            for effects in [false, true] {
                let make = |clip| {
                    let mut root = div()
                        .w(W as f32)
                        .h(H as f32)
                        .bg(Color::BLUE)
                        .child(
                            canvas(move |ctx, _| {
                                ctx.push_clip(ClipShape::Polygon(vec![
                                    Point::new(400.0, 400.0),
                                    Point::new(450.0, 400.0),
                                    Point::new(450.0, 450.0),
                                    Point::new(400.0, 450.0),
                                ]));
                                ctx.fill_rect(
                                    Rect::new(400.0, 400.0, 50.0, 50.0),
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
                        )
                        .child(
                            div().foreground().w(1.0).h(1.0).child(
                                canvas(move |ctx, _| {
                                    if effects {
                                        ctx.push_layer(LayerConfig::new().opacity(0.5));
                                    }
                                    ctx.push_clip(ClipShape::Polygon(vec![
                                        Point::new(460.0, 460.0),
                                        Point::new(490.0, 460.0),
                                        Point::new(490.0, 490.0),
                                    ]));
                                    ctx.fill_rect(
                                        Rect::new(460.0, 460.0, 10.0, 10.0),
                                        CornerRadius::ZERO,
                                        Color::GREEN.into(),
                                    );
                                    ctx.pop_clip();
                                    if effects {
                                        ctx.pop_layer();
                                    }
                                })
                                .w(1.0)
                                .h(1.0),
                            ),
                        );
                    let mut parent = div().absolute().left(20.0).top(20.0).w(140.0).h(90.0);
                    if clip {
                        parent = parent.clip_path(ClipPath::Path {
                            vertices: vec![(0.0, 0.0), (75.0, 0.0), (0.0, 80.0)],
                        });
                    }
                    root = root.child(
                        parent.child(text("MMMMMMMM").size(32.0).color(Color::RED).no_wrap()),
                    );
                    let mut t = RenderTree::from_element(&root);
                    t.compute_layout(W as f32, H as f32);
                    t
                };
                let t = make(true);
                let mut probe = GpuPaintContext::new(W as f32, H as f32);
                t.render_to_layer(&mut probe, RenderLayer::Foreground);
                let mut b = probe.take_batch();
                b.take_foreground();
                assert_eq!(
                    b.requires_ordered_rendering(),
                    effects,
                    "test must reach the foreground effect branch"
                );
                let baseline = render(&mut c, &make(false));
                let page = render(&mut c, &t);
                compare(&page, &baseline, |x, y| triangle(x - 20.0, y - 20.0));
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn main_text_clip_styled_scissors_changed_empty_and_removed_frames() {
    use blinc_layout::rich_text::rich_text;
    for unified in [false, true] {
        let mut c = context(unified, 1);
        let ui = div().w(W as f32).h(H as f32).bg(Color::BLUE).child(
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
        let owner = t.query_by_id("owner").unwrap();
        let baseline = render(&mut c, &t);
        for shift in [0.0, 25.0, 0.0] {
            t.update_render_props(owner, |p| {
                p.clip_path = Some(ClipPath::Path {
                    vertices: vec![(shift, 0.0), (shift + 75.0, 0.0), (shift, 80.0)],
                })
            });
            let page = render(&mut c, &t);
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
        for vertices in [vec![], vec![(0.0, 0.0), (f32::NAN, 30.0), (70.0, 80.0)]] {
            t.update_render_props(owner, |p| p.clip_path = Some(ClipPath::Path { vertices }));
            assert!(render(&mut c, &t).pixels().all(|p| p.0 == BLUE));
        }
        t.update_render_props(owner, |p| p.clip_path = None);
        assert_eq!(render(&mut c, &t), baseline);
    }
}
