//! Audio and video player widgets
//!
//! Both widgets use `MediaControls` which is generic over
//! the `Player` trait — shared play/pause, seek, time, volume.
//!
//! All elements have CSS classes (`.blinc-audio-*`, `.blinc-video-*`,
//! `.blinc-media-*`) for stylesheet targeting.

mod controls;
mod format_time;
mod waveform;

pub use controls::MediaControls;
pub use format_time::format_time_ms;
pub use waveform::waveform;

use std::rc::Rc;
use std::sync::Arc;

use blinc_core::Color;
use blinc_media::{Frame, Player, VideoState};
use blinc_theme::{ColorToken, ThemeState};

use crate::div::{Div, ElementBuilder, ElementTypeId, div};
use crate::element::RenderProps;
use crate::key::InstanceKey;
use crate::text::text;
use crate::tree::{LayoutNodeId, LayoutTree};

// ============================================================================
// Audio Player Widget
// ============================================================================

/// Audio player widget with optional waveform visualization
pub struct AudioPlayerWidget {
    player: Rc<blinc_media::AudioPlayer>,
    samples: Option<Arc<Vec<f32>>>,
    inner: Div,
}

/// Create an audio player widget
pub fn audio_player(player: Rc<blinc_media::AudioPlayer>) -> AudioPlayerWidget {
    AudioPlayerWidget {
        player,
        samples: None,
        inner: div().class("blinc-audio-player"),
    }
}

impl AudioPlayerWidget {
    /// Add waveform visualization from audio samples
    pub fn waveform_data(mut self, samples: &blinc_media::AudioSamples) -> Self {
        let f32_samples = samples.as_f32();
        let channels = samples.channels as usize;
        let mono: Vec<f32> = if channels > 1 {
            f32_samples
                .chunks_exact(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                .collect()
        } else {
            f32_samples
        };
        let bucket_count = 200;
        let bucket_size = (mono.len() / bucket_count).max(1);
        let buckets: Vec<f32> = mono
            .chunks(bucket_size)
            .map(|chunk| chunk.iter().map(|s| s.abs()).fold(0.0f32, f32::max))
            .collect();
        self.samples = Some(Arc::new(buckets));
        self
    }

    pub fn w(mut self, v: f32) -> Self {
        self.inner = self.inner.w(v);
        self
    }
    pub fn h(mut self, v: f32) -> Self {
        self.inner = self.inner.h(v);
        self
    }
    pub fn w_full(mut self) -> Self {
        self.inner = self.inner.w_full();
        self
    }
    pub fn bg(mut self, color: Color) -> Self {
        self.inner = self.inner.bg(color);
        self
    }
    pub fn rounded(mut self, r: f32) -> Self {
        self.inner = self.inner.rounded(r);
        self
    }
    pub fn class(mut self, class: &str) -> Self {
        self.inner = self.inner.class(class);
        self
    }
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }

    pub fn into_div(self) -> Div {
        let mut container = self.inner.flex_col().gap_px(4.0);

        // Waveform
        if let Some(ref buckets) = self.samples {
            container = container.child(
                waveform(Arc::clone(buckets))
                    .w_full()
                    .h(60.0)
                    .class("blinc-audio-waveform")
                    .into_div(),
            );
        }

        // Controls (via Player trait)
        let controls = MediaControls::new(self.player).class("blinc-audio-controls");
        container = container.child(controls.into_div());

        container
    }
}

// ============================================================================
// Video Player Widget
// ============================================================================

/// Video player widget with frame display and signal-driven controls
pub struct VideoPlayerWidget {
    player: Rc<blinc_media::VideoPlayer>,
    inner: Div,
    show_dimensions: bool,
}

/// Create a video player widget
pub fn video_player(player: Rc<blinc_media::VideoPlayer>) -> VideoPlayerWidget {
    VideoPlayerWidget {
        player,
        inner: div().class("blinc-video-player"),
        show_dimensions: false,
    }
}

