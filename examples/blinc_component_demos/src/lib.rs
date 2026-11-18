//! Catalog fixtures shared by native recording and worker rendering.
pub mod demos;
pub mod gallery;
#[cfg(target_arch = "wasm32")]
mod worker;

pub const DEMO_CSS: &str = include_str!("../demo.css");
pub const FONTS: &[&[u8]] = &[
    include_bytes!("../assets/NotoSans-Regular.ttf"),
    include_bytes!("../assets/NotoSans-Bold.ttf"),
    include_bytes!("../assets/JetBrainsMono-Regular.ttf"),
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
    "image",
    "waveform",
    "audio-player",
    "video-player",
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
    "card",
    "badge",
    "alert",
    "avatar",
    "separator",
    "aspect-ratio",
    "kbd",
    "icons",
    "typography",
    "table",
    "tree",
    "skeleton",
    "spinner",
    "scroll-area",
    "resizable",
    "collapsible",
    "accordion",
    "tabs",
    "breadcrumb",
    "pagination",
    "dropdown-menu",
    "context-menu",
    "menubar",
    "navigation-menu",
    "sidebar",
    "code-block",
    "code-editor",
    "rich-text-editor",
    "markdown",
    "virtual-list",
    "canvas",
    "gestures",
    "motion",
    "lists",
    "blockquote",
    "link",
    "svg",
    "notch",
    "checkbox",
    "switch",
    "radio-group",
    "toggle",
    "toggle-group",
    "number-input",
    "number-gauge",
    "number-range-gauge",
    "slider",
    "textarea",
    "input-otp",
    "combobox",
    "calendar",
    "date-picker",
    "time-picker",
    "color-picker",
    "label",
    "tooltip",
    "hover-card",
    "popover",
    "sheet",
    "drawer",
    "toast",
];

pub fn is_chart(slug: &str) -> bool {
    matches!(
        slug,
        "line-chart"
            | "charts"
            | "area-chart"
            | "stacked-area-chart"
            | "bar-chart"
            | "stacked-bar-chart"
            | "horizontal-bar-chart"
            | "pie-chart"
            | "donut-chart"
            | "sparkline"
            | "threshold-chart"
            | "histogram"
            | "comparison-chart"
    )
}
