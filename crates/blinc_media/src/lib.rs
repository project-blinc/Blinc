//! Cross-platform media for Blinc
//!
//! Audio playback, video decoding, and frame data utilities.
//! Desktop uses royalty-free codecs (Vorbis/Opus, OpenH264, VP9/AV1).
//! Mobile delegates to platform codecs via native bridge.
//!
//! # Audio
//!
//! ```ignore
//! use blinc_media::audio::{AudioPlayer, AudioSource};
//!
//! let player = AudioPlayer::new();
//! player.play(AudioSource::file("music.ogg"));
//! player.set_volume(0.8);
//! ```
//!
//! # Frame Utilities
//!
//! ```ignore
//! use blinc_media::frame::{Frame, PixelFormat};
//!
//! let frame = Frame::from_rgba(rgba_bytes, 640, 480);
//! let scaled = frame.scale(320, 240);
//! let gray = frame.to_gray();
//! ```

pub mod audio;
pub mod frame;
pub mod player;
pub mod rtc;
pub mod video;
#[cfg(target_arch = "wasm32")]
pub(crate) mod web_video;

#[cfg(target_arch = "wasm32")]
pub use web_video::set_web_media_bridge;

pub use audio::{AudioPlayer, AudioSource, PlaybackState};
pub use frame::{AudioSamples, Frame, PixelFormat, SampleFormat};
pub use player::Player;
pub use rtc::{AudioRecorder, AudioRecorderConfig, CameraConfig, CameraFacing, CameraStream};
pub use video::{VideoDecoder, VideoFrame, VideoPlayer, VideoState};
