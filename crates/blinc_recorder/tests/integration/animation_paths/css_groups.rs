use super::*;

fn sliding_fade() -> Scene {
    Scene {
        name: "CSS sliding group with overlapping fills and text",
        steps: 24,
        step_ms: 16,
        build: Box::new(|ui| {
            ui.css(
                "@keyframes slide-fade {
                 from { transform: translateX(-20px); opacity: 0.4; }
                 to { transform: translateX(20px); opacity: 1; }
                 }
                 .slide-fade { animation: slide-fade 1600ms linear infinite alternate; }",
            );
            ui.mount(|_| {
                stage().child(
                    div().w(120.0).h(64.0).child(
                        div()
                            .class("slide-fade")
                            .w(120.0)
                            .h(64.0)
                            .relative()
                            .child(div().absolute().w(84.0).h(50.0).bg(Color::RED))
                            .child(
                                div()
                                    .absolute()
                                    .left(36.0)
                                    .top(14.0)
                                    .w(84.0)
                                    .h(50.0)
                                    .bg(Color::GREEN)
                                    .child(text("Calendar").color(Color::WHITE)),
                            ),
                    ),
                )
            });
            Box::new(|_, _| {})
        }),
    }
}

#[test]
fn css_sliding_fade_keeps_group_overlap_and_text_on_rows() {
    let scene = sliding_fade();
    let Some(reference) = run(&scene, Path::Full) else {
        return;
    };
    for path in [Path::Rows, Path::Table] {
        let (frames, paths) = run_logged(&scene, path).unwrap();
        if let Ok(dir) = std::env::var("BLINC_ANIM_PATHS_OUT") {
            std::fs::create_dir_all(&dir).unwrap();
            ScreenshotExporter::save_png(
                &sheet(&reference, &frames, &[0, 8, 16, 23]),
                format!("{dir}/css-group-{path:?}.png"),
            )
            .unwrap();
        }
        let retained = paths.iter().filter(|p| p.ends_with("rows:applied")).count();
        assert_eq!(retained, scene.steps, "{path:?}: {paths:?}");
        assert_ne!(frames.first().unwrap().data, frames.last().unwrap().data);
        for (step, (frame, full)) in frames.iter().zip(&reference).enumerate() {
            let d = difference(frame, full, 3);
            assert!(
                d.differing < 150,
                "{path:?} step {step}: {} pixels differ from the full group fade",
                d.differing,
            );
        }
    }
}

fn overlapping_slides(covered: bool) -> Scene {
    Scene {
        name: "overlapping CSS siblings in a clipped viewport",
        steps: 24,
        step_ms: 16,
        build: Box::new(move |ui| {
            ui.css(
                "@keyframes sibling-a {
                    from { transform: translateX(-18px); opacity: 0.4; }
                    to { transform: translateX(18px); opacity: 1; }
                 }
                 @keyframes sibling-b {
                    from { transform: translateX(18px); opacity: 1; }
                    to { transform: translateX(-18px); opacity: 0.4; }
                 }
                 .sibling-a { animation: sibling-a 1600ms linear infinite alternate; }
                 .sibling-b { animation: sibling-b 1600ms linear infinite alternate; }",
            );
            ui.mount(move |_| {
                let viewport = div()
                    .w(200.0)
                    .h(120.0)
                    .relative()
                    .overflow_clip()
                    .child(
                        div()
                            .class("sibling-a")
                            .absolute()
                            .w(150.0)
                            .h(100.0)
                            .bg(Color::RED)
                            .child(text("Leaving").color(Color::WHITE)),
                    )
                    .child(
                        div()
                            .class("sibling-b")
                            .absolute()
                            .left(70.0)
                            .top(10.0)
                            .w(150.0)
                            .h(100.0)
                            .bg(Color::GREEN)
                            .child(text("Arriving").color(Color::WHITE)),
                    );
                let root = stage().relative().child(viewport);
                if covered {
                    root.child(
                        div()
                            .absolute()
                            .left(130.0)
                            .top(60.0)
                            .w(70.0)
                            .h(100.0)
                            .bg(Color::BLUE),
                    )
                } else {
                    root
                }
            });
            Box::new(|_, _| {})
        }),
    }
}

#[test]
fn overlapping_slides_share_rows_without_covering_later_content() {
    for covered in [false, true] {
        let scene = overlapping_slides(covered);
        let Some(reference) = run(&scene, Path::Vector) else {
            return;
        };
        for path in [Path::Rows, Path::Table] {
            let (frames, paths) = run_logged(&scene, path).unwrap();
            assert_ne!(frames.first().unwrap().data, frames.last().unwrap().data);
            if !covered {
                assert_eq!(
                    paths.iter().filter(|p| p.ends_with("rows:applied")).count(),
                    scene.steps,
                    "overlapping siblings must retain their ordered paint: {path:?} {paths:?}",
                );
            }
            for (step, (frame, full)) in frames.iter().zip(&reference).enumerate() {
                let d = difference(frame, full, 3);
                assert!(
                    d.differing < 150,
                    "{path:?} covered={covered} step {step}: {} pixels differ",
                    d.differing
                );
                if covered {
                    let x = (165.0 * frame.width as f32 / W as f32).round() as u32;
                    let y = (100.0 * frame.height as f32 / H as f32).round() as u32;
                    assert_eq!(
                        frame.get_pixel(x, y),
                        Some([0, 0, 255, 255]),
                        "the later cover must stay above the slides"
                    );
                }
            }
        }
    }
}
