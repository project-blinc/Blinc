//! Records from `blinc_abi`, drawn by the SDF pipelines, against the same
//! scene drawn by Blinc's own paint walk.
//!
//! The fixtures are shared with `blinc_gpu`'s decoder tests. Each scene is a
//! 200 by 120 page with 20px of padding holding one 100 by 60 box.

#[cfg(test)]
mod tests {
    use crate::harness::TestHarness;
    use blinc_core::{Affine2D, Brush, Color, DrawContext, Gradient, Point, Shadow, Transform};
    use blinc_gpu::GpuImage;
    use blinc_gpu::PrimitiveBatch;
    use blinc_gpu::display_list::Unsupported;
    use blinc_gpu::display_list::atlas::{AtlasError, AtlasTexture, AtlasUpdate};
    use blinc_gpu::display_list::plan::{plan, render};
    use blinc_gpu::display_list::{DISPLAY_LIST_VERSION, RECORD_FLOATS, decode};
    use blinc_gpu::primitives::{GpuPrimitive, PrimitiveType};
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

    /// The pixel at logical `(x, y)`, as 0 to 255 channels.
    fn at(img: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
        img.get_pixel(x, y).0
    }

    fn assert_color(img: &RgbaImage, x: u32, y: u32, want: [f32; 3], what: &str) {
        let got = at(img, x, y);
        for (c, w) in want.iter().enumerate() {
            let want = (w * 255.0).round() as i32;
            assert!(
                (got[c] as i32 - want).abs() <= 6,
                "{what}: pixel ({x}, {y}) is {got:?}, wanted {want:?} in channel {c}"
            );
        }
    }

    #[test]
    #[ignore] // Requires GPU
    fn per_side_border_colours_meet_on_the_diagonal() {
        let harness = TestHarness::new().unwrap();
        let img = from_records(&harness, "box_sides", &fixture!("box_sides"), 1.0);
        // The box is at (20, 20), 100 by 60, with sides of 4, 2, 6 and 3.
        let (red, green, blue, yellow) = (
            [0.9, 0.1, 0.1],
            [0.1, 0.7, 0.1],
            [0.1, 0.1, 0.9],
            [0.9, 0.7, 0.1],
        );
        assert_color(&img, 70, 21, red, "top edge");
        assert_color(&img, 118, 50, green, "right edge");
        assert_color(&img, 70, 78, blue, "bottom edge");
        assert_color(&img, 21, 50, yellow, "left edge");
        assert_color(&img, 70, 50, [0.95, 0.95, 0.95], "inside");
        // Along the top-left corner the nearer side, in widths, wins.
        assert_color(&img, 21, 21, red, "corner, above the diagonal");
        assert_color(&img, 21, 23, yellow, "corner, below the diagonal");
    }

    /// A fixture's mask atlas: `[width, height, x, y, w, h]` as u32, then the
    /// bytes of the used rectangle. Returned as the whole atlas, zero elsewhere.
    fn atlas_fixture(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
        let head: Vec<u32> = bytes[..24]
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        let (width, height, x, y, w, h) = (head[0], head[1], head[2], head[3], head[4], head[5]);
        let mut atlas = vec![0u8; (width * height) as usize];
        for row in 0..h {
            let from = 24 + (row * w) as usize;
            let to = ((y + row) * width + x) as usize;
            atlas[to..to + w as usize].copy_from_slice(&bytes[from..from + w as usize]);
        }
        (width, height, atlas)
    }

