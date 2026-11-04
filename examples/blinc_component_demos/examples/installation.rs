use blinc_app::prelude::{ColorScheme, Result, WindowConfig};
use blinc_app::windowed::WindowedApp;
use blinc_cn::{button, cn_styles::CN_STYLES};
use blinc_core::reactive::computed;
use blinc_layout::prelude::div;
use blinc_theme::HybridTheme;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    let theme = HybridTheme::bundle().with_css(CN_STYLES).with_css(
        r#"
        .first-app {
            width: 100%; height: 100%; display: flex;
            align-items: center; justify-content: center;
            background: var(--background);
        }
    "#,
    );
    WindowedApp::run_with_theme(
        WindowConfig {
            title: "My Blinc app".into(),
            ..Default::default()
        },
        theme,
        ColorScheme::Dark,
        |ctx| {
            let count = ctx.use_signal(0u32);
            let label = computed(move |graph| format!("Clicks: {}", graph.get(count).unwrap_or(0)));
            div()
                .class("first-app")
                .child(button(label).on_click(move |_| count.set(count.get() + 1)))
        },
    )
}
