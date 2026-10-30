//! A shared renderer keeps cached output within the owning tree namespace.
use super::*;
use blinc_animation::AnimationScheduler;

fn scene(label: &str, color: Color) -> RenderTree {
    let target_node = Arc::new(Mutex::new(None));
    let canvas_node = target_node.clone();
    let mut tree = RenderTree::from_element(
        &div()
            .id("card")
            .w(64.0)
            .h(48.0)
            .bg(color)
            .child(text(label).size(18.0).color(Color::WHITE))
            .child(
                canvas(move |_, _| {
                    if let Some(node) = *canvas_node.lock().unwrap() {
                        blinc_layout::queue_prop_update_partial(
                            node,
                            blinc_layout::PropertyId::Opacity,
                            blinc_layout::SideEffects::VISUAL,
                            |props| props.opacity = 1.0,
                        );
                    }
                })
                .w(1.0)
                .h(1.0),
            ),
    );
    tree.compute_layout(64.0, 48.0);
    *target_node.lock().unwrap() = tree.root();
    tree
}

#[test]
#[ignore] // Requires GPU
fn tree_scope_cache_survives_interleaved_window_frames() {
    let first = scene("AAAA", Color::RED);
    let second = scene("BBBB", Color::BLUE);
    assert_eq!(first.root(), second.root());
    assert_eq!(
        first.stable_id(first.root().unwrap()),
        second.stable_id(second.root().unwrap())
    );
    assert_ne!(first.tree_scope_id(), second.tree_scope_id());
    let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
    for compositor in [false, true] {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let target = context.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("Interleaved windows"),
            size: wgpu::Extent3d {
                width: 64,
                height: 48,
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
        let view = target.create_view(&Default::default());
        let mut reference: Option<Vec<u8>> = None;
        for (index, (tree, primary)) in [
            (&first, true),
            (&first, true),
            (&second, false),
            (&first, true),
            (&first, true),
            (&second, false),
            (&first, true),
        ]
        .into_iter()
        .enumerate()
        {
            context
                .render_tree_with_motion_opt(
                    tree,
                    &state,
                    64,
                    48,
                    &view,
                    (primary && compositor).then_some(&target),
                    primary,
                )
                .unwrap();
            let queued = tree.take_pending_property_updates();
            assert!(
                !queued.is_empty(),
                "canvas did not run: comp={compositor} frame={index}"
            );
            assert!(
                queued
                    .iter()
                    .all(|update| update.tree_scope == tree.tree_scope_id()),
                "canvas replay lost its owner: comp={compositor} frame={index}"
            );
            let page = crate::tests::read_rendered_pixels(
                context.device(),
                context.queue(),
                &target,
                64,
                48,
            );
            if primary {
                if let Some(reference) = &reference {
                    assert_eq!(
                        page.get_pixel(50, 40).0,
                        [255, 0, 0, 255],
                        "comp={compositor} frame={index}"
                    );
                    assert!(
                        page.as_raw()
                            .iter()
                            .zip(reference.iter())
                            .all(|(a, b)| a.abs_diff(*b) <= 2),
                        "text cache crossed trees: comp={compositor} frame={index}"
                    );
                } else {
                    reference = Some(page.into_raw());
                }
            } else {
                assert_eq!(page.get_pixel(50, 40).0, [0, 0, 255, 255]);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn tree_scope_cache_rebuilds_promoted_textures_after_owner_changes() {
    use super::opacity_group_tests::{CssSource, css_fixture, init_theme, pixel, surface};
    use crate::app::{BlincApp, BlincConfig};
    use slotmap::Key;

    init_theme();
    for source in [CssSource::Animation, CssSource::Transition] {
        for compositor in [false, true] {
            let mut app = BlincApp::with_config(BlincConfig {
                sample_count: 1,
                ..Default::default()
            })
            .unwrap();
            let mut trees = [css_fixture(0.25, source), css_fixture(0.75, source)];
            for (tree, owner) in &mut trees {
                let store = tree.css_anim_store();
                let mut store = store.lock().unwrap();
                let entries = match source {
                    CssSource::Animation => &mut store.animations,
                    CssSource::Transition => &mut store.transitions,
                };
                entries
                    .get_mut(&tree.stable_id(*owner).unwrap())
                    .unwrap()
                    .current_properties
                    .background_color = None;
                drop(store);
                tree.apply_all_css_animation_props();
                tree.apply_all_css_transition_props();
            }
            let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
            for index in [0, 0, 1, 1, 0, 0, 1, 0] {
                let (tree, owner) = &trees[index];
                let target = surface(&app);
                let view = target.create_view(&Default::default());
                app.render_tree_with_motion_opt(
                    tree,
                    &state,
                    &view,
                    (compositor && index == 0).then_some(&target),
                    160,
                    120,
                    true,
                )
                .unwrap();
                assert!(
                    app.context()
                        .css_composited_textures
                        .contains_key(&owner.data().as_ffi())
                );
                let page = crate::tests::read_rendered_pixels(
                    app.device(),
                    app.queue(),
                    &target,
                    160,
                    120,
                );
                let (color, blue) = if index == 0 { (64, 191) } else { (191, 64) };
                pixel(&page, (25, 25), [color, 0, blue, 255]);
                pixel(&page, (55, 55), [0, color, blue, 255]);
            }
        }
    }
}
