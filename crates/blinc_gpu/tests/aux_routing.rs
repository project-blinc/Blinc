//! Referenced payloads stay in the batch that owns their primitives.
#[path = "fixtures/aux_scene.rs"]
mod fixture;
use blinc_gpu::primitives::{ClipType, PrimitiveType};
use fixture::{Route, SHAPE, scene};

fn polygon() -> blinc_core::ClipShape {
    use blinc_core::{ClipShape, Point};
    ClipShape::Polygon(vec![
        Point::new(0.0, 0.0),
        Point::new(80.0, 0.0),
        Point::new(0.0, 80.0),
    ])
}

fn draw(ctx: &mut blinc_gpu::GpuPaintContext<'_>) {
    use blinc_core::{Color, CornerRadius, DrawContext, Rect};
    ctx.fill_rect(
        Rect::new(0.0, 0.0, 80.0, 80.0),
        CornerRadius::ZERO,
        Color::RED.into(),
    );
}

fn assert_polygon(batch: &blinc_gpu::PrimitiveBatch) {
    assert!(!batch.primitives.is_empty());
    assert_eq!(
        batch.aux_data,
        [[0.0, 0.0, 80.0, 0.0], [0.0, 80.0, 0.0, 0.0]]
    );
    for p in &batch.primitives {
        assert_eq!(p.type_info[2], ClipType::Polygon as u32);
        assert_eq!(p.clip_radius, [1.0, 0.0, 3.0, 0.0]);
    }
}

#[test]
fn an_inherited_polygon_is_cached_in_each_batch_and_survives_drains() {
    use blinc_core::{DrawContext, FillRule};
    let mut ctx = blinc_gpu::GpuPaintContext::new(100.0, 100.0);
    ctx.set_clip_fill_rule(FillRule::EvenOdd);
    ctx.push_clip(polygon());
    draw(&mut ctx);
    for _ in 0..2 {
        ctx.push_motion_subtree();
        ctx.push_motion_subtree();
        draw(&mut ctx);
        ctx.pop_motion_subtree();
        draw(&mut ctx);
        ctx.pop_motion_subtree();
        assert_polygon(&ctx.take_dynamic_batch());
        draw(&mut ctx);
    }
    assert_polygon(&ctx.take_batch());
    draw(&mut ctx);
    assert_polygon(&ctx.take_batch());
}

#[test]
fn intrinsic_composite_polygon_offsets_are_invalidated_on_drain() {
    use blinc_core::{DrawContext, FillRule};
    let mut ctx = blinc_gpu::GpuPaintContext::new(100.0, 100.0);
    ctx.push_composite_layer(9);
    ctx.set_clip_fill_rule(FillRule::EvenOdd);
    ctx.push_clip(polygon());
    for _ in 0..2 {
        draw(&mut ctx);
        draw(&mut ctx);
        assert_polygon(&ctx.take_composite_layer_batches().remove(&9).unwrap());
    }
    ctx.pop_clip();
    ctx.pop_composite_layer();
    assert!(ctx.take_batch().aux_data.is_empty());
}

#[test]
fn a_composite_bake_still_strips_ancestor_clips_and_restores_them_afterward() {
    use blinc_core::{DrawContext, FillRule};
    let mut ctx = blinc_gpu::GpuPaintContext::new(100.0, 100.0);
    ctx.set_clip_fill_rule(FillRule::EvenOdd);
    ctx.push_clip(polygon());
    ctx.push_composite_layer(11);
    draw(&mut ctx);
    ctx.pop_composite_layer();
    let composite = ctx.take_composite_layer_batches().remove(&11).unwrap();
    assert_eq!(composite.primitives[0].type_info[2], ClipType::None as u32);
    assert!(composite.aux_data.is_empty());
    draw(&mut ctx);
    assert_polygon(&ctx.take_batch());
}

#[test]
fn mixed_auxiliary_payloads_follow_static_dynamic_and_composite_routing() {
    for route in [Route::Static, Route::Dynamic, Route::Composite] {
        for foreground in [false, true] {
            let (static_batch, batch) = scene(route, foreground);
            if route != Route::Static {
                let (_, prefix) = scene(Route::Static, false);
                assert_eq!(
                    static_batch.aux_data,
                    prefix.aux_data[..4],
                    "{route:?}: static payload changed"
                );
            }
            let primitives = if foreground && route != Route::Composite {
                assert!(batch.primitives.len() <= 2);
                &batch.foreground_primitives
            } else {
                assert!(batch.foreground_primitives.is_empty());
                &batch.primitives
            };
            let mut meshes = 0;
            let mut gradients = 0;
            let mut groups = 0;
            for p in primitives {
                if p.type_info[2] == ClipType::Polygon as u32 {
                    let offset = p.clip_radius[3] as usize;
                    assert_eq!(p.clip_radius[2], 4.0);
                    assert_eq!(
                        batch.aux_data[offset..offset + 2],
                        [[0.0, 0.0, 200.0, 0.0], [200.0, 120.0, 0.0, 120.0]]
                    );
                }
                if p.type_info[0] == PrimitiveType::Mesh as u32 {
                    meshes += 1;
                    let offset = p.border[2] as usize;
                    let rows = &batch.aux_data[offset..offset + 2];
                    for point in [
                        [rows[0][0], rows[0][1]],
                        [rows[0][2], rows[0][3]],
                        [rows[1][0], rows[1][1]],
                    ] {
                        assert!(point[0] >= p.bounds[0] && point[0] <= p.bounds[0] + p.bounds[2]);
                        assert!(point[1] >= p.bounds[1] && point[1] <= p.bounds[1] + p.bounds[3]);
                    }
                    if p.type_info[1] != 0 || p.type_info[3] == 1 {
                        gradients += 1;
                    }
                    if p.type_info[3] == 1 {
                        for color in &batch.aux_data[offset + 2..offset + 5] {
                            assert_eq!(color[3], 1.0);
                            assert!(color[..3].iter().any(|v| *v > 0.5));
                        }
                    }
                } else if p.perspective[3] == 6.0 {
                    groups += 1;
                    assert_eq!(p.border[1], 1.0);
                    let offset = p.border[2] as usize;
                    assert_eq!(
                        batch.aux_data[offset..offset + 4],
                        [SHAPE.offset, SHAPE.params, SHAPE.half_ext, SHAPE.color]
                    );
                }
            }
            assert!(meshes >= 4, "{route:?}: mesh primitives missing");
            assert!(gradients > 0, "{route:?}: gradient mesh missing");
            assert_eq!(groups, 1, "{route:?}: 3D group missing");
        }
    }
}
