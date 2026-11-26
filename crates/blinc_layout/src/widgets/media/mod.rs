//! Audio and video player widgets
//!
//! Both widgets show one set of controls: play/pause, time, seek and volume.
//! A video player and a browser audio player publish signals that the controls
//! bind to. [`MediaControls`] shows the same set for any `Player`, and reads
//! the player as it paints.
//!
//! All elements have CSS classes (`.blinc-audio-*`, `.blinc-video-*`,
//! `.blinc-media-*`) for stylesheet targeting.

mod controls;
mod format_time;
mod waveform;

pub use controls::MediaControls;
use controls::{Shown, Source};
pub use format_time::format_time_ms;
pub use waveform::waveform;

use std::rc::Rc;
use std::sync::Arc;

use blinc_core::Color;
use blinc_media::Frame;

use crate::div::{Div, ElementBuilder, ElementTypeId, div};
use crate::element::RenderProps;
use crate::key::InstanceKey;
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
                    .playback(self.player.clone())
                    .w_full()
                    .h(60.0)
                    .class("blinc-audio-waveform")
                    .into_div(),
            );
        }

        // The same controls on both: a browser player publishes its signals, and
        // elsewhere the controls read the player.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let controls = MediaControls::new(self.player).class("blinc-audio-controls");
            container = container.child(controls.into_div());
        }
        #[cfg(target_arch = "wasm32")]
        {
            container = container.child(
                VideoControlsBuilder::new(self.player.browser_backend(), false)
                    .class("blinc-audio-controls"),
            );
        }

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
    /// Show the video's size in pixels under the controls, once a frame is decoded.
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
        let controls = VideoControlsBuilder::new((*player).clone(), self.show_dimensions);
        container = container.child(controls);

        container
    }
}

/// The media controls, built on first access, inside the tree-building
/// context.
struct VideoControlsBuilder {
    player: blinc_media::VideoPlayer,
    show_dimensions: bool,
    class: Option<&'static str>,
    key: InstanceKey,
    built: std::cell::OnceCell<Div>,
}

impl VideoControlsBuilder {
    fn new(player: blinc_media::VideoPlayer, show_dimensions: bool) -> Self {
        Self {
            player,
            show_dimensions,
            class: None,
            key: InstanceKey::new("video-controls"),
            built: std::cell::OnceCell::new(),
        }
    }

    /// Adds a class to the controls bar.
    #[cfg(target_arch = "wasm32")]
    fn class(mut self, class: &'static str) -> Self {
        self.class = Some(class);
        self
    }

    fn get_or_build(&self) -> &Div {
        self.built.get_or_init(|| {
            let player = Rc::new(self.player.clone());
            let shown = Shown::published(&player);
            let mut bar =
                controls::controls(player, shown, Source::Published, false, self.key.get());
            if let Some(class) = self.class {
                bar = bar.class(class);
            }
            if self.show_dimensions {
                div().flex_col().child(bar).child(dimensions(&self.player))
            } else {
                bar
            }
        })
    }
}

/// The size of the current frame, empty while there is none.
fn dimensions(player: &blinc_media::VideoPlayer) -> Div {
    use crate::binding::IntoReactive;
    use blinc_core::reactive::{ReactiveGraph, computed};

    let size = player.dimensions_signal.signal();
    let label = computed(move |g: &ReactiveGraph| match g.get(size) {
        Some((width, height)) if width > 0 => format!("{width} × {height}"),
        _ => String::new(),
    });
    div().class("blinc-video-dimensions").px(1.5).pb(1.0).child(
        crate::text::Text::bound(label.into_reactive())
            .size(11.0)
            .monospace(),
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
