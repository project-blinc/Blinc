//! @flow Shader Demo
//!
//! Demonstrates the @flow DAG-based shader system.
//! Custom GPU fragment shaders are written as `@flow` blocks, parsed into a graph and
//! drawn by a canvas with `Canvas::flow`.
//!
//! Run with: cargo run -p blinc_app_examples --example flow_demo

use blinc_app::prelude::*;
use blinc_app::windowed::WindowedApp;
use blinc_app::windowed::WindowedContext;
use blinc_core::{Color, FlowGraph};
use blinc_layout::parse_flow_string;

const RIPPLE: &str = r#"
    @flow ripple {
        target: fragment;
        input uv: builtin(uv);
        input time: builtin(time);

        node centered = uv - vec2(0.5, 0.5);
        node dist = length(centered);
        node wave = sin(dist * 30.0 - time * 4.0) * 0.5 + 0.5;
        node falloff = smoothstep(0.5, 0.0, dist);
        node intensity = wave * falloff;

        output color = vec4(intensity * 0.3, intensity * 0.6, intensity, intensity * 0.9);
    }
"#;

const PLASMA: &str = r#"
    @flow plasma {
        target: fragment;
        input uv: builtin(uv);
        input time: builtin(time);

        node t = time * 0.8;
        node px = uv.x * 6.0;
        node py = uv.y * 6.0;
        node v1 = sin(px + t);
        node v2 = sin(py + t * 0.7);
        node v3 = sin(px + py + t * 0.5);
        node v4 = sin(length(vec2(px - 3.0, py - 3.0)) * 1.5 - t);
        node v = (v1 + v2 + v3 + v4) * 0.25 + 0.5;

        output color = vec4(
            sin(v * 3.14159) * 0.5 + 0.5,
            sin(v * 3.14159 + 2.094) * 0.5 + 0.5,
            sin(v * 3.14159 + 4.189) * 0.5 + 0.5,
            0.85
        );
    }
"#;

const PULSE: &str = r#"
    @flow pulse {
        target: fragment;
        input uv: builtin(uv);
        input time: builtin(time);

        node centered = uv - vec2(0.5, 0.5);
        node dist = length(centered);
        node pulse_size = sin(time * 2.0) * 0.1 + 0.35;
        node edge = smoothstep(pulse_size + 0.05, pulse_size - 0.05, dist);

        output color = vec4(0.2 * edge, 0.8 * edge, 0.6 * edge, edge * 0.9);
    }
"#;

const SDF_CIRCLE: &str = r#"
    @flow sdf_circle {
        target: fragment;
        input uv: builtin(uv);
        input time: builtin(time);

        node p = uv * 2.0 - vec2(1.0, 1.0);
        node d = length(p) - 0.6;
        node ring = abs(d) - 0.02;
        node glow = smoothstep(0.05, 0.0, ring);
        node hue = time * 0.5;
        node r = sin(hue) * 0.5 + 0.5;
        node g = sin(hue + 2.094) * 0.5 + 0.5;
        node b = sin(hue + 4.189) * 0.5 + 0.5;

        output color = vec4(r * glow, g * glow, b * glow, glow);
    }
"#;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let config = WindowConfig {
        title: "@flow Shader Demo".to_string(),
        width: 1000,
        height: 700,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, build_ui)
}

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    let card = |source: &str, title: &str, desc: &str| {
        let graph = parse_flow_string(source).expect("flow parses");
        flow_card(graph, title, desc)
    };

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.05, 0.05, 0.08, 1.0))
        .flex_col()
        .items_center()
        .justify_center()
        .gap(20.0)
        .child(
            h1("@flow Shader Demo")
                .size(28.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE),
        )
        .child(
            text("Custom GPU shaders drawn by a canvas")
                .size(14.0)
                .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
        )
        .child(
            scroll()
                .h_full()
                .w_full()
                .direction(ScrollDirection::Horizontal)
                .child(
                    div()
                        .flex_row()
                        .gap_px(20.0)
                        .p(10.0)
                        .child(card(RIPPLE, "Ripple", "Concentric ring waves"))
                        .child(card(PLASMA, "Plasma", "Classic plasma effect"))
                        .child(card(PULSE, "Pulse", "Breathing radial gradient"))
                        .child(card(SDF_CIRCLE, "SDF Circle", "Distance field ring")),
                ),
        )
}

fn flow_card(graph: FlowGraph, title: &str, desc: &str) -> impl ElementBuilder + use<> {
    div()
        .flex_col()
        .flex_shrink_0()
        .items_center()
        .gap(8.0)
        .child(
            // The shader renders over the canvas's bounds.
            canvas(|_, _| {})
                .w(200.0)
                .h(200.0)
                .rounded(16.0)
                .flow(graph),
        )
        .child(
            h2(title)
                .size(16.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE),
        )
        .child(text(desc).size(12.0).color(Color::rgba(0.5, 0.5, 0.6, 1.0)))
}
