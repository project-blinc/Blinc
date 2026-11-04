//! Catalog fixtures shared by native recording and worker rendering.
pub mod demos;
#[cfg(target_arch = "wasm32")]
mod worker;

pub const DEMO_CSS: &str = include_str!("../demo.css");
pub const FONTS: &[&[u8]] = &[
    include_bytes!("../assets/NotoSans-Regular.ttf"),
    include_bytes!("../assets/NotoSans-Bold.ttf"),
];

/// The same default Hybrid theme for every native and web fixture.
pub fn theme_bundle() -> blinc_theme::ThemeBundle {
    theme_bundle_for("hybrid")
}

pub fn theme_bundle_for(name: &str) -> blinc_theme::ThemeBundle {
    match name {
        "restrained" => blinc_theme::RestrainedTheme::bundle(),
        "expressive" => blinc_theme::ExpressiveTheme::bundle(),
        _ => blinc_theme::HybridTheme::bundle(),
    }
    .with_css(blinc_cn::cn_styles::CN_STYLES)
}
pub const SLUGS: &[&str] = &[
    "button",
    "input",
    "select",
    "dialog",
    "progress",
    "charts",
    "area-chart",
    "stacked-area-chart",
    "bar-chart",
    "stacked-bar-chart",
    "horizontal-bar-chart",
    "pie-chart",
    "donut-chart",
    "sparkline",
    "threshold-chart",
    "histogram",
    "comparison-chart",
];

pub fn is_chart(slug: &str) -> bool {
    slug == "line-chart"
        || (SLUGS.contains(&slug)
            && !matches!(slug, "button" | "input" | "select" | "dialog" | "progress"))
}
