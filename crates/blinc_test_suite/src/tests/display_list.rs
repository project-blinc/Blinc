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
                    let px = (x + dx) as f32 + 0.5;
                    let py = (y + dy) as f32 + 0.5;
                    let fade = fade_alpha(
                        px / scale,
                        py / scale,
                        record[80..84].try_into().unwrap(),
                        record[84..88].try_into().unwrap(),
                    );
                    let alpha = color[3] * (atlas[texel] as f32 / 255.0).powf(0.7) * fade;
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
        for (name, scale) in [
            ("text_hello_x1", 1.0f32),
            ("text_hello_x2", 2.0),
            ("fade_text_x1", 1.0),
            ("fade_text_x2", 2.0),
        ] {
            let list = match name {
                "text_hello_x1" => fixture!("text_hello_x1"),
                "text_hello_x2" => fixture!("text_hello_x2"),
                "fade_text_x1" => fixture!("fade_text_x1"),
                _ => fixture!("fade_text_x2"),
            };
            let bytes: &[u8] = match name {
                "text_hello_x1" => include_bytes!(
                    "../../../blinc_gpu/tests/fixtures/display_list/text_hello_x1.atlas"
                ),
                "text_hello_x2" => include_bytes!(
                    "../../../blinc_gpu/tests/fixtures/display_list/text_hello_x2.atlas"
                ),
                "fade_text_x1" => include_bytes!(
                    "../../../blinc_gpu/tests/fixtures/display_list/fade_text_x1.atlas"
                ),
                _ => include_bytes!(
                    "../../../blinc_gpu/tests/fixtures/display_list/fade_text_x2.atlas"
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
        draw_records(
            harness,
            name,
            list,
            list.len() / RECORD_FLOATS,
            image,
            allowed,
        )
    }

    /// As `draw_plan`, for a list with `count` records and data after them.
    fn draw_records(
        harness: &TestHarness,
        name: &str,
        list: &[f32],
        count: usize,
        image: Option<&GpuImage>,
        allowed: &[Unsupported],
    ) -> (RgbaImage, Vec<u32>) {
        draw_records_at(harness, name, list, count, 1.0, image, allowed)
    }

    /// As `draw_records`, with the page `scale` times as large in device pixels.
    #[allow(clippy::too_many_arguments)]
    fn draw_records_at(
        harness: &TestHarness,
        name: &str,
        list: &[f32],
        count: usize,
        scale: f32,
        image: Option<&GpuImage>,
        allowed: &[Unsupported],
    ) -> (RgbaImage, Vec<u32>) {
        let planned = plan(list, count, DISPLAY_LIST_VERSION, scale).unwrap();
        for (_, why) in &planned.unsupported {
            assert!(allowed.contains(why), "{why:?} is not allowed");
        }
        let path = harness.output_path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut missing = Vec::new();
        harness
            .render_with(
                (W * scale) as u32,
                (H * scale) as u32,
                &path,
                |renderer, view| {
                    if name == "fade_plain_x1" {
                        println!("fade adapter: {:?}", renderer.adapter().get_info());
                    }
                    missing = render(
                        renderer,
                        view,
                        ((W * scale) as u32, (H * scale) as u32),
                        &planned,
                        [1.0; 4],
                        &|slot| image.filter(|_| slot == 7),
                    );
                },
            )
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
    fn a_layer_fades_its_group_as_one() {
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
    }

    /// The weights the effects shader takes: offsets in pairs from `-reach`, each
    /// pair's fetch standing for the two texels it lies between, then normalised.
    fn gaussian_weights(sigma: f32) -> Vec<(i32, f32)> {
        let reach = (sigma * 3.0).ceil() as i32;
        let w = |i: i32| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp();
        let mut taps = Vec::new();
        let mut i = -reach;
        while i <= reach {
            taps.push((i, w(i)));
            taps.push((i + 1, w(i + 1)));
            i += 2;
        }
        let total: f32 = taps.iter().map(|t| t.1).sum();
        taps.iter().map(|&(o, w)| (o, w / total)).collect()
    }

    /// An indicator of `lo..hi` over `len` texels, blurred, with edges clamped.
    fn blur_span(len: usize, lo: usize, hi: usize, sigma: f32) -> Vec<f32> {
        let taps = gaussian_weights(sigma);
        (0..len as i32)
            .map(|x| {
                taps.iter()
                    .map(|&(o, w)| {
                        let at = (x + o).clamp(0, len as i32 - 1) as usize;
                        w * if (lo..hi).contains(&at) { 1.0 } else { 0.0 }
                    })
                    .sum()
            })
            .collect()
    }

    fn assert_page(got: &RgbaImage, want: impl Fn(u32, u32) -> [f32; 3], slack: i32, what: &str) {
        let mut worst = 0;
        let mut at_worst = (0, 0);
        for (x, y, p) in got.enumerate_pixels() {
            let w = want(x, y);
            for c in 0..3 {
                let d = (p[c] as i32 - w[c].round() as i32).abs();
                if d > worst {
                    worst = d;
                    at_worst = (x, y);
                }
            }
        }
        println!("{what}: worst {worst} at {at_worst:?}");
        assert!(worst <= slack, "{what}: off by {worst} at {at_worst:?}");
    }

    #[test]
    #[ignore] // Requires GPU
    fn a_layers_blur_is_the_exact_gaussian() {
        let harness = TestHarness::new().unwrap();
        let (page, _) = draw_plan(&harness, "layer_blur", &fixture!("layer_blur"), None, &[]);
        // A red box of 40 by 30 at (20, 20), blurred with a deviation of 4: its
        // alpha is the product of a blurred span in each direction.
        let (w, h) = (W as usize, H as usize);
        let (hx, vy) = (blur_span(w, 20, 60, 4.0), blur_span(h, 20, 50, 4.0));
        assert_page(
            &page,
            |x, y| {
                let a = hx[x as usize] * vy[y as usize];
                [
                    255.0 * (1.0 - a) + 229.5 * a,
                    255.0 * (1.0 - a) + 25.5 * a,
                    255.0 * (1.0 - a) + 25.5 * a,
                ]
            },
            4,
            "blur",
        );
        // It really is soft: the box's own edge is half covered.
        assert!(at(&page, 20, 35)[1] > 100 && at(&page, 20, 35)[1] < 200);
    }

    #[test]
    #[ignore] // Requires GPU
    fn a_layers_colour_filter_maps_straight_colour() {
        let harness = TestHarness::new().unwrap();
        let (page, _) = draw_plan(
            &harness,
            "layer_grayscale",
            &fixture!("layer_grayscale"),
            None,
            &[],
        );
        let grey = 0.2126 * 229.5 + 0.7152 * 25.5 + 0.0722 * 25.5;
        assert_page(
            &page,
            |x, y| {
                if (20..60).contains(&x) && (20..50).contains(&y) {
                    [grey; 3]
                } else {
                    [255.0; 3]
                }
            },
            3,
            "grayscale",
        );
    }

    #[test]
    #[ignore] // Requires GPU
    fn a_layers_drop_shadow_falls_blurred_behind_it() {
        let harness = TestHarness::new().unwrap();
        let (page, _) = draw_plan(
            &harness,
            "layer_shadow",
            &fixture!("layer_shadow"),
            None,
            &[],
        );
        // The box is sharp and red; its alpha, blurred by 4 and moved by (6, 5), is a
        // black shadow of 0.6 under it.
        let (w, h) = (W as usize, H as usize);
        let (hx, vy) = (blur_span(w, 20, 60, 4.0), blur_span(h, 20, 50, 4.0));
        assert_page(
            &page,
            |x, y| {
                if (20..60).contains(&x) && (20..50).contains(&y) {
                    return [229.5, 25.5, 25.5];
                }
                let (sx, sy) = (x as i32 - 6, y as i32 - 5);
                let s = if sx >= 0 && sy >= 0 {
                    hx[sx as usize] * vy[sy as usize]
                } else {
                    0.0
                };
                [255.0 * (1.0 - 0.6 * s); 3]
            },
            4,
            "shadow",
        );
        assert!(
            at(&page, 64, 52)[0] < 200,
            "the shadow is below and to the right"
        );
    }

    /// `layer_blur` with its blur set to `sigma`, and its bounds, as the producer
    /// draws them, the box widened by three deviations each way.
    fn blurred_by(sigma: f32) -> Vec<f32> {
        let mut list = fixture!("layer_blur");
        let record = 2 * RECORD_FLOATS;
        list[record + 2 * 4] = sigma;
        let spread = (sigma * 3.0).ceil();
        list[record..record + 4].copy_from_slice(&[
            20.0 - spread,
            20.0 - spread,
            40.0 + 2.0 * spread,
            30.0 + 2.0 * spread,
        ]);
        list
    }

    #[test]
    #[ignore] // Requires GPU
    fn a_wide_blur_taken_at_reduced_size_stays_close_to_the_exact_one() {
        let harness = TestHarness::new().unwrap();
        let (w, h) = (W as usize, H as usize);
        for (sigma, slack) in [(8.0f32, 4), (12.0, 6), (16.0, 6), (32.0, 6)] {
            let name = format!("layer_blur_{sigma}");
            let (page, _) = draw_plan(&harness, &name, &blurred_by(sigma), None, &[]);
            let (hx, vy) = (blur_span(w, 20, 60, sigma), blur_span(h, 20, 50, sigma));
            assert_page(
                &page,
                |x, y| {
                    let a = hx[x as usize] * vy[y as usize];
                    [
                        255.0 * (1.0 - a) + 229.5 * a,
                        255.0 * (1.0 - a) + 25.5 * a,
                        255.0 * (1.0 - a) + 25.5 * a,
                    ]
                },
                slack,
                &format!("blur {sigma}"),
            );
        }
    }

    #[test]
    #[ignore] // Requires GPU
    fn a_layer_changes_nothing_outside_its_bounds() {
        let harness = TestHarness::new().unwrap();
        // A blur of 4 whose bounds are the box alone, with none of the reach the
        // producer would give it: nothing outside the bounds is changed.
        let mut list = blurred_by(4.0);
        let record = 2 * RECORD_FLOATS;
        list[record..record + 4].copy_from_slice(&[20.0, 20.0, 40.0, 30.0]);
        let (page, _) = draw_plan(&harness, "layer_cut", &list, None, &[]);
        assert!(within(at(&page, 15, 35), [255, 255, 255], 0), "left of it");
        assert!(within(at(&page, 40, 55), [255, 255, 255], 0), "below it");
        // Inside, the box is there: the blur is cut off at the bounds, and the content
        // texture's edge texels repeat past it, so with no margin the edge stays sharp.
        assert!(within(at(&page, 40, 35), [230, 26, 26], 3), "inside");
    }

    /// `list` with every record's box moved by `(dx, dy)`.
    fn shifted(list: &[f32], dx: f32, dy: f32) -> Vec<f32> {
        let mut out = list.to_vec();
        for record in out.chunks_exact_mut(RECORD_FLOATS) {
            // A box record's top-left and its layer's bounds are both in row 0, and a
            // gradient's geometry is relative to the box, so only row 0 moves.
            if record[44] == 0.0 || record[44] == 41.0 || record[44] == 7.0 {
                record[0] += dx;
                record[1] += dy;
            }
        }
        out
    }

    #[test]
    #[ignore] // Requires GPU
    fn a_layer_away_from_the_corner_draws_where_it_is() {
        let harness = TestHarness::new().unwrap();
        // The blurred box moved to (120, 60): the layer's texture covers only its
        // region, so everything in it is placed against that, not the page.
        let list = shifted(&fixture!("layer_blur"), 100.0, 40.0);
        let (page, _) = draw_plan(&harness, "layer_moved", &list, None, &[]);
        let (w, h) = (W as usize, H as usize);
        let (hx, vy) = (blur_span(w, 120, 160, 4.0), blur_span(h, 60, 90, 4.0));
        assert_page(
            &page,
            |x, y| {
                let a = hx[x as usize] * vy[y as usize];
                [
                    255.0 * (1.0 - a) + 229.5 * a,
                    255.0 * (1.0 - a) + 25.5 * a,
                    255.0 * (1.0 - a) + 25.5 * a,
                ]
            },
            4,
            "moved blur",
        );
    }

    #[test]
    #[ignore] // Requires GPU
    fn a_gradient_and_text_in_a_faded_layer_keep_their_place() {
        let harness = TestHarness::new().unwrap();
        let (device, queue) = harness.gpu();
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

        // A gradient box and the word, in a group at half opacity, at (20, 20) in a
        // layer whose own corner is not the page's.
        let layer = fixture!("layer_opacity");
        let group = |inner: &[f32], bounds: [f32; 4]| {
            let mut closing = layer[3 * RECORD_FLOATS..4 * RECORD_FLOATS].to_vec();
            closing[0..4].copy_from_slice(&bounds);
            [layer[..RECORD_FLOATS].to_vec(), inner.to_vec(), closing].concat()
        };
        let gradient = group(&fixture!("box_gradient"), [20.0, 20.0, 100.0, 60.0]);
        let (page, _) = draw_plan(&harness, "layer_gradient", &gradient, None, &[]);
        // The gradient runs red to blue across the box, half faded over white.
        for x in [30u32, 70, 110] {
            let t = (x as f32 - 20.0) / 100.0;
            let (r, g, b) = (1.0 - 0.8 * t, 0.2, 0.2 + 0.8 * t);
            let want = [r, g, b].map(|c| 255.0 * (0.5 + 0.5 * c));
            let got = at(&page, x, 50);
            for c in 0..3 {
                assert!(
                    (got[c] as f32 - want[c]).abs() <= 4.0,
                    "gradient at {x}: {got:?} wanted {want:?}"
                );
            }
        }

        let words = group(&fixture!("text_hello_x1"), [18.0, 23.0, 50.0, 20.0]);
        let (page, _) = draw_plan(&harness, "layer_text", &words, None, &[]);
        let expected = glyph_reference(&fixture!("text_hello_x1"), &atlas, width, 1.0);
        assert_page(
            &page,
            |x, y| {
                let p = expected.get_pixel(x, y);
                [0, 1, 2].map(|c| 255.0 * 0.5 + p[c] as f32 * 0.5)
            },
            4,
            "text in a layer",
        );
    }

    /// How a gradient measures a pixel, in the box's own pixels.
    #[derive(Clone, Copy)]
    enum Shape {
        Linear { to: f32 },
        Radial { centre: (f32, f32), radius: f32 },
        Conic { centre: (f32, f32), angle: f32 },
    }

    /// The gradient's parameter at `(x, y)` from the box's corner.
    fn gradient_t(shape: Shape, x: f32, y: f32) -> f32 {
        match shape {
            Shape::Linear { to } => x / to,
            Shape::Radial { centre, radius } => (x - centre.0).hypot(y - centre.1) / radius,
            Shape::Conic { centre, angle } => {
                let (dx, dy) = (x - centre.0, y - centre.1);
                let turns = (dx.atan2(-dy) - angle) / std::f32::consts::TAU;
                turns - turns.floor()
            }
        }
    }

    /// A stop table as the shader reads it: folded by its spread, then stops
    /// mixed in premultiplied colour. `spread` is 0 pad, 1 repeat, 2 reflect.
    fn gradient_color(stops: &[(f32, [f32; 4])], spread: u32, t: f32) -> [f32; 4] {
        let (start, end) = (stops[0].0, stops[stops.len() - 1].0);
        let mut t = t;
        if spread > 0 {
            let phase = (t - start) / (end - start);
            let phase = if spread == 2 {
                1.0 - (2.0 * (phase * 0.5 - (phase * 0.5).floor()) - 1.0).abs()
            } else {
                phase - phase.floor()
            };
            t = start + phase * (end - start);
        }
        let (mut color, mut offset) = (stops[0].1, start);
        for &(at, next) in &stops[1..] {
            if t < at {
                let amount = ((t - offset) / (at - offset).max(1e-20)).clamp(0.0, 1.0);
                let pre = |c: [f32; 4]| [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]];
                let (a, b) = (pre(color), pre(next));
                let m: Vec<f32> = (0..4).map(|i| a[i] + (b[i] - a[i]) * amount).collect();
                return if m[3] > 0.0 {
                    [m[0] / m[3], m[1] / m[3], m[2] / m[3], m[3]]
                } else {
                    [0.0; 4]
                };
            }
            color = next;
            offset = at;
        }
        color
    }

    #[test]
    #[ignore] // Requires GPU
    fn gradients_with_stops_spreads_and_angles_match_the_formulas() {
        let harness = TestHarness::new().unwrap();
        let (red, yellow, blue) = (
            [1.0, 0.1, 0.1, 1.0],
            [1.0, 0.9, 0.1, 1.0],
            [0.1, 0.1, 1.0, 1.0],
        );
        let three = vec![(0.0, red), (0.5, yellow), (1.0, blue)];
        let five = vec![
            (0.0, red),
            (0.2, yellow),
            (0.5, [0.1, 0.8, 0.2, 1.0]),
            (0.8, [0.1, 0.6, 1.0, 1.0]),
            (1.0, [0.6, 0.1, 0.9, 1.0]),
        ];
        let two_ends = vec![(0.0, red), (1.0, blue)];
        let centre = (50.0, 30.0);
        let cases: Vec<(&str, Vec<f32>, Vec<(f32, [f32; 4])>, u32, Shape)> = vec![
            (
                "grad_three",
                fixture!("grad_three"),
                three.clone(),
                0,
                Shape::Linear { to: 100.0 },
            ),
            (
                "grad_offsets",
                fixture!("grad_offsets"),
                vec![(0.25, red), (0.75, blue)],
                0,
                Shape::Linear { to: 100.0 },
            ),
            (
                "grad_five",
                fixture!("grad_five"),
                five.clone(),
                0,
                Shape::Linear { to: 100.0 },
            ),
            (
                "grad_repeat",
                fixture!("grad_repeat"),
                two_ends.clone(),
                1,
                Shape::Linear { to: 40.0 },
            ),
            (
                "grad_reflect_radial",
                fixture!("grad_reflect_radial"),
                three.clone(),
                2,
                Shape::Radial {
                    centre,
                    radius: 20.0,
                },
            ),
            (
                "grad_radial_stops",
                fixture!("grad_radial_stops"),
                three.clone(),
                0,
                Shape::Radial {
                    centre,
                    radius: 45.0,
                },
            ),
            (
                "grad_conic",
                fixture!("grad_conic"),
                two_ends,
                0,
                Shape::Conic { centre, angle: 0.0 },
            ),
            (
                "grad_conic_stops",
                fixture!("grad_conic_stops"),
                five,
                0,
                Shape::Conic { centre, angle: 0.5 },
            ),
        ];
        for (name, list, stops, spread, shape) in cases {
            let (page, _) = draw_records(&harness, name, &list, 1, None, &[]);
            let (mut worst, mut checked) = (0i32, 0);
            for y in 20..80u32 {
                for x in 20..120u32 {
                    let (px, py) = (x as f32 + 0.5 - 20.0, y as f32 + 0.5 - 20.0);
                    let t = gradient_t(shape, px, py);
                    // A fold or a wrap jumps in colour where its parameter crosses a whole
                    // period; a conic's centre has no angle. Leave a margin around both.
                    let skip = match shape {
                        Shape::Conic { centre, .. } => {
                            (px - centre.0).hypot(py - centre.1) < 2.0 || t < 0.02 || t > 0.98
                        }
                        _ if spread == 1 => {
                            let phase = (t - stops[0].0) / (stops[stops.len() - 1].0 - stops[0].0);
                            let f = phase - phase.floor();
                            f < 0.03 || f > 0.97
                        }
                        _ => false,
                    };
                    if skip {
                        continue;
                    }
                    let c = gradient_color(&stops, spread, t);
                    let got = at(&page, x, y);
                    for ch in 0..3 {
                        let want = 255.0 * (1.0 - c[3]) + c[ch] * c[3] * 255.0;
                        worst = worst.max((got[ch] as i32 - want.round() as i32).abs());
                    }
                    checked += 1;
                }
            }
            println!("{name}: worst {worst} over {checked} pixels");
            assert!(worst <= 4, "{name}: off by {worst}");
            assert!(checked > 3000, "{name}: only {checked} pixels were checked");
            assert!(
                within(at(&page, 10, 10), [255, 255, 255], 0),
                "{name}: outside the box"
            );
        }
    }

    /// A mask's stops as alpha: folded by the spread, then joined in a line.
    fn mask_alpha_of(stops: &[(f32, f32)], t: f32) -> f32 {
        let (mut color, mut offset) = (stops[0].1, stops[0].0);
        for &(at, next) in &stops[1..] {
            if t < at {
                return color
                    + (next - color) * ((t - offset) / (at - offset).max(1e-20)).clamp(0.0, 1.0);
            }
            color = next;
            offset = at;
        }
        color
    }

    #[test]
    #[ignore] // Requires GPU
    fn a_layers_mask_is_the_gradient_measured_in_its_elements_box() {
        let harness = TestHarness::new().unwrap();
        let ends = vec![(0.0, 1.0), (1.0, 0.0)];
        let wavy = vec![(0.0, 1.0), (0.25, 0.2), (0.5, 0.8), (0.75, 0.0), (1.0, 1.0)];
        let (rot_c, rot_s) = (0.4f32.cos(), 0.4f32.sin());
        // Each case: its fixture, its stops, the scale it was laid out at, how a point of
        // the box in fractions measures it, and whether the box is turned about its centre.
        type Measure = fn(f32, f32) -> f32;
        let cases: Vec<(&str, Vec<f32>, usize, Vec<(f32, f32)>, f32, Measure, bool)> = vec![
            (
                "mask_linear",
                fixture!("mask_linear"),
                3,
                ends.clone(),
                1.0,
                |u, _| u,
                false,
            ),
            (
                "mask_stops",
                fixture!("mask_stops"),
                3,
                wavy,
                1.0,
                |u, _| u,
                false,
            ),
            (
                "mask_radial",
                fixture!("mask_radial"),
                3,
                ends.clone(),
                1.0,
                |u, v| (u - 0.5).hypot(v - 0.5) / 0.5,
                false,
            ),
            (
                "mask_rotated",
                fixture!("mask_rotated"),
                3,
                ends.clone(),
                1.0,
                |u, _| u,
                true,
            ),
            (
                "mask_linear_x2",
                fixture!("mask_linear_x2"),
                3,
                ends,
                2.0,
                |u, _| u,
                false,
            ),
            (
                "mask_conic",
                fixture!("mask_conic"),
                3,
                vec![(0.0, 0.1), (1.0, 1.0)],
                1.0,
                // The angle about the centre, in the box's own pixels; none at the centre or
                // where it wraps, which are left out.
                |u, v| {
                    let (dx, dy) = ((u - 0.5) * 100.0, (v - 0.5) * 60.0);
                    let t = (dx.atan2(-dy) / std::f32::consts::TAU).rem_euclid(1.0);
                    if dx.hypot(dy) < 2.0 || !(0.02..=0.98).contains(&t) {
                        f32::NAN
                    } else {
                        t
                    }
                },
                false,
            ),
        ];
        for (name, list, count, stops, scale, measure, turned) in cases {
            let (page, _) = draw_records_at(&harness, name, &list, count, scale, None, &[]);
            let (mut worst, mut checked, mut faded) = (0i32, 0, 0);
            for y in 0..page.height() {
                for x in 0..page.width() {
                    // The pixel's centre in layout units, then back into the box it belongs to.
                    let (px, py) = ((x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale);
                    let (lx, ly) = if turned {
                        let (dx, dy) = (px - 70.0, py - 50.0);
                        (
                            70.0 + rot_c * dx + rot_s * dy,
                            50.0 - rot_s * dx + rot_c * dy,
                        )
                    } else {
                        (px, py)
                    };
                    let (u, v) = ((lx - 20.0) / 100.0, (ly - 20.0) / 60.0);
                    // Inside the box by a margin the edge's antialiasing does not reach, or
                    // clearly outside it.
                    let margin = 1.5 / scale;
                    let inside = lx > 20.0 + margin
                        && lx < 120.0 - margin
                        && ly > 20.0 + margin
                        && ly < 80.0 - margin;
                    let outside = lx < 20.0 - margin
                        || lx > 120.0 + margin
                        || ly < 20.0 - margin
                        || ly > 80.0 + margin;
                    if !inside && !outside {
                        continue;
                    }
                    let t = measure(u, v);
                    if inside && t.is_nan() {
                        continue;
                    }
                    let a = if inside {
                        mask_alpha_of(&stops, t)
                    } else {
                        0.0
                    };
                    let got = at(&page, x, y);
                    for (c, red) in [229.5f32, 25.5, 25.5].into_iter().enumerate() {
                        let want = 255.0 * (1.0 - a) + red * a;
                        worst = worst.max((got[c] as i32 - want.round() as i32).abs());
                    }
                    checked += 1;
                    faded += (inside && a < 0.9) as usize;
                }
            }
            println!("{name}: worst {worst} over {checked} pixels, {faded} of them faded");
            assert!(worst <= 4, "{name}: off by {worst}");
            assert!(
                faded > 500,
                "{name}: the mask barely shows ({faded} pixels faded)"
            );
        }
    }

    /// The signed distance from `(x, y)` to a rounded rectangle given by its centre,
    /// half size and corner radius: negative inside.
    fn rounded_box(x: f32, y: f32, centre: (f32, f32), half: (f32, f32), radius: f32) -> f32 {
        let (qx, qy) = (
            (x - centre.0).abs() - half.0 + radius,
            (y - centre.1).abs() - half.1 + radius,
        );
        qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
    }

    #[test]
    #[ignore] // Requires GPU
    fn a_turned_parents_clip_cuts_its_child_to_the_turned_rounded_box() {
        let harness = TestHarness::new().unwrap();
        let (c, sn) = (0.4f32.cos(), 0.4f32.sin());
        for scale in [1.0f32, 2.0] {
            let name = format!("clip_turned_x{scale}");
            let (page, _) = draw_records_at(
                &harness,
                &name,
                &fixture!("clip_turned"),
                2,
                scale,
                None,
                &[],
            );
            let (mut worst, mut red, mut grey, mut white) = (0i32, 0, 0, 0);
            for y in 0..page.height() {
                for x in 0..page.width() {
                    // The pixel in the card's own frame: taken back through its turn about its centre.
                    let (px, py) = ((x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale);
                    let (dx, dy) = (px - 70.0, py - 50.0);
                    let (lx, ly) = (70.0 + c * dx + sn * dy, 50.0 - sn * dx + c * dy);
                    let card = rounded_box(lx, ly, (70.0, 50.0), (50.0, 30.0), 16.0);
                    // The child's rect, 10 to 70 across and 10 to 50 down, in the same frame.
                    let child = rounded_box(lx, ly, (40.0, 30.0), (30.0, 20.0), 0.0);
                    // Leave out what an edge's antialiasing reaches.
                    let margin = 1.5 / scale;
                    if card.abs() < margin || (card < 0.0 && child.abs() < margin) {
                        continue;
                    }
                    let want: [f32; 3] = if card > 0.0 {
                        white += 1;
                        [255.0; 3]
                    } else if child < 0.0 {
                        red += 1;
                        [229.5, 51.0, 51.0]
                    } else {
                        grey += 1;
                        [216.75; 3]
                    };
                    let got = at(&page, x, y);
                    for ch in 0..3 {
                        worst = worst.max((got[ch] as i32 - want[ch].round() as i32).abs());
                    }
                }
            }
            println!("{name}: worst {worst}; red {red}, grey {grey}, white {white}");
            assert!(worst <= 4, "{name}: off by {worst}");
            assert!(
                red > 400 && grey > 400,
                "{name}: the scene is not what it should be"
            );
        }
    }

    #[test]
    #[ignore] // Requires GPU
    fn a_rounded_clip_cut_by_a_plain_one_is_both() {
        let harness = TestHarness::new().unwrap();
        let (page, _) = draw_records(&harness, "clip_cut", &fixture!("clip_cut"), 1, None, &[]);
        let (mut worst, mut blue) = (0i32, 0);
        for y in 0..page.height() {
            for x in 0..page.width() {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                // The box [50, 110] by [20, 80], rounded by 24, and the plain clip's edge at x = 90.
                let round = rounded_box(px, py, (80.0, 50.0), (30.0, 30.0), 24.0);
                if round.abs() < 1.5 || (px - 90.0).abs() < 1.5 {
                    continue;
                }
                let inside = round < 0.0 && px < 90.0;
                let want: [f32; 3] = if inside {
                    blue += 1;
                    [51.0, 127.5, 229.5]
                } else {
                    [255.0; 3]
                };
                let got = at(&page, x, y);
                for ch in 0..3 {
                    worst = worst.max((got[ch] as i32 - want[ch].round() as i32).abs());
                }
            }
        }
        println!("clip_cut: worst {worst} over {blue} pixels inside");
        assert!(worst <= 4, "off by {worst}");
        assert!(blue > 1000, "only {blue} pixels are inside");
    }
    /// The ABI fade is the product of four clamped distances from a screen rect.
    fn fade_alpha(px: f32, py: f32, rect: [f32; 4], fade: [f32; 4]) -> f32 {
        [
            py - rect[1],
            rect[0] + rect[2] - px,
            rect[1] + rect[3] - py,
            px - rect[0],
        ]
        .into_iter()
        .zip(fade)
        .map(|(distance, width)| {
            if width > 0.0 {
                (distance / width).clamp(0.0, 1.0)
            } else {
                1.0
            }
        })
        .product()
    }

    #[test]
    #[ignore] // Requires GPU
    fn fading_clips_use_the_innermost_fade_rectangle_on_screen() {
        let harness = TestHarness::new().unwrap();
        for scale in [1.0f32, 2.0] {
            for (name, list, count, cut, turned, layered) in [
                ("fade_plain", fixture!("fade_plain"), 1, false, false, false),
                ("fade_cut", fixture!("fade_cut"), 1, true, false, false),
                (
                    "fade_turned",
                    fixture!("fade_turned"),
                    1,
                    false,
                    true,
                    false,
                ),
                ("fade_layer", fixture!("fade_layer"), 3, true, false, true),
                (
                    "fade_shadow",
                    fixture!("fade_shadow"),
                    1,
                    false,
                    false,
                    false,
                ),
            ] {
                let label = format!("{name}_x{scale}");
                let (page, _) = draw_records_at(&harness, &label, &list, count, scale, None, &[]);
                let fade_rect = if cut {
                    [10.0, 10.0, 100.0, 80.0]
                } else if turned {
                    let (c, s) = (0.4f32.cos(), 0.4f32.sin());
                    let (w, h) = (100.0 * c + 60.0 * s, 100.0 * s + 60.0 * c);
                    [70.0 - w / 2.0, 50.0 - h / 2.0, w, h]
                } else {
                    [20.0, 20.0, 100.0, 60.0]
                };
                let (mut worst, mut faded) = (0i32, 0usize);
                for y in 0..page.height() {
                    for x in 0..page.width() {
                        let (px, py) = ((x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale);
                        let (lx, ly) = if turned {
                            let (c, s) = (0.4f32.cos(), 0.4f32.sin());
                            let (dx, dy) = (px - 70.0, py - 50.0);
                            (70.0 + c * dx + s * dy, 50.0 - s * dx + c * dy)
                        } else {
                            (px, py)
                        };
                        let half = if cut { (35.0, 30.0) } else { (50.0, 30.0) };
                        let centre = if cut { (55.0, 50.0) } else { (70.0, 50.0) };
                        let distance = rounded_box(lx, ly, centre, half, 0.0);
                        if distance.abs() < 1.5 / scale {
                            continue;
                        }
                        let mut alpha = if distance < 0.0 {
                            fade_alpha(px, py, fade_rect, [20.0, 32.0, 24.0, 28.0])
                        } else {
                            0.0
                        };
                        if name == "fade_shadow" {
                            let inner = rounded_box(px, py, (70.0, 50.0), (30.0, 10.0), 0.0);
                            let outer = rounded_box(px, py, (70.0, 50.0), (42.0, 22.0), 0.0);
                            if inner.abs() < 1.5 / scale || outer.abs() < 1.5 / scale {
                                continue;
                            }
                            if inner < 0.0 || outer > 0.0 {
                                alpha = 0.0;
                            }
                        }
                        faded += (alpha > 0.05 && alpha < 0.9) as usize;
                        if layered {
                            alpha *= 0.5;
                        }
                        let color = if layered {
                            [0.1, 0.25, 0.45]
                        } else {
                            [0.2, 0.5, 0.9]
                        };
                        let got = at(&page, x, y);
                        for ch in 0..3 {
                            let want = (255.0 * (1.0 - alpha + color[ch] * alpha)).round() as i32;
                            worst = worst.max((got[ch] as i32 - want).abs());
                        }
                    }
                }
                println!("{label}: worst {worst}, {faded} faded pixels");
                assert!(worst <= 3, "{label}: off by {worst}");
                assert!(faded > 500, "{label}: only {faded} faded pixels");
            }
        }
    }

    #[test]
    #[ignore] // Requires GPU
    fn native_fades_still_use_the_screen_clip_when_no_fade_rectangle_is_set() {
        let harness = TestHarness::new().unwrap();
        let mut batch = PrimitiveBatch::new();
        batch.primitives.push(GpuPrimitive {
            bounds: [10.0, 10.0, 140.0, 100.0],
            color: [0.2, 0.5, 0.9, 1.0],
            clip_bounds: [20.0, 20.0, 100.0, 60.0],
            clip_fade: [20.0, 32.0, 24.0, 28.0],
            type_info: [PrimitiveType::Rect as u32, 0, 1, 0],
            ..GpuPrimitive::default()
        });
        let native = draw(&harness, "fade_native", &batch, 1.0);
        batch.primitives[0].clip_fade_bounds = batch.primitives[0].clip_bounds;
        let explicit = draw(&harness, "fade_explicit", &batch, 1.0);
        assert_eq!(compare(&native, &explicit, 0), (0, 0));
    }
}
