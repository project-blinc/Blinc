use super::grouped_text_tests::frame;
use super::opacity_group_tests::{CssSource, css_value, pixel};
use super::*;
use blinc_animation::{AnimatedValue, AnimationScheduler, SharedAnimatedValue};
use blinc_layout::rich_text;

#[derive(Clone, Copy, Debug)]
enum Source {
    Static,
    Binding,
    Css(CssSource),
}

fn tree(
    foreground: bool,
    bindings: Option<(SharedAnimatedValue, SharedAnimatedValue)>,
    wrapped: bool,
) -> (RenderTree, LayoutNodeId) {
    let label = rich_text("<span color=\"#00ff00\"><b>éMM</b></span> <span color=\"#00ffff\"><i><u>MMM MMM</u></i></span> <span color=\"#ff00ff\"><s>MMMM</s></span>")
        .size(24.0).bold_range(2..12);
    let label = if wrapped {
        label.wrap_to_width(100.0)
    } else {
        label.no_wrap()
    };
    let content = div().id("styled").child(label);
    let content = if foreground {
        content.foreground()
    } else {
        content
    };
    let owner = div()
        .id("group")
        .absolute()
        .left(20.0)
        .top(20.0)
        .w(180.0)
        .h(130.0)
        .bg(Color::RED)
        .child(content);
    let ui = div().w(240.0).h(180.0).bg(Color::BLUE);
    let ui = if let Some((opacity, x)) = bindings {
        ui.child(owner.motion().opacity(opacity).translate_x(x))
    } else {
        ui.child(owner)
    };
    let ui = ui
        .child(
            div()
                .absolute()
                .left(185.0)
                .top(155.0)
                .w(10.0)
                .h(10.0)
                .bg(Color::YELLOW),
        )
        .child(
            div()
                .absolute()
                .left(210.0)
                .top(20.0)
                .child(text("X").size(24.0).color(Color::WHITE)),
        );
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(240.0, 180.0);
    let wrapper = tree.query_by_id("styled").unwrap();
    let label = tree.layout_tree.children(wrapper)[0];
    if wrapped {
        tree.update_render_props(wrapper, |p| p.opacity = 0.5);
        tree.update_render_props(label, |p| {
            p.text_shadow = Some(blinc_core::Shadow::new(2.0, 2.0, 0.0, Color::WHITE))
        });
        let owner = tree.query_by_id("group").unwrap();
        tree.update_render_props(owner, |p| {
            p.transform = Some(blinc_core::Transform::scale(0.95, 1.05));
            p.transform_origin = Some([0.0, 0.0]);
            p.clip_path = Some(blinc_core::ClipPath::Path {
                vertices: vec![(0.0, 0.0), (180.0, 0.0), (160.0, 120.0), (0.0, 120.0)],
            });
        });
    }
    (tree, label)
}

#[test]
#[ignore] // Requires GPU
fn styled_group_static_runs_blend_once_and_keep_first_and_last_spans() {
    frames(Source::Static, false);
}

#[test]
#[ignore] // Requires GPU
fn styled_group_wrapped_multibyte_styles_and_clips_keep_all_runs() {
    frames(Source::Static, true);
}

#[test]
#[ignore] // Requires GPU
fn styled_group_motion_and_css_sources_recover_and_replay_all_runs() {
    for source in [
        Source::Binding,
        Source::Css(CssSource::Animation),
        Source::Css(CssSource::Transition),
    ] {
        frames(source, true);
    }
}