impl VideoPlayerWidget {
    pub fn show_dimensions(mut self) -> Self {
        self.show_dimensions = true;
        self
    }
    pub fn w(mut self, v: f32) -> Self {
        self.inner = self.inner.w(v);
        self
    }
    pub fn h(mut self, v: f32) -> Self {
        self.inner = self.inner.h(v);
        self
    }
    pub fn w_full(mut self) -> Self {
        self.inner = self.inner.w_full();
        self
    }
    pub fn bg(mut self, color: Color) -> Self {
        self.inner = self.inner.bg(color);
        self
    }
    pub fn rounded(mut self, r: f32) -> Self {
        self.inner = self.inner.rounded(r);
        self
    }
    pub fn class(mut self, class: &str) -> Self {
        self.inner = self.inner.class(class);
        self
    }
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }

    pub fn into_div(self) -> Div {
        let player = Rc::clone(&self.player);
        let mut container = self.inner.flex_col();

        // Video surface — caches last frame to avoid mutex + Arc clone
        // when frame_generation hasn't changed between paints
        let player_for_canvas = Rc::clone(&player);
        let cached: std::rc::Rc<std::cell::RefCell<(u64, Option<std::sync::Arc<Frame>>)>> =
            std::rc::Rc::new(std::cell::RefCell::new((0, None)));
        let surface = crate::canvas::canvas(
            move |ctx: &mut dyn blinc_core::DrawContext, bounds: crate::canvas::CanvasBounds| {
                // Only fetch a new frame from the mutex when generation advances
                let frame_gen = player_for_canvas.frame_generation();
                let mut cache = cached.borrow_mut();
                if frame_gen != cache.0 {
                    cache.1 = player_for_canvas.current_frame();
                    cache.0 = frame_gen;
                }

                ctx.fill_rect(
                    blinc_core::Rect::new(0.0, 0.0, bounds.width, bounds.height),
                    blinc_core::CornerRadius::default(),
                    blinc_core::Brush::Solid(Color::BLACK),
                );

                if let Some(ref frame) = cache.1 {
                    let rgba = frame.as_rgba();
                    let vid_w = frame.width as f32;
                    let vid_h = frame.height as f32;
                    let vid_aspect = vid_w / vid_h;
                    let box_aspect = bounds.width / bounds.height;

                    let (dest_w, dest_h) = if vid_aspect > box_aspect {
                        (bounds.width, bounds.width / vid_aspect)
                    } else {
                        (bounds.height * vid_aspect, bounds.height)
                    };
                    let dest_x = (bounds.width - dest_w) / 2.0;
                    let dest_y = (bounds.height - dest_h) / 2.0;

                    ctx.draw_rgba_pixels(
                        &rgba,
                        frame.width,
                        frame.height,
                        blinc_core::Rect::new(dest_x, dest_y, dest_w, dest_h),
                    );
                }
            },
        )
        .w_full()
        .flex_grow();

        container = container.child(surface);

        // Controls, bound to the player's signals
        let controls = VideoControlsBuilder::new((*player).clone());
        container = container.child(controls);

        container
    }
}

/// The media controls, built on first access, inside the tree-building
/// context.
struct VideoControlsBuilder {
    player: blinc_media::VideoPlayer,
    key: InstanceKey,
    built: std::cell::OnceCell<Div>,
}