    /// The page the glyph records draw, worked out on the CPU: each glyph's
    /// atlas rect laid over the page at its quad, coverage through the same
    /// 0.7 power the shader applies, blended in paint order over white.
    fn glyph_reference(list: &[f32], atlas: &[u8], atlas_width: u32, scale: f32) -> RgbaImage {
        let mut page = RgbaImage::from_pixel(
            (W * scale) as u32,
            (H * scale) as u32,
            image::Rgba([255, 255, 255, 255]),
        );
        for record in list.chunks_exact(RECORD_FLOATS) {
            let (quad, uv, color) = (&record[0..4], &record[40..44], &record[8..12]);
            let (x, y) = (
                (quad[0] * scale).round() as i32,
                (quad[1] * scale).round() as i32,
            );
            let (w, h) = (
                (quad[2] * scale).round() as i32,
                (quad[3] * scale).round() as i32,
            );
            assert_eq!(
                ((uv[2] - uv[0]) as i32, (uv[3] - uv[1]) as i32),
                (w, h),
                "1:1"
            );
            for dy in 0..h {
                for dx in 0..w {
                    let texel = (uv[1] as i32 + dy) as usize * atlas_width as usize
                        + (uv[0] as i32 + dx) as usize;
                    let alpha = color[3] * (atlas[texel] as f32 / 255.0).powf(0.7);
                    let pixel = page.get_pixel_mut((x + dx) as u32, (y + dy) as u32);
                    for c in 0..3 {
                        let blended = pixel[c] as f32 * (1.0 - alpha) + color[c] * 255.0 * alpha;
                        pixel[c] = blended.round() as u8;
                    }
                }
            }
        }
        page
    }

    #[test]
    #[ignore] // Requires GPU
    fn glyph_records_draw_from_the_producers_atlas() {
        let harness = TestHarness::new().unwrap();
        let (device, queue) = harness.gpu();
        for (name, scale) in [("text_hello_x1", 1.0f32), ("text_hello_x2", 2.0)] {
            let list = match name {
                "text_hello_x1" => fixture!("text_hello_x1"),
                _ => fixture!("text_hello_x2"),
            };
            let bytes: &[u8] = match name {
                "text_hello_x1" => include_bytes!(
                    "../../../blinc_gpu/tests/fixtures/display_list/text_hello_x1.atlas"
                ),
                _ => include_bytes!(
                    "../../../blinc_gpu/tests/fixtures/display_list/text_hello_x2.atlas"
                ),
            };
            let (width, height, atlas) = atlas_fixture(bytes);

            let mut mask = AtlasTexture::new(device, false);
            let color = AtlasTexture::new(device, true);
            mask.update(
                device,
                queue,
                &AtlasUpdate {
                    width,
                    height,
                    x: 0,
                    y: 0,
                    update_width: width,
                    update_height: height,
                    bytes: &atlas,
                },
            )
            .unwrap();
            harness.set_glyph_atlas(mask.view(), color.view());

            let drawn = from_records(&harness, name, &list, scale);
            let expected = glyph_reference(&list, &atlas, width, scale);
            let (over, worst) = compare(&drawn, &expected, 3);
            println!("{name}: {over} pixels differ by more than 3, worst {worst}");
            assert_eq!(over, 0, "{name}: {over} pixels, worst {worst}");
        }
    }

    #[test]
    #[ignore] // Requires GPU
    fn an_atlas_update_writes_its_rectangle_and_a_resize_replaces_it() {
        fn full(width: u32, height: u32, bytes: &[u8]) -> AtlasUpdate<'_> {
            AtlasUpdate {
                width,
                height,
                x: 0,
                y: 0,
                update_width: width,
                update_height: height,
                bytes,
            }
        }
        let harness = TestHarness::new().unwrap();
        let (device, queue) = harness.gpu();
        let mut mask = AtlasTexture::new(device, false);
        let color = AtlasTexture::new(device, true);
        mask.update(device, queue, &full(4, 4, &[0; 16])).unwrap();
        assert_eq!(mask.size(), (4, 4));

        // A patch of 2 by 2 at (1, 1).
        let patch = AtlasUpdate {
            width: 4,
            height: 4,
            x: 1,
            y: 1,
            update_width: 2,
            update_height: 2,
            bytes: &[255; 4],
        };
        mask.update(device, queue, &patch).unwrap();

