#![cfg(target_os = "ios")]

use blinc::app::windowed::WindowedContext;
use blinc::layout::widgets::text_input::text_input_data;
use blinc::prelude::*;

#[unsafe(no_mangle)]
pub extern "C" fn ios_app_init() {
    ThemeState::init(HybridTheme::bundle(), ColorScheme::Dark);
    blinc::app::ios::register_rust_ui_builder(fixture);
}

fn fixture(ctx: &mut WindowedContext) -> Div {
    if ctx.rebuild_count == 0 {
        ctx.add_css(blinc::cn::cn_styles::CN_STYLES);
        ctx.set_css("accessibility-fixture", include_str!("../fixture.css"))
            .expect("valid fixture CSS");
    }
    let count = ctx.use_signal_keyed("a11y-count", || 0u32);
    let checked = ctx.use_signal_keyed("a11y-checked", || false);
    let input = ctx.use_signal_keyed("a11y-input", text_input_data).get();
    let caption = computed(move |graph| format!("Count: {}", graph.get(count).unwrap_or(0)));
    div()
        .class("a11y-fixture")
        .child(text("Accessibility fixture").class("a11y-title"))
        .child(Text::bound(caption).class("a11y-counter"))
        .child(
            blinc::cn::button("Increment")
                .on_click(move |_| count.set(count.get() + 1))
                .build_component()
                .id("increment"),
        )
        .child(
            blinc::cn::input(&input)
                .label("Name")
                .placeholder("Your name"),
        )
        .child(blinc::cn::checkbox(checked).label("Enable notifications"))
        .child(blinc::cn::button("Unavailable").disabled(true))
}
