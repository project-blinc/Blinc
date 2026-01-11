//! Session management for recordings.
//!
//! This module contains:
//! - `RecordingSession` - The main state machine for recording
//! - `RecordingConfig` - Configuration presets for different use cases

mod config;
mod recording;

pub use config::*;
pub use recording::*;