        // The whole atlas drawn over 8 by 8 pixels: the patch is the middle, a quarter of it.
        harness.set_glyph_atlas(mask.view(), color.view());
        let mut batch = PrimitiveBatch::new();
        batch.primitives.push(GpuPrimitive {
            bounds: [10.0, 10.0, 8.0, 8.0],
            gradient_params: [0.0, 0.0, 4.0, 4.0],
            color: [0.0, 0.0, 0.0, 1.0],
            type_info: [PrimitiveType::Text as u32, 0, 0, 0],
            ..GpuPrimitive::default()
        });
        let img = draw(&harness, "atlas_patch", &batch, 1.0);
        assert!(at(&img, 14, 14)[0] < 10, "the patch is drawn");
        assert!(at(&img, 10, 10)[0] > 245, "the rest of the atlas is empty");
        assert!(at(&img, 17, 17)[0] > 245, "the rest of the atlas is empty");

        // A different size is accepted only whole, a patch only inside its atlas, and
        // only with the bytes to fill it.
        assert_eq!(
            mask.update(
                device,
                queue,
                &AtlasUpdate {
                    width: 8,
                    height: 8,
                    ..patch
                }
            ),
            Err(AtlasError::NeedsFullUpload)
        );
        assert_eq!(
            mask.update(device, queue, &AtlasUpdate { x: 3, ..patch }),
            Err(AtlasError::BadRect)
        );
        assert_eq!(
            mask.update(
                device,
                queue,
                &AtlasUpdate {
                    bytes: &[255; 3],
                    ..patch
                }
            ),
            Err(AtlasError::BadRect)
        );
        assert_eq!(mask.size(), (4, 4), "a refused update changes nothing");
        mask.update(device, queue, &full(8, 8, &[0; 64])).unwrap();
        assert_eq!(mask.size(), (8, 8));
    }

    /// A 40 by 30 image, red on the left half and green on the right.
    fn test_image(harness: &TestHarness) -> GpuImage {
        let (device, queue) = harness.gpu();
        let mut pixels = Vec::new();
        for _y in 0..30 {
            for x in 0..40 {
                pixels.extend_from_slice(if x < 20 {
                    &[255, 0, 0, 255]
                } else {
                    &[0, 200, 0, 255]
                });
            }
        }
        GpuImage::from_rgba(
            device,
            queue,
            &pixels,
            40,
            30,
            Some("display list test image"),
        )
    }

    /// Draws a plan of `list` through the ordered executor, the image in slot 7,
    /// and returns the page and the slots that had no image.
    fn draw_plan(
        harness: &TestHarness,
        name: &str,
        list: &[f32],
        image: Option<&GpuImage>,
        allowed: &[Unsupported],
    ) -> (RgbaImage, Vec<u32>) {
        let planned = plan(list, list.len() / RECORD_FLOATS, DISPLAY_LIST_VERSION, 1.0).unwrap();
        for (_, why) in &planned.unsupported {
            assert!(allowed.contains(why), "{why:?} is not allowed");
        }
        let path = harness.output_path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut missing = Vec::new();
        harness
            .render_with(W as u32, H as u32, &path, |renderer, view| {
                missing = render(
                    renderer,
                    view,
                    (W as u32, H as u32),
                    &planned,
                    [1.0; 4],
                    &|slot| image.filter(|_| slot == 7),
                );
            })
            .unwrap();
        (image::open(&path).unwrap().to_rgba8(), missing)
    }

    fn within(got: [u8; 4], want: [u8; 3], slack: i32) -> bool {
        (0..3).all(|c| (got[c] as i32 - want[c] as i32).abs() <= slack)
    }

    #[test]
    #[ignore] // Requires GPU
    fn an_image_draws_between_the_primitives_around_it() {
        let harness = TestHarness::new().unwrap();
        let (device, queue) = harness.gpu();

        // Text is drawn from the producer's atlas, over the image.
        let (width, height, atlas) = atlas_fixture(include_bytes!(
            "../../../blinc_gpu/tests/fixtures/display_list/text_hello_x1.atlas"
        ));
        let mut mask = AtlasTexture::new(device, false);
        let color = AtlasTexture::new(device, true);
        mask.update(
            device,
            queue,
            &AtlasUpdate {
                width,
                height,
                x: 0,
                y: 0,
                update_width: width,
                update_height: height,
                bytes: &atlas,
            },
        )
        .unwrap();
        harness.set_glyph_atlas(mask.view(), color.view());

        let image = test_image(&harness);
        let list = [
            fixture!("box_solid"),
            fixture!("image_plain"),
            fixture!("text_hello_x1"),
        ]
        .concat();
        let (page, missing) = draw_plan(&harness, "image_between", &list, Some(&image), &[]);
        assert!(missing.is_empty());

        // Below the text, the image is drawn 1:1 over the box: red then green.
        for x in 20..40 {
            assert!(within(at(&page, x, 45), [255, 0, 0], 2), "red at {x}");
        }
        for x in 40..60 {
            assert!(within(at(&page, x, 45), [0, 200, 0], 2), "green at {x}");
        }
        // Past the image, the box under it; past the box, the page.
        assert!(within(at(&page, 61, 45), [51, 102, 229], 2), "the box");
        assert!(within(at(&page, 150, 100), [255, 255, 255], 2), "the page");
        // The glyphs come after the image, so they are dark over it.
        let dark = (20..64)
            .flat_map(|x| (25..40).map(move |y| (x, y)))
            .filter(|&(x, y)| at(&page, x, y)[..3].iter().all(|c| *c < 100))
            .count();
        assert!(dark > 25, "the text is over the image: {dark} dark pixels");
    }

    #[test]
    #[ignore] // Requires GPU
    fn an_images_opacity_clip_and_missing_slot_are_honoured() {
        let harness = TestHarness::new().unwrap();
        let image = test_image(&harness);

        let (page, _) = draw_plan(
            &harness,
            "image_faded",
            &fixture!("image_faded"),
            Some(&image),
            &[],
        );
        // Half of red over white.
        assert!(
            within(at(&page, 25, 45), [255, 128, 128], 3),
            "{:?}",
            at(&page, 25, 45)
        );

        // 100 by 60, rounded by 16: the corner is clipped away, the middle is the image.
        let (page, _) = draw_plan(
            &harness,
            "image_clipped",
            &fixture!("image_clipped"),
            Some(&image),
            &[],
        );
        assert!(
            within(at(&page, 21, 21), [255, 255, 255], 2),
            "the corner is clipped"
        );
        assert!(within(at(&page, 36, 21), [255, 0, 0], 3), "inside the arc");
        assert!(within(at(&page, 30, 50), [255, 0, 0], 3), "the middle");

        let (page, missing) = draw_plan(
            &harness,
            "image_missing",
            &fixture!("image_plain"),
            None,
            &[],
        );
        assert_eq!(missing, vec![7]);
        assert!(
            within(at(&page, 30, 30), [255, 255, 255], 0),
            "nothing is drawn"
        );
    }

    #[test]
    #[ignore] // Requires GPU
    fn a_layer_fades_its_group_as_one_and_an_effect_is_drawn_flat() {
        let harness = TestHarness::new().unwrap();

        // A red box and a green one over part of it, in a group at half opacity.
        let (page, missing) = draw_plan(
            &harness,
            "layer_opacity",
            &fixture!("layer_opacity"),
            None,
            &[],
        );
        assert!(missing.is_empty());
        assert!(
            within(at(&page, 30, 30), [242, 140, 140], 3),
            "red alone: {:?}",
            at(&page, 30, 30)
        );
        assert!(
            within(at(&page, 100, 50), [140, 230, 140], 3),
            "green alone: {:?}",
            at(&page, 100, 50)
        );
        // Where they overlap the group shows its top, green, faded: not the red under it.
        assert!(
            within(at(&page, 60, 50), [140, 230, 140], 3),
            "overlap: {:?}",
            at(&page, 60, 50)
        );
        assert!(within(at(&page, 150, 100), [255, 255, 255], 1), "outside");

        // A blur is not drawn yet, so its content is drawn as it is: red, sharp-edged.
        let (page, _) = draw_plan(
            &harness,
            "layer_blur",
            &fixture!("layer_blur"),
            None,
            &[Unsupported::LayerEffect],
        );
        assert!(
            within(at(&page, 40, 35), [230, 26, 26], 3),
            "{:?}",
            at(&page, 40, 35)
        );
        assert!(
            within(at(&page, 19, 35), [255, 255, 255], 1),
            "the edge is sharp"
        );
    }
}
