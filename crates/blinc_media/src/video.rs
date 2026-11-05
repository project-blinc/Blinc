//! Video decoding — RGBA frames from H.264 streams
//!
//! Desktop: OpenH264 (royalty-free, Cisco covers patents)
//! Mobile: platform decoders via native bridge
//!
//! # Example
//!
//! ```ignore
//! use blinc_media::video::VideoDecoder;
//!
//! let mut decoder = VideoDecoder::new();
//! if let Some(frame) = decoder.decode_nal(h264_packet) {
//!     canvas_render(frame.as_rgba(), frame.width, frame.height);
//! }
//! ```

use crate::frame::Frame;

/// Video playback state
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VideoState {
    #[default]
    Idle,
    Playing,
    Paused,
    Ended,
}

/// Re-export Frame as VideoFrame for convenience
pub type VideoFrame = Frame;

/// Video decoder — extracts RGBA frames from H.264 NAL units
pub struct VideoDecoder {
    #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
    decoder: Option<openh264::decoder::Decoder>,
    state: VideoState,
}

impl VideoDecoder {
    pub fn new() -> Self {
        #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
        {
            use openh264::decoder::{DecoderConfig, Flush};
            let config = DecoderConfig::new().flush_after_decode(Flush::NoFlush);
            let api = openh264::OpenH264API::from_source();
            let decoder = openh264::decoder::Decoder::with_api_config(api, config).ok();
            Self {
                decoder,
                state: VideoState::Idle,
            }
        }

        #[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32"))]
        {
            Self {
                state: VideoState::Idle,
            }
        }
    }

    /// Decode a single H.264 NAL unit and return an RGBA frame if available
    #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
    pub fn decode_nal(&mut self, nal_data: &[u8]) -> Option<Frame> {
        let decoder = self.decoder.as_mut()?;
        match decoder.decode(nal_data) {
            Ok(Some(yuv)) => {
                let (uv_w, uv_h) = yuv.dimensions_uv();
                let w = uv_w * 2;
                let h = uv_h * 2;
                let mut rgba = vec![0u8; w * h * 4];
                yuv.write_rgba8(&mut rgba);
                self.state = VideoState::Playing;
                Some(Frame::from_rgba(rgba, w as u32, h as u32))
            }
            Ok(None) => None,
            Err(e) => {
                tracing::warn!("decode error: {e:?}");
                None
            }
        }
    }

    #[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32"))]
    pub fn decode_nal(&mut self, _nal_data: &[u8]) -> Option<Frame> {
        tracing::warn!("Use native_stream for mobile video decoding");
        None
    }

    pub fn state(&self) -> VideoState {
        self.state
    }
}

impl Default for VideoDecoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Video player with playback controls
///
/// Wraps a decoder and provides play/pause/seek/stop.
/// Frames are delivered to a callback or polled via `current_frame()`.
///
/// Desktop: decodes locally via OpenH264
/// Mobile: delegates to platform player via native bridge
///
/// Cloning shares the same playback state — all clones see the same
/// frames, position, and play/pause state via the inner `Arc<Mutex>`.
#[derive(Clone)]
pub struct VideoPlayer {
    state: std::sync::Arc<std::sync::Mutex<VideoPlayerInner>>,
    #[allow(dead_code)]
    player_id: u64,
    pub playing_signal: blinc_core::State<bool>,
    pub position_signal: blinc_core::State<u64>,
    pub duration_signal: blinc_core::State<u64>,
    pub volume_signal: blinc_core::State<f32>,
    /// End of the streaming buffer in ms. Drives the "ghost" fill
    /// behind the seek bar (the part of the timeline that's already
    /// been downloaded and is safe to scrub into). On native this
    /// tracks `duration_signal`; on web it polls `<video>.buffered`.
    pub buffered_signal: blinc_core::State<u64>,
}

struct VideoPlayerInner {
    playback_state: VideoState,
    volume: f32,
    current_frame: Option<std::sync::Arc<Frame>>,
    source: Option<String>,
    position_ms: u64,
    duration_ms: u64,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    generation: u64,
    /// Incremented each time a new decoded frame is stored.
    /// Consumers compare against their last-seen value to skip
    /// redundant GPU uploads when the frame hasn't changed.
    frame_generation: u64,
    #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
    source_bytes: Option<std::sync::Arc<Vec<u8>>>,
}

