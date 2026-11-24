use super::*;

fn hover_fill(moving: bool, text_color: bool) -> Scene {
    hover_fill_with_handoff(moving, text_color, false)
}

fn hover_fill_with_handoff(moving: bool, text_color: bool, handoff: bool) -> Scene {
    Scene {
        name: "retained hover fill",
        steps: 24,
        step_ms: 16,
        build: Box::new(move |ui| {
            let mixed = signal(false);
            let extra = "color: white; transition: background 800ms linear, color 800ms linear;";
            let hover = if text_color { "color: black;" } else { "" };
            ui.css(&format!(
                ".tint {{ background: transparent; {extra} }}
                 .tint:hover {{ background: rgba(230, 70, 60, 0.85); {hover} }}
                 @keyframes drift {{
                 from {{ transform: translateX(-12px); opacity: 0.4; }}
                 to {{ transform: translateX(12px); opacity: 1; }}
                 }}
                 .drift {{ animation: drift 1600ms linear infinite alternate; }}
                 .tint.mix {{ transition: background 800ms linear, color 800ms linear; }}
                 .tint.mix:hover {{ color: black; }}"
            ));
            ui.mount(move |_| {
                stage().child(
                    div()
                        .w(140.0)
                        .h(96.0)
                        .when(moving, |d| d.class("drift"))
                        .child(
                            div()
                                .id("target")
                                .class("tint")
                                .class_when("mix", mixed)
                                .w(120.0)
                                .h(64.0)
                                .relative()
                                .child(
                                    div()
                                        .absolute()
                                        .left(12.0)
                                        .top(12.0)
                                        .w(40.0)
                                        .h(40.0)
                                        .bg(Color::TRANSPARENT),
                                )
                                .child(text("Keep this label").color(Color::WHITE)),
                        ),
                )
            });
            Box::new(move |ui, step| {
                if handoff && step == 8 {
                    mixed.set(true);
                    ui.move_to(2.0, 2.0);
                }
                if step == 0 || (handoff && step == 8) {
                    let node = ui.find("#target").unwrap();
                    let b = ui.tree().get_absolute_bounds(node).unwrap();
                    ui.move_to(b.x + b.width * 0.5, b.y + b.height * 0.5);
                }
                if handoff && step == 9 {
                    let tree = ui.tree();
                    let stable = tree.stable_id(ui.find("#target").unwrap()).unwrap();
                    assert!(
                        tree.css_anim_store()
                            .lock()
                            .unwrap()
                            .transitions
                            .get(&stable)
                            .is_some_and(
                                |t| t.is_playing && t.current_properties.text_color.is_some()
                            ),
                        "the fixture must start a text-colour transition"
                    );
                }
            })
        }),
    }
}

#[test]
fn background_only_hover_keeps_text_children_and_moving_group_retained() {
    for moving in [false, true] {
        let scene = hover_fill(moving, false);
        let Some(reference) = run(&scene, Path::Full) else {
            return;
        };
        for path in [Path::Rows, Path::Table] {
            let (frames, paths) = run_logged(&scene, path).unwrap();
            if let Ok(dir) = std::env::var("BLINC_ANIM_PATHS_OUT") {
                std::fs::create_dir_all(&dir).unwrap();
                ScreenshotExporter::save_png(
                    &sheet(&reference, &frames, &[0, 4, 8, 16, 23]),
                    format!("{dir}/background-{moving}-{path:?}.png"),
                )
                .unwrap();
            }
            let retained = paths.iter().filter(|p| !p.starts_with("full")).count();
            assert!(retained >= scene.steps - 2, "{moving} {path:?}: {paths:?}");
            assert_ne!(frames.first().unwrap().data, frames.last().unwrap().data);
            for (step, (frame, full)) in frames.iter().zip(&reference).enumerate() {
                let d = difference(frame, full, 3);
                assert!(
                    d.differing < 150,
                    "{moving} {path:?} step {step}: {} differing pixels",
                    d.differing
                );
            }
        }
    }
}

#[test]
fn text_color_with_background_keeps_the_full_paint_fallback() {
    let scene = hover_fill(false, true);
    let Some(reference) = run(&scene, Path::Full) else {
        return;
    };
    let (frames, paths) = run_logged(&scene, Path::Rows).unwrap();
    assert!(paths.iter().all(|p| p == "full"), "{paths:?}");
    for (frame, full) in frames.iter().zip(&reference) {
        assert_eq!(difference(frame, full, 3).differing, 0);
    }
}

#[test]
fn a_retained_fill_repaints_when_its_source_adds_text_color() {
    let scene = hover_fill_with_handoff(true, false, true);
    let Some(reference) = run(&scene, Path::Full) else {
        return;
    };
    for path in [Path::Rows, Path::Table] {
        let (frames, paths) = run_logged(&scene, path).unwrap();
        assert!(
            paths[..8].iter().filter(|p| !p.starts_with("full")).count() >= 6,
            "{paths:?}"
        );
        assert!(paths[9..].iter().all(|p| p == "full"), "{paths:?}");
        for (frame, full) in frames.iter().zip(&reference) {
            assert!(difference(frame, full, 3).differing < 150);
        }
    }
}
