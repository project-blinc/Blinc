//! Canvas Element Demo
//!
//! This example demonstrates the canvas element for custom GPU drawing
//! within the layout system.
//!
//! Features demonstrated:
//! - Custom 2D drawing with DrawContext
//! - Canvas respects layout transforms and clipping
//! - Procedural graphics (animated shapes, patterns)
//! - Canvas for cursor/indicator rendering
//!
//! Run with: cargo run -p blinc_app --example canvas_demo --features windowed

use blinc_animation::{AnimatedValue, SpringConfig};
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::{
    Brush, Color, CornerRadius, DrawContext, Gradient, GradientStop, Point, Rect, TextStyle,
};
use std::cell::RefCell;
use std::rc::Rc;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Canvas Element Demo".to_string(),
        width: 900,
        height: 700,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
        .flex_col()
        .gap(20.0)
        .p(30.0)
        // Title
        .child(text("Canvas Element Demo").size(28.0).color(Color::WHITE))
        .child(
            text("Custom GPU drawing within the layout system")
                .size(14.0)
                .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
        )
        // Demo grid
        .child(
            div()
                .w_fit()
                .flex_row()
                .flex_wrap()
                .gap(20.0)
                .child(demo_card("Simple Rectangle", simple_rectangle_canvas()))
                .child(demo_card("Gradient Fill", gradient_canvas()))
                .child(demo_card("Nested Shapes", nested_shapes_canvas()))
                .child(demo_card("Custom Cursor", cursor_demo_canvas()))
                .child(demo_card("Progress Bar", progress_bar_canvas(0.65)))
                .child(demo_card("Color Palette", color_palette_canvas()))
                .child(animated_demo_card(ctx)),
        )
}

/// Wraps a canvas in a demo card with a title
fn demo_card(title: &'static str, canvas_element: Canvas) -> Div {
    div()
        .w(300.0)
        .p(16.0) // Uniform padding on all sides
        .flex_col()
        .justify_center()
        .items_center()
        .gap(8.0)
        .bg(Color::rgba(0.12, 0.12, 0.16, 1.0))
        .rounded(12.0)
        .overflow_clip() // Clip children to card bounds
        .child(
            text(title)
                .size(14.0)
                .color(Color::rgba(0.8, 0.8, 0.9, 1.0)),
        )
        .child(canvas_element)
}

/// Demo 1: Simple filled rectangle
fn simple_rectangle_canvas() -> Canvas {
    canvas(|ctx: &mut dyn DrawContext, bounds| {
        // Fill with a blue rectangle
        ctx.fill_rect(
            Rect::new(10.0, 10.0, bounds.width - 20.0, bounds.height - 20.0),
            CornerRadius::uniform(8.0),
            Brush::Solid(Color::rgba(0.3, 0.5, 0.9, 1.0)),
        );
    })
    .w(228.0)
    .h(120.0)
}

