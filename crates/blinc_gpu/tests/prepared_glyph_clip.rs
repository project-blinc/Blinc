//! Separate prepared glyphs retain the owning native path header.
use blinc_core::{ClipShape, DrawContext, FillRule, Path, Rect, Transform};
use blinc_gpu::{
    GpuPaintContext, PrimitiveBatch,
    primitives::{GpuGlyph, GpuPrimitive},
};

#[test]
fn prepared_glyph_clip_owners_drains_and_conversion_share_headers() {
    assert_eq!(std::mem::size_of::<GpuGlyph>(), 96);
    let mut p = GpuPaintContext::new(160.0, 130.0);
    p.set_clip_fill_rule(FillRule::EvenOdd);
    p.push_transform(Transform::translate(5.0, 10.0));
    p.push_clip(ClipShape::Path(Path::rect(Rect::new(
        20.0, 20.0, 80.0, 80.0,
    ))));
    p.pop_transform();
    p.push_clip(ClipShape::rect(Rect::new(40.0, 30.0, 50.0, 60.0)));
    let g = GpuGlyph {
        bounds: [3.0, 4.0, 50.0, 60.0],
        flags: [1.0, 999.0, 0.0, 0.0],
        ..Default::default()
    };
    for dynamic in [false, true] {
        for _ in 0..2 {
            if dynamic {
                p.push_motion_subtree();
            }
            p.push_prepared_glyphs(&[g, g]);
            if dynamic {
                p.pop_motion_subtree();
            }
            let mut b = if dynamic {
                p.take_dynamic_batch()
            } else {
                p.take_batch()
            };
            assert_eq!(b.glyphs.len(), 2);
            assert_eq!(b.glyphs[0].bounds, g.bounds);
            assert_eq!(b.glyphs[0].flags, [1.0, 1.0, 0.0, 0.0]);
            assert_eq!(b.glyphs[0].clip_bounds, [40.0, 30.0, 50.0, 60.0]);
            assert_eq!(b.aux_data[2][2], 1.0);
            assert_eq!(&b.aux_data[3][..2], [25.0, 30.0]);
            assert_eq!(
                b.aux_data.len(),
                3 + (b.aux_data[2][1] as usize).div_ceil(2)
            );
            let snapshot = b.get_unified_foreground_primitives();
            assert_eq!(snapshot[0].clip_shape, [1.0, 0.0, 0.0, 0.0]);
            assert_eq!(snapshot[0].type_info[1], 1);
            assert_eq!(snapshot[0].type_info[2], 1);
            b.convert_glyphs_to_primitives();
            assert!(b.glyphs.is_empty());
            assert_eq!(
                b.foreground_primitives[0].clip_shape,
                snapshot[0].clip_shape
            );
            assert_eq!(b.take_foreground().aux_data.len(), b.aux_data.len());
        }
    }
}

#[test]
fn prepared_glyph_clip_merges_and_no_clip_keep_sentinels() {
    let mut p = GpuPaintContext::new(160.0, 130.0);
    p.push_prepared_glyphs(&[GpuGlyph::default()]);
    p.push_clip(ClipShape::Path(Path::new()));
    p.push_prepared_glyphs(&[GpuGlyph::default()]);
    let mut b = PrimitiveBatch::new();
    b.aux_data = vec![[999.0; 4]; 7];
    b.merge(p.take_batch());
    assert_eq!(b.glyphs[0].flags[1], 0.0);
    assert_eq!(b.glyphs[1].flags[1], 8.0);
    assert_eq!(b.aux_data[9][1], 0.0);
    assert_eq!(GpuPrimitive::from_glyph(&b.glyphs[1]).clip_shape[0], 8.0);
    assert_eq!(b.glyphs[1].with_no_clip().flags[1], 0.0);
}

#[test]
fn prepared_glyph_clip_scratch_strips_ancestors_and_restores_capture() {
    let mut p = GpuPaintContext::new(160.0, 130.0);
    p.push_clip(ClipShape::Path(Path::new()));
    p.push_composite_layer(7);
    p.push_prepared_glyphs(&[GpuGlyph::default()]);
    p.push_clip(ClipShape::Path(Path::new()));
    p.push_prepared_glyphs(&[GpuGlyph::default()]);
    p.pop_clip();
    p.pop_composite_layer();
    let b = p.take_composite_layer_batches().remove(&7).unwrap();
    assert_eq!(b.glyphs[0].flags[1], 0.0);
    assert_eq!(b.glyphs[1].flags[1], 1.0);
    p.push_prepared_glyphs(&[GpuGlyph::default()]);
    assert_eq!(p.take_batch().glyphs[0].flags[1], 1.0);
}
