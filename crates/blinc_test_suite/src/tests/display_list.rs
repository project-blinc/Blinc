//! Records from `blinc_abi`, drawn by the SDF pipelines, against the same
//! scene drawn by Blinc's own paint walk.
//!
//! The fixtures are shared with `blinc_gpu`'s decoder tests. Each scene is a
//! 200 by 120 page with 20px of padding holding one 100 by 60 box.

#[cfg(test)]
mod tests {
    use crate::harness::TestHarness;
    use blinc_core::{Affine2D, Brush, Color, DrawContext, Gradient, Point, Shadow, Transform};
    use blinc_gpu::PrimitiveBatch;
    use blinc_gpu::display_list::{DISPLAY_LIST_VERSION, RECORD_FLOATS, decode};
    use blinc_layout::prelude::*;
    use image::RgbaImage;

    const W: f32 = 200.0;
    const H: f32 = 120.0;

    macro_rules! fixture {
        ($name:literal) => {
            floats(include_bytes!(concat!(
                "../../../blinc_gpu/tests/fixtures/display_list/",
                $name,
                ".f32"
            )))
        };
    }

    fn floats(bytes: &[u8]) -> Vec<f32> {
        bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect()
    }

    fn page(child: Div) -> Div {
        div()
            .w(W)
            .h(H)
            .p_px(20.0)
            .flex_row()
            .items_start()
            .child(child)
    }

    fn rgb(r: f32, g: f32, b: f32) -> Color {
        Color::rgb(r, g, b)
    }

    fn draw(harness: &TestHarness, name: &str, batch: &PrimitiveBatch, scale: f32) -> RgbaImage {
        let path = harness.output_path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        harness
            .render_to_png(batch, (W * scale) as u32, (H * scale) as u32, &path)
            .unwrap();
        image::open(&path).unwrap().to_rgba8()
    }

    fn native(harness: &TestHarness, name: &str, ui: &Div, scale: f32) -> RgbaImage {
        let mut ctx = harness.create_context_with_size(name, W * scale, H * scale);
        let mut tree = RenderTree::from_element(ui);
        tree.compute_layout(W, H);
        ctx.ctx().push_transform(Transform::scale(scale, scale));
        tree.render(ctx.ctx());
        draw(
            harness,
            &format!("{name}_native_x{scale}"),
            &ctx.take_batch(),
            scale,
        )
    }

    fn from_records(harness: &TestHarness, name: &str, list: &[f32], scale: f32) -> RgbaImage {
        let count = list.len() / RECORD_FLOATS;
        let out = decode(list, count, DISPLAY_LIST_VERSION, scale).unwrap();
        assert!(out.unsupported.is_empty(), "{name}: {:?}", out.unsupported);
        let mut batch = PrimitiveBatch::new();
        batch.primitives.extend(out.primitives);
        draw(harness, &format!("{name}_records_x{scale}"), &batch, scale)
    }

    /// How many pixels differ by more than `slack` in some channel, and the
    /// largest difference in any.
    fn compare(a: &RgbaImage, b: &RgbaImage, slack: u8) -> (usize, u8) {
        assert_eq!(a.dimensions(), b.dimensions());
        let (mut over, mut worst) = (0, 0u8);
        for (p, q) in a.pixels().zip(b.pixels()) {
            let d = (0..4).map(|i| p[i].abs_diff(q[i])).max().unwrap();
            worst = worst.max(d);
            if d > slack {
                over += 1;
            }
        }
        (over, worst)
    }

    #[test]
    #[ignore] // Requires GPU
    fn records_draw_as_blinc_draws_them() {
        let harness = TestHarness::new().unwrap();
        let scenes: Vec<(&str, Vec<f32>, Div)> =
            vec![
                (
                    "box_solid",
                    fixture!("box_solid"),
                    page(div().w(100.0).h(60.0).bg(rgb(0.2, 0.4, 0.9)).rounded(8.0)),
                ),
                (
                    "box_border",
                    fixture!("box_border"),
                    page(
                        div()
                            .w(100.0)
                            .h(60.0)
                            .bg(rgb(0.2, 0.4, 0.9))
                            .rounded(12.0)
                            .border(3.0, rgb(0.9, 0.1, 0.1)),
                    ),
                ),
                (
                    "box_shadow",
                    fixture!("box_shadow"),
                    page(
                        div()
                            .w(100.0)
                            .h(60.0)
                            .bg(rgb(1.0, 1.0, 1.0))
                            .rounded(10.0)
                            .shadow(Shadow {
                                offset_x: 4.0,
                                offset_y: 6.0,
                                blur: 12.0,
                                spread: 2.0,
                                color: Color::rgba(0.0, 0.0, 0.0, 0.4),
                            }),
                    ),
                ),
                (
                    "box_rotated",
                    fixture!("box_rotated"),
                    page(
                        div()
                            .w(100.0)
                            .h(60.0)
                            .bg(rgb(0.2, 0.7, 0.4))
                            .rounded(6.0)
                            .transform(Transform::rotate(0.5)),
                    ),
                ),
                (
                    "box_scaled",
                    fixture!("box_scaled"),
                    page(div().w(100.0).h(60.0).bg(rgb(0.7, 0.3, 0.6)).transform(
                        Transform::Affine2D(Affine2D {
                            elements: [1.5, 0.0, 0.0, 0.75, 0.0, 0.0],
                        }),
                    )),
                ),
                (
                    "box_gradient",
                    fixture!("box_gradient"),
                    page(
                        div()
                            .w(100.0)
                            .h(60.0)
                            .rounded(8.0)
                            .background(Brush::Gradient(Gradient::linear(
                                Point::new(0.0, 0.0),
                                Point::new(100.0, 0.0),
                                rgb(1.0, 0.2, 0.2),
                                rgb(0.2, 0.2, 1.0),
                            ))),
                    ),
                ),
                (
                    "box_radial",
                    fixture!("box_radial"),
                    page(
                        div()
                            .w(100.0)
                            .h(60.0)
                            .background(Brush::Gradient(Gradient::radial(
                                Point::new(50.0, 30.0),
                                40.0,
                                rgb(1.0, 1.0, 0.2),
                                rgb(0.1, 0.1, 0.5),
                            ))),
                    ),
                ),
                (
                    "clip_child",
                    fixture!("clip_child"),
                    page(
                        div()
                            .w(100.0)
                            .h(60.0)
                            .bg(rgb(0.9, 0.9, 0.9))
                            .rounded(16.0)
                            .overflow_clip()
                            .child(
                                div()
                                    .absolute()
                                    .left(-10.0)
                                    .top(-10.0)
                                    .w(60.0)
                                    .h(40.0)
                                    .bg(rgb(0.9, 0.2, 0.2)),
                            ),
                    ),
                ),
            ];

        let mut report = Vec::new();
        for (name, list, ui) in &scenes {
            for scale in [1.0, 2.0] {
                let records = from_records(&harness, name, list, scale);
                let blinc = native(&harness, name, ui, scale);
                let (over, worst) = compare(&records, &blinc, 8);
                println!("{name} x{scale}: {over} pixels differ by more than 8, worst {worst}");
                if over > 0 {
                    report.push(format!("{name} x{scale}: {over} pixels, worst {worst}"));
                }
            }
        }
        assert!(
            report.is_empty(),
            "records differ from the paint walk:\n{}",
            report.join("\n")
        );
    }
}