impl VideoControlsBuilder {
    fn new(player: blinc_media::VideoPlayer) -> Self {
        Self {
            player,
            key: InstanceKey::new("video-controls"),
            built: std::cell::OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &Div {
        self.built
            .get_or_init(|| controls(&self.player, self.key.get()))
    }
}

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

/// The controls bar, built once: what changes with playback is bound to the
/// player's signals, and the two bars are canvases that read them each paint.
fn controls(player: &blinc_media::VideoPlayer, key: &str) -> Div {
    use crate::binding::IntoReactive;
    use blinc_core::reactive::{ReactiveGraph, computed};

    let theme = ThemeState::get();
    let fg = theme.color(ColorToken::TextPrimary);
    let fg_secondary = theme.color(ColorToken::TextSecondary);
    let fg_tertiary = theme.color(ColorToken::TextTertiary);
    let surface = theme.color(ColorToken::SurfaceElevated);
    let accent = theme.color(ColorToken::Primary);
    let border = theme.color(ColorToken::Border);

    let player_click = player.clone();
    let play_btn = div()
        .id(format!("vp-play-btn-{key}"))
        .class("blinc-media-play-btn")
        .w(28.0)
        .h(28.0)
        .rounded(14.0)
        .bg(surface)
        .items_center()
        .justify_center()
        .cursor_pointer()
        .show_or(
            &player.playing_signal,
            move || crate::svg::svg(PAUSE_ICON).square(14.0).color(fg),
            move || crate::svg::svg(PLAY_ICON).square(14.0).color(fg),
        )
        .on_click(move |_| {
            if player_click.is_playing() {
                player_click.pause();
            } else {
                player_click.play();
            }
        });

    let (position, duration) = (
        player.position_signal.signal(),
        player.duration_signal.signal(),
    );
    let time = computed(move |g: &ReactiveGraph| {
        format!(
            "{} / {}",
            format_time_ms(g.get(position).unwrap_or(0)),
            format_time_ms(g.get(duration).unwrap_or(0))
        )
    });
    let time_label = div().child(
        crate::text::Text::bound(time.into_reactive())
            .size(11.0)
            .color(fg_tertiary)
            .monospace()
            .class("blinc-media-time"),
    );

    // Played, buffered (between the dim track and the bright fill, in the
    // border's hue: downloaded but not played yet) and the rest.
    let seek_player = player.clone();
    let seek_bar = crate::canvas::canvas(move |ctx: &mut dyn blinc_core::DrawContext, bounds| {
        let pos = seek_player.position_signal.get();
        let dur = seek_player.duration_signal.get();
        let buf = seek_player.buffered_signal.get().min(dur);
        let fraction = |ms: u64| {
            if dur > 0 {
                (ms as f32 / dur as f32).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let (progress, buffered) = (fraction(pos), fraction(buf));
        let track_y = (bounds.height - 4.0) / 2.0;
        let bar = |ctx: &mut dyn blinc_core::DrawContext, width: f32, color: Color| {
            ctx.fill_rect(
                blinc_core::Rect::new(0.0, track_y, width, 4.0),
                2.0.into(),
                blinc_core::Brush::Solid(color),
            );
        };
        bar(ctx, bounds.width, surface);
        if buffered > progress {
            bar(ctx, bounds.width * buffered, border);
        }
        if progress > 0.0 {
            bar(ctx, bounds.width * progress, accent);
        }
    })
    .flex_grow()
    .h(12.0);

    let player_seek = player.clone();
    let seek = div()
        .id(format!("vp-seek-{key}"))
        .class("blinc-media-seek")
        .flex_grow()
        .h(12.0)
        .cursor_pointer()
        .on_click(move |evt| {
            let dur = player_seek.duration_ms();
            if dur > 0 && evt.bounds_width > 0.0 {
                let ratio = (evt.local_x / evt.bounds_width).clamp(0.0, 1.0);
                player_seek.seek((ratio * dur as f32) as u64);
            }
        })
        .child(div().flex_grow().h(12.0).child(seek_bar));

    let volume = player.volume_signal.signal();
    let level = computed(move |g: &ReactiveGraph| vec![volume_level(g.get(volume).unwrap_or(1.0))]);
    let player_mute = player.clone();
    let mute_btn = div()
        .id(format!("vp-mute-{key}"))
        .class("blinc-media-mute-btn")
        .w(20.0)
        .h(20.0)
        .items_center()
        .justify_center()
        .cursor_pointer()
        .for_each(
            &level,
            |level| *level,
            move |level| {
                crate::svg::svg(VOLUME_ICONS[level])
                    .square(16.0)
                    .color(fg_secondary)
            },
        )
        .on_click(move |_| {
            if player_mute.volume() < 0.01 {
                player_mute.set_volume(1.0);
            } else {
                player_mute.set_volume(0.0);
            }
        });

    let vol_sig = player.volume_signal.clone();
    let vol_canvas = crate::canvas::canvas(move |ctx: &mut dyn blinc_core::DrawContext, bounds| {
        let v = vol_sig.get();
        let cols = 20u32;
        let col_w = bounds.width / cols as f32;
        for i in 0..cols {
            let frac = (i as f32 + 0.5) / cols as f32;
            let col_h = bounds.height * frac;
            let color = if frac <= v + 0.025 {
                surface
            } else {
                fg_secondary
            };
            ctx.fill_rect(
                blinc_core::Rect::new(i as f32 * col_w, bounds.height - col_h, col_w - 1.0, col_h),
                0.0.into(),
                blinc_core::Brush::Solid(color),
            );
        }
    })
    .w(50.0)
    .h(14.0);

    let player_vol = player.clone();
    div()
        .class("blinc-media-controls")
        .flex_row()
        .items_center()
        .gap_px(8.0)
        .p_px(6.0)
        .child(play_btn)
        .child(time_label)
        .child(seek)
        .child(
            div()
                .class("blinc-media-volume")
                .flex_row()
                .items_center()
                .gap_px(4.0)
                .child(mute_btn)
                .child(
                    div()
                        .id(format!("vp-vol-sl-{key}"))
                        .class("blinc-media-vol-slider")
                        .w(50.0)
                        .h(14.0)
                        .cursor_pointer()
                        .on_click(move |evt| {
                            if evt.bounds_width > 0.0 {
                                let ratio = (evt.local_x / evt.bounds_width).clamp(0.0, 1.0);
                                player_vol.set_volume(ratio);
                            }
                        })
                        .child(vol_canvas),
                ),
        )
}

impl ElementBuilder for VideoControlsBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Div
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(self.get_or_build())
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }
}
