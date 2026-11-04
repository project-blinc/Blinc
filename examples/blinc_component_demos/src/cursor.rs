//! Recording-only pointer, drawn after overlays without joining hit testing.
use blinc_core::{Brush, Color, DrawContext, Point, Stroke};
use blinc_layout::{
    canvas::canvas,
    prelude::{Div, div},
    svg::svg,
};

pub fn annotation(x: f32, y: f32, pressed: bool, pulse: Option<f32>) -> Div {
    let glow = canvas(move |ctx: &mut dyn DrawContext, _| {
        let cyan = Color::from_hex(0x22d3ee);
        for layer in 0..14 {
            ctx.fill_circle(
                Point::new(x + 6.0, y + 7.0),
                27.0 - layer as f32 * 1.5,
                Brush::Solid(cyan.with_alpha(0.018 + layer as f32 * 0.0015)),
            );
        }
        if let Some(age) = pulse {
            ctx.stroke_circle(
                Point::new(x, y),
                7.0 + 24.0 * age,
                &Stroke::new(1.6),
                Brush::Solid(cyan.with_alpha(0.8 * (1.0 - age))),
            );
        }
    })
    .w(560.0)
    .h(320.0)
    .absolute();
    let scale = if pressed { 0.94 } else { 1.0 };
    div().w(560.0).h(320.0).child(glow).child(
        div()
            .absolute()
            .left(x - 5.0 * scale)
            .top(y - 5.0 * scale)
            .child(svg(include_str!("../assets/cursor.svg")).size(30.0 * scale, 32.0 * scale)),
    )
}
