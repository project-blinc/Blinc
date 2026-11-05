# Changelog

All notable changes to `blinc_media` will be documented in this file.

## [Unreleased]

### Added
- Browser media can use a host bridge when rendering in an OffscreenCanvas worker. Browser audio uses the same URL streaming engine; playback state, duration, buffering and decoded video frames follow the browser transport.

## [0.6.0] - 2026-11-04

### Changed
- **Bumped `rodio` 0.19 → 0.22** (closes #31). Decoders route through `symphonia` (`vorbis` / `wav` / `flac` features map to `symphonia/{vorbis,wav,flac}`); the `recording` feature pulls `cpal` directly and rodio re-exports it as `rodio::cpal`.
- Dropped the standalone `cpal = "0.17"` direct dep — `rtc.rs` now consumes `cpal` through `rodio::cpal`. The workspace consequently sees a single `cpal` version (was duplicated between rodio 0.19's bundled `cpal 0.15.3` and the standalone `cpal 0.17.1`). Saves ~14 transitive crates workspace-wide.
- `AudioPlayerInner` field shape: dropped `stream_handle: Option<OutputStreamHandle>` (handle is now intrinsic to the device sink); renamed `_stream` from `OutputStream` to `MixerDeviceSink`; renamed the active sink type from `rodio::Sink` to `rodio::Player` (rodio 0.22's terminology).
- API call updates inside `audio.rs`:
  - `rodio::OutputStream::try_default()` (returned a tuple) → `rodio::DeviceSinkBuilder::open_default_sink()` (returns `MixerDeviceSink` directly).
  - `rodio::Sink::try_new(&handle)` → `rodio::Player::connect_new(stream.mixer())`.
  - `rodio::Decoder::new(reader)` kept; format autodetect now goes through symphonia.

## [0.5.1] - 2026-04-13

### Added
- MP4 demuxer + `VideoPlayer::load_file` for desktop H.264 playback via OpenH264
- `VideoPlayer::load_bytes` for in-memory MP4 decoding
- Browser-native `<video>` playback via `web_video.rs` (wasm32): `create`, `load_bytes`, `play`, `pause`, `seek`, `set_volume`, `capture_frame`
- `video_player()` widget with signal-driven controls (play/pause, seek bar, time display, volume)
- `VideoPlayer::frame_generation()` counter for consumer-side redundant upload skip
- `SdfVertexInstance` struct for vertex-buffer-based SDF vertex data (VERTEX_STORAGE fallback)

### Fixed
- Native audio retains decoded source duration so waveform progress can use the actual track length.
- Wasm video: `duration_signal` now propagated from tick callback (was always 0:00)
- Wasm video: `playing_signal.set(false)` on video end (button no longer stuck on pause)
- Wasm video: `load_bytes` no longer prematurely sets playing state before user interaction
- Wasm video: frame capture skipped when `currentTime` unchanged (saves ~8MB/tick at 1080p)
- Desktop video: wall-clock pacing replaces fixed `thread::sleep` per frame (no more playback drift)
- Video seek bar wrapped in own Stateful with position/duration deps (was invisible until volume clicked)
- Video surface canvas caches last `Arc<Frame>` to skip mutex lock + Arc clone on unchanged frames
- `BlobPropertyBag::type_` → `set_type` (deprecated API warning)
- `#[cfg_attr(target_arch = "wasm32", allow(dead_code))]` on `generation` field
- `#[allow(dead_code)]` on `is_paused` (reserved for future use)

## [0.4.0] - 2026-04-05

### Added
- `AudioPlayer` — Desktop: rodio (Vorbis/WAV/FLAC), Mobile: native bridge
- `VideoDecoder` — Desktop: OpenH264 (H.264 NAL to RGBA), Mobile: native bridge
- `VideoPlayer` — play/pause/seek/volume, frame push API
- `CameraStream` — RTC-like reactive capture with RGBA frame delivery
- `AudioRecorder` — Desktop: cpal, Mobile: native bridge stream
- `Player` trait shared by `AudioPlayer`, `VideoPlayer`, and live streams
- `Player::is_live()` for live stream detection (LIVE badge, seek-less controls)
- `Frame` struct: RGBA/RGB/BGRA/YUV420/Gray conversion, scale, format convert
- `AudioSamples` struct: f32/i16/u8 conversion, resample, mono downmix
- `audio_player()` widget — waveform canvas, `MediaControls` via `Player` trait
- `video_player()` widget — canvas surface, shared controls, dimensions
- All widget elements have CSS classes (`.blinc-media-*`, `.blinc-audio-*`, `.blinc-video-*`)
