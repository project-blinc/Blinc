//! Drawing samples adapted from the application Canvas example.

use crate::demos::DemoState;
use blinc_core::{
    Brush, Color, CornerRadius, DrawContext, Gradient, Point, Rect, TextAlign, TextBaseline,
    TextStyle,
};
use blinc_layout::prelude::*;

pub fn canvas_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div()
        .class("demo-canvas-examples")
        .max_w(600.0)
        .overflow_y_scroll()
        .child(sample("Gradient fill", gradient_canvas()))
        .child(sample("Layered shapes", nested_shapes_canvas()))
        .child(sample("Colour palette", color_palette_canvas()))
        .child(sample(
            "Progress and canvas text",
            progress_bar_canvas(0.65),
        ))
}

fn sample(label: &'static str, drawing: Canvas) -> Div {
    div()
        .class("demo-canvas-sample")
        .child(text(label).class("demo-label"))
        .child(div().class("demo-canvas-artwork").child(drawing))
}

fn gradient_canvas() -> Canvas {
    canvas(|ctx: &mut dyn DrawContext, bounds| {
        let gradient = Brush::Gradient(Gradient::linear(
            Point::new(0.0, bounds.height / 2.0),
            Point::new(bounds.width, bounds.height / 2.0),
            Color::rgba(0.9, 0.2, 0.5, 1.0),
            Color::rgba(0.9, 0.5, 0.2, 1.0),
        ));

        ctx.fill_rect(
            Rect::new(0.0, 0.0, bounds.width, bounds.height),
            CornerRadius::uniform(8.0),
            gradient,
        );
    })
    .w(228.0)
    .h(120.0)
}

fn nested_shapes_canvas() -> Canvas {
    canvas(|ctx: &mut dyn DrawContext, bounds| {
        let cx = bounds.width / 2.0;
        let cy = bounds.height / 2.0;

        let colors = [
            Color::rgba(0.2, 0.3, 0.8, 0.9),
            Color::rgba(0.3, 0.6, 0.9, 0.8),
            Color::rgba(0.4, 0.8, 0.9, 0.7),
            Color::rgba(0.6, 0.9, 0.8, 0.6),
        ];

        for (i, color) in colors.iter().enumerate() {
            let size = (4 - i) as f32 * 24.0;
            ctx.fill_rect(
                Rect::new(cx - size / 2.0, cy - size / 2.0, size, size),
                CornerRadius::uniform(4.0 + i as f32 * 2.0),
                Brush::Solid(*color),
            );
        }
    })
    .w(228.0)
    .h(120.0)
}

fn color_palette_canvas() -> Canvas {
    canvas(|ctx: &mut dyn DrawContext, bounds| {
        let cols = 8;
        let rows = 3;
        let cell_w = bounds.width / cols as f32;
        let cell_h = bounds.height / rows as f32;
        let gap = 2.0;

        for row in 0..rows {
            for col in 0..cols {
                let hue = col as f32 / cols as f32;
                let sat = 1.0 - (row as f32 * 0.25);
                let val = 0.9 - (row as f32 * 0.15);

                let color = hsv_to_rgb(hue, sat, val);

                let x = col as f32 * cell_w + gap / 2.0;
                let y = row as f32 * cell_h + gap / 2.0;

                ctx.fill_rect(
                    Rect::new(x, y, cell_w - gap, cell_h - gap),
                    CornerRadius::uniform(3.0),
                    Brush::Solid(color),
                );
            }
        }
    })
    .w(228.0)
    .h(90.0)
}

fn progress_bar_canvas(progress: f32) -> Canvas {
    canvas(move |ctx: &mut dyn DrawContext, bounds| {
        let bar_height = 20.0;
        let bar_y = (bounds.height - bar_height) / 2.0;
        let radius = CornerRadius::uniform(bar_height / 2.0);

        ctx.fill_rect(
            Rect::new(0.0, bar_y, bounds.width, bar_height),
            radius,
            Brush::Solid(Color::rgba(0.2, 0.2, 0.25, 1.0)),
        );

        let fill_width = bounds.width * progress.clamp(0.0, 1.0);
        if fill_width > 0.0 {
            let gradient = Brush::Gradient(Gradient::linear(
                Point::new(0.0, bar_y),
                Point::new(fill_width, bar_y),
                Color::rgba(0.4, 0.6, 1.0, 1.0),
                Color::rgba(0.6, 0.4, 1.0, 1.0),
            ));
            ctx.fill_rect(
                Rect::new(0.0, bar_y, fill_width, bar_height),
                radius,
                gradient,
            );
        }

        let percent = (progress * 100.0) as i32;
        let bubble = Rect::new(bounds.width / 2.0 - 25.0, bar_y - 28.0, 50.0, 22.0);
        ctx.fill_rect(
            bubble,
            CornerRadius::uniform(4.0),
            Brush::Solid(Color::rgba(0.1, 0.1, 0.15, 0.9)),
        );

        ctx.draw_text(
            &format!("{}%", percent),
            Point::new(
                bubble.x() + bubble.width() / 2.0,
                bubble.y() + bubble.height() / 2.0,
            ),
            &TextStyle::new(13.0)
                .with_color(Color::WHITE)
                .with_align(TextAlign::Center)
                .with_baseline(TextBaseline::Middle),
        );
    })
    .w(228.0)
    .h(80.0)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Color {
    let c = v * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = match (h * 6.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    Color::rgba(r + m, g + m, b + m, 1.0)
}
