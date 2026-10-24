use super::opacity_group_tests::{CssSource, css_fixture, css_value, init_theme, pixel, surface};
use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::read_rendered_pixels;
use blinc_animation::AnimationScheduler;
use blinc_layout::render_state::ActiveCssAnimation;
use slotmap::Key;

fn app() -> BlincApp {
    BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })
    .expect("offscreen GPU required")
}

fn state() -> RenderState {
    let mut state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
    state.set_viewport_size(160.0, 120.0);
    state
}

fn change_source(
    tree: &RenderTree,
    node: LayoutNodeId,
    source: CssSource,
    change: impl FnOnce(&mut ActiveCssAnimation),
) {
    let store = tree.css_anim_store();
    let mut store = store.lock().unwrap();
    let entries = match source {
        CssSource::Animation => &mut store.animations,
        CssSource::Transition => &mut store.transitions,
    };
    change(entries.get_mut(&tree.stable_id(node).unwrap()).unwrap());
}

fn remove_source(tree: &RenderTree, node: LayoutNodeId, source: CssSource) {
    let store = tree.css_anim_store();
    let mut store = store.lock().unwrap();
    let entries = match source {
        CssSource::Animation => &mut store.animations,
        CssSource::Transition => &mut store.transitions,
    };
    entries.remove(&tree.stable_id(node).unwrap());
}

fn apply(tree: &mut RenderTree) {
    tree.apply_all_css_animation_props();
    tree.apply_all_css_transition_props();
}

fn frame(
    app: &mut BlincApp,
    tree: &RenderTree,
    state: &RenderState,
    compositor: bool,
    opacity: f32,
) {
    for _ in 0..2 {
        let texture = surface(app);
        let view = texture.create_view(&Default::default());
        app.render_tree_with_motion_opt(
            tree,
            state,
            &view,
            compositor.then_some(&texture),
            160,
            120,
            true,
        )
        .unwrap();
        let page = read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120);
        pixel(
            &page,
            (55, 55),
            [
                0,
                (255.0 * opacity).round() as u8,
                (255.0 * (1.0 - opacity)).round() as u8,
                255,
            ],
        );
        pixel(
            &page,
            (25, 25),
            [
                (255.0 * opacity).round() as u8,
                0,
                (255.0 * (1.0 - opacity)).round() as u8,
                255,
            ],
        );
        pixel(&page, (135, 105), [128, 128, 255, 255]);
    }
}

