//! Routed meshes and 3D groups retain their variable-length payloads.
#[cfg(test)]
#[path = "../../../blinc_gpu/tests/fixtures/aux_scene.rs"]
mod fixture;

#[cfg(test)]
mod tests {
    use super::fixture::{Route, scene};
    use crate::harness::{TestHarness, TestHarnessConfig};

    #[test]
    #[ignore] // Requires GPU
    fn an_ancestor_polygon_clips_dynamic_pixels() {
        use blinc_core::{ClipShape, Color, CornerRadius, DrawContext, Point, Rect};
        let harness = TestHarness::with_config(TestHarnessConfig {
            sample_count: 1,
            ..Default::default()
        })
        .unwrap();
        let mut ctx = blinc_gpu::GpuPaintContext::new(100.0, 100.0);
        ctx.push_clip(ClipShape::Polygon(vec![
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            Point::new(0.0, 100.0),
        ]));
        ctx.push_motion_subtree();
        ctx.fill_rect(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            CornerRadius::ZERO,
            Color::RED.into(),
        );
        ctx.pop_motion_subtree();
        let path = harness.output_path("aux_inherited_polygon");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        harness
            .render_to_png(&ctx.take_dynamic_batch(), 100, 100, &path)
            .unwrap();
        let page = image::open(path).unwrap().to_rgba8();
        assert_eq!(page.get_pixel(20, 20).0, [255, 0, 0, 255]);
        assert_eq!(page.get_pixel(80, 80).0, [255; 4]);
    }

    #[test]
    #[ignore] // Requires GPU
    fn routed_auxiliary_payloads_draw_like_the_static_scene() {
        let harness = TestHarness::with_config(TestHarnessConfig {
            sample_count: 1,
            ..Default::default()
        })
        .unwrap();
        let render = |route, foreground| {
            let (_, mut batch) = scene(route, foreground);
            batch.primitives.append(&mut batch.foreground_primitives);
            let path = harness.output_path(&format!("aux_{route:?}_{foreground}"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            harness.render_to_png(&batch, 200, 120, &path).unwrap();
            image::open(path).unwrap().to_rgba8()
        };
        let reference = render(Route::Static, false);
        assert_eq!(reference.get_pixel(20, 30).0, [255, 0, 0, 255]);
        assert_ne!(reference.get_pixel(90, 30).0, [255; 4]);
        assert!(reference.get_pixel(170, 54)[1] > reference.get_pixel(170, 54)[0]);
        assert_eq!(reference.get_pixel(25, 100).0, [0, 0, 255, 255]);
        for route in [Route::Dynamic, Route::Composite] {
            for foreground in [false, true] {
                let actual = render(route, foreground);
                let difference = actual
                    .pixels()
                    .zip(reference.pixels())
                    .enumerate()
                    .find(|(_, (a, b))| a != b);
                assert!(
                    difference.is_none(),
                    "{route:?}, foreground={foreground}: {difference:?}"
                );
            }
        }
    }
}
