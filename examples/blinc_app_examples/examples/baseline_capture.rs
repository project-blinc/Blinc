//! Offscreen capture for baseline alignment and text vertical placement.
//!
//! Renders rows that mix font sizes and families under
//! `align-items: baseline`, plus cn controls whose text is vertically
//! centred, and writes a PNG. Used to see what the baseline pass and the
//! half-leading change do, rather than inferring it from numbers.
//!
//! cargo run -p blinc_app_examples --features cn --example baseline_capture -- out.png

use blinc_app::{BlincApp, BlincConfig};
use blinc_layout::prelude::*;
use blinc_theme::{ColorScheme, ColorToken, ThemeState};

const W: u32 = 900;
const H: u32 = 420;

/// A row of text that should share one baseline.
fn baseline_row(caption: &str, parts: Vec<Box<dyn ElementBuilder>>) -> impl ElementBuilder {
    let theme = ThemeState::get();
    div()
        .flex_col()
        .gap(1.0)
        .child(
            text(caption)
                .size(10.0)
                .color(theme.color(ColorToken::TextTertiary)),
        )
        .child(
            div()
                .flex_row()
                .items_baseline()
                .gap(2.0)
                .p(2.0)
                .bg(theme.color(ColorToken::Surface))
                .rounded(6.0)
                .children(parts),
        )
}

fn scene() -> impl ElementBuilder {
    let theme = ThemeState::get();
    let fg = theme.color(ColorToken::TextPrimary);
    let accent = theme.color(ColorToken::Primary);

    div()
        .w(W as f32)
        .h(H as f32)
        .bg(theme.color(ColorToken::Background))
        .flex_col()
        .gap(3.0)
        .p(5.0)
        .child(text("baseline alignment").size(18.0).color(fg).bold())
        // The case from the report: body text with inline code.
        .child(baseline_row(
            "16px sans + 14px mono, 1px padding",
            vec![
                Box::new(text("paragraph with").size(16.0).color(fg).no_wrap()),
                Box::new(
                    div()
                        .flex_shrink_0()
                        .p(1.0)
                        .bg(theme.color(ColorToken::SurfaceElevated))
                        .rounded(3.0)
                        .child(text("code").size(14.0).monospace().color(accent).no_wrap()),
                ),
                Box::new(text("inline").size(16.0).color(fg).no_wrap()),
            ],
        ))
        // A wide size spread makes any misalignment obvious.
        .child(baseline_row(
            "32px / 12px / 20px sans",
            vec![
                Box::new(text("Hxq").size(32.0).color(fg).no_wrap()),
                Box::new(text("Hxq").size(12.0).color(fg).no_wrap()),
                Box::new(text("Hxq").size(20.0).color(accent).no_wrap()),
            ],
        ))
        // Mixed families at one size isolates ascender differences.
        .child(baseline_row(
            "same size, sans + mono",
            vec![
                Box::new(text("Hxq sans").size(20.0).color(fg).no_wrap()),
                Box::new(
                    text("Hxq mono")
                        .size(20.0)
                        .monospace()
                        .color(accent)
                        .no_wrap(),
                ),
            ],
        ))
        // Half-leading moves every text element, so watch centring too.
        .child(
            div()
                .flex_row()
                .items_center()
                .gap(2.0)
                .child(
                    div()
                        .flex_shrink_0()
                        .px(4.0)
                        .py(2.0)
                        .bg(accent)
                        .rounded(6.0)
                        .child(
                            text("Button")
                                .size(14.0)
                                .color(theme.color(ColorToken::TextInverse))
                                .no_wrap(),
                        ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .px(4.0)
                        .py(2.0)
                        .border(1.0, theme.color(ColorToken::Border))
                        .rounded(6.0)
                        .child(text("Outlined").size(14.0).color(fg).no_wrap()),
                )
                .child(
                    text("vertical centring check")
                        .size(12.0)
                        .color(theme.color(ColorToken::TextSecondary))
                        .no_wrap(),
                ),
        )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "baseline_capture.png".to_string());

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    ThemeState::init(
        blinc_theme::themes::universal::HybridTheme::bundle(),
        ColorScheme::Dark,
    );
    blinc_layout::text_measurer::init_text_measurer();

    let mut app = BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })?;

    // Repeat the walk so anything epoch-driven (the glyph atlas GC) has
    // previous walks to act on. One render exercises none of it.
    let walks: u32 = std::env::args()
        .nth(2)
        .and_then(|n| n.parse().ok())
        .unwrap_or(1);
    let mut pixels = Vec::new();
    for _ in 0..walks {
        pixels = app.render_to_rgba8(&scene(), W, H)?;
    }

    let img: image::RgbaImage = image::ImageBuffer::from_raw(W, H, pixels)
        .ok_or("pixel buffer did not match the requested size")?;
    img.save(&out)?;
    eprintln!("wrote {out}");
    Ok(())
}
