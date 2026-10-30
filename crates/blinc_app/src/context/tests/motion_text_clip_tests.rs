use super::main_text_clip_tests::{compare, context, triangle};
use super::*;
use crate::tests::read_rendered_pixels;
const W: u32 = 240;
const H: u32 = 180;
fn motion_tree(
    clip: bool,
    polygon: bool,
    inside: bool,
    value: blinc_layout::motion::SharedAnimatedValue,
) -> RenderTree {
    use blinc_core::{ClipLength, ClipPath};
    let child = div()
        .id("child")
        .child(text("MMMMMMMM").size(32.0).color(Color::RED).no_wrap());
    let mut owner = div()
        .id("owner")
        .absolute()
        .left(20.0)
        .top(20.0)
        .w(140.0)
        .h(90.0);
    if clip {
        owner = owner.clip_path(if polygon {
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
    let root = div().w(W as f32).h(H as f32).bg(Color::BLUE);
    let ui = if inside {
        root.child(owner.child(child).motion().translate_x(value))
    } else {
        root.child(owner.child(child.motion().translate_x(value)))
    };
    let mut t = RenderTree::from_element(&ui);
    t.compute_layout(W as f32, H as f32);
    t
}
fn value() -> blinc_layout::motion::SharedAnimatedValue {
    let scheduler = Arc::new(Mutex::new(blinc_animation::AnimationScheduler::new()));
    Arc::new(Mutex::new(blinc_animation::AnimatedValue::with_default(
        scheduler.lock().unwrap().handle(),
        0.0,
    )))
}
fn render(c: &mut RenderContext, t: &RenderTree, compositor: bool, fast: bool) -> image::RgbaImage {
    let state = blinc_layout::RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    let texture = c.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("Motion text clip surface"),
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
fn motion_text_clip_public_pool_owns_path_and_polygon() {
    for compositor in [false, true] {
        for inside in [false, true] {
            let mut c = context(true, 1);
            let v = value();
            let make = |clip, polygon| motion_tree(clip, polygon, inside, v.clone());
            let baseline = render(&mut c, &make(false, false), compositor, false);
            for polygon in [false, true] {
                c.invalidate_render_cache();
                let t = make(true, polygon);
                for fast in [false, true] {
                    eprintln!(
                        "compositor={compositor} inside={inside} polygon={polygon} fast={fast}"
                    );
                    let page = render(&mut c, &t, compositor, fast);
                    compare(&page, &baseline, |x, y| triangle(x - 20.0, y - 20.0));
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn motion_text_clip_binding_updates_keep_owner_frames_and_dpi() {
    for compositor in [false, true] {
        for inside in [false, true] {
            for dpi in [1.0, 1.5] {
                let mut c = context(true, 1);
                let v = value();
                let mut t = motion_tree(true, false, inside, v.clone());
                let mut plain = motion_tree(false, false, inside, v.clone());
                t.set_scale_factor(dpi);
                plain.set_scale_factor(dpi);
                for dx in [0.0, 18.0, -6.0, 0.0] {
                    v.lock().unwrap().set_immediate(dx);
                    c.invalidate_render_cache();
                    let baseline = render(&mut c, &plain, compositor, false);
                    c.invalidate_render_cache();
                    let page = render(&mut c, &t, compositor, false);
                    assert!(
                        !c.cached_motion_subtree_text_prims
                            .as_ref()
                            .unwrap()
                            .is_empty()
                    );
                    assert!(
                        c.motion_subtree_text_patch.is_empty(),
                        "text must stay outside CSS promotion"
                    );
                    eprintln!("compositor={compositor} inside={inside} dpi={dpi} dx={dx}");
                    compare(&page, &baseline, |x, y| {
                        triangle(
                            x / dpi - 20.0 - if inside { dx } else { 0.0 },
                            y / dpi - 20.0,
                        )
                    });
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn motion_text_clip_replay_restores_rebased_rows_and_damage_scissor() {
    for transformed in [false, true] {
        let mut c = context(true, 1);
        let v = value();
        let make = |clip| {
            let mut t = motion_tree(clip, false, true, v.clone());
            if transformed {
                let owner = t.query_by_id("owner").unwrap();
                t.update_render_props(owner, |p| {
                    p.transform = Some(blinc_core::Transform::scale(1.2, 0.7));
                    p.transform_origin = Some([0.0, 0.0]);
                });
            }
            t
        };
        let baseline = render(&mut c, &make(false), false, false);
        c.invalidate_render_cache();
        let t = make(true);
        render(&mut c, &t, false, false);
        let snapshot = c.cached_motion_subtree_text_prims.clone().unwrap();
        assert!(!snapshot.aux_data.is_empty());
        let mut owned = cached_text::TextBatch {
            aux_data: Arc::new(vec![[0.0; 4]; 2048]),
            ..Default::default()
        };
        let mut source = PrimitiveBatch::new();
        source.primitives = snapshot.primitives.clone();
        source.aux_data = (*snapshot.aux_data).clone();
        owned.append(source);
        assert!(owned.primitives.iter().all(|p| p.clip_shape[0] > 2048.0));
        let selected = owned.filtered(|[x, _, w, _]| x + w > 35.0);
        assert!(Arc::ptr_eq(&owned.aux_data, &selected.aux_data));
        c.invalidate_render_cache();
        c.cached_motion_subtree_text_prims = Some(selected);
        for _ in 0..2 {
            let (texture, view) =
                crate::tests::create_test_texture(c.device(), c.texture_format(), W, H);
            c.renderer.clear_target(&view, wgpu::Color::BLUE);
            c.renderer.update_aux_data_slice(&vec![[0.0; 4]; 4096]);
            c.renderer.set_pending_scissor((35, 0, W - 35, H));
            c.dispatch_motion_subtree_overlay_pools(&t, &view);
            c.renderer.clear_pending_scissor();
            assert!(Arc::ptr_eq(
                &owned.aux_data,
                &c.cached_motion_subtree_text_prims
                    .as_ref()
                    .unwrap()
                    .aux_data
            ));
            let page = read_rendered_pixels(c.device(), c.queue(), &texture, W, H);
            compare(&page, &baseline, |x, y| {
                if (x - 35.0).abs() < 2.0 {
                    return None;
                }
                let (sx, sy) = if transformed { (1.2, 0.7) } else { (1.0, 1.0) };
                triangle((x - 20.0) / sx, (y - 20.0) / sy).map(|keep| keep && x > 35.0)
            });
        }
    }
}

#[test]
fn motion_text_clip_css_subtrees_require_repaint() {
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    let mut t = motion_tree(true, false, true, value());
    let owner = t.query_by_id("owner").unwrap();
    let sid = t.stable_id(owner).unwrap();
    let mut animation = blinc_layout::render_state::ActiveCssAnimation::new(
        blinc_animation::MultiKeyframeAnimation::new(1000),
    );
    animation.current_properties.opacity = Some(0.5);
    animation.current_properties.translate_x = Some(12.0);
    t.css_anim_store()
        .lock()
        .unwrap()
        .animations
        .insert(sid, animation);
    t.compute_animation_status();
    assert!(!t.composite_promotion().contains(&owner));
    assert!(!t.css_active_patchable());
}
