//! The controls of the media players: play/pause, time, seek and volume.
//!
//! Built once. What changes with playback is bound to signals, and the seek bar
//! and the volume level are canvases that read them each paint.

use std::rc::Rc;

use blinc_core::Color;
use blinc_core::reactive::{ReactiveGraph, Signal, computed, signal};
use blinc_media::{Player, VideoPlayer};
use blinc_theme::ColorToken;

use crate::binding::IntoReactive;
use crate::canvas::canvas;
use crate::div::{Div, div};
use crate::key::InstanceKey;
use crate::svg::svg;
use crate::text::{Text, text};

use super::format_time::format_time_ms;

const PLAY_ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z"/></svg>"#;
const PAUSE_ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z"/></svg>"#;
/// Muted, low and high.
const VOLUME_ICONS: [&str; 3] = [
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M16.5 12c0-1.77-1.02-3.29-2.5-4.03v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51C20.63 14.91 21 13.5 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3L3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06c1.38-.31 2.63-.95 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4L9.91 6.09 12 8.18V4z"/></svg>"#,
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M18.5 12c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM5 9v6h4l5 5V4L9 9H5z"/></svg>"#,
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z"/></svg>"#,
];

/// Which of [`VOLUME_ICONS`] a volume shows.
fn volume_level(volume: f32) -> usize {
    if volume < 0.01 {
        0
    } else if volume < 0.5 {
        1
    } else {
        2
    }
}

/// A colour the canvases draw with: a `--blinc-media-*` variable and the
/// theme token it defaults to.
type Ink = (&'static str, ColorToken);
const TRACK: Ink = (
    "var(--blinc-media-track, var(--surface-elevated))",
    ColorToken::SurfaceElevated,
);
const BUFFERED: Ink = (
    "var(--blinc-media-buffered, var(--border))",
    ColorToken::Border,
);
const PROGRESS: Ink = (
    "var(--blinc-media-progress, var(--primary))",
    ColorToken::Primary,
);
const LEVEL: Ink = (
    "var(--blinc-media-level, var(--surface-elevated))",
    ColorToken::SurfaceElevated,
);
const LEVEL_REST: Ink = (
    "var(--blinc-media-level-rest, var(--text-secondary))",
    ColorToken::TextSecondary,
);

/// Resolved each paint, so a theme or stylesheet change shows.
fn ink((css, token): Ink) -> Color {
    crate::ua_styles::stylesheet_color(css, token)
}

/// What the controls show of a player.
#[derive(Clone, Copy)]
pub(super) struct Shown {
    playing: Signal<bool>,
    position: Signal<u64>,
    duration: Signal<u64>,
    buffered: Signal<u64>,
    volume: Signal<f32>,
}

impl Shown {
    fn new(playing: bool, position: u64, duration: u64, buffered: u64, volume: f32) -> Self {
        Self {
            playing: signal(playing),
            position: signal(position),
            duration: signal(duration),
            buffered: signal(buffered),
            volume: signal(volume),
        }
    }

    /// The signals a video player writes as it plays.
    pub(super) fn published(player: &VideoPlayer) -> Self {
        Self {
            playing: player.playing_signal.signal(),
            position: player.position_signal.signal(),
            duration: player.duration_signal.signal(),
            buffered: player.buffered_signal.signal(),
            volume: player.volume_signal.signal(),
        }
    }

    fn of(player: &impl Player) -> Self {
        Self::new(
            player.is_playing(),
            player.position_ms(),
            player.duration_ms(),
            player.buffered_ms(),
            player.volume(),
        )
    }

    /// Copies the player's state. Writes only what changed, so a still player asks for no frame.
    fn follow(&self, player: &impl Player) {
        write(self.playing, player.is_playing());
        write(self.position, player.position_ms());
        write(self.duration, player.duration_ms());
        write(self.buffered, player.buffered_ms());
        write(self.volume, player.volume());
    }
}

fn write<T: Clone + PartialEq + Send + 'static>(signal: Signal<T>, value: T) {
    if signal.try_get().is_some_and(|old| old != value) {
        signal.set(value);
    }
}

