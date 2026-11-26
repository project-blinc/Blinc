# Audio & Video

To use the playback and capture APIs in this chapter, enable the `media` feature on your `blinc` dependency.

The `blinc_media` crate provides cross-platform audio and video with royalty-free codecs.
Media widgets in `blinc_layout` (behind the `media` feature) provide player UIs.

## Audio Playback

```rust
use blinc::media::{AudioPlayer, AudioSource};

let player = AudioPlayer::new();
player.play(AudioSource::file("music.ogg"));
player.set_volume(0.8);
player.pause();
player.seek(30_000); // seek to 30s
player.resume();

println!("Position: {}ms", player.position_ms());
```

Desktop plays Vorbis, WAV, and FLAC through rodio. Mobile uses platform codecs through a native bridge.

## Video Playback

```rust
use blinc::media::{VideoPlayer, VideoDecoder};

let mut decoder = VideoDecoder::new();
let player = VideoPlayer::new();

// Decode H.264 NAL units → RGBA frames
if let Some(frame) = decoder.decode_nal(h264_packet) {
    player.push_frame(frame);
}

player.play();
player.seek(10_000);
```

Desktop uses OpenH264 (royalty-free). Mobile uses platform decoders through a native bridge.

## Player Trait

Both players implement the shared `Player` trait:

```rust
use blinc::media::Player;

fn show_status(p: &dyn Player) {
    println!("{} / {} | vol: {}", p.position_ms(), p.duration_ms(), p.volume());
}
```

| Method | Description |
|--------|-------------|
| `play()` / `pause()` / `stop()` | Playback controls |
| `seek(ms)` | Seek to a position |
| `position_ms()` / `duration_ms()` | Time tracking |
| `volume()` / `set_volume(f32)` | Volume (0.0–1.0) |
| `is_playing()` / `is_live()` | State queries |

On desktop, a seek past the end of an audio track goes to the end. After a track finishes, a seek
leaves the audio player paused at the new position, and `resume()` plays from there.

## Audio Widget

```rust
use std::rc::Rc;
use blinc::layout::widgets::media::audio_player;

let player = Rc::new(AudioPlayer::new());

// Basic controls
audio_player(Rc::clone(&player)).w_full().into_div()

// With waveform
audio_player(Rc::clone(&player))
    .waveform_data(&samples)
    .w_full()
    .into_div()
```

The waveform follows the position and duration of the player. Its played region
advances during playback, holds during a pause, and follows seeks. The waveform
requests animation frames only while its player plays.

## Video Widget

```rust
use std::rc::Rc;
use blinc::layout::widgets::media::video_player;

let player = Rc::new(VideoPlayer::new());

video_player(Rc::clone(&player))
    .w_full()
    .h(400.0)
    .into_div()
```

## Waveform

A standalone view of amplitude. `buckets` is an `Arc<Vec<f32>>` of amplitudes:

```rust
use blinc::layout::widgets::media::waveform;

waveform(buckets)
    .progress(0.5)
    .played_color(Color::BLUE)
    .unplayed_color(Color::GRAY)
    .w_full().h(60.0)
    .into_div()
```

## Shared Controls

`MediaControls` is generic over `Player`:

```rust
use blinc::layout::widgets::media::MediaControls;

MediaControls::new(player_rc).class("my-controls").into_div()
```

Layout: `[ ▶ ] [ 1:23 / 3:45 ] [ ══seek══ ] [ 80% ]`

Live streams: `[ ▶ ] [ LIVE ] [ ════════════ ]`

## Camera & Recording

```rust
use blinc::media::rtc::{CameraStream, CameraConfig, AudioRecorder};

let camera = CameraStream::open(CameraConfig::default());
let frame = camera.latest_frame(); // RGBA Frame

let recorder = AudioRecorder::open(Default::default());
let samples = recorder.latest_samples(); // AudioSamples

drop(camera);   // stops capture
drop(recorder);  // stops recording
```

## Frame Utilities

```rust
use blinc::media::{Frame, AudioSamples};

// Video
let small = Frame::from_rgba(data, 640, 480).scale(320, 240);
let gray = small.to_gray();

// Audio
let mono = AudioSamples::from_f32(&pcm, 2, 44100).to_mono();
let resampled = mono.resample(48000);
```

## CSS Classes

The controls take their colors from the rules of the default stylesheet. As a
result, they follow the theme, and a stylesheet can restyle them:

| Class | Element | Default |
|-------|---------|---------|
| `.blinc-audio-player` | Audio container | |
| `.blinc-video-player` | Video container | |
| `.blinc-audio-waveform`, `.blinc-waveform` | Waveform | see below |
| `.blinc-media-controls` | Controls row | |
| `.blinc-media-play-btn` | Play/pause | `--surface-elevated` fill, `--text-primary` icon |
| `.blinc-media-time` | Time display | `--text-tertiary` |
| `.blinc-media-live-badge` | LIVE indicator | `--error` fill, white text |
| `.blinc-media-seek`, `.blinc-media-seek-track` | Seek bar | see below |
| `.blinc-media-seek-rail` | Seek bar track | `--surface-elevated` |
| `.blinc-media-seek-fill` | Seek progress | `--primary` |
| `.blinc-media-volume` | Volume | `--text-tertiary` |
| `.blinc-media-mute-btn` | Mute | `--text-secondary` |

Blinc draws the video seek bar, the volume level, and the waveform. Their colors
come from these stylesheet variables:

| Variable | Draws | Default |
|----------|-------|---------|
| `--blinc-media-track` | the track of the seek bar | `--surface-elevated` |
| `--blinc-media-buffered` | what is buffered ahead | `--border` |
| `--blinc-media-progress` | what already played | `--primary` |
| `--blinc-media-level`, `--blinc-media-level-rest` | the volume columns up to the level, and the columns above it | `--surface-elevated`, `--text-secondary` |
| `--blinc-waveform-played`, `--blinc-waveform-unplayed` | the bars of the waveform | `--primary`, `--border-secondary` |

```css
:root { --blinc-media-progress: #22c55e; }
.blinc-media-play-btn { background: var(--primary); color: var(--text-inverse); }
```

`waveform(..).played_color(..)` and `.unplayed_color(..)` set the colors of the
waveform. These colors override the variables.

## Licensing

Desktop uses only royalty-free codecs. It uses no ffmpeg and has no patent fees:

| Codec | License |
|-------|---------|
| Vorbis, WAV, FLAC | BSD / Public domain |
| OpenH264 | BSD, Cisco covers patents |

Mobile uses the codecs that the OS provides. The OS handles their licensing.

## Gallery samples

The component gallery has separate pages for Audio Player, Video Player, and
Waveform. Their native recordings use the original Night Drive soundtrack and an
outdoor handpan session. The waveform uses amplitude peaks from the same Night
Drive excerpt. For the source assets and conversion details, see
`examples/blinc_component_demos/assets/README.md`.

Native recordings start with the audio paused and muted. Browser playback starts
only after you press Play. The video sample has sound, which plays after you press Play. Native playback needs the
`media` feature of `blinc_layout`.

The gallery connects media decoding in the browser to the dedicated worker.
Controls, time, volume, seek, and decoded video frames use the real Blinc
players. A hidden Live demo pauses playback. A disposed demo releases its media
elements and frame transport.