/// Demo 2: Gradient fill
fn gradient_canvas() -> Canvas {
    canvas(|ctx: &mut dyn DrawContext, bounds| {
        // Create a horizontal gradient
        let gradient = Brush::Gradient(Gradient::linear_with_stops(
            Point::new(0.0, bounds.height / 2.0),
            Point::new(bounds.width, bounds.height / 2.0),
            vec![
                GradientStop::new(0.0, Color::rgba(0.9, 0.2, 0.5, 1.0)),
                GradientStop::new(0.5, Color::rgba(0.9, 0.5, 0.2, 1.0)),
                GradientStop::new(1.0, Color::rgba(0.2, 0.8, 0.6, 1.0)),
            ],
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

/// Demo 3: Nested shapes
fn nested_shapes_canvas() -> Canvas {
    canvas(|ctx: &mut dyn DrawContext, bounds| {
        let cx = bounds.width / 2.0;
        let cy = bounds.height / 2.0;

        // Draw concentric rectangles
        let colors = [
            Color::rgba(0.2, 0.3, 0.8, 0.9),
            Color::rgba(0.3, 0.6, 0.9, 0.8),
            Color::rgba(0.4, 0.8, 0.9, 0.7),
            Color::rgba(0.6, 0.9, 0.8, 0.6),
        ];

        for (i, color) in colors.iter().enumerate() {
            let offset = i as f32 * 12.0;
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

/// Demo 4: Custom cursor indicator (like in text inputs)
fn cursor_demo_canvas() -> Canvas {
    canvas(|ctx: &mut dyn DrawContext, bounds| {
        // Background
        ctx.fill_rect(
            Rect::new(0.0, 0.0, bounds.width, bounds.height),
            CornerRadius::uniform(6.0),
            Brush::Solid(Color::rgba(0.15, 0.15, 0.2, 1.0)),
        );

        // Simulated text (horizontal lines)
        let text_color = Color::rgba(0.7, 0.7, 0.8, 1.0);
        for i in 0..3 {
            let y = 20.0 + i as f32 * 25.0;
            let width = if i == 2 { 80.0 } else { 180.0 };
            ctx.fill_rect(
                Rect::new(15.0, y, width, 12.0),
                CornerRadius::uniform(2.0),
                Brush::Solid(text_color),
            );
        }

        // Blinking cursor (just draw it solid for demo)
        let cursor_x = 100.0;
        let cursor_y = 15.0;
        let cursor_height = bounds.height - 30.0;
        ctx.fill_rect(
            Rect::new(cursor_x, cursor_y, 2.0, cursor_height),
            CornerRadius::default(),
            Brush::Solid(Color::rgba(0.4, 0.6, 1.0, 1.0)),
        );
    })
    .w(228.0)
    .h(100.0)
}

/// Demo 5: Progress bar with custom styling
fn progress_bar_canvas(progress: f32) -> Canvas {
    canvas(move |ctx: &mut dyn DrawContext, bounds| {
        let bar_height = 20.0;
        let bar_y = (bounds.height - bar_height) / 2.0;
        let radius = CornerRadius::uniform(bar_height / 2.0);

        // Background track
        ctx.fill_rect(
            Rect::new(0.0, bar_y, bounds.width, bar_height),
            radius,
            Brush::Solid(Color::rgba(0.2, 0.2, 0.25, 1.0)),
        );

        // Progress fill with gradient
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

        // Progress percentage indicator with text
        let percent = (progress * 100.0) as i32;
        let text_x = bounds.width / 2.0 - 15.0;
        let text_bg = Rect::new(text_x - 5.0, bar_y - 25.0, 50.0, 18.0);
        ctx.fill_rect(
            text_bg,
            CornerRadius::uniform(4.0),
            Brush::Solid(Color::rgba(0.1, 0.1, 0.15, 0.9)),
        );

        // Draw the percentage text
        ctx.draw_text(
            &format!("{}%", percent),
            Point::new(text_x, bar_y),
            &TextStyle::new(18.0).with_color(Color::WHITE),
        );
    })
    .w(228.0)
    .h(80.0)
}

/// Demo 6: Color palette grid
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

                // Convert HSV to RGB (simplified)
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

/// Simple HSV to RGB conversion
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

/// Demo 7: Animated bouncing ball using AnimatedValue
///
/// Uses the built-in AnimatedValue wrapper which handles spring management.
/// With Rc<RefCell> - no thread-safety overhead since UI is single-threaded.
fn animated_demo_card(ctx: &WindowedContext) -> Div {
    // AnimatedValue manages the spring internally
    let ball_x = Rc::new(RefCell::new(AnimatedValue::new(
        ctx.animation_handle(),
        20.0,
        SpringConfig::wobbly(),
    )));

    // Track toggle state
    let is_right = Rc::new(RefCell::new(false));

    let render_ball_x = Rc::clone(&ball_x);
    let click_ball_x = Rc::clone(&ball_x);
    let click_is_right = Rc::clone(&is_right);

    div()
        .w(300.0)
        .p(16.0)
        .flex_col()
        .justify_center()
        .items_center()
        .gap(8.0)
        .bg(Color::rgba(0.12, 0.12, 0.16, 1.0))
        .rounded(12.0)
        .overflow_clip()
        .child(
            text("Animated (Click!)")
                .size(14.0)
                .color(Color::rgba(0.8, 0.8, 0.9, 1.0)),
        )
        .child(
            canvas(move |ctx: &mut dyn DrawContext, bounds| {
                // Get current animated value - AnimatedValue handles all the complexity
                let current_x = render_ball_x.borrow().get();

                // Draw track
                let track_y = bounds.height / 2.0;
                ctx.fill_rect(
                    Rect::new(10.0, track_y - 2.0, bounds.width - 20.0, 4.0),
                    CornerRadius::uniform(2.0),
                    Brush::Solid(Color::rgba(0.2, 0.2, 0.25, 1.0)),
                );

                // Draw bouncing ball
                let ball_size = 24.0;
                let ball_y = track_y - ball_size / 2.0;
                ctx.fill_rect(
                    Rect::new(current_x, ball_y, ball_size, ball_size),
                    CornerRadius::uniform(ball_size / 2.0),
                    Brush::Gradient(Gradient::linear(
                        Point::new(current_x, ball_y),
                        Point::new(current_x + ball_size, ball_size + ball_y),
                        Color::rgba(0.9, 0.4, 0.3, 1.0),
                        Color::rgba(0.9, 0.6, 0.2, 1.0),
                    )),
                );
            })
            .w(228.0)
            .h(80.0),
        )
        .on_click(move |_| {
            // Toggle direction
            let mut is_right = click_is_right.borrow_mut();
            *is_right = !*is_right;
            let new_target = if *is_right { 194.0 } else { 20.0 };

            // Set new target - AnimatedValue handles the spring
            click_ball_x.borrow_mut().set_target(new_target);
        })
}