fn frames(source: Source, wrapped: bool) {
    use slotmap::Key as _;
    for unified in [false, true] {
        for compositor in [false, true] {
            for foreground in [false, true] {
                let mut context = super::main_text_clip_tests::context(unified, 1);
                let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
                let alpha = Arc::new(Mutex::new(AnimatedValue::with_default(
                    scheduler.lock().unwrap().handle(),
                    1.0,
                )));
                let x = Arc::new(Mutex::new(AnimatedValue::with_default(
                    scheduler.lock().unwrap().handle(),
                    1.0,
                )));
                let (mut tree, label) = tree(
                    foreground,
                    matches!(source, Source::Binding).then(|| (alpha.clone(), x.clone())),
                    wrapped,
                );
                let group = tree.query_by_id("group").unwrap();
                let state = RenderState::new(scheduler);
                let legacy = frame(&mut context, &tree, &state, compositor);
                tree.update_render_props(group, |p| p.opacity = 0.999999);
                context.invalidate_render_cache();
                let captured = frame(&mut context, &tree, &state, compositor);
                let runs: Vec<_> = context
                    .cached_texts
                    .as_ref()
                    .unwrap()
                    .iter()
                    .filter(|t| t.node == label)
                    .collect();
                assert!(runs.len() >= 4);
                assert!(runs.iter().any(|run| run.content.contains('é')));
                if wrapped {
                    assert!(runs.iter().any(|run| run.y > runs[0].y + 5.0));
                    let cyan: Vec<_> = runs
                        .iter()
                        .filter(|run| run.color == [0.0, 1.0, 1.0, 1.0])
                        .collect();
                    assert!(cyan.len() >= 2);
                    assert!(cyan.iter().any(|run| run.y > cyan[0].y + 5.0));
                }
                assert_native_runs(&context, label.data().as_ffi());
                tree.update_render_props(group, |p| p.opacity = 1.0);
                for (index, opacity) in [0.5_f32, 0.0, 0.75, 1.0, 0.5].into_iter().enumerate() {
                    let offset = if matches!(source, Source::Binding) {
                        [0, 5, 0, 2, 0][index]
                    } else {
                        0
                    };
                    eprintln!(
                        "{source:?} wrapped={wrapped} unified={unified} compositor={compositor} foreground={foreground} opacity={opacity}"
                    );
                    match source {
                        Source::Static => {
                            tree.update_render_props(group, |p| p.opacity = opacity);
                            context.invalidate_render_cache();
                        }
                        Source::Binding => {
                            alpha.lock().unwrap().set_immediate(opacity);
                            x.lock().unwrap().set_immediate(1.0 + offset as f32);
                        }
                        Source::Css(source) => {
                            css_value(&tree, group, source, Some(opacity));
                            tree.update_render_props(group, |p| p.opacity = opacity);
                        }
                    }
                    let baseline = if opacity == 1.0 && !matches!(source, Source::Binding) {
                        &legacy
                    } else {
                        &captured
                    };
                    for _ in 0..2 {
                        let page = frame(&mut context, &tree, &state, compositor);
                        let origin = u32::from(matches!(source, Source::Binding));
                        for y in 20..140 {
                            for x in 20..190 {
                                let base = baseline.get_pixel(x + origin, y).0;
                                pixel(
                                    &page,
                                    (x + origin + offset, y),
                                    [
                                        (base[0] as f32 * opacity).round() as u8,
                                        (base[1] as f32 * opacity).round() as u8,
                                        (base[2] as f32 * opacity + 255.0 * (1.0 - opacity)).round()
                                            as u8,
                                        255,
                                    ],
                                );
                            }
                        }
                        pixel(&page, (190, 160), [255, 255, 0, 255]);
                        for y in 20..55 {
                            for x in 210..238 {
                                assert_eq!(page.get_pixel(x, y), legacy.get_pixel(x, y));
                            }
                        }
                        if opacity > 0.0 {
                            assert_native_runs(&context, label.data().as_ffi());
                        }
                    }
                }
            }
        }
    }
}

fn assert_native_runs(context: &RenderContext, label: u64) {
    let batch = context.cached_bg_batch.as_ref().unwrap();
    if batch.cross_stream_groups.text_nodes.contains(&label) {
        for color in [Color::GREEN, Color::CYAN, Color::MAGENTA] {
            assert!(
                batch
                    .primitives
                    .iter()
                    .chain(&batch.foreground_primitives)
                    .any(|p| p.color == [color.r, color.g, color.b, color.a]),
                "missing span {color:?}"
            );
        }
    }
}

