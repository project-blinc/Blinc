//! Tessellated brush vertices retain shared contour headers through ownership changes.
use blinc_core::{
    ClipShape, Color, DrawContext, FillRule, ImageBrush, LayerConfig, Path, Point, Rect, Stroke,
    Transform,
};
use blinc_gpu::{GpuPaintContext, PrimitiveBatch, path::PathVertex};
fn brush() -> blinc_core::Brush {
    blinc_core::Brush::Image(ImageBrush::new("placeholder").tint(Color::RED))
}
fn draw(p: &mut GpuPaintContext<'_>) {
    p.fill_path(&Path::rect(Rect::new(10.0, 10.0, 120.0, 100.0)), brush());
}
fn header(b: &PrimitiveBatch) {
    assert!(!b.paths.vertices.is_empty());
    for v in &b.paths.vertices {
        assert_eq!(v.clip_type, 1);
        assert_eq!(v.clip_bounds, [45.0, 30.0, 50.0, 60.0]);
        assert!(v.clip_shape > 0);
        let at = v.clip_shape as usize - 1;
        assert_eq!(b.aux_data[at], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(b.aux_data[at + 1], [0.0, 0.0, 3.0, 0.0]);
        assert_eq!(b.aux_data[at + 2][2], 1.0);
        assert_eq!(&b.aux_data[at + 3][..2], [35.0, 30.0]);
    }
}
#[test]
fn path_brush_clip_headers_are_shared_with_primitives_and_survive_owner_drains() {
    assert_eq!(std::mem::size_of::<PathVertex>(), 120);
    assert_eq!(std::mem::offset_of!(PathVertex, clip_shape), 108);
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
        for route in [0, 1] {
            for _ in 0..2 {
                if route == 1 {
                    p.push_motion_subtree();
                }
                draw(&mut p);
                p.stroke_path(
                    &Path::line(Point::new(10.0, 60.0), Point::new(130.0, 60.0)),
                    &Stroke::new(30.0),
                    brush(),
                );
                p.fill_rect(
                    Rect::new(10.0, 10.0, 120.0, 100.0),
                    0.0.into(),
                    Color::RED.into(),
                );
                if route == 1 {
                    p.pop_motion_subtree();
                }
                let mut b = if route == 1 {
                    p.take_dynamic_batch()
                } else {
                    p.take_batch()
                };
                if foreground {
                    b = b.take_foreground();
                }
                header(&b);
                assert_eq!(
                    b.primitives[0].clip_shape[0],
                    b.paths.vertices[0].clip_shape as f32
                );
                assert_eq!(
                    b.aux_data.len(),
                    3 + (b.aux_data[2][1] as usize).div_ceil(2)
                );
            }
        }
    }
}
#[test]
fn path_brush_clip_merge_rebases_both_streams_and_background_group_cursors() {
    for foreground in [false, true] {
        let mut prefix = GpuPaintContext::new(180.0, 150.0);
        prefix.set_foreground_layer(foreground);
        draw(&mut prefix);
        let mut out = prefix.take_batch();
        out.aux_data = vec![[999.0; 4]; 7];
        let prefix_vertices = if foreground {
            out.foreground_paths.vertices.len()
        } else {
            out.paths.vertices.len()
        };
        let prefix_indices = if foreground {
            out.foreground_paths.indices.len()
        } else {
            out.paths.indices.len()
        };
        let mut p = GpuPaintContext::new(180.0, 150.0);
        p.set_foreground_layer(foreground);
        p.set_clip_fill_rule(FillRule::EvenOdd);
        p.push_clip(ClipShape::Path(Path::rect(Rect::new(
            35.0, 30.0, 80.0, 80.0,
        ))));
        p.push_clip(ClipShape::rect(Rect::new(45.0, 30.0, 50.0, 60.0)));
        p.push_layer(LayerConfig::new().opacity(0.5));
        draw(&mut p);
        p.pop_layer();
        out.merge(p.take_batch());
        if foreground {
            out = out.take_foreground();
        }
        assert_eq!(out.layer_commands[0].path_vertex_index, prefix_vertices);
        assert_eq!(out.layer_commands[0].path_index_count, prefix_indices);
        assert_eq!(out.paths.vertices[prefix_vertices].clip_shape, 8);
        out.paths.vertices.drain(..prefix_vertices);
        header(&out);
    }
}
#[test]
fn path_brush_clip_scratch_strips_ancestors_and_rebuilds_intrinsic_rows_after_drain() {
    let mut p = GpuPaintContext::new(180.0, 150.0);
    p.push_clip(ClipShape::Path(Path::new()));
    p.push_composite_layer(7);
    draw(&mut p);
    let b = p.take_composite_layer_batches().remove(&7).unwrap();
    assert!(b.aux_data.is_empty());
    assert!(b.paths.vertices.iter().all(|v| v.clip_shape == 0));
    p.push_clip(ClipShape::Path(Path::new()));
    for _ in 0..2 {
        draw(&mut p);
        let b = p.take_composite_layer_batches().remove(&7).unwrap();
        assert!(b.paths.vertices.iter().all(|v| v.clip_shape == 1));
        assert_eq!(b.aux_data.len(), 3);
        assert_eq!(b.aux_data[2][1], 0.0);
    }
}
