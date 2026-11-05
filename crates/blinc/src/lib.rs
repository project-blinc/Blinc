#![doc = include_str!("../README.md")]

extern crate self as blinc;

pub use blinc_animation as animation;
pub use blinc_app as app;
pub use blinc_core as core;
pub use blinc_css as css;
pub use blinc_flow as flow;
pub use blinc_gpu as gpu;
pub use blinc_image as image;
pub use blinc_layout as layout;
pub use blinc_macros as macros;
pub use blinc_paint as paint;
pub use blinc_platform as platform;
pub use blinc_router as router;
pub use blinc_text as text;
pub use blinc_theme as theme;

#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub use blinc_platform_web as platform_web;

#[cfg(feature = "cn")]
pub use blinc_cn as cn;
#[cfg(feature = "icons")]
pub use blinc_icons as icons;
#[cfg(feature = "media")]
pub use blinc_media as media;
#[cfg(feature = "svg")]
pub use blinc_svg as svg;

/// Application builders, core types and reactive primitives.
pub mod prelude {
    pub use blinc_app::prelude::*;
    pub use blinc_core::{computed, effect, signal};
}
