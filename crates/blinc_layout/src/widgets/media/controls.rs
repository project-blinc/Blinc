//! Shared media playback controls
//!
//! Generic over any `Player` implementation (audio or video).

use std::rc::Rc;

use blinc_core::Color;
use blinc_media::Player;

use crate::div::{Div, div};
use crate::svg::svg;
use crate::text::text;

use super::format_time::format_time_ms;

const PLAY_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z"/></svg>"#;
const PAUSE_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z"/></svg>"#;

/// Shared media controls driven by State signals.
pub struct MediaControls {
    inner: Div,
}

impl MediaControls {
    /// Build controls from individual signals + a player for actions.
    pub fn from_signals<P: Player + 'static>(
        player: Rc<P>,
        is_playing: bool,
        position: u64,
        duration: u64,
        volume: f32,
        is_live: bool,
    ) -> Self {
        let play_icon = if is_playing { PAUSE_SVG } else { PLAY_SVG };
        let player_for_click = Rc::clone(&player);

        let mut row = div()
            .flex_row()
            .items_center()
            .gap_px(8.0)
            .p_px(6.0)
            .class("blinc-media-controls")
            .child(
                div()
                    .w(28.0)
                    .h(28.0)
                    .rounded(14.0)
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .class("blinc-media-play-btn")
                    .child(svg(play_icon).square(14.0))
                    .on_click(move |_| {
                        tracing::info!("play/pause clicked, is_playing={}", is_playing);
                        if player_for_click.is_playing() {
                            player_for_click.pause();
                        } else {
                            player_for_click.play();
                        }
                    }),
            );

        if is_live {
            row = row.child(
                div()
                    .rounded(3.0)
                    .p_px(4.0)
                    .class("blinc-media-live-badge")
                    .child(text("LIVE").size(9.0).bold()),
            );
        } else {
            row = row.child(
                text(format!(
                    "{} / {}",
                    format_time_ms(position),
                    format_time_ms(duration)
                ))
                .size(11.0)
                .monospace()
                .class("blinc-media-time"),
            );
        }

        if !is_live {
            let progress = if duration > 0 {
                (position as f32 / duration as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let player_for_seek = Rc::clone(&player);
            let seek_duration = duration;

            let fill = div()
                .h(4.0)
                .rounded(2.0)
                .flex_grow_value(progress.max(0.001))
                .class("blinc-media-seek-fill");

            let remainder = div().h(4.0).flex_grow_value((1.0 - progress).max(0.001));

            row = row.child(
                div()
                    .flex_grow()
                    .h(12.0)
                    .items_center()
                    .cursor_pointer()
                    .class("blinc-media-seek-track")
                    .on_click(move |evt| {
                        if seek_duration > 0 && evt.bounds_width > 0.0 {
                            let ratio = (evt.local_x / evt.bounds_width).clamp(0.0, 1.0);
                            let target_ms = (ratio * seek_duration as f32) as u64;
                            player_for_seek.seek(target_ms);
                        }
                    })
                    .child(
                        div()
                            .class("blinc-media-seek-rail")
                            .w_full()
                            .h(4.0)
                            .rounded(2.0)
                            .overflow_clip()
                            .flex_row()
                            .child(fill)
                            .child(remainder),
                    ),
            );
        }

        row = row.child(
            text(format!("{}%", (volume * 100.0) as u32))
                .size(10.0)
                .monospace()
                .class("blinc-media-volume"),
        );

        Self { inner: row }
    }

    /// Legacy constructor — reads values from player once.
    pub fn new<P: Player + 'static>(player: Rc<P>) -> Self {
        let is_playing = player.is_playing();
        let position = player.position_ms();
        let duration = player.duration_ms();
        let volume = player.volume();
        let is_live = player.is_live();
        Self::from_signals(player, is_playing, position, duration, volume, is_live)
    }

    pub fn class(mut self, class: &str) -> Self {
        self.inner = self.inner.class(class);
        self
    }

    pub fn into_div(self) -> Div {
        self.inner
    }
}
