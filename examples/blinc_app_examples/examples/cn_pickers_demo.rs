//! Run with `cargo run -p blinc_app_examples --features cn --example cn_pickers_demo`.

use blinc_app::{
    prelude::*,
    windowed::{WindowedApp, WindowedContext},
};
use blinc_cn::{TimeFormat, TimeOfDay, cn, cn_bundle};
use blinc_core::{Color, reactive::computed};
use blinc_layout::binding::IntoReactive;

pub fn theme_bundle() -> ThemeBundle {
    cn_bundle().with_css(
        r#"
        .picker-demo { background: var(--background); color: var(--text-primary); }
        .picker-demo-heading { font-size: 22px; font-weight: 600; }
        .picker-demo-caption { color: var(--text-secondary); font-size: 13px; }
    "#,
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    WindowedApp::run_with_theme(
        WindowConfig {
            title: "CN Time and Colour Pickers".into(),
            width: 880,
            height: 640,
            ..Default::default()
        },
        theme_bundle(),
        blinc_theme::detect_system_color_scheme(),
        build_ui,
    )
}

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    let time = ctx.use_signal(TimeOfDay::new(14, 30, 15).unwrap());
    let color = ctx.use_signal(Color::from_hex(0x6366f1));
    let disabled = ctx.use_signal(false);
    let time_label = computed(move |g| format!("Selected: {}", g.get(time).unwrap_or_default()));
    let color_label = computed(move |g| g.get(color).unwrap_or_default().to_hex_string(true));
    let palette = vec![
        0xef4444, 0xf59e0b, 0xeab308, 0x22c55e, 0x14b8a6, 0x3b82f6, 0x6366f1, 0xec4899, 0xa855f7,
        0x111827, 0xffffff,
    ]
    .into_iter()
    .map(Color::from_hex)
    .collect::<Vec<_>>();
    let popup_palette = palette.clone();
    div()
        .class("picker-demo")
        .w(ctx.width)
        .h(ctx.height)
        .p(8.0)
        .flex_col()
        .gap(5.0)
        .child(text("Time and colour pickers").class("picker-demo-heading"))
        .child(cn::checkbox(disabled).label("Disable controls"))
        .child(
            div()
                .flex_row()
                .gap(8.0)
                .child(
                    div()
                        .flex_col()
                        .gap(3.0)
                        .child(text("24-hour time").class("picker-demo-caption"))
                        .child(cn::time_picker(time).seconds(true).disabled(disabled))
                        .child(text("12-hour time").class("picker-demo-caption"))
                        .child(
                            cn::time_picker(time)
                                .format(TimeFormat::TwelveHour)
                                .disabled(disabled),
                        )
                        .child(Text::bound(time_label.into_reactive()).class("picker-demo-caption"))
                        .child(
                            cn::button("Reset time")
                                .disabled(disabled)
                                .on_click(move |_| {
                                    time.set(TimeOfDay::new(14, 30, 15).unwrap());
                                }),
                        ),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(3.0)
                        .child(text("Inline colour editor").class("picker-demo-caption"))
                        .child(
                            cn::color_picker(color)
                                .inline()
                                .alpha(true)
                                .swatches(palette)
                                .disabled(disabled),
                        ),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(3.0)
                        .child(text("Colour popover").class("picker-demo-caption"))
                        .child(
                            cn::color_picker(color)
                                .alpha(true)
                                .swatches(popup_palette)
                                .disabled(disabled),
                        )
                        .child(
                            Text::bound(color_label.into_reactive()).class("picker-demo-caption"),
                        )
                        .child(
                            cn::button("Reset colour")
                                .disabled(disabled)
                                .on_click(move |_| {
                                    color.set(Color::from_hex(0x6366f1));
                                }),
                        ),
                ),
        )
}
