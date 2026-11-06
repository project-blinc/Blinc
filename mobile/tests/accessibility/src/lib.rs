#![cfg(target_os = "ios")]

use blinc::app::windowed::WindowedContext;
use blinc::layout::binding::IntoReactive;
use blinc::layout::widgets::text_input::{SharedTextInputData, text_input_data};
use blinc::prelude::*;

#[unsafe(no_mangle)]
pub extern "C" fn ios_app_init() {
    ThemeState::init(HybridTheme::bundle(), ColorScheme::Dark);
    let input = text_input_data();
    let styles = std::sync::Once::new();
    blinc::app::ios::register_rust_ui_builder(move |ctx| {
        styles.call_once(|| {
            ctx.add_css(blinc::cn::cn_styles::CN_STYLES);
            ctx.set_css("accessibility-fixture", include_str!("../fixture.css"))
                .expect("valid fixture CSS");
        });
        fixture(ctx, &input)
    });
}

fn fixture(ctx: &mut WindowedContext, input: &SharedTextInputData) -> Div {
    let count = ctx.use_signal_keyed("a11y-count", || 0u32);
    let checked = ctx.use_signal_keyed("a11y-checked", || false);
    let volume = ctx.use_signal_keyed("a11y-volume", || 50.0f32);
    let caption = computed(move |graph| format!("Count: {}", graph.get(count).unwrap_or(0)));
    div()
        .class("a11y-fixture")
        .child(text("Accessibility fixture").class("a11y-title"))
        .child(Text::bound(caption.into_reactive()).class("a11y-counter"))
        .child(
            blinc::cn::button("Increment")
                .on_click(move |_| count.set(count.get() + 1))
                .build_component()
                .id("increment"),
        )
        .child(
            blinc::cn::input(input)
                .label("Name")
                .placeholder("Your name")
                .on_submit(move |_| count.set(count.get() + 10)),
        )
        .child(blinc::cn::checkbox(checked).label("Enable notifications"))
        .child(
            blinc::cn::slider(volume)
                .max(100.0)
                .step(5.0)
                .label("Volume")
                .show_value(),
        )
        .child(blinc::cn::button("Unavailable").disabled(true))
}
