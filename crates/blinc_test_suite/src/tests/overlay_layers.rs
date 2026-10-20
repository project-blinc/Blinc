//! Native overlays preserve their target and composite layers in paint order.
#[cfg(test)]
mod tests {
    use crate::harness::{TestHarness, TestHarnessConfig};
    use blinc_core::{LayerConfig, LayerEffect};
    use blinc_gpu::{GpuPrimitive, PrimitiveBatch, primitives::LayerCommand};
    use image::RgbaImage;

    fn rect(batch: &mut PrimitiveBatch, bounds: [f32; 4], color: [f32; 4]) {
        let [x, y, w, h] = bounds;
        let mut p = GpuPrimitive::rect(x, y, w, h);
        p.color = color;
        batch.push(p);
    }

    fn render(
        harness: &TestHarness,
        batch: &PrimitiveBatch,
        name: &str,
        samples: u32,
    ) -> RgbaImage {
        let path = harness.output_path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        harness
            .render_with(160, 120, &path, |renderer, target| {
                renderer.render_with_clear(target, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
                renderer.render_overlay_msaa(target, batch, samples);
            })
            .unwrap();
        image::open(path).unwrap().to_rgba8()
    }

    #[test]
    #[ignore] // Requires GPU
    fn native_overlay_layers_preserve_opacity_nesting_order_and_target() {
        let harness = TestHarness::with_config(TestHarnessConfig {
            sample_count: 1,
            ..Default::default()
        })
        .unwrap();
        let mut batch = PrimitiveBatch::new();
        batch.push_layer_command(LayerCommand::Push {
            config: LayerConfig::new().opacity(0.5),
        });
        rect(&mut batch, [20.0, 20.0, 60.0, 60.0], [1.0, 0.0, 0.0, 1.0]);
        batch.push_layer_command(LayerCommand::Push {
            config: LayerConfig::new().opacity(0.5),
        });
        rect(&mut batch, [40.0, 40.0, 20.0, 20.0], [0.0, 1.0, 0.0, 1.0]);
        batch.push_layer_command(LayerCommand::Pop);
        batch.push_layer_command(LayerCommand::Pop);
        rect(&mut batch, [70.0, 30.0, 20.0, 20.0], [1.0, 1.0, 0.0, 1.0]);
        for samples in [1, 4] {
            let page = render(
                &harness,
                &batch,
                &format!("overlay_order_{samples}"),
                samples,
            );
            for (xy, expected) in [
                ((5, 5), [0, 0, 255, 255]),
                ((30, 30), [128, 0, 127, 255]),
                ((50, 50), [64, 64, 127, 255]),
                ((75, 35), [255, 255, 0, 255]),
            ] {
                let actual = page.get_pixel(xy.0, xy.1).0;
                assert!(
                    actual
                        .iter()
                        .zip(expected)
                        .all(|(a, b)| (*a as i32 - b).abs() <= 1),
                    "{samples} samples at {xy:?}: {actual:?}"
                );
            }
        }
    }

    #[test]
    #[ignore] // Requires GPU
    fn native_overlay_blur_reaches_outside_content() {
        let harness = TestHarness::with_config(TestHarnessConfig {
            sample_count: 1,
            ..Default::default()
        })
        .unwrap();
        let mut batch = PrimitiveBatch::new();
        batch.push_layer_command(LayerCommand::Push {
            config: LayerConfig::new()
                .opacity(1.0)
                .effect(LayerEffect::blur(4.0)),
        });
        rect(&mut batch, [50.0, 40.0, 40.0, 40.0], [1.0, 0.0, 0.0, 1.0]);
        batch.push_layer_command(LayerCommand::Pop);
        let page = render(&harness, &batch, "overlay_blur", 4);
        assert!(page.get_pixel(48, 60)[0] > 0, "blur reaches beyond the box");
        assert_eq!(page.get_pixel(5, 5).0, [0, 0, 255, 255]);
    }

    fn harness() -> TestHarness {
        TestHarness::with_config(TestHarnessConfig {
            sample_count: 1,
            ..Default::default()
        })
        .unwrap()
    }

    #[test]
    #[ignore] // Requires GPU
    fn native_overlay_identity_effect_keeps_mesh_coverage() {
        use blinc_core::{Color, DrawContext, Path};
        let harness = harness();
        let mut ctx = blinc_gpu::GpuPaintContext::new(160.0, 120.0);
        ctx.fill_path(
            &Path::new()
                .move_to(20.25, 20.5)
                .line_to(130.5, 35.25)
                .line_to(45.25, 95.75)
                .close(),
            Color::WHITE.into(),
        );
        let bare = ctx.take_batch();
        let baseline = render(&harness, &bare, "overlay_mesh_bare", 4);
        let mut batch = PrimitiveBatch::new();
        batch.push_layer_command(LayerCommand::Push {
            config: LayerConfig::new()
                .opacity(1.0)
                .effect(LayerEffect::ColorMatrix {
                    matrix: [
                        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
                        0.0, 0.0, 0.0, 1.0, 0.0,
                    ],
                }),
        });
        batch.primitives.extend_from_slice(&bare.primitives);
        batch.aux_data.clone_from(&bare.aux_data);
        batch.push_layer_command(LayerCommand::Pop);
        let actual = render(&harness, &batch, "overlay_mesh_identity", 4);
        let worst = actual
            .as_raw()
            .iter()
            .zip(baseline.as_raw())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(
            worst <= 1,
            "identity effect changed mesh coverage by {worst}/255"
        );
        if let LayerCommand::Push { config } = &mut batch.layer_commands[0].command {
            config.effects = vec![LayerEffect::blur(0.0)];
        }
        let zero = render(&harness, &batch, "overlay_mesh_zero_blur", 4);
        assert_eq!(
            zero.as_raw(),
            baseline.as_raw(),
            "zero blur must keep the no-effect pixels"
        );
    }

    #[test]
    #[ignore] // Requires GPU
    fn native_overlay_path_only_layers_keep_geometry_and_opacity() {
        use blinc_core::{Color, Path, Point};
        let harness = harness();
        let mut batch = PrimitiveBatch::new();
        // Preceding geometry makes the path range start at a nonzero vertex index.
        batch.push_path(blinc_gpu::path::tessellate_fill(
            &Path::circle(Point::new(130.0, 90.0), 8.0),
            &Color::GREEN.into(),
        ));
        batch.push_layer_command(LayerCommand::Push {
            config: LayerConfig::new().opacity(0.5),
        });
        batch.push_path(blinc_gpu::path::tessellate_fill(
            &Path::rect(blinc_core::Rect::new(20.0, 30.0, 50.0, 40.0)),
            &Color::RED.into(),
        ));
        batch.push_layer_command(LayerCommand::Pop);
        let actual = render(&harness, &batch, "overlay_path_layer", 4);
        let p = actual.get_pixel(40, 50).0;
        assert!(
            p[0].abs_diff(128) <= 1 && p[2].abs_diff(127) <= 1,
            "path opacity: {p:?}"
        );
        assert_eq!(actual.get_pixel(130, 90).0, [0, 255, 0, 255]);
        assert_eq!(actual.get_pixel(5, 5).0, [0, 0, 255, 255]);
    }

    #[test]
    #[ignore] // Requires GPU
    fn native_overlay_effects_respect_common_rounded_clip() {
        let harness = harness();
        let mut batch = PrimitiveBatch::new();
        batch.push_layer_command(LayerCommand::Push {
            config: LayerConfig::new().opacity(1.0).blur(4.0),
        });
        let mut p = GpuPrimitive::rect(30.0, 30.0, 60.0, 60.0)
            .with_clip_rounded_rect(30.0, 30.0, 60.0, 60.0, 15.0, 15.0, 15.0, 15.0);
        p.color = [1.0, 0.0, 0.0, 1.0];
        batch.push(p);
        batch.push_layer_command(LayerCommand::Pop);
        let actual = render(&harness, &batch, "overlay_rounded_clip", 4);
        assert_eq!(actual.get_pixel(29, 50).0, [0, 0, 255, 255]);
        assert_eq!(actual.get_pixel(31, 31).0, [0, 0, 255, 255]);
        assert!(actual.get_pixel(60, 60)[0] > 240);
    }

    #[test]
    #[ignore] // Requires GPU
    fn native_overlay_shadow_glow_colour_filter_and_blend_are_applied() {
        use blinc_core::{BlendMode, Color};
        let harness = harness();
        let effects = [
            LayerEffect::DropShadow {
                offset_x: 20.0,
                offset_y: 0.0,
                blur: 0.0,
                spread: 0.0,
                color: Color::GREEN,
            },
            LayerEffect::Glow {
                color: Color::GREEN,
                blur: 2.0,
                range: 2.0,
                opacity: 1.0,
            },
            LayerEffect::ColorMatrix {
                matrix: [
                    0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
                    0.0, 0.0, 1.0, 0.0,
                ],
            },
        ];
        for (i, effect) in effects.into_iter().enumerate() {
            let mut batch = PrimitiveBatch::new();
            batch.push_layer_command(LayerCommand::Push {
                config: LayerConfig::new().opacity(1.0).effect(effect),
            });
            rect(&mut batch, [50.0, 40.0, 40.0, 40.0], [1.0, 0.0, 0.0, 1.0]);
            batch.push_layer_command(LayerCommand::Pop);
            let page = render(&harness, &batch, &format!("overlay_effect_{i}"), 4);
            let (x, y) = match i {
                0 => (100, 60),
                1 => (48, 60),
                _ => (60, 60),
            };
            assert!(page.get_pixel(x, y)[1] > 0, "effect {i} is visible");
            assert_eq!(page.get_pixel(5, 5).0, [0, 0, 255, 255]);
        }
        let mut batch = PrimitiveBatch::new();
        batch.push_layer_command(LayerCommand::Push {
            config: LayerConfig::new()
                .opacity(1.0)
                .blend_mode(BlendMode::Screen),
        });
        rect(&mut batch, [50.0, 40.0, 40.0, 40.0], [1.0, 0.0, 0.0, 1.0]);
        batch.push_layer_command(LayerCommand::Pop);
        let page = render(&harness, &batch, "overlay_screen", 4);
        assert_eq!(page.get_pixel(60, 60).0, [255, 0, 255, 255]);
    }

    #[test]
    #[ignore] // Requires GPU
    fn native_overlay_path_gradient_stays_in_its_local_frame() {
        use blinc_core::{Color, Gradient, Path, Point, Rect};
        let harness = harness();
        let mut batch = PrimitiveBatch::new();
        batch.push_layer_command(LayerCommand::Push {
            config: LayerConfig::new().opacity(0.5),
        });
        batch.push_path(blinc_gpu::path::tessellate_fill(
            &Path::rect(Rect::new(20.0, 30.0, 50.0, 40.0)),
            &blinc_core::Brush::Gradient(Gradient::linear(
                Point::new(20.0, 30.0),
                Point::new(70.0, 30.0),
                Color::RED,
                Color::BLUE,
            )),
        ));
        batch.push_layer_command(LayerCommand::Pop);
        let page = render(&harness, &batch, "overlay_path_gradient", 4);
        let (a, b) = (page.get_pixel(30, 50), page.get_pixel(60, 50));
        assert!(
            a[0] > 80 && a[0] > b[0] + 50,
            "local gradient: {a:?} -> {b:?}"
        );
        assert!(a[2] < b[2]);
    }
}