/// Who writes the signals the controls show.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Source {
    /// The player.
    Published,
    /// The controls, from the player.
    Followed,
    /// Nobody: the controls show what they were given.
    Fixed,
}

/// Controls for any [`Player`], as the media widgets show them.
pub struct MediaControls {
    inner: Div,
}

impl MediaControls {
    /// Controls that follow `player`: they read it on each frame they are painted, and ask for
    /// frames while it plays.
    #[track_caller]
    pub fn new<P: Player + 'static>(player: Rc<P>) -> Self {
        let key = InstanceKey::new("media-controls");
        let shown = Shown::of(&*player);
        let is_live = player.is_live();
        Self {
            inner: controls(player, shown, Source::Followed, is_live, key.get()),
        }
    }

    /// Controls showing the given values, which stay as given.
    #[track_caller]
    pub fn from_signals<P: Player + 'static>(
        player: Rc<P>,
        is_playing: bool,
        position: u64,
        duration: u64,
        volume: f32,
        is_live: bool,
    ) -> Self {
        let key = InstanceKey::new("media-controls");
        let shown = Shown::new(is_playing, position, duration, duration, volume);
        Self {
            inner: controls(player, shown, Source::Fixed, is_live, key.get()),
        }
    }

    pub fn class(mut self, class: &str) -> Self {
        self.inner = self.inner.class(class);
        self
    }

    pub fn into_div(self) -> Div {
        self.inner
    }
}

/// The controls bar for `player`, with what changes bound to `shown`.
pub(super) fn controls<P: Player + 'static>(
    player: Rc<P>,
    shown: Shown,
    source: Source,
    is_live: bool,
    key: &str,
) -> Div {
    // What a control does shows at once, with no frame between.
    let acted = move |player: &P| {
        if source == Source::Followed {
            shown.follow(player);
        }
    };

    let mut row = div()
        .class("blinc-media-controls")
        .flex_row()
        .items_center()
        .gap_px(8.0)
        .p_px(6.0);

    if source == Source::Followed {
        // The canvas draws nothing: its paint is where the player is read. It is the first
        // child, so the canvases after it draw what it read.
        let drawn = Rc::clone(&player);
        let watched = Rc::clone(&player);
        row = row.child(
            canvas(move |_, _| shown.follow(&*drawn))
                .redraw_while(move || watched.is_playing() || shown.playing.get())
                .absolute()
                .left(0.0)
                .top(0.0)
                .size(1.0, 1.0)
                .pointer_events_none(),
        );
    }

    let player_click = Rc::clone(&player);
    let playing = computed(move |g: &ReactiveGraph| g.get(shown.playing).unwrap_or(false));
    row = row.child(
        div()
            .id(format!("media-play-{key}"))
            .class("blinc-media-play-btn")
            .w(28.0)
            .h(28.0)
            .rounded(14.0)
            .items_center()
            .justify_center()
            .cursor_pointer()
            .show_or(
                playing,
                move || svg(PAUSE_ICON).square(14.0),
                move || svg(PLAY_ICON).square(14.0),
            )
            .on_click(move |_| {
                if player_click.is_playing() {
                    player_click.pause();
                } else {
                    player_click.play();
                }
                acted(&player_click);
            }),
    );

    row = if is_live {
        row.child(
            div()
                .rounded(3.0)
                .p_px(4.0)
                .class("blinc-media-live-badge")
                .child(text("LIVE").size(9.0).bold()),
        )
        .child(div().flex_grow())
    } else {
        row.child(time_label(shown))
            .child(seek_bar(Rc::clone(&player), shown, acted, key))
    };

    row.child(volume(player, shown, acted, key))
}

fn time_label(shown: Shown) -> Div {
    let time = computed(move |g: &ReactiveGraph| {
        format!(
            "{} / {}",
            format_time_ms(g.get(shown.position).unwrap_or(0)),
            format_time_ms(g.get(shown.duration).unwrap_or(0))
        )
    });
    div().child(
        Text::bound(time.into_reactive())
            .size(11.0)
            .monospace()
            .class("blinc-media-time"),
    )
}

