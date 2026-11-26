//! Video Player Demo
//!
//! Demonstrates the video_player widget with `blinc_media::VideoPlayer` instance and controls.
//!
//! Run with:
//! ```sh
//! cargo run -p blinc_app_examples --example video_demo --features media
//! ```

use std::rc::Rc;
use std::sync::OnceLock;

use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;
use blinc_core::Color;
use blinc_layout::widgets::media::video_player;
use blinc_media::VideoPlayer;
use blinc_platform::assets::asset_url;

const VIDEO_PATH: &str =
    "examples/blinc_app_examples/examples/assets/german-shepherd-hd_1920_1080_25fps.mp4";

static PLAYER: OnceLock<VideoPlayer> = OnceLock::new();

fn shared_player() -> VideoPlayer {
    PLAYER
        .get_or_init(|| {
            let p = VideoPlayer::new();

            // Both targets go through the platform loader's URL hook
            // now. On native this resolves to a `file://<abs>` URL;
            // `load_url` strips the prefix and decodes via the
            // desktop byte path. On wasm it resolves to the asset's
            // fetch URL, which the `<video>` element consumes
            // directly — HTTP range requests + progressive buffering,
            // no full preload, no blob copy. The seek bar's buffered
            // ghost fill reflects the download tip in real time.
            if let Some(url) = asset_url(VIDEO_PATH) {
                p.load_url(&url);
            } else {
                tracing::warn!("no asset URL available for {VIDEO_PATH}");
            }

            p
        })
        .clone()
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    blinc_app::windowed::WindowedApp::run(
        WindowConfig {
            title: "Video Player Demo".into(),
            width: 960,
            height: 640,
            ..Default::default()
        },
        build_ui,
    )
}

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    let player = shared_player();

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.05, 0.06, 0.10, 1.0))
        .flex_col()
        .items_center()
        .justify_center()
        .p_px(16.0)
        .child(
            video_player(Rc::new(player))
                .w(ctx.width - 32.0)
                .h(ctx.height - 32.0)
                .bg(Color::BLACK)
                .rounded(8.0)
                .show_dimensions()
                .into_div(),
        )
}
