//! A polygon clip drawn under each fill rule.
//!
//! A pentagram's centre is wound twice: inside under the nonzero rule, a
//! hole under even-odd. Its arms are wound once and are inside under both.

#[cfg(test)]
mod tests {
    use crate::harness::TestHarness;
    use blinc_core::{ClipShape, Color, CornerRadius, DrawContext, FillRule, Point, Rect};
    use image::RgbaImage;

    const SIZE: f32 = 200.0;

    /// A pentagram of radius 80 about (100, 100), its first point at the top,
    /// drawn by joining every second corner of a regular pentagon.
    fn pentagram() -> ClipShape {
        let corner = |k: usize| {
            let angle = (-90.0 + 72.0 * k as f32).to_radians();
            Point::new(100.0 + 80.0 * angle.cos(), 100.0 + 80.0 * angle.sin())
        };
        ClipShape::Polygon([0, 2, 4, 1, 3].map(corner).to_vec())
    }

    fn render(harness: &TestHarness, name: &str, rule: Option<FillRule>) -> RgbaImage {
        let mut ctx = harness.create_context_with_size(name, SIZE, SIZE);
        if let Some(rule) = rule {
            ctx.ctx().set_clip_fill_rule(rule);
        }
        ctx.ctx().push_clip(pentagram());
        // The clip's points are in the clipped primitive's own frame, so the
        // fill starts at the origin.
        ctx.ctx().fill_rect(
            Rect::new(0.0, 0.0, SIZE, SIZE),
            CornerRadius::ZERO,
            Color::rgb(0.9, 0.1, 0.1).into(),
        );
        ctx.ctx().pop_clip();
        let path = harness.output_path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        harness
            .render_to_png(&ctx.take_batch(), SIZE as u32, SIZE as u32, &path)
            .unwrap();
        image::open(&path).unwrap().to_rgba8()
    }

    fn is_red(img: &RgbaImage, x: u32, y: u32) -> bool {
        let p = img.get_pixel(x, y).0;
        p[0] > 200 && p[1] < 60 && p[2] < 60
    }

    fn is_white(img: &RgbaImage, x: u32, y: u32) -> bool {
        img.get_pixel(x, y).0[..3].iter().all(|c| *c > 245)
    }

    #[test]
    #[ignore] // Requires GPU
    fn the_fill_rule_decides_the_middle_of_a_pentagram() {
        let harness = TestHarness::new().unwrap();

        let nonzero = render(&harness, "clip_nonzero", None);
        assert!(is_red(&nonzero, 100, 100), "nonzero fills the middle");
        assert!(is_red(&nonzero, 100, 35), "nonzero fills an arm");
        assert!(is_white(&nonzero, 10, 10), "nothing outside the star");

        let explicit = render(&harness, "clip_nonzero_set", Some(FillRule::NonZero));
        assert!(
            is_red(&explicit, 100, 100),
            "an explicit nonzero is the default"
        );

        let even_odd = render(&harness, "clip_even_odd", Some(FillRule::EvenOdd));
        assert!(is_white(&even_odd, 100, 100), "even-odd leaves a hole");
        assert!(is_red(&even_odd, 100, 35), "even-odd fills an arm");
        assert!(is_white(&even_odd, 10, 10), "nothing outside the star");
    }
}