#[test]
fn styled_group_capability_and_unsupported_owners_preserve_fallback() {
    use blinc_layout::renderer::{AmbientPaintState, DynamicKind, DynamicRegion};
    super::opacity_group_tests::init_theme();
    for (enabled, unsupported) in [(false, false), (true, false), (true, true)] {
        let (mut tree, label) = tree(true, None, false);
        let group = tree.query_by_id("group").unwrap();
        tree.update_render_props(group, |p| {
            p.opacity = 0.5;
            if unsupported {
                p.background = Some(blinc_core::ImageBrush::new("placeholder").into());
            }
        });
        let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
        let mut paint = GpuPaintContext::new(240.0, 180.0);
        if enabled {
            paint.enable_deferred_text();
        }
        tree.render_with_motion(&mut paint, &state);
        let mut batch = paint.take_batch();
        use slotmap::Key as _;
        let count = batch.layer_commands.iter().chain(&batch.foreground_layer_commands)
            .filter(|e| matches!(e.command, blinc_gpu::LayerCommand::DeferredText { id, .. } if id == label.data().as_ffi())).count();
        assert_eq!(count, usize::from(enabled && !unsupported));
        batch.resolve_deferred_text(&mut Default::default());
        assert_eq!(
            batch.cross_stream_groups.text_nodes.len(),
            usize::from(enabled && !unsupported)
        );
        let region = DynamicRegion {
            root: group,
            screen_aabb: [20.0, 20.0, 180.0, 130.0],
            visit_seq: 0,
            ambient: AmbientPaintState {
                affine: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                opacity: 1.0,
                clip_aabb: None,
                z_layer: 0,
                in_overlay_subtree: false,
            },
            kind: DynamicKind::MotionSubtree,
        };
        let mut fallback = GpuPaintContext::new(240.0, 180.0);
        tree.render_dynamic_region(&mut fallback, &region, &state);
        assert!(
            fallback
                .take_batch()
                .cross_stream_groups
                .text_nodes
                .is_empty()
        );
    }
}

#[test]
#[ignore] // Requires GPU
fn styled_group_changed_empty_and_recovered_runs_replace_cached_content() {
    use blinc_gpu::primitives::PrimitiveType;
    for compositor in [false, true] {
        let mut context = super::main_text_clip_tests::context(true, 1);
        for color in [
            Some(Color::GREEN),
            None,
            Some(Color::YELLOW),
            Some(Color::GREEN),
        ] {
            let label = rich_text(if color.is_some() { "MMMM" } else { "" })
                .size(32.0)
                .default_color(color.unwrap_or(Color::GREEN))
                .no_wrap();
            let ui = div().w(240.0).h(180.0).bg(Color::BLUE).child(
                div()
                    .absolute()
                    .left(20.0)
                    .top(20.0)
                    .w(180.0)
                    .h(130.0)
                    .bg(Color::RED)
                    .opacity(0.5)
                    .child(label),
            );
            let mut tree = RenderTree::from_element(&ui);
            tree.compute_layout(240.0, 180.0);
            let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
            context.invalidate_render_cache();
            for _ in 0..2 {
                let page = frame(&mut context, &tree, &state, compositor);
                let batch = context.cached_bg_batch.as_ref().unwrap();
                let glyphs: Vec<_> = batch
                    .primitives
                    .iter()
                    .filter(|p| p.type_info[0] == PrimitiveType::Text as u32)
                    .collect();
                if let Some(color) = color {
                    assert!(glyphs.len() >= 4);
                    assert!(
                        glyphs
                            .iter()
                            .all(|p| p.color == [color.r, color.g, color.b, color.a])
                    );
                } else {
                    assert!(glyphs.is_empty());
                }
                for y in 25..140 {
                    for x in 25..195 {
                        assert!(
                            page.get_pixel(x, y).0[2].abs_diff(128) <= 2,
                            "{color:?} ({x},{y})"
                        );
                    }
                }
            }
        }
    }
}
