//! Audio playback and recording
//!
//! Desktop: rodio (Vorbis/Opus/WAV/FLAC) + cpal (recording)
//! Mobile: platform codecs via native bridge
//!
//! # Example
//!
//! ```ignore
//! use blinc_media::audio::{AudioPlayer, AudioSource};
//!
//! let player = AudioPlayer::new();
//! player.play(AudioSource::file("music.ogg"));
//! player.set_volume(0.8);
//! player.pause();
//! player.resume();
//! ```

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// Audio playback state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

/// Audio source
#[derive(Clone, Debug)]
pub enum AudioSource {
    /// Load from file path
    File(String),
    /// Load from raw bytes (format auto-detected)
    Bytes(Arc<Vec<u8>>),
    /// Load from asset path (cross-platform)
    Asset(String),
}

impl AudioSource {
    pub fn file(path: impl Into<String>) -> Self {
        Self::File(path.into())
    }

    pub fn bytes(data: Vec<u8>) -> Self {
        Self::Bytes(Arc::new(data))
    }

    pub fn asset(path: impl Into<String>) -> Self {
        Self::Asset(path.into())
    }
}

/// Cross-platform audio player
pub struct AudioPlayer {
    #[cfg(target_arch = "wasm32")]
    browser: Rc<crate::VideoPlayer>,
    inner: Rc<RefCell<AudioPlayerInner>>,
}

struct AudioPlayerInner {
    state: PlaybackState,
    volume: f32,
    #[allow(dead_code)]
    looping: bool,
    /// Time when playback started (for position tracking)
    play_start: Option<std::time::Instant>,
    /// Position offset (from seek or pause)
    position_offset_ms: u64,
    /// rodio 0.22 Player (renamed from Sink in 0.19); appended sources
    /// play sequentially, volume + transport ops route through here.
    #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
    sink: Option<rodio::Player>,
    /// MixerDeviceSink (renamed from OutputStream in 0.19). Owns the
    /// audio device handle; dropped when the player goes away. The
    /// `mixer()` accessor returns the `&Mixer` we connect new
    /// `Player`s to.
    #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
    _stream: Option<rodio::MixerDeviceSink>,
}

