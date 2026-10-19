//! Mixed variable-length payloads shared by CPU and GPU routing tests.
use blinc_core::{
    Brush, ClipShape, Color, CornerRadius, DrawContext, Gradient, GradientStop, Path, Point, Rect,
};
use blinc_gpu::{GpuPaintContext, PrimitiveBatch, primitives::ShapeDesc};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Route {
    Static,
    Dynamic,
    Composite,
}

pub const SHAPE: ShapeDesc = ShapeDesc {
    offset: [0.0; 4],
    params: [1.0, 20.0, 0.0, 0.0],
    half_ext: [18.0, 20.0, 10.0, 0.0],
    color: [0.2, 0.7, 0.3, 1.0],
};

pub fn scene(route: Route, foreground: bool) -> (PrimitiveBatch, PrimitiveBatch) {
    let mut ctx = GpuPaintContext::new(200.0, 120.0);
    // Give the static batch unrelated payload before entering the routed scope.
    ctx.fill_path(
        &Path::rect(Rect::new(300.0, 300.0, 10.0, 10.0)),
        Color::BLUE.into(),
    );
    if route != Route::Static {
        ctx.push_motion_subtree();
    }
    if route == Route::Composite {
        ctx.push_composite_layer(19);
    }
    ctx.set_foreground(foreground);
    ctx.push_clip(ClipShape::Polygon(vec![
        Point::new(0.0, 0.0),
        Point::new(200.0, 0.0),
        Point::new(200.0, 120.0),
        Point::new(0.0, 120.0),
    ]));
    let triangle = |x: f32| {
        Path::new()
            .move_to(x, 20.0)
            .line_to(x + 60.0, 20.0)
            .line_to(x, 80.0)
            .close()
    };
    ctx.fill_path(&triangle(10.0), Color::RED.into());
    ctx.fill_path(
        &triangle(80.0),
        Brush::Gradient(Gradient::linear_with_stops(
            Point::new(80.0, 20.0),
            Point::new(140.0, 20.0),
            vec![
                GradientStop::new(0.0, Color::RED),
                GradientStop::new(0.5, Color::GREEN),
                GradientStop::new(1.0, Color::BLUE),
            ],
        )),
    );
    ctx.set_3d_shape(6.0, 20.0, 1.0, 256.0);
    ctx.set_3d_light([1.0, 0.0, 0.0], 0.0);
    ctx.set_3d_group(&[SHAPE]);
    ctx.fill_rect(
        Rect::new(150.0, 30.0, 40.0, 48.0),
        CornerRadius::ZERO,
        Color::WHITE.into(),
    );
    ctx.clear_3d();
    ctx.pop_clip();
    ctx.fill_path(
        &Path::rect(Rect::new(15.0, 90.0, 40.0, 20.0)),
        Color::BLUE.into(),
    );
    ctx.set_foreground(false);
    if route == Route::Composite {
        ctx.pop_composite_layer();
    }
    if route != Route::Static {
        ctx.pop_motion_subtree();
    }
    let routed = match route {
        Route::Static => ctx.take_batch(),
        Route::Dynamic => ctx.take_dynamic_batch(),
        Route::Composite => {
            assert!(ctx.take_dynamic_batch().is_empty());
            ctx.take_composite_layer_batches().remove(&19).unwrap()
        }
    };
    (ctx.take_batch(), routed)
}
