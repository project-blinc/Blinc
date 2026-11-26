//! Shared media playback controls
//!
//! Generic over any `Player` implementation (audio or video).

use std::rc::Rc;

use blinc_core::reactive::{ReactiveGraph, Signal, computed, signal};
use blinc_media::Player;

use crate::binding::IntoReactive;
use crate::canvas::canvas;
use crate::div::{Div, div};
use crate::svg::svg;
use crate::text::Text;
use crate::text::text;

use super::format_time::format_time_ms;

const PLAY_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z"/></svg>"#;
const PAUSE_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z"/></svg>"#;

/// What the controls show of a player.
#[derive(Clone, Copy)]
struct Shown {
    playing: Signal<bool>,
    position: Signal<u64>,
    duration: Signal<u64>,
    volume: Signal<f32>,
}

impl Shown {
    fn new(is_playing: bool, position: u64, duration: u64, volume: f32) -> Self {
        Self {
            playing: signal(is_playing),
            position: signal(position),
            duration: signal(duration),
            volume: signal(volume),
        }
    }

    fn of(player: &impl Player) -> Self {
        Self::new(
            player.is_playing(),
            player.position_ms(),
            player.duration_ms(),
            player.volume(),
        )
    }

    /// Copies the player's state. Writes only what changed, so a still player asks for no frame.
    fn follow(&self, player: &impl Player) {
        write(self.playing, player.is_playing());
        write(self.position, player.position_ms());
        write(self.duration, player.duration_ms());
        write(self.volume, player.volume());
    }
}

fn write<T: Clone + PartialEq + Send + 'static>(signal: Signal<T>, value: T) {
    if signal.try_get().is_some_and(|old| old != value) {
        signal.set(value);
    }
}

/// How far through the track `position` is, from 0 to 1.
fn progress(graph: &ReactiveGraph, shown: Shown) -> f32 {
    let duration = graph.get(shown.duration).unwrap_or(0);
    if duration == 0 {
        return 0.0;
    }
    let position = graph.get(shown.position).unwrap_or(0);
    (position as f32 / duration as f32).clamp(0.0, 1.0)
}

/// Shared media controls for any [`Player`].
pub struct MediaControls {
    inner: Div,
}

impl MediaControls {
    /// Controls that follow `player`: they read it on each frame they are painted, and ask for
    /// frames while it plays.
    pub fn new<P: Player + 'static>(player: Rc<P>) -> Self {
        let shown = Shown::of(&*player);
        let is_live = player.is_live();
        let row = row(Rc::clone(&player), shown, is_live);

        // The canvas draws nothing: its paint is where the player is read.
        let drawn = Rc::clone(&player);
        let follower = canvas(move |_, _| shown.follow(&*drawn))
            .redraw_while(move || player.is_playing() || shown.playing.get())
            .absolute()
            .left(0.0)
            .top(0.0)
            .size(1.0, 1.0)
            .pointer_events_none();

        Self {
            inner: row.child(follower),
        }
    }

    /// Controls showing the given values. They change only when one of them is used.
    pub fn from_signals<P: Player + 'static>(
        player: Rc<P>,
        is_playing: bool,
        position: u64,
        duration: u64,
        volume: f32,
        is_live: bool,
    ) -> Self {
        let shown = Shown::new(is_playing, position, duration, volume);
        Self {
            inner: row(player, shown, is_live),
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

/// The controls bar, with what changes bound to `shown`.
fn row<P: Player + 'static>(player: Rc<P>, shown: Shown, is_live: bool) -> Div {
    let player_for_click = Rc::clone(&player);
    let playing = computed(move |g: &ReactiveGraph| g.get(shown.playing).unwrap_or(false));

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
                .show_or(
                    playing,
                    || svg(PAUSE_SVG).square(14.0),
                    || svg(PLAY_SVG).square(14.0),
                )
                .on_click(move |_| {
                    if player_for_click.is_playing() {
                        player_for_click.pause();
                    } else {
                        player_for_click.play();
                    }
                    shown.follow(&*player_for_click);
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
        let time = computed(move |g: &ReactiveGraph| {
            format!(
                "{} / {}",
                format_time_ms(g.get(shown.position).unwrap_or(0)),
                format_time_ms(g.get(shown.duration).unwrap_or(0))
            )
        });
        row = row.child(
            Text::bound(time.into_reactive())
                .size(11.0)
                .monospace()
                .class("blinc-media-time"),
        );

        let player_for_seek = Rc::clone(&player);
        let played = computed(move |g: &ReactiveGraph| progress(g, shown).max(0.001));
        let rest = computed(move |g: &ReactiveGraph| (1.0 - progress(g, shown)).max(0.001));
        let fill = div()
            .h(4.0)
            .rounded(2.0)
            .flex_grow_value(played)
            .class("blinc-media-seek-fill");
        let remainder = div().h(4.0).flex_grow_value(rest);

        row = row.child(
            div()
                .flex_grow()
                .h(12.0)
                .items_center()
                .cursor_pointer()
                .class("blinc-media-seek-track")
                .on_click(move |evt| {
                    let duration = player_for_seek.duration_ms();
                    if duration > 0 && evt.bounds_width > 0.0 {
                        let ratio = (evt.local_x / evt.bounds_width).clamp(0.0, 1.0);
                        player_for_seek.seek((ratio * duration as f32) as u64);
                        shown.follow(&*player_for_seek);
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

    let volume = computed(move |g: &ReactiveGraph| {
        format!("{}%", (g.get(shown.volume).unwrap_or(1.0) * 100.0) as u32)
    });
    row.child(
        Text::bound(volume.into_reactive())
            .size(10.0)
            .monospace()
            .class("blinc-media-volume"),
    )
}
