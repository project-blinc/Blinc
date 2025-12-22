//! SVG rendering tests
//!
//! Tests for SVG parsing and rendering: basic shapes, paths, gradients, strokes

use crate::runner::TestSuite;
use blinc_core::{DrawContext, Rect};
use blinc_svg::{SvgDocument, SvgDrawCommand};

/// Create the SVG test suite
pub fn suite() -> TestSuite {
    let mut suite = TestSuite::new("svg");

    // Basic rectangle SVG
    suite.add("svg_rect", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <rect x="50" y="50" width="200" height="150" fill="blue"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Rectangle with stroke
    suite.add("svg_rect_stroke", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <rect x="50" y="50" width="200" height="150" fill="lightblue" stroke="darkblue" stroke-width="4"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Circle SVG
    suite.add("svg_circle", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <circle cx="200" cy="150" r="100" fill="red"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Ellipse SVG
    suite.add("svg_ellipse", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <ellipse cx="200" cy="150" rx="150" ry="80" fill="green"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Line SVG
    suite.add("svg_line", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <line x1="50" y1="50" x2="350" y2="250" stroke="black" stroke-width="3"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Polyline SVG
    suite.add("svg_polyline", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <polyline points="50,250 100,50 200,200 300,100 350,150"
                          fill="none" stroke="purple" stroke-width="3"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Polygon SVG (star)
    suite.add("svg_polygon_star", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <polygon points="200,30 240,120 340,130 260,190 280,280 200,240 120,280 140,190 60,130 160,120"
                         fill="gold" stroke="orange" stroke-width="2"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Simple path SVG
    suite.add("svg_path_simple", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <path d="M50,50 L150,50 L150,150 L50,150 Z" fill="cyan" stroke="navy" stroke-width="2"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Path with curves (heart shape)
    suite.add("svg_path_heart", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <path d="M200,80
                         C160,40 80,40 80,120
                         C80,200 200,260 200,260
                         C200,260 320,200 320,120
                         C320,40 240,40 200,80 Z"
                      fill="red"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Path with quadratic curves
    suite.add("svg_path_quadratic", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <path d="M50,200 Q200,50 350,200" fill="none" stroke="blue" stroke-width="4"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Path with arc
    suite.add("svg_path_arc", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <path d="M100,150 A80,80 0 0,1 300,150" fill="none" stroke="green" stroke-width="4"/>
                <path d="M100,180 A80,80 0 1,0 300,180" fill="none" stroke="red" stroke-width="4"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Linear gradient
    suite.add("svg_gradient_linear", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <defs>
                    <linearGradient id="grad1" x1="0%" y1="0%" x2="100%" y2="0%">
                        <stop offset="0%" style="stop-color:rgb(255,255,0);stop-opacity:1"/>
                        <stop offset="100%" style="stop-color:rgb(255,0,0);stop-opacity:1"/>
                    </linearGradient>
                </defs>
                <rect x="50" y="50" width="300" height="200" fill="url(#grad1)"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Radial gradient
    suite.add("svg_gradient_radial", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <defs>
                    <radialGradient id="grad2" cx="50%" cy="50%" r="50%">
                        <stop offset="0%" style="stop-color:white;stop-opacity:1"/>
                        <stop offset="100%" style="stop-color:blue;stop-opacity:1"/>
                    </radialGradient>
                </defs>
                <circle cx="200" cy="150" r="100" fill="url(#grad2)"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Stroke dash array
    suite.add("svg_stroke_dash", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <line x1="50" y1="100" x2="350" y2="100" stroke="black" stroke-width="3" stroke-dasharray="10,5"/>
                <line x1="50" y1="150" x2="350" y2="150" stroke="red" stroke-width="3" stroke-dasharray="20,10,5,10"/>
                <line x1="50" y1="200" x2="350" y2="200" stroke="blue" stroke-width="3" stroke-dasharray="1,5"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Stroke line cap
    suite.add("svg_stroke_linecap", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <line x1="50" y1="80" x2="350" y2="80" stroke="black" stroke-width="20" stroke-linecap="butt"/>
                <line x1="50" y1="150" x2="350" y2="150" stroke="black" stroke-width="20" stroke-linecap="round"/>
                <line x1="50" y1="220" x2="350" y2="220" stroke="black" stroke-width="20" stroke-linecap="square"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Stroke line join
    suite.add("svg_stroke_linejoin", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <polyline points="50,250 100,50 150,250" fill="none" stroke="red" stroke-width="20" stroke-linejoin="miter"/>
                <polyline points="150,250 200,50 250,250" fill="none" stroke="green" stroke-width="20" stroke-linejoin="round"/>
                <polyline points="250,250 300,50 350,250" fill="none" stroke="blue" stroke-width="20" stroke-linejoin="bevel"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Opacity
    suite.add("svg_opacity", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <rect x="50" y="50" width="150" height="150" fill="blue"/>
                <rect x="100" y="100" width="150" height="150" fill="red" opacity="0.5"/>
                <circle cx="280" cy="150" r="80" fill="green" fill-opacity="0.7"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Multiple shapes composition
    suite.add("svg_composition", |ctx| {
        let c = ctx.ctx();

        let svg = r##"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <!-- Background -->
                <rect x="0" y="0" width="400" height="300" fill="#e0e0e0"/>

                <!-- Sun -->
                <circle cx="320" cy="60" r="40" fill="yellow" stroke="orange" stroke-width="3"/>

                <!-- House base -->
                <rect x="100" y="150" width="150" height="120" fill="brown"/>

                <!-- Roof -->
                <polygon points="75,150 175,80 275,150" fill="darkred"/>

                <!-- Door -->
                <rect x="150" y="200" width="50" height="70" fill="saddlebrown"/>

                <!-- Window -->
                <rect x="120" y="180" width="30" height="30" fill="lightblue" stroke="white" stroke-width="2"/>
                <rect x="200" y="180" width="30" height="30" fill="lightblue" stroke="white" stroke-width="2"/>

                <!-- Ground -->
                <rect x="0" y="270" width="400" height="30" fill="green"/>
            </svg>
        "##;

        render_svg(c, svg);
    });

    // Groups (nested elements)
    suite.add("svg_groups", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <g fill="blue" stroke="black" stroke-width="2">
                    <circle cx="100" cy="100" r="50"/>
                    <circle cx="200" cy="100" r="50"/>
                    <circle cx="300" cy="100" r="50"/>
                </g>
                <g fill="red">
                    <rect x="75" y="175" width="50" height="50"/>
                    <rect x="175" y="175" width="50" height="50"/>
                    <rect x="275" y="175" width="50" height="50"/>
                </g>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Complex path (Bézier curves logo-style)
    suite.add("svg_complex_path", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <path d="M50,150
                         C50,50 150,50 200,150
                         S350,250 350,150
                         C350,50 250,50 200,150
                         S50,250 50,150 Z"
                      fill="purple" fill-opacity="0.6" stroke="darkviolet" stroke-width="3"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Rounded rectangle
    suite.add("svg_rounded_rect", |ctx| {
        let c = ctx.ctx();

        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
                <rect x="50" y="50" width="300" height="200" rx="30" ry="30" fill="teal" stroke="darkcyan" stroke-width="4"/>
            </svg>
        "#;

        render_svg(c, svg);
    });

    // Font Awesome arrow icon (complex path with curves)
    suite.add("svg_fontawesome_arrow", |ctx| {
        let c = ctx.ctx();

        // Font Awesome share/arrow icon - original viewBox, rendered with render_fit
        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640">
                <path d="M371.8 82.4C359.8 87.4 352 99 352 112L352 192L240 192C142.8 192 64 270.8 64 368C64 481.3 145.5 531.9 164.2 542.1C166.7 543.5 169.5 544 172.3 544C183.2 544 192 535.1 192 524.3C192 516.8 187.7 509.9 182.2 504.8C172.8 496 160 478.4 160 448.1C160 395.1 203 352.1 256 352.1L352 352.1L352 432.1C352 445 359.8 456.7 371.8 461.7C383.8 466.7 397.5 463.9 406.7 454.8L566.7 294.8C579.2 282.3 579.2 262 566.7 249.5L406.7 89.5C397.5 80.3 383.8 77.6 371.8 82.6z" fill="black"/>
            </svg>
        "#;

        render_svg_fit(c, svg, Rect::new(50.0, 25.0, 300.0, 250.0));
    });

    // Lucide podcast icon (stroked icon with arcs and circles)
    suite.add("svg_lucide_podcast", |ctx| {
        let c = ctx.ctx();

        // Lucide podcast icon - uses stroke-based rendering
        // Note: "currentColor" is resolved to black by usvg
        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                <path d="M13 17a1 1 0 1 0-2 0l.5 4.5a0.5 0.5 0 0 0 1 0z" fill="black"/>
                <path d="M16.85 18.58a9 9 0 1 0-9.7 0"/>
                <path d="M8 14a5 5 0 1 1 8 0"/>
                <circle cx="12" cy="11" r="1" fill="black"/>
            </svg>
        "#;

        render_svg_fit(c, svg, Rect::new(100.0, 50.0, 200.0, 200.0));
    });

    suite
}

/// Helper function to render an SVG string to the draw context
fn render_svg(ctx: &mut dyn DrawContext, svg_str: &str) {
    match SvgDocument::from_str(svg_str) {
        Ok(doc) => {
            let commands = doc.commands();
            for cmd in commands {
                match cmd {
                    SvgDrawCommand::FillPath { path, brush } => {
                        ctx.fill_path(&path, brush);
                    }
                    SvgDrawCommand::StrokePath {
                        path,
                        stroke,
                        brush,
                    } => {
                        ctx.stroke_path(&path, &stroke, brush);
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to parse SVG: {}", e);
        }
    }
}

/// Helper function to render an SVG string scaled to fit a bounding rectangle
fn render_svg_fit(ctx: &mut dyn DrawContext, svg_str: &str, bounds: Rect) {
    match SvgDocument::from_str(svg_str) {
        Ok(doc) => {
            doc.render_fit(ctx, bounds);
        }
        Err(e) => {
            tracing::error!("Failed to parse SVG: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::TestHarness;

    #[test]
    #[ignore] // Requires GPU
    fn run_svg_suite() {
        let harness = TestHarness::new().unwrap();
        let mut suite = suite();

        for case in suite.cases.drain(..) {
            let result = harness.run_test(&case.name, case.test_fn).unwrap();
            assert!(result.is_passed(), "Test {} failed", case.name);
        }
    }
}
