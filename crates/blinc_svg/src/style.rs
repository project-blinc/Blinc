//! SVG style conversion to Blinc types

use blinc_core::{Brush, Color, Gradient, GradientStop, LineCap, LineJoin, Stroke};

/// Convert usvg Paint to Blinc Brush
pub fn paint_to_brush(paint: &usvg::Paint, opacity: f32) -> Option<Brush> {
    match paint {
        usvg::Paint::Color(color) => Some(Brush::Solid(Color::rgba(
            color.red as f32 / 255.0,
            color.green as f32 / 255.0,
            color.blue as f32 / 255.0,
            opacity,
        ))),
        usvg::Paint::LinearGradient(lg) => {
            let stops: Vec<GradientStop> = lg
                .stops()
                .iter()
                .map(|s| GradientStop {
                    offset: s.offset().get(),
                    color: Color::rgba(
                        s.color().red as f32 / 255.0,
                        s.color().green as f32 / 255.0,
                        s.color().blue as f32 / 255.0,
                        s.opacity().get() * opacity,
                    ),
                })
                .collect();

            let gradient = Gradient::linear_with_stops(
                blinc_core::Point::new(lg.x1() as f32, lg.y1() as f32),
                blinc_core::Point::new(lg.x2() as f32, lg.y2() as f32),
                stops,
            );
            Some(Brush::Gradient(gradient))
        }
        usvg::Paint::RadialGradient(rg) => {
            let stops: Vec<GradientStop> = rg
                .stops()
                .iter()
                .map(|s| GradientStop {
                    offset: s.offset().get(),
                    color: Color::rgba(
                        s.color().red as f32 / 255.0,
                        s.color().green as f32 / 255.0,
                        s.color().blue as f32 / 255.0,
                        s.opacity().get() * opacity,
                    ),
                })
                .collect();

            let gradient = Gradient::radial_with_stops(
                blinc_core::Point::new(rg.cx() as f32, rg.cy() as f32),
                rg.r().get() as f32,
                stops,
            );
            Some(Brush::Gradient(gradient))
        }
        usvg::Paint::Pattern(_) => {
            // Patterns not yet supported - use fallback color
            Some(Brush::Solid(Color::rgba(0.5, 0.5, 0.5, opacity)))
        }
    }
}

/// Convert usvg Fill to Blinc Brush
pub fn fill_to_brush(fill: &usvg::Fill) -> Option<Brush> {
    paint_to_brush(fill.paint(), fill.opacity().get())
}

/// Convert usvg Stroke to Blinc Stroke and Brush
pub fn stroke_to_blinc(stroke: &usvg::Stroke) -> Option<(Stroke, Brush)> {
    let brush = paint_to_brush(stroke.paint(), stroke.opacity().get())?;

    let cap = match stroke.linecap() {
        usvg::LineCap::Butt => LineCap::Butt,
        usvg::LineCap::Round => LineCap::Round,
        usvg::LineCap::Square => LineCap::Square,
    };

    let join = match stroke.linejoin() {
        usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => LineJoin::Miter,
        usvg::LineJoin::Round => LineJoin::Round,
        usvg::LineJoin::Bevel => LineJoin::Bevel,
    };

    let mut blinc_stroke = Stroke::new(stroke.width().get() as f32)
        .with_cap(cap)
        .with_join(join);

    // Handle dash pattern
    if let Some(dasharray) = stroke.dasharray() {
        let dashes: Vec<f32> = dasharray.iter().map(|&d| d as f32).collect();
        blinc_stroke = blinc_stroke.with_dash(dashes, stroke.dashoffset() as f32);
    }

    Some((blinc_stroke, brush))
}
