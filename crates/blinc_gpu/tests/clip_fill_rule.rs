//! The fill rule of a polygon clip reaches the primitive it clips.
//!
//! These tests need no GPU.

use blinc_core::{ClipShape, Color, CornerRadius, DrawContext, FillRule, Point, Rect};
use blinc_gpu::GpuPaintContext;
use blinc_gpu::primitives::{ClipType, GpuPrimitive};

fn triangle() -> ClipShape {
    ClipShape::Polygon(vec![
        Point::new(0.0, 0.0),
        Point::new(100.0, 0.0),
        Point::new(50.0, 80.0),
    ])
}

/// The primitive drawn under whatever clip `push` leaves in place.
fn drawn_under(push: impl FnOnce(&mut GpuPaintContext<'static>)) -> GpuPrimitive {
    let mut ctx = GpuPaintContext::new(200.0, 200.0);
    push(&mut ctx);
    ctx.fill_rect(
        Rect::new(0.0, 0.0, 100.0, 100.0),
        CornerRadius::ZERO,
        Color::RED.into(),
    );
    ctx.take_batch().primitives.remove(0)
}

#[test]
fn a_polygon_clip_is_nonzero_by_default() {
    let p = drawn_under(|ctx| ctx.push_clip(triangle()));
    assert_eq!(p.type_info[2], ClipType::Polygon as u32);
    assert_eq!(p.clip_radius[0], 0.0);
    assert_eq!(p.clip_radius[2], 3.0, "the vertex count survives");
}

#[test]
fn an_even_odd_polygon_clip_says_so_and_keeps_its_vertex_count() {
    let p = drawn_under(|ctx| {
        ctx.set_clip_fill_rule(FillRule::EvenOdd);
        ctx.push_clip(triangle());
    });
    assert_eq!(p.type_info[2], ClipType::Polygon as u32);
    assert_eq!(p.clip_radius[0], 1.0);
    assert_eq!(p.clip_radius[2], 3.0);
}

#[test]
fn the_rule_applies_to_the_next_clip_only() {
    let p = drawn_under(|ctx| {
        ctx.set_clip_fill_rule(FillRule::EvenOdd);
        ctx.push_clip(ClipShape::Rect(Rect::new(0.0, 0.0, 50.0, 50.0)));
        ctx.push_clip(triangle());
    });
    assert_eq!(p.clip_radius[0], 0.0);
}