/// Played, buffered (between the dim track and the bright fill, in the border's hue: downloaded
/// but not played yet) and the rest. A click seeks.
fn seek_bar<P: Player + 'static>(
    player: Rc<P>,
    shown: Shown,
    acted: impl Fn(&P) + 'static,
    key: &str,
) -> Div {
    let bar = canvas(move |ctx: &mut dyn blinc_core::DrawContext, bounds| {
        let duration = shown.duration.get();
        let fraction = |ms: u64| {
            if duration > 0 {
                (ms as f32 / duration as f32).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let progress = fraction(shown.position.get());
        let buffered = fraction(shown.buffered.get());
        let track_y = (bounds.height - 4.0) / 2.0;
        let line = |ctx: &mut dyn blinc_core::DrawContext, width: f32, color: Color| {
            ctx.fill_rect(
                blinc_core::Rect::new(0.0, track_y, width, 4.0),
                2.0.into(),
                blinc_core::Brush::Solid(color),
            );
        };
        line(ctx, bounds.width, ink(TRACK));
        if buffered > progress {
            line(ctx, bounds.width * buffered, ink(BUFFERED));
        }
        if progress > 0.0 {
            line(ctx, bounds.width * progress, ink(PROGRESS));
        }
    })
    // Repaints with the frames that a change of what it reads asks for.
    .redraw_while(|| false)
    .flex_grow()
    .h(12.0);

    div()
        .id(format!("media-seek-{key}"))
        .class("blinc-media-seek")
        .flex_grow()
        .h(12.0)
        .cursor_pointer()
        .on_click(move |evt| {
            let duration = player.duration_ms();
            if duration > 0 && evt.bounds_width > 0.0 {
                let ratio = (evt.local_x / evt.bounds_width).clamp(0.0, 1.0);
                player.seek((ratio * duration as f32) as u64);
                acted(&player);
            }
        })
        .child(div().flex_grow().h(12.0).child(bar))
}

/// The mute button and the volume level, which a click sets.
fn volume<P: Player + 'static>(
    player: Rc<P>,
    shown: Shown,
    acted: impl Fn(&P) + Copy + 'static,
    key: &str,
) -> Div {
    let level =
        computed(move |g: &ReactiveGraph| vec![volume_level(g.get(shown.volume).unwrap_or(1.0))]);
    let player_mute = Rc::clone(&player);
    let mute_button = div()
        .id(format!("media-mute-{key}"))
        .class("blinc-media-mute-btn")
        .w(20.0)
        .h(20.0)
        .items_center()
        .justify_center()
        .cursor_pointer()
        .for_each(
            &level,
            |level| *level,
            move |level| svg(VOLUME_ICONS[level]).square(16.0),
        )
        .on_click(move |_| {
            if player_mute.volume() < 0.01 {
                player_mute.set_volume(1.0);
            } else {
                player_mute.set_volume(0.0);
            }
            acted(&player_mute);
        });

    let columns = canvas(move |ctx: &mut dyn blinc_core::DrawContext, bounds| {
        let volume = shown.volume.get();
        let (level, rest) = (ink(LEVEL), ink(LEVEL_REST));
        let cols = 20u32;
        let col_w = bounds.width / cols as f32;
        for i in 0..cols {
            let frac = (i as f32 + 0.5) / cols as f32;
            let col_h = bounds.height * frac;
            let color = if frac <= volume + 0.025 { level } else { rest };
            ctx.fill_rect(
                blinc_core::Rect::new(i as f32 * col_w, bounds.height - col_h, col_w - 1.0, col_h),
                0.0.into(),
                blinc_core::Brush::Solid(color),
            );
        }
    })
    .redraw_while(|| false)
    .w(50.0)
    .h(14.0);

    div()
        .class("blinc-media-volume")
        .flex_row()
        .items_center()
        .gap_px(4.0)
        .child(mute_button)
        .child(
            div()
                .id(format!("media-volume-{key}"))
                .class("blinc-media-vol-slider")
                .w(50.0)
                .h(14.0)
                .cursor_pointer()
                .on_click(move |evt| {
                    if evt.bounds_width > 0.0 {
                        let ratio = (evt.local_x / evt.bounds_width).clamp(0.0, 1.0);
                        player.set_volume(ratio);
                        acted(&player);
                    }
                })
                .child(columns),
        )
}
