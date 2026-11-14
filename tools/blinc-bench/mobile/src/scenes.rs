//! Benchmark scenes, chosen by the `BLINC_BENCH` parameter (see [`crate::param`]): `cards`, `cards_plain`,
//! `list`, `spinners` or `flow`. Each animates by itself; `list` is driven by the runner's
//! scripted touches (`BLINC_BENCH_SCROLL`).

use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;
use blinc_cn as cn;
use blinc_core::Color;

/// Appended to the example's stylesheet.
pub const CSS: &str = r#"
@keyframes bench-bob {
  from { transform: translateY(0px) scale(1); opacity: 1; }
  to { transform: translateY(-4px) scale(1.06); opacity: 0.8; }
}
.bench-card { animation: bench-bob 800ms ease-in-out infinite alternate; }
"#;

pub fn scene(name: &str, ctx: &WindowedContext) -> Option<Div> {
    let body = match name {
        "cards" => cards(true),
        "cards_plain" => cards(false),
        "list" => list(ctx),
        "spinners" => spinners(),
        "flow" => flow(ctx),
        _ => return None,
    };
    Some(
        div()
            .w(ctx.width)
            .h(ctx.height)
            .bg(Color::rgb(0.07, 0.08, 0.1))
            .pt(ctx.safe_top())
            .child(body),
    )
}

/// Small cards, 200 unless `BLINC_BENCH_CARDS` says otherwise, each easing transform and opacity
/// through a CSS animation.
fn cards(with_text: bool) -> Div {
    let count = crate::param("BLINC_BENCH_CARDS")
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    (0..count).fold(
        div().w_full().flex_row().flex_wrap().gap(0.5).p(1.0),
        |grid, i| {
            let tint = (i % 7) as f32 / 7.0;
            let card = div()
                .class("bench-card")
                .w(38.0)
                .h(24.0)
                .rounded(5.0)
                .bg(Color::rgba(0.15 + tint * 0.3, 0.3, 0.6 - tint * 0.3, 1.0))
                .items_center()
                .justify_center();
            grid.child(if with_text {
                card.child(text(format!("{i}")).size(9.0).color(Color::WHITE))
            } else {
                card
            })
        },
    )
}

/// 2,000 rows in a scroll view.
fn list(ctx: &WindowedContext) -> Div {
    let rows = (0..2_000).fold(div().w_full().flex_col(), |col, i| {
        col.child(
            div()
                .w_full()
                .h(44.0)
                .flex_shrink_0()
                .flex_row()
                .items_center()
                .px(4.0)
                .bg(Color::rgb(0.1 + (i % 5) as f32 * 0.02, 0.12, 0.16))
                .child(text(format!("Row {i}")).size(16.0).color(Color::WHITE)),
        )
    });
    div()
        .w(ctx.width)
        .h(ctx.height)
        .child(scroll().w(ctx.width).h(ctx.height).child(rows))
}

/// Three spinners on an otherwise idle screen.
fn spinners() -> Div {
    div()
        .w_full()
        .h(300.0)
        .flex_row()
        .items_center()
        .justify_center()
        .gap(4.0)
        .child(cn::spinner())
        .child(cn::spinner())
        .child(cn::spinner())
}

/// A flow shader that reads time, in a canvas.
fn flow(ctx: &WindowedContext) -> Div {
    let shader = blinc_layout::flow_shader(
        "@flow m { target: fragment; input time: builtin(time); input uv: builtin(uv); \
         output color = vec4(sin(time) * 0.5 + 0.5, uv.x, uv.y, 1.0); }",
    )
    .expect("flow shader");
    div()
        .w_full()
        .child(canvas(|_, _| {}).flow(shader).w(ctx.width).h(300.0))
}
