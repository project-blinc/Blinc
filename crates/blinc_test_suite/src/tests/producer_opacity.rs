//! Pixel coverage for opacity emitted by GpuPaintContext.
#[cfg(test)]
mod tests {
    use crate::harness::{TestHarness, TestHarnessConfig};
    use blinc_core::{Color, DrawContext, LayerConfig, Path, Rect};
    use blinc_gpu::GpuPaintContext;
    use image::RgbaImage;

    fn harness() -> TestHarness {
        TestHarness::with_config(TestHarnessConfig {
            sample_count: 1,
            ..Default::default()
        })
        .unwrap()
    }

    fn pixel(page: &RgbaImage, xy: (u32, u32), expected: [u8; 4]) {
        let actual = page.get_pixel(xy.0, xy.1).0;
        assert!(
            actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
            "at {xy:?}: {actual:?}, expected {expected:?}"
        );
    }

    #[test]
    #[ignore] // Requires GPU
    fn producer_group_opacity_composites_overlapping_children_once() {
        let harness = harness();
        for use_paths in [false, true] {
            let mut ctx = GpuPaintContext::new(160.0, 120.0);
            ctx.push_layer(LayerConfig::new().opacity(0.5));
            for (rect, color) in [
                (Rect::new(20.0, 20.0, 60.0, 60.0), Color::RED),
                (Rect::new(40.0, 40.0, 60.0, 60.0), Color::GREEN),
            ] {
                if use_paths {
                    ctx.fill_path(&Path::rect(rect), color.into());
                } else {
                    ctx.fill_rect(rect, 0.0.into(), color.into());
                }
            }
            ctx.pop_layer();
            let batch = ctx.take_batch();
            // Both public full-batch routes must agree at opaque interior samples.
            for route in [0, 1, 4, 5] {
                let path = harness.output_path(&format!("producer_group_{use_paths}_{route}"));
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                harness
                    .render_with(160, 120, &path, |renderer, target| {
                        if route == 0 {
                            renderer.render_with_clear(target, &batch, [0.0, 0.0, 1.0, 1.0]);
                        } else {
                            renderer.render_with_clear(
                                target,
                                &Default::default(),
                                [0.0, 0.0, 1.0, 1.0],
                            );
                            if route == 5 {
                                renderer.render_overlay(target, &batch);
                            } else {
                                renderer.render_overlay_msaa(target, &batch, route);
                            }
                        }
                    })
                    .unwrap();
                let page = image::open(path).unwrap().to_rgba8();
                pixel(&page, (30, 30), [128, 0, 127, 255]);
                pixel(&page, (55, 55), [0, 128, 127, 255]);
                pixel(&page, (90, 90), [0, 128, 127, 255]);
                pixel(&page, (5, 5), [0, 0, 255, 255]);
            }
        }
    }

    #[test]
    #[ignore] // Requires GPU
    fn producer_nested_groups_keep_explicit_opacity_and_restore_after_zero() {
        let harness = harness();
        let mut ctx = GpuPaintContext::new(160.0, 120.0);
        ctx.push_layer(LayerConfig::new().opacity(0.5));
        ctx.fill_rect(
            Rect::new(20.0, 20.0, 60.0, 60.0),
            0.0.into(),
            Color::RED.into(),
        );
        ctx.push_layer(LayerConfig::new().opacity(0.5));
        ctx.push_opacity(0.5);
        ctx.fill_path(
            &Path::rect(Rect::new(40.0, 40.0, 20.0, 20.0)),
            Color::GREEN.into(),
        );
        ctx.pop_opacity();
        ctx.pop_layer();
        ctx.pop_layer();
        ctx.push_layer(LayerConfig::new().opacity(0.0));
        ctx.fill_rect(
            Rect::new(90.0, 20.0, 20.0, 20.0),
            0.0.into(),
            Color::RED.into(),
        );
        ctx.pop_layer();
        ctx.fill_rect(
            Rect::new(120.0, 20.0, 20.0, 20.0),
            0.0.into(),
            Color::GREEN.into(),
        );
        let batch = ctx.take_batch();
        for samples in [1, 4] {
            let path = harness.output_path(&format!("producer_nested_{samples}"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            harness
                .render_with(160, 120, &path, |renderer, target| {
                    renderer.render_with_clear(target, &Default::default(), [0.0, 0.0, 1.0, 1.0]);
                    renderer.render_overlay_msaa(target, &batch, samples);
                })
                .unwrap();
            let page = image::open(path).unwrap().to_rgba8();
            pixel(&page, (30, 30), [128, 0, 127, 255]);
            pixel(&page, (50, 50), [96, 32, 127, 255]);
            pixel(&page, (100, 30), [0, 0, 255, 255]);
            pixel(&page, (130, 30), [0, 255, 0, 255]);
        }
    }
}