impl AudioPlayer {
    /// Create a new audio player
    pub fn new() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            Self {
                browser: Rc::new(crate::VideoPlayer::new()),
                inner: Rc::new(RefCell::new(AudioPlayerInner {
                    state: PlaybackState::Stopped,
                    volume: 1.0,
                    looping: false,
                    play_start: None,
                    position_offset_ms: 0,
                })),
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
            {
                let stream = rodio::DeviceSinkBuilder::open_default_sink().ok();

                Self {
                    inner: Rc::new(RefCell::new(AudioPlayerInner {
                        state: PlaybackState::Stopped,
                        volume: 1.0,
                        looping: false,
                        play_start: None,
                        position_offset_ms: 0,
                        sink: None,
                        _stream: stream,
                    })),
                }
            }

            #[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32"))]
            {
                Self {
                    inner: Rc::new(RefCell::new(AudioPlayerInner {
                        state: PlaybackState::Stopped,
                        volume: 1.0,
                        looping: false,
                        play_start: None,
                        position_offset_ms: 0,
                    })),
                }
            }
        }
    }

    /// Browser transport shared with the reactive media controls.
    #[cfg(target_arch = "wasm32")]
    #[doc(hidden)]
    pub fn browser_backend(&self) -> crate::VideoPlayer {
        (*self.browser).clone()
    }

    /// Prepare a browser URL without starting playback.
    #[cfg(target_arch = "wasm32")]
    pub fn load_url(&self, url: &str) {
        self.browser.load_url(url);
    }

    /// Play an audio source
    pub fn play(&self, source: AudioSource) {
        #[cfg(target_arch = "wasm32")]
        {
            match source {
                AudioSource::File(url) | AudioSource::Asset(url) => self.browser.load_url(&url),
                AudioSource::Bytes(bytes) => self.browser.load_bytes(bytes.to_vec()),
            };
            self.browser.play();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut inner = self.inner.borrow_mut();

            #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
            {
                if let Some(ref stream) = inner._stream {
                    let sink = rodio::Player::connect_new(stream.mixer());
                    match &source {
                        AudioSource::File(path) => {
                            if let Ok(file) = std::fs::File::open(path) {
                                let reader = std::io::BufReader::new(file);
                                if let Ok(decoder) = rodio::Decoder::new(reader) {
                                    sink.append(decoder);
                                }
                            }
                        }
                        AudioSource::Bytes(data) => {
                            let cursor = std::io::Cursor::new(data.as_ref().clone());
                            if let Ok(decoder) = rodio::Decoder::new(cursor) {
                                sink.append(decoder);
                            }
                        }
                        AudioSource::Asset(path) => {
                            // Try loading via platform asset loader
                            if let Ok(data) = blinc_platform::assets::load_asset(path) {
                                let cursor = std::io::Cursor::new(data);
                                if let Ok(decoder) = rodio::Decoder::new(cursor) {
                                    sink.append(decoder);
                                }
                            }
                        }
                    }
                    sink.set_volume(inner.volume);
                    inner.sink = Some(sink);
                }
            }

            #[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32"))]
            {
                // Mobile: use native bridge
                match &source {
                    AudioSource::File(path) | AudioSource::Asset(path) => {
                        let _ = blinc_core::native_bridge::native_call::<(), _>(
                            "audio",
                            "play",
                            vec![blinc_core::native_bridge::NativeValue::String(path.clone())],
                        );
                    }
                    AudioSource::Bytes(_) => {
                        tracing::warn!("Bytes audio source not yet supported on mobile");
                    }
                }
            }

            inner.state = PlaybackState::Playing;
            inner.play_start = Some(std::time::Instant::now());
        }
    }

    /// Pause playback
    pub fn pause(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            self.browser.pause();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut inner = self.inner.borrow_mut();
            #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
            if let Some(ref sink) = inner.sink {
                sink.pause();
            }
            #[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32"))]
            {
                let _ = blinc_core::native_bridge::native_call::<(), _>("audio", "pause", ());
            }
            // Track elapsed position
            if let Some(start) = inner.play_start.take() {
                inner.position_offset_ms += start.elapsed().as_millis() as u64;
            }
            inner.state = PlaybackState::Paused;
        }
    }

    /// Resume playback
    pub fn resume(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            self.browser.play();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut inner = self.inner.borrow_mut();
            #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
            if let Some(ref sink) = inner.sink {
                sink.play();
            }
            #[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32"))]
            {
                let _ = blinc_core::native_bridge::native_call::<(), _>("audio", "resume", ());
            }
            inner.state = PlaybackState::Playing;
            inner.play_start = Some(std::time::Instant::now());
        }
    }

    /// Stop playback
    pub fn stop(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            self.browser.stop();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut inner = self.inner.borrow_mut();
            #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
            {
                inner.sink = None;
            }
            #[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32"))]
            {
                let _ = blinc_core::native_bridge::native_call::<(), _>("audio", "stop", ());
            }
            inner.state = PlaybackState::Stopped;
            inner.play_start = None;
            inner.position_offset_ms = 0;
        }
    }

    /// Get current playback position in milliseconds
    pub fn position_ms(&self) -> u64 {
        #[cfg(target_arch = "wasm32")]
        {
            self.browser.position_ms()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let inner = self.inner.borrow();
            let elapsed = inner
                .play_start
                .map(|s| s.elapsed().as_millis() as u64)
                .unwrap_or(0);
            inner.position_offset_ms + elapsed
        }
    }

    /// Seek to a position in milliseconds
    ///
    /// Note: seeking restarts playback from the new position on desktop.
    /// On mobile, the platform player handles seeking natively.
    pub fn seek(&self, _position_ms: u64) {
        #[cfg(target_arch = "wasm32")]
        {
            self.browser.seek(_position_ms);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut inner = self.inner.borrow_mut();
            inner.position_offset_ms = _position_ms;
            inner.play_start = if inner.state == PlaybackState::Playing {
                Some(std::time::Instant::now())
            } else {
                None
            };

            #[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32"))]
            {
                let _ = blinc_core::native_bridge::native_call::<(), _>(
                    "audio",
                    "seek",
                    vec![blinc_core::native_bridge::NativeValue::Int64(
                        _position_ms as i64,
                    )],
                );
            }
        }
    }

    /// Set volume (0.0 = silent, 1.0 = full)
    pub fn set_volume(&self, volume: f32) {
        #[cfg(target_arch = "wasm32")]
        {
            self.browser.set_volume(volume);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut inner = self.inner.borrow_mut();
            inner.volume = volume.clamp(0.0, 1.0);
            #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
            if let Some(ref sink) = inner.sink {
                sink.set_volume(inner.volume);
            }
            #[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32"))]
            {
                let _ = blinc_core::native_bridge::native_call::<(), _>(
                    "audio",
                    "set_volume",
                    vec![blinc_core::native_bridge::NativeValue::Float32(
                        inner.volume,
                    )],
                );
            }
        }
    }

    /// Get current playback state
    pub fn state(&self) -> PlaybackState {
        #[cfg(target_arch = "wasm32")]
        {
            match self.browser.playback_state() {
                crate::VideoState::Playing => PlaybackState::Playing,
                crate::VideoState::Paused => PlaybackState::Paused,
                _ => PlaybackState::Stopped,
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let inner = self.inner.borrow_mut();

            #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
            {
                if let Some(ref sink) = inner.sink {
                    if sink.empty() {
                        return PlaybackState::Stopped;
                    }
                    if sink.is_paused() {
                        return PlaybackState::Paused;
                    }
                    return PlaybackState::Playing;
                }
            }

            inner.state
        }
    }

    /// Get current volume
    pub fn volume(&self) -> f32 {
        #[cfg(target_arch = "wasm32")]
        {
            self.browser.volume()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.inner.borrow_mut().volume
        }
    }

    /// Check if currently playing
    pub fn is_playing(&self) -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            self.browser.is_playing()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.state() == PlaybackState::Playing
        }
    }
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::player::Player for AudioPlayer {
    fn play(&self) {
        self.resume();
    }
    fn pause(&self) {
        AudioPlayer::pause(self);
    }
    fn stop(&self) {
        AudioPlayer::stop(self);
    }
    fn seek(&self, position_ms: u64) {
        AudioPlayer::seek(self, position_ms);
    }
    fn position_ms(&self) -> u64 {
        AudioPlayer::position_ms(self)
    }
    fn duration_ms(&self) -> u64 {
        #[cfg(target_arch = "wasm32")]
        {
            self.browser.duration_ms()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            0
        }
    }
    fn volume(&self) -> f32 {
        AudioPlayer::volume(self)
    }
    fn set_volume(&self, volume: f32) {
        AudioPlayer::set_volume(self, volume);
    }
    fn is_playing(&self) -> bool {
        AudioPlayer::is_playing(self)
    }
}
