//! Offscreen capture of bordered notch shapes.
//!
//! A notch is a union of a body and added pieces (concave flares, a
//! bulge cap, a peak triangle). Where a piece shares an edge with the
//! body exactly, the union's interior distance near that edge measures
//! to the piece's own edge, which is not part of the outline, so a
//! border derived from the distance rings the join.
//!
//! cargo run -p blinc_app_examples --features cn --example notch_border_capture -- out.png

use blinc_app::{BlincApp, BlincConfig};
use blinc_layout::notch::notch;
use blinc_layout::prelude::*;
use blinc_theme::{ColorScheme, ColorToken, ThemeState};

const W: u32 = 880;
const H: u32 = 420;

fn labelled(caption: &str, shape: impl ElementBuilder + 'static) -> impl ElementBuilder {
    let theme = ThemeState::get();
    div()
        .flex_col()
        .gap(2.0)
        .child(
            text(caption)
                .size(11.0)
                .color(theme.color(ColorToken::TextTertiary))
                .no_wrap(),
        )
        .child(shape)
}

fn scene() -> impl ElementBuilder {
    let theme = ThemeState::get();
    let fill = theme.color(ColorToken::Surface);
    // A bright border makes any line across a join obvious.
    let edge = theme.color(ColorToken::Primary);

    div()
        .w(W as f32)
        .h(H as f32)
        .bg(theme.color(ColorToken::Background))
        .flex_col()
        .gap(4.0)
        .p(6.0)
        .child(
            text("bordered notch joins")
                .size(16.0)
                .bold()
                .color(theme.color(ColorToken::TextPrimary))
                .no_wrap(),
        )
        .child(
            div()
                .flex_row()
                .gap(5.0)
                .child(labelled(
                    "concave top (flare joins)",
                    notch()
                        .concave_top(18.0)
                        .w(240.0)
                        .h(90.0)
                        .bg(fill)
                        .border(2.0, edge),
                ))
                .child(labelled(
                    "center bulge top (cap base)",
                    notch()
                        .center_bulge_top(60.0, 16.0)
                        .w(240.0)
                        .h(90.0)
                        .bg(fill)
                        .border(2.0, edge),
                )),
        )
        .child(
            div()
                .flex_row()
                .gap(5.0)
                .child(labelled(
                    "center peak top (triangle base)",
                    notch()
                        .center_peak_top(60.0, 18.0)
                        .w(240.0)
                        .h(90.0)
                        .bg(fill)
                        .border(2.0, edge),
                ))
                .child(labelled(
                    "center scoop top (subtraction, clean)",
                    notch()
                        .center_scoop_top(60.0, 14.0)
                        .w(240.0)
                        .h(90.0)
                        .bg(fill)
                        .border(2.0, edge),
                )),
        )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "notch_border.png".to_string());

    ThemeState::init(
        blinc_theme::themes::universal::HybridTheme::bundle(),
        ColorScheme::Dark,
    );
    blinc_layout::text_measurer::init_text_measurer();

    let mut app = BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })?;

    let pixels = app.render_to_rgba8(&scene(), W, H)?;
    let img: image::RgbaImage = image::ImageBuffer::from_raw(W, H, pixels)
        .ok_or("pixel buffer did not match the requested size")?;
    img.save(&out)?;
    eprintln!("wrote {out}");
    Ok(())
}
