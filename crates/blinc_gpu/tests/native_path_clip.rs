//! Native primitive path rows follow batch ownership and auxiliary rebasing.
use blinc_core::{ClipShape, Color, CornerRadius, DrawContext, FillRule, Path, Rect, Transform};
use blinc_gpu::{GpuPaintContext, PrimitiveBatch, primitives::ClipType};
fn draw(p: &mut GpuPaintContext<'_>) {
    p.fill_rect(
        Rect::new(10.0, 10.0, 120.0, 100.0),
        CornerRadius::ZERO,
        Color::RED.into(),
    );
}
fn assert_header(b: &PrimitiveBatch, expected: [f32; 2]) {
    for p in &b.primitives {
        assert_eq!(p.type_info[2], ClipType::Rect as u32);
        assert_eq!(p.clip_bounds, [45.0, 30.0, 50.0, 60.0]);
        assert!(p.clip_shape[0] > 0.5);
        let at = p.clip_shape[0] as usize - 1;
        assert_eq!(b.aux_data[at], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(b.aux_data[at + 1], [0.0, 0.0, 3.0, 0.0]);
        assert_eq!(b.aux_data[at + 2][2], 1.0);
        assert_eq!(&b.aux_data[at + 3][..2], expected);
    }
}
#[test]
fn native_path_clip_reuses_rows_per_owner_and_rebuilds_after_drains() {
    let mut p = GpuPaintContext::new(180.0, 150.0);
    p.set_clip_fill_rule(FillRule::EvenOdd);
    p.push_transform(Transform::translate(15.0, 10.0));
    p.push_clip(ClipShape::Path(Path::rect(Rect::new(
        20.0, 20.0, 80.0, 80.0,
    ))));
    p.pop_transform();
    p.push_clip(ClipShape::rect(Rect::new(45.0, 30.0, 50.0, 60.0)));
    for foreground in [false, true] {
        p.set_foreground_layer(foreground);
        for _ in 0..2 {
            p.push_motion_subtree();
            draw(&mut p);
            draw(&mut p);
            p.pop_motion_subtree();
            let mut b = p.take_dynamic_batch();
            if foreground {
                b = b.take_foreground();
            }
            assert_eq!(b.primitives.len(), 2);
            assert_eq!(b.primitives[0].clip_shape, b.primitives[1].clip_shape);
            let count = b.aux_data[2][1] as usize;
            assert_eq!(b.aux_data.len(), 3 + count.div_ceil(2));
            assert_header(&b, [35.0, 30.0]);
        }
    }
    p.set_foreground_layer(false);
    draw(&mut p);
    assert_header(&p.take_batch(), [35.0, 30.0]);
    p.push_composite_layer(7);
    draw(&mut p);
    p.push_clip(ClipShape::Path(Path::new()));
    draw(&mut p);
    p.pop_clip();
    p.pop_composite_layer();
    let b = p.take_composite_layer_batches().remove(&7).unwrap();
    assert_eq!(b.primitives[0].clip_shape, [0.0; 4]);
    let at = b.primitives[1].clip_shape[0] as usize - 1;
    assert_eq!(b.aux_data[at + 2][1], 0.0);
    draw(&mut p);
    assert_header(&p.take_batch(), [35.0, 30.0]);
}
#[test]
fn native_path_clip_merge_rebases_headers_and_meshes_in_both_streams() {
    let mut out = PrimitiveBatch::new();
    out.aux_data = vec![[999.0; 4]; 7];
    for foreground in [false, true] {
        let mut p = GpuPaintContext::new(180.0, 150.0);
        p.set_foreground_layer(foreground);
        p.set_clip_fill_rule(FillRule::EvenOdd);
        p.push_clip(ClipShape::Path(Path::rect(Rect::new(
            35.0, 30.0, 80.0, 80.0,
        ))));
        p.push_clip(ClipShape::rect(Rect::new(45.0, 30.0, 50.0, 60.0)));
        draw(&mut p);
        p.fill_path(
            &Path::rect(Rect::new(10.0, 10.0, 120.0, 100.0)),
            Color::RED.into(),
        );
        let b = p.take_batch();
        let source = if foreground {
            &b.foreground_primitives
        } else {
            &b.primitives
        };
        let offset = out.aux_data.len() as f32;
        let before = source.clone();
        out.merge(b);
        let target = if foreground {
            &out.foreground_primitives
        } else {
            &out.primitives
        };
        for (a, b) in before
            .iter()
            .zip(target.iter().rev().take(before.len()).rev())
        {
            assert_eq!(b.clip_shape[0], a.clip_shape[0] + offset);
            if a.type_info[0] == blinc_gpu::primitives::PrimitiveType::Mesh as u32 {
                assert_eq!(b.border[2], a.border[2] + offset);
            }
        }
    }
    assert_header(&out, [35.0, 30.0]);
    assert_header(&out.take_foreground(), [35.0, 30.0]);
}

#[test]
fn native_path_clip_topmost_selection_and_composite_drain_reset_offsets() {
    let mut p = GpuPaintContext::new(180.0, 150.0);
    draw(&mut p);
    assert!(p.take_batch().aux_data.is_empty());
    p.push_composite_layer(7);
    p.push_clip(ClipShape::Path(Path::new()));
    for _ in 0..2 {
        draw(&mut p);
        p.push_clip(ClipShape::Circle {
            center: blinc_core::Point::new(70.0, 70.0),
            radius: 30.0,
        });
        draw(&mut p);
        p.pop_clip();
        let b = p.take_composite_layer_batches().remove(&7).unwrap();
        assert_eq!(b.primitives[0].clip_shape[0], 1.0);
        assert_eq!(b.aux_data.len(), 3);
        assert_eq!(b.aux_data[2][1], 0.0);
        assert_eq!(b.primitives[1].clip_shape, [0.0; 4]);
        assert_eq!(b.primitives[1].type_info[2], ClipType::Circle as u32);
    }
}

#[test]
fn native_path_clip_merge_preserves_per_side_border_widths() {
    let mut p = GpuPaintContext::new(180.0, 150.0);
    p.push_clip(ClipShape::Path(Path::rect(Rect::new(
        20.0, 20.0, 80.0, 80.0,
    ))));
    p.fill_rect_with_per_side_border(
        Rect::new(10.0, 10.0, 120.0, 100.0),
        CornerRadius::ZERO,
        Color::RED.into(),
        [3.0, 7.0, 11.0, 5.0],
        Color::WHITE,
    );
    let mut out = PrimitiveBatch::new();
    out.aux_data = vec![[999.0; 4]; 7];
    out.merge(p.take_batch());
    assert_eq!(out.primitives[0].border, [3.0, 7.0, 11.0, 5.0]);
    let at = out.primitives[0].clip_shape[0] as usize - 1;
    assert_eq!(&out.aux_data[at + 3][..2], [20.0, 20.0]);
}
