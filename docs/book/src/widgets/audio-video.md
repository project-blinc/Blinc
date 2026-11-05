# Audio & Video

The `blinc_media` crate provides cross-platform audio/video with royalty-free codecs.
Media widgets in `blinc_layout` (behind the `media` feature) provide player UIs.

## Audio Playback

```rust
use blinc_media::{AudioPlayer, AudioSource};

let player = AudioPlayer::new();
player.play(AudioSource::file("music.ogg"));
player.set_volume(0.8);
player.pause();
player.seek(30_000); // seek to 30s
player.resume();

println!("Position: {}ms", player.position_ms());
```

Desktop: Vorbis, WAV, FLAC via rodio. Mobile: platform codecs via native bridge.

## Video Playback

```rust
use blinc_media::{VideoPlayer, VideoDecoder};

let mut decoder = VideoDecoder::new();
let player = VideoPlayer::new();

// Decode H.264 NAL units → RGBA frames
if let Some(frame) = decoder.decode_nal(h264_packet) {
    player.push_frame(frame);
}

player.play();
player.seek(10_000);
```

Desktop: OpenH264 (royalty-free). Mobile: platform decoders via native bridge.

## Player Trait

Both players implement the shared `Player` trait:

```rust
use blinc_media::Player;

fn show_status(p: &dyn Player) {
    println!("{} / {} | vol: {}", p.position_ms(), p.duration_ms(), p.volume());
}
```

| Method | Description |
|--------|-------------|
| `play()` / `pause()` / `stop()` | Playback controls |
| `seek(ms)` | Seek to position |
| `position_ms()` / `duration_ms()` | Time tracking |
| `volume()` / `set_volume(f32)` | Volume (0.0–1.0) |
| `is_playing()` / `is_live()` | State queries |

## Audio Widget

```rust
use std::rc::Rc;
use blinc_layout::widgets::media::audio_player;

let player = Rc::new(AudioPlayer::new());

// Basic controls
audio_player(Rc::clone(&player)).w_full().into_div()

// With waveform
audio_player(Rc::clone(&player))
    .waveform_data(&samples)
    .w_full()
    .into_div()
```

The waveform follows the player's position and duration: its played region
advances during playback, holds when paused, and follows seeks. The waveform
asks for animation frames only while its player is playing.

## Video Widget

```rust
use std::rc::Rc;
use blinc_layout::widgets::media::video_player;

let player = Rc::new(VideoPlayer::new());

video_player(Rc::clone(&player))
    .show_dimensions()
    .w_full()
    .h(400.0)
    .into_div()
```

## Waveform

Standalone amplitude visualization:

```rust
use blinc_layout::widgets::media::waveform;

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
use blinc_layout::widgets::media::MediaControls;

MediaControls::new(player_rc).class("my-controls").into_div()
```

Layout: `[ ▶ ] [ 1:23 / 3:45 ] [ ══seek══ ] [ 80% ]`

Live streams: `[ ▶ ] [ LIVE ] [ ════════════ ]`

## Camera & Recording

```rust
use blinc_media::rtc::{CameraStream, CameraConfig, AudioRecorder};

let camera = CameraStream::open(CameraConfig::default());
let frame = camera.latest_frame(); // RGBA Frame

let recorder = AudioRecorder::open(Default::default());
let samples = recorder.latest_samples(); // AudioSamples

drop(camera);   // stops capture
drop(recorder);  // stops recording
```

## Frame Utilities

```rust
use blinc_media::{Frame, AudioSamples};

// Video
let small = Frame::from_rgba(data, 640, 480).scale(320, 240);
let gray = small.to_gray();

// Audio
let mono = AudioSamples::from_f32(&pcm, 2, 44100).to_mono();
let resampled = mono.resample(48000);
```

## CSS Classes

The controls take their colours from the default stylesheet's rules, so they
follow the theme and a stylesheet restyles them:

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

The video seek bar, the volume level and the waveform are drawn, in colours
the stylesheet's variables give:

| Variable | Draws | Default |
|----------|-------|---------|
| `--blinc-media-track` | the seek bar's track | `--surface-elevated` |
| `--blinc-media-buffered` | what is buffered ahead | `--border` |
| `--blinc-media-progress` | what has played | `--primary` |
| `--blinc-media-level`, `--blinc-media-level-rest` | the volume columns at and above the level | `--surface-elevated`, `--text-secondary` |
| `--blinc-waveform-played`, `--blinc-waveform-unplayed` | the waveform's bars | `--primary`, `--border-secondary` |

```css
:root { --blinc-media-progress: #22c55e; }
.blinc-media-play-btn { background: var(--primary); color: var(--text-inverse); }
```

`waveform(..).played_color(..)` and `.unplayed_color(..)` set the waveform's
colours over the variables.

## Licensing

Desktop uses royalty-free codecs only — no ffmpeg, no patent fees:

| Codec | License |
|-------|---------|
| Vorbis, WAV, FLAC | BSD / Public domain |
| OpenH264 | BSD, Cisco covers patents |

Mobile uses OS-provided codecs (licensing handled by the OS).

## Gallery samples

The component gallery has separate Audio Player, Video Player and Waveform pages.
Their native recordings use ashui’s original Night Drive soundtrack and outdoor
handpan session. The waveform uses amplitude peaks from the same Night Drive
excerpt. Source assets and conversion details live in
`examples/blinc_component_demos/assets/README.md`.

Native recordings start with audio paused and muted. Browser playback starts
only after Play; the video sample is silent. Native playback needs the
`blinc_layout` `media` feature.

The gallery connects browser media decoding to the dedicated worker. Controls,
time, volume, seek and decoded video frames use the real Blinc players. Hiding a
Live demo pauses playback; disposing it releases its media elements and frame
transport.