#[test]
#[ignore] // Requires GPU
fn css_lifecycle_transparent_first_paint_recovers_with_and_without_other_records() {
    init_theme();
    for source in [CssSource::Animation, CssSource::Transition] {
        for compositor in [false, true] {
            for other_record in [false, true] {
                let mut app = app();
                let (mut tree, owner) = css_fixture(0.0, source);
                if other_record {
                    let peer = tree.query_by_id("css-peer").unwrap();
                    css_value(&tree, peer, source, Some(0.5));
                }
                let state = state();
                for opacity in [0.0, 0.0, 0.5, 0.0, 0.75, 1.0] {
                    eprintln!(
                        "{source:?} compositor={compositor} other_record={other_record} opacity={opacity}"
                    );
                    css_value(&tree, owner, source, Some(opacity));
                    apply(&mut tree);
                    frame(&mut app, &tree, &state, compositor, opacity);
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn css_lifecycle_source_start_refreshes_an_existing_static_cache() {
    init_theme();
    for source in [CssSource::Animation, CssSource::Transition] {
        for compositor in [false, true] {
            let mut app = app();
            let (mut tree, owner) = css_fixture(1.0, source);
            remove_source(&tree, owner, source);
            let peer = tree.query_by_id("css-peer").unwrap();
            css_value(&tree, peer, source, Some(0.5));
            let state = state();
            frame(&mut app, &tree, &state, compositor, 1.0);
            assert!(!tree.css_anim_paint_records().contains_key(&owner));
            css_value(&tree, owner, source, Some(0.5));
            apply(&mut tree);
            eprintln!("source start {source:?} compositor={compositor}");
            frame(&mut app, &tree, &state, compositor, 0.5);
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn css_lifecycle_completion_restart_and_removal_refresh_cached_output() {
    init_theme();
    for source in [CssSource::Animation, CssSource::Transition] {
        for compositor in [false, true] {
            for other_record in [false, true] {
                let mut app = app();
                let (mut tree, owner) = css_fixture(0.5, source);
                if other_record {
                    let peer = tree.query_by_id("css-peer").unwrap();
                    css_value(&tree, peer, source, Some(0.5));
                }
                let state = state();
                apply(&mut tree);
                frame(&mut app, &tree, &state, compositor, 0.5);
                change_source(&tree, owner, source, |entry| {
                    entry.current_properties.opacity = Some(0.75);
                    entry.is_playing = false;
                });
                apply(&mut tree);
                assert_eq!(tree.get_render_node(owner).unwrap().props.opacity, 0.75);
                eprintln!(
                    "completion {source:?} compositor={compositor} other_record={other_record}"
                );
                frame(&mut app, &tree, &state, compositor, 0.75);
                assert!(!tree.css_anim_paint_records().contains_key(&owner));
                css_value(&tree, owner, source, Some(0.25));
                change_source(&tree, owner, source, |entry| entry.is_playing = true);
                apply(&mut tree);
                frame(&mut app, &tree, &state, compositor, 0.25);
                remove_source(&tree, owner, source);
                tree.update_render_props(owner, |props| props.opacity = 1.0);
                apply(&mut tree);
                frame(&mut app, &tree, &state, compositor, 1.0);
                assert!(!tree.css_anim_paint_records().contains_key(&owner));
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn css_lifecycle_promotion_changes_and_completion_transfer_cached_ownership() {
    init_theme();
    for source in [CssSource::Animation, CssSource::Transition] {
        for compositor in [false, true] {
            let mut app = app();
            let (mut tree, owner) = css_fixture(1.0, source);
            let state = state();
            let key = owner.data().as_ffi();
            for (promoted, opacity) in [(true, 0.5), (false, 0.75), (true, 0.25)] {
                css_value(&tree, owner, source, Some(opacity));
                if promoted {
                    change_source(&tree, owner, source, |entry| {
                        entry.current_properties.background_color = None
                    });
                }
                apply(&mut tree);
                eprintln!("promotion {source:?} compositor={compositor} promoted={promoted}");
                frame(&mut app, &tree, &state, compositor, opacity);
                assert_eq!(
                    app.context().css_composited_textures.contains_key(&key),
                    promoted
                );
            }
            change_source(&tree, owner, source, |entry| {
                entry.current_properties.opacity = Some(0.6);
                entry.is_playing = false;
            });
            apply(&mut tree);
            assert_eq!(tree.get_render_node(owner).unwrap().props.opacity, 0.6);
            frame(&mut app, &tree, &state, compositor, 0.6);
            assert!(!app.context().css_composited_textures.contains_key(&key));
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn css_lifecycle_unchanged_transparent_offscreen_and_settled_frames_keep_cache() {
    init_theme();
    for source in [CssSource::Animation, CssSource::Transition] {
        for offscreen in [false, true] {
            let mut app = app();
            let (mut tree, owner) = if offscreen {
                let ui = div().w(160.0).h(120.0).bg(Color::BLUE).child(
                    div()
                        .id("offscreen")
                        .absolute()
                        .top(200.0)
                        .w(160.0)
                        .h(120.0)
                        .bg(Color::GREEN)
                        .opacity(0.5),
                );
                let mut tree = RenderTree::from_element(&ui);
                tree.compute_layout(160.0, 120.0);
                let owner = tree.query_by_id("offscreen").unwrap();
                css_value(&tree, owner, source, Some(0.5));
                (tree, owner)
            } else {
                css_fixture(0.0, source)
            };
            let state = state();
            apply(&mut tree);
            let texture = surface(&app);
            let view = texture.create_view(&Default::default());
            app.render_tree_with_motion_opt(&tree, &state, &view, Some(&texture), 160, 120, true)
                .unwrap();
            assert_eq!(tree.painted_node_ids().contains(&owner), !offscreen);
            tree.css_anim_paint_records_mut().remove(&owner);
            let context = app.context();
            let before = bytemuck::cast_slice::<GpuPrimitive, u8>(
                &context.cached_bg_batch.as_ref().unwrap().primitives,
            )
            .to_vec();
            for _ in 0..3 {
                assert!(!context.refresh_css_cache_ownership(&tree));
                assert_eq!(
                    bytemuck::cast_slice::<GpuPrimitive, u8>(
                        &context.cached_bg_batch.as_ref().unwrap().primitives
                    ),
                    before
                );
            }
            change_source(&tree, owner, source, |entry| {
                entry.current_properties.opacity = Some(0.75);
                entry.is_playing = false;
            });
            apply(&mut tree);
            let texture = surface(&app);
            let view = texture.create_view(&Default::default());
            app.render_tree_with_motion_opt(&tree, &state, &view, Some(&texture), 160, 120, true)
                .unwrap();
            assert!(!tree.css_has_active());
            assert!(!app.context().refresh_css_cache_ownership(&tree));
            assert!(app.context().cached_bg_batch.is_some());
        }
    }
}
