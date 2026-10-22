//! Glass shares the native contour cache while retaining its fixed instance layout.
use blinc_core::{
    Brush, ClipShape, Color, CornerRadius, DrawContext, FillRule, GlassStyle, Path, Rect,
};
use blinc_gpu::{GpuPaintContext, PrimitiveBatch, primitives::GpuGlassPrimitive};
fn draw(p: &mut GpuPaintContext<'_>, nested: bool) {
    p.fill_rect(
        Rect::new(10.0, 10.0, 120.0, 100.0),
        CornerRadius::ZERO,
        Brush::Glass(GlassStyle {
            simple: true,
            depth: u32::from(nested),
            ..Default::default()
        }),
    );
}
#[test]
fn glass_path_clip_headers_follow_active_owners_and_repeated_drains() {
    assert_eq!(std::mem::size_of::<GpuGlassPrimitive>(), 144);
    let mut p = GpuPaintContext::new(180.0, 150.0);
    p.set_clip_fill_rule(FillRule::EvenOdd);
    p.push_clip(ClipShape::Path(Path::rect(Rect::new(
        20.0, 20.0, 80.0, 80.0,
    ))));
    p.push_clip(ClipShape::rect(Rect::new(40.0, 30.0, 50.0, 60.0)));
    for dynamic in [false, true] {
        for _ in 0..2 {
            if dynamic {
                p.push_motion_subtree();
            }
            draw(&mut p, false);
            draw(&mut p, true);
            p.fill_rect(
                Rect::new(10.0, 10.0, 120.0, 100.0),
                CornerRadius::ZERO,
                Color::RED.into(),
            );
            if dynamic {
                p.pop_motion_subtree();
            }
            let b = if dynamic {
                p.take_dynamic_batch()
            } else {
                p.take_batch()
            };
            assert_eq!(b.glass_primitives[0].clip_radius, [1.0, 0.0, 0.0, -1.0]);
            assert_eq!(b.glass_primitives[0].clip_bounds, [40.0, 30.0, 50.0, 60.0]);
            assert_eq!(
                b.nested_glass_primitives[0].clip_radius,
                b.glass_primitives[0].clip_radius
            );
            assert_eq!(b.primitives[0].clip_shape[0], 1.0);
            assert_eq!(b.aux_data[2][2], 1.0);
            assert_eq!(
                b.aux_data.len(),
                3 + (b.aux_data[2][1] as usize).div_ceil(2)
            );
        }
    }
}
#[test]
fn glass_path_clip_merge_rebases_top_and_nested_headers() {
    let mut p = GpuPaintContext::new(180.0, 150.0);
    p.push_clip(ClipShape::Path(Path::rect(Rect::new(
        20.0, 20.0, 80.0, 80.0,
    ))));
    draw(&mut p, false);
    draw(&mut p, true);
    let mut out = PrimitiveBatch::new();
    out.aux_data = vec![[999.0; 4]; 7];
    out.merge(p.take_batch());
    for glass in out
        .glass_primitives
        .iter()
        .chain(&out.nested_glass_primitives)
    {
        assert_eq!(glass.clip_radius, [8.0, 0.0, 0.0, -1.0]);
        assert_eq!(&out.aux_data[10][..2], [20.0, 20.0]);
    }
    assert_eq!(out.nested_glass_primitives.len(), 1);
}
#[test]
fn glass_path_clip_scratch_strips_ancestors_and_restores_outer_capture() {
    let mut p = GpuPaintContext::new(180.0, 150.0);
    p.push_clip(ClipShape::Path(Path::new()));
    p.push_composite_layer(7);
    draw(&mut p, false);
    p.push_clip(ClipShape::Path(Path::new()));
    draw(&mut p, true);
    p.pop_clip();
    p.pop_composite_layer();
    let b = p.take_composite_layer_batches().remove(&7).unwrap();
    assert_eq!(b.glass_primitives[0].clip_radius, [0.0; 4]);
    assert_eq!(
        b.nested_glass_primitives[0].clip_radius,
        [1.0, 0.0, 0.0, -1.0]
    );
    assert_eq!(b.aux_data.len(), 3);
    assert_eq!(b.aux_data[2][1], 0.0);
    draw(&mut p, false);
    assert_eq!(
        p.take_batch().glass_primitives[0].clip_radius,
        [1.0, 0.0, 0.0, -1.0]
    );
}