static PLAYER_ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl VideoPlayer {
    pub fn new() -> Self {
        let id = PLAYER_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let playing_signal = blinc_core::use_state_keyed(&format!("vp:{id}:playing"), || false);
        let position_signal = blinc_core::use_state_keyed(&format!("vp:{id}:position"), || 0u64);
        let duration_signal = blinc_core::use_state_keyed(&format!("vp:{id}:duration"), || 0u64);
        let volume_signal = blinc_core::use_state_keyed(&format!("vp:{id}:volume"), || 1.0f32);
        let buffered_signal = blinc_core::use_state_keyed(&format!("vp:{id}:buffered"), || 0u64);

        let state = std::sync::Arc::new(std::sync::Mutex::new(VideoPlayerInner {
            playback_state: VideoState::Idle,
            volume: 1.0,
            current_frame: None,
            source: None,
            position_ms: 0,
            duration_ms: 0,
            generation: 0,
            frame_generation: 0,
            #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
            source_bytes: None,
        }));

        let state_for_tick = state.clone();
        let pos_for_tick = position_signal.clone();
        #[cfg(target_arch = "wasm32")]
        let dur_for_tick = duration_signal.clone();
        #[cfg(target_arch = "wasm32")]
        let playing_for_tick = playing_signal.clone();
        #[cfg(target_arch = "wasm32")]
        let buf_for_tick = buffered_signal.clone();
        #[cfg(target_arch = "wasm32")]
        let last_buffered: std::sync::Arc<std::sync::atomic::AtomicU64> =
            std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let _tick_id = id;
        if let Some(handle) = blinc_animation::try_get_scheduler() {
            let redraw_handle = handle.clone();
            handle.register_tick_callback(move |_dt| {
                #[cfg(target_arch = "wasm32")]
                {
                    let mut inner = state_for_tick.lock().unwrap();
                    let new_pos = crate::web_video::position_ms(_tick_id);
                    let new_dur = crate::web_video::duration_ms(_tick_id);
                    let pos_changed = inner.position_ms != new_pos;
                    let dur_changed = inner.duration_ms != new_dur;
                    let previous = inner.playback_state;
                    let playback = if crate::web_video::is_ended(_tick_id) {
                        VideoState::Ended
                    } else if crate::web_video::is_paused(_tick_id) {
                        VideoState::Paused
                    } else {
                        VideoState::Playing
                    };
                    inner.position_ms = new_pos;
                    inner.duration_ms = new_dur;
                    inner.playback_state = playback;
                    let needs_frame = pos_changed || inner.current_frame.is_none();
                    drop(inner);
                    if needs_frame {
                        if let Some(frame) = crate::web_video::capture_frame(_tick_id) {
                            let mut inner = state_for_tick.lock().unwrap();
                            inner.current_frame = Some(std::sync::Arc::new(frame));
                            inner.frame_generation += 1;
                            redraw_handle.request_redraw();
                        }
                    }
                    if pos_changed {
                        pos_for_tick.set(new_pos);
                    }
                    if dur_changed {
                        dur_for_tick.set(new_dur);
                    }
                    if previous != playback {
                        playing_for_tick.set(playback == VideoState::Playing);
                    }
                    let buffered = crate::web_video::buffered_end_ms(_tick_id);
                    if buffered
                        != last_buffered.swap(buffered, std::sync::atomic::Ordering::Relaxed)
                    {
                        buf_for_tick.set(buffered);
                    }
                }

                #[cfg(not(target_arch = "wasm32"))]
                {
                    let inner = state_for_tick.lock().unwrap();
                    if inner.playback_state == VideoState::Playing {
                        let pos = inner.position_ms;
                        drop(inner);
                        pos_for_tick.set(pos);
                        redraw_handle.request_redraw();
                    }
                }
            });
        }

        #[cfg(target_arch = "wasm32")]
        crate::web_video::create(id);

        Self {
            state,
            player_id: id,
            playing_signal,
            position_signal,
            duration_signal,
            volume_signal,
            buffered_signal,
        }
    }

    fn sync_signals(&self) {
        let inner = self.state.lock().unwrap();
        let pos = inner.position_ms;
        let dur = inner.duration_ms;
        let vol = inner.volume;
        drop(inner);
        self.position_signal.set(pos);
        self.duration_signal.set(dur);
        self.volume_signal.set(vol);
        // Native paths decode from an owned byte buffer — buffered
        // == duration once the file is on disk/in memory. Web
        // overrides this from its own polling loop, so skip there
        // to avoid clobbering the real buffered tip with duration.
        #[cfg(not(target_arch = "wasm32"))]
        self.buffered_signal.set(dur);
    }

    /// End of the streaming buffer in ms (how far the seek bar can
    /// safely scrub without waiting on download). Mirrors
    /// [`Self::buffered_signal`] but returns the current value
    /// without requiring signal subscription.
    pub fn buffered_ms(&self) -> u64 {
        self.buffered_signal.get()
    }

    /// Load a video source
    pub fn load(&self, path: &str) {
        {
            let mut inner = self.state.lock().unwrap();
            inner.source = Some(path.to_string());
            inner.playback_state = VideoState::Idle;
            inner.position_ms = 0;
        }
        self.sync_signals();

        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = blinc_core::native_bridge::native_call::<(), _>(
                "video",
                "load",
                vec![blinc_core::native_bridge::NativeValue::String(
                    path.to_string(),
                )],
            );
        }
    }

    /// Start or resume playback
    pub fn play(&self) {
        {
            let mut inner = self.state.lock().unwrap();
            if inner.playback_state == VideoState::Ended {
                drop(inner);
                self.replay();
                return;
            }
            inner.playback_state = VideoState::Playing;
        }
        self.playing_signal.set(true);
        if let Some(handle) = blinc_animation::try_get_scheduler() {
            handle.request_redraw();
        }

        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = blinc_core::native_bridge::native_call::<(), _>("video", "play", ());
        }
        #[cfg(target_arch = "wasm32")]
        crate::web_video::play(self.player_id);
    }

    /// Pause playback
    pub fn pause(&self) {
        self.state.lock().unwrap().playback_state = VideoState::Paused;
        self.playing_signal.set(false);
        if let Some(handle) = blinc_animation::try_get_scheduler() {
            handle.request_redraw();
        }

        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = blinc_core::native_bridge::native_call::<(), _>("video", "pause", ());
        }
        #[cfg(target_arch = "wasm32")]
        crate::web_video::pause(self.player_id);
    }

    /// Stop playback and reset position
    pub fn stop(&self) {
        {
            let mut inner = self.state.lock().unwrap();
            inner.playback_state = VideoState::Idle;
            inner.position_ms = 0;
            inner.current_frame = None;
        }
        self.sync_signals();

        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = blinc_core::native_bridge::native_call::<(), _>("video", "stop", ());
        }
        #[cfg(target_arch = "wasm32")]
        crate::web_video::pause(self.player_id);
    }

    /// Seek to a position in milliseconds.
    #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
    pub fn seek(&self, position_ms: u64) {
        let bytes = {
            let mut inner = self.state.lock().unwrap();
            inner.playback_state = VideoState::Idle;
            inner.position_ms = position_ms;
            inner.generation += 1;
            inner.source_bytes.clone()
        };
        self.position_signal.set(position_ms);
        std::thread::sleep(std::time::Duration::from_millis(15));
        if let Some(bytes) = bytes {
            self.start_decode_thread_from(bytes, position_ms);
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn seek(&self, position_ms: u64) {
        self.state.lock().unwrap().position_ms = position_ms;
        self.position_signal.set(position_ms);
        crate::web_video::seek(self.player_id, position_ms);
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub fn seek(&self, position_ms: u64) {
        self.state.lock().unwrap().position_ms = position_ms;
        self.sync_signals();
        let _ = blinc_core::native_bridge::native_call::<(), _>(
            "video",
            "seek",
            vec![blinc_core::native_bridge::NativeValue::Int64(
                position_ms as i64,
            )],
        );
    }

    /// Set volume (0.0 to 1.0)
    pub fn set_volume(&self, volume: f32) {
        self.state.lock().unwrap().volume = volume.clamp(0.0, 1.0);
        self.sync_signals();

        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = blinc_core::native_bridge::native_call::<(), _>(
                "video",
                "set_volume",
                vec![blinc_core::native_bridge::NativeValue::Float32(volume)],
            );
        }
        #[cfg(target_arch = "wasm32")]
        crate::web_video::set_volume(self.player_id, volume);
    }

    /// Get the current decoded frame (cheap Arc clone)
    pub fn current_frame(&self) -> Option<std::sync::Arc<Frame>> {
        self.state.lock().unwrap().current_frame.clone()
    }

    /// Get the current frame generation counter.
    /// Incremented each time a new decoded frame is stored.
    /// Consumers can compare against their last-seen value to skip
    /// redundant work (e.g. GPU texture uploads) when the frame
    /// hasn't changed.
    pub fn frame_generation(&self) -> u64 {
        self.state.lock().unwrap().frame_generation
    }

    /// Push a decoded frame (called by decoder thread or native bridge)
    pub fn push_frame(&self, frame: Frame) {
        let mut inner = self.state.lock().unwrap();
        inner.current_frame = Some(std::sync::Arc::new(frame));
        inner.frame_generation += 1;
        drop(inner);
        self.sync_signals();
    }

    /// Get playback state
    pub fn playback_state(&self) -> VideoState {
        self.state.lock().unwrap().playback_state
    }

    /// Get current position in milliseconds
    pub fn position_ms(&self) -> u64 {
        self.state.lock().unwrap().position_ms
    }

    /// Get duration in milliseconds
    pub fn duration_ms(&self) -> u64 {
        self.state.lock().unwrap().duration_ms
    }

    /// Get volume
    pub fn volume(&self) -> f32 {
        self.state.lock().unwrap().volume
    }

    /// Check if playing
    pub fn is_playing(&self) -> bool {
        self.playback_state() == VideoState::Playing
    }

    /// Load and play an MP4 file from disk.
    #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
    pub fn load_file(&self, path: &str) {
        let bytes =
            std::fs::read(path).unwrap_or_else(|e| panic!("failed to read video: {path}: {e}"));
        self.load_bytes(bytes);
    }

    /// Replay from the beginning using stored source bytes.
    #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
    pub fn replay(&self) {
        let bytes = {
            let mut inner = self.state.lock().unwrap();
            inner.playback_state = VideoState::Idle;
            inner.position_ms = 0;
            inner.current_frame = None;
            inner.source_bytes.clone()
        };
        self.sync_signals();
        std::thread::sleep(std::time::Duration::from_millis(10));
        if let Some(bytes) = bytes {
            self.start_decode_thread(bytes);
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn replay(&self) {
        {
            let mut inner = self.state.lock().unwrap();
            inner.playback_state = VideoState::Idle;
            inner.position_ms = 0;
            inner.current_frame = None;
        }
        crate::web_video::seek(self.player_id, 0);
        self.play();
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub fn replay(&self) {
        let _ = blinc_core::native_bridge::native_call::<(), _>("video", "replay", ());
    }

    /// Load and play an MP4 from in-memory bytes.
    #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
    pub fn load_bytes(&self, bytes: Vec<u8>) {
        let bytes = std::sync::Arc::new(bytes);
        self.state.lock().unwrap().source_bytes = Some(std::sync::Arc::clone(&bytes));
        self.start_decode_thread(bytes);
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load_bytes(&self, bytes: Vec<u8>) {
        crate::web_video::load_bytes(self.player_id, &bytes);
    }

    /// Load a video from a URL the platform's media stack can open
    /// directly. Skips any preload / byte-copy path.
    ///
    /// - **Web**: hands the URL to the `<video src>` attribute. The
    ///   browser runs HTTP range requests, progressive buffering,
    ///   seek-ahead, and MSE decoding — a shipped ~50 MB video
    ///   starts playing in hundreds of milliseconds instead of
    ///   waiting for the whole file to download.
    /// - **Native desktop**: reads the underlying file (FFmpeg
    ///   decoder) — the "URL" is a `file://<abs-path>` the
    ///   platform asset loader produced.
    /// - **Mobile (Android/iOS)**: forwards the path through the
    ///   native bridge so the platform's `MediaPlayer` / `AVPlayer`
    ///   streams the file over whatever transport it was fetched
    ///   from (local, `content://`, remote).
    ///
    /// Prefer this over `load_bytes` for shipped content; keep
    /// `load_bytes` for in-memory buffers without an addressable
    /// URL (test harnesses, procedurally generated clips).
    #[cfg(target_arch = "wasm32")]
    pub fn load_url(&self, url: &str) {
        {
            let mut inner = self.state.lock().unwrap();
            inner.source = Some(url.to_string());
            inner.playback_state = VideoState::Idle;
            inner.position_ms = 0;
        }
        self.sync_signals();
        crate::web_video::load_url(self.player_id, url);
    }

    /// Desktop: file URLs are resolved to bytes and forwarded to
    /// the decode thread. The URL path exists for API symmetry
    /// with the web target; on native, there's no separate
    /// streaming decoder — FFmpeg owns the byte buffer.
    #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
    pub fn load_url(&self, url: &str) {
        // `file:///abs/path` → `/abs/path`. On Windows the path
        // comes out as `file:///C:/...` — strip the three slashes.
        let path = url
            .strip_prefix("file:///")
            .map(std::string::ToString::to_string)
            .or_else(|| {
                url.strip_prefix("file://")
                    .map(std::string::ToString::to_string)
            })
            .unwrap_or_else(|| url.to_string());
        // Windows paths come out as `C:/...` after the prefix
        // strip — absolute already. Unix paths from `file://host/x`
        // (no triple-slash) come out without the leading `/`.
        let path = if cfg!(target_os = "windows") || path.starts_with('/') {
            path
        } else {
            format!("/{path}")
        };
        self.load_file(&path);
    }

    /// Mobile: forward the URL through the native bridge; the
    /// platform's `AVPlayer` / `MediaPlayer` handles streaming.
    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub fn load_url(&self, url: &str) {
        self.load(url);
    }

    #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
    fn start_decode_thread(&self, bytes: std::sync::Arc<Vec<u8>>) {
        self.start_decode_thread_from(bytes, 0);
    }

    #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
    fn start_decode_thread_from(&self, bytes: std::sync::Arc<Vec<u8>>, start_ms: u64) {
        let file_size = bytes.len() as u64;

        let mp4 = mp4::Mp4Reader::read_header(std::io::Cursor::new(bytes.as_ref()), file_size)
            .unwrap_or_else(|e| panic!("failed to parse MP4: {e}"));

        let video_track_id = mp4
            .tracks()
            .values()
            .find(|t| t.media_type().ok() == Some(mp4::MediaType::H264))
            .map(|t| t.track_id())
            .unwrap_or_else(|| panic!("no H.264 video track found"));

        let track = &mp4.tracks()[&video_track_id];
        let sample_count = track.sample_count();
        let duration_ms = track.duration().as_millis() as u64;

        let my_generation;
        {
            let mut inner = self.state.lock().unwrap();
            inner.duration_ms = duration_ms;
            inner.playback_state = VideoState::Playing;
            my_generation = inner.generation;
        }
        self.playing_signal.set(true);
        self.sync_signals();

        // Build the bitstream converter (MP4 length-prefixed → Annex B)
        // following the openh264 crate's official Mp4BitstreamConverter pattern
        let avc1 = track
            .trak
            .mdia
            .minf
            .stbl
            .stsd
            .avc1
            .as_ref()
            .expect("no AVC1 config in video track");
        let avcc = &avc1.avcc;
        let length_size = avcc.length_size_minus_one + 1;
        let sps_list: Vec<Vec<u8>> = avcc
            .sequence_parameter_sets
            .iter()
            .map(|n| n.bytes.clone())
            .collect();
        let pps_list: Vec<Vec<u8>> = avcc
            .picture_parameter_sets
            .iter()
            .map(|n| n.bytes.clone())
            .collect();

        let state = self.state.clone();
        let playing_sig = self.playing_signal.clone();
        let position_sig = self.position_signal.clone();

        std::thread::spawn(move || {
            let mut decoder = VideoDecoder::new();

            tracing::info!(
                "video: decode start — {} SPS, {} PPS, {} samples, {}ms, length_size={}",
                sps_list.len(),
                pps_list.len(),
                sample_count,
                duration_ms,
                length_size,
            );

            let mut mp4 = match mp4::Mp4Reader::read_header(
                std::io::Cursor::new(bytes.as_ref()),
                file_size,
            ) {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("failed to re-parse MP4: {e}");
                    return;
                }
            };

            let frame_duration = if sample_count > 0 {
                std::time::Duration::from_millis(duration_ms / sample_count as u64)
            } else {
                std::time::Duration::from_millis(33)
            };

            let mut decoded_count: u32 = 0;
            let mut playback_clock: Option<std::time::Instant> = None;

            // Calculate starting sample from seek position, snap to nearest keyframe
            let target_sample = if start_ms > 0 && duration_ms > 0 {
                let s = ((start_ms as f64 / duration_ms as f64) * sample_count as f64) as u32;
                s.min(sample_count).max(1)
            } else {
                1
            };

            // Find nearest sync sample (keyframe) at or before target
            let track_ref = &mp4.tracks()[&video_track_id];
            let start_sample = if let Some(ref stss) = track_ref.trak.mdia.minf.stbl.stss {
                let mut best = 1u32;
                for &sync_id in &stss.entries {
                    if sync_id <= target_sample {
                        best = sync_id;
                    } else {
                        break;
                    }
                }
                best
            } else {
                target_sample
            };

            tracing::info!(
                "video: seek to {}ms → target sample {}, keyframe {}",
                start_ms,
                target_sample,
                start_sample
            );
            let mut annex_b = Vec::new();

            // Feed SPS/PPS to decoder so it can decode from any position
            for sps in &sps_list {
                let mut buf = vec![0u8, 0, 0, 1];
                buf.extend_from_slice(sps);
                decoder.decode_nal(&buf);
            }
            for pps in &pps_list {
                let mut buf = vec![0u8, 0, 0, 1];
                buf.extend_from_slice(pps);
                decoder.decode_nal(&buf);
            }

            // Bitstream converter state (mirrors openh264 example)
            let mut new_idr = true;
            let mut sps_seen = false;
            let mut pps_seen = false;

            for sample_idx in start_sample..=sample_count {
                loop {
                    let inner = state.lock().unwrap();
                    if inner.generation != my_generation {
                        return;
                    }
                    let ps = inner.playback_state;
                    drop(inner);
                    match ps {
                        VideoState::Playing => break,
                        VideoState::Paused => {
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            continue;
                        }
                        VideoState::Idle | VideoState::Ended => return,
                    }
                }

                let sample = match mp4.read_sample(video_track_id, sample_idx) {
                    Ok(Some(s)) => s,
                    Ok(None) => break,
                    Err(e) => {
                        tracing::warn!("failed to read sample {sample_idx}: {e}");
                        continue;
                    }
                };

                // Convert MP4 length-prefixed NALUs → Annex B with SPS/PPS injection
                annex_b.clear();
                let mut stream = sample.bytes.as_ref();

                while !stream.is_empty() {
                    if stream.len() < length_size as usize {
                        break;
                    }
                    let mut nal_size: u32 = 0;
                    for &byte in stream.iter().take(length_size as usize) {
                        nal_size = (nal_size << 8) | byte as u32;
                    }
                    stream = &stream[length_size as usize..];

                    if nal_size == 0 || nal_size as usize > stream.len() {
                        break;
                    }

                    let nal_data = &stream[..nal_size as usize];
                    let nal_type = nal_data[0] & 0x1F;
                    stream = &stream[nal_size as usize..];

                    match nal_type {
                        7 => sps_seen = true, // SPS
                        8 => pps_seen = true, // PPS
                        5 => {
                            // IDR slice — inject SPS/PPS if not already in-stream
                            if !new_idr && nal_data.len() > 1 && nal_data[1] & 0x80 != 0 {
                                new_idr = true;
                            }
                            if new_idr && !sps_seen && !pps_seen {
                                new_idr = false;
                                for sps in &sps_list {
                                    annex_b.extend_from_slice(&[0, 0, 1]);
                                    annex_b.extend_from_slice(sps);
                                }
                                for pps in &pps_list {
                                    annex_b.extend_from_slice(&[0, 0, 1]);
                                    annex_b.extend_from_slice(pps);
                                }
                            }
                            if new_idr && sps_seen && !pps_seen {
                                for pps in &pps_list {
                                    annex_b.extend_from_slice(&[0, 0, 1]);
                                    annex_b.extend_from_slice(pps);
                                }
                            }
                        }
                        _ => {}
                    }

                    annex_b.extend_from_slice(&[0, 0, 1]);
                    annex_b.extend_from_slice(nal_data);

                    if !new_idr && nal_type == 1 {
                        // Non-IDR slice — reset for next IDR
                        new_idr = true;
                        sps_seen = false;
                        pps_seen = false;
                    }
                }

                if let Some(frame) = decoder.decode_nal(&annex_b) {
                    decoded_count += 1;
                    // Skip display for samples before the seek target (decoder warmup)
                    if sample_idx >= target_sample {
                        let mut inner = state.lock().unwrap();
                        inner.current_frame = Some(std::sync::Arc::new(frame));
                        inner.frame_generation += 1;
                        inner.position_ms = (sample_idx as u64 * duration_ms) / sample_count as u64;
                    }
                }

                // Wall-clock pacing: sleep only for remaining time after decode
                if sample_idx >= target_sample {
                    let clock = playback_clock.get_or_insert_with(std::time::Instant::now);
                    let frames_since_start = (sample_idx - target_sample) + 1;
                    let target_time = *clock + frame_duration * frames_since_start;
                    let now = std::time::Instant::now();
                    if target_time > now {
                        std::thread::sleep(target_time - now);
                    }
                }
            }

            // Flush remaining buffered frames
            if let Some(ref mut raw_decoder) = decoder.decoder {
                if let Ok(remaining) = raw_decoder.flush_remaining() {
                    for yuv in remaining {
                        let (uv_w, uv_h) = yuv.dimensions_uv();
                        let w = uv_w * 2;
                        let h = uv_h * 2;
                        let mut rgba = vec![0u8; w * h * 4];
                        yuv.write_rgba8(&mut rgba);
                        decoded_count += 1;
                        let mut inner = state.lock().unwrap();
                        inner.current_frame = Some(std::sync::Arc::new(Frame::from_rgba(
                            rgba, w as u32, h as u32,
                        )));
                        inner.frame_generation += 1;
                        inner.position_ms = duration_ms;
                    }
                }
            }

            tracing::info!("video: decode complete — {decoded_count}/{sample_count} frames");

            state.lock().unwrap().playback_state = VideoState::Ended;
            playing_sig.set(false);
            position_sig.set(duration_ms);
        });
    }
}

impl Default for VideoPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::player::Player for VideoPlayer {
    fn play(&self) {
        VideoPlayer::play(self);
    }
    fn pause(&self) {
        VideoPlayer::pause(self);
    }
    fn stop(&self) {
        VideoPlayer::stop(self);
    }
    fn seek(&self, position_ms: u64) {
        VideoPlayer::seek(self, position_ms);
    }
    fn position_ms(&self) -> u64 {
        VideoPlayer::position_ms(self)
    }
    fn duration_ms(&self) -> u64 {
        VideoPlayer::duration_ms(self)
    }
    fn volume(&self) -> f32 {
        VideoPlayer::volume(self)
    }
    fn set_volume(&self, volume: f32) {
        VideoPlayer::set_volume(self, volume);
    }
    fn is_playing(&self) -> bool {
        VideoPlayer::is_playing(self)
    }
    fn buffered_ms(&self) -> u64 {
        VideoPlayer::buffered_ms(self)
    }
}
