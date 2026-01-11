//! Recording configuration presets.

use serde::{Deserialize, Serialize};

/// Configuration for a recording session.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordingConfig {
    /// Maximum number of events to store (ring buffer size).
    pub max_events: usize,
    /// Maximum number of tree snapshots to store.
    pub max_snapshots: usize,
    /// Whether to capture mouse move events (can be noisy).
    pub capture_mouse_moves: bool,
    /// Minimum interval between mouse move captures (ms).
    pub mouse_move_throttle_ms: u32,
    /// Whether to capture tree snapshots on every frame.
    pub capture_every_frame: bool,
    /// Whether to capture visual properties in snapshots.
    pub capture_visual_props: bool,
    /// Whether to capture text content in snapshots.
    pub capture_text_content: bool,
    /// Application name (for debug server identification).
    pub app_name: String,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self::standard()
    }
}

impl RecordingConfig {
    /// Standard configuration for general use.
    pub fn standard() -> Self {
        Self {
            max_events: 10_000,
            max_snapshots: 100,
            capture_mouse_moves: false,
            mouse_move_throttle_ms: 16, // ~60fps
            capture_every_frame: false,
            capture_visual_props: false,
            capture_text_content: false,
            app_name: "blinc_app".to_string(),
        }
    }

    /// Debug configuration with more verbose capture.
    pub fn debug() -> Self {
        Self {
            max_events: 50_000,
            max_snapshots: 500,
            capture_mouse_moves: true,
            mouse_move_throttle_ms: 16,
            capture_every_frame: true,
            capture_visual_props: true,
            capture_text_content: true,
            app_name: "blinc_app".to_string(),
        }
    }

    /// Minimal configuration for low overhead.
    pub fn minimal() -> Self {
        Self {
            max_events: 1_000,
            max_snapshots: 10,
            capture_mouse_moves: false,
            mouse_move_throttle_ms: 100,
            capture_every_frame: false,
            capture_visual_props: false,
            capture_text_content: false,
            app_name: "blinc_app".to_string(),
        }
    }

    /// Testing configuration optimized for test runs.
    pub fn testing() -> Self {
        Self {
            max_events: 10_000,
            max_snapshots: 1000,
            capture_mouse_moves: true,
            mouse_move_throttle_ms: 0, // No throttle for determinism
            capture_every_frame: true,
            capture_visual_props: true,
            capture_text_content: true,
            app_name: "blinc_test".to_string(),
        }
    }

    /// Set the application name.
    pub fn with_app_name(mut self, name: impl Into<String>) -> Self {
        self.app_name = name.into();
        self
    }

    /// Set the maximum number of events.
    pub fn with_max_events(mut self, max: usize) -> Self {
        self.max_events = max;
        self
    }

    /// Set the maximum number of snapshots.
    pub fn with_max_snapshots(mut self, max: usize) -> Self {
        self.max_snapshots = max;
        self
    }

    /// Enable or disable mouse move capture.
    pub fn with_mouse_moves(mut self, capture: bool) -> Self {
        self.capture_mouse_moves = capture;
        self
    }

    /// Enable or disable per-frame snapshot capture.
    pub fn with_every_frame(mut self, capture: bool) -> Self {
        self.capture_every_frame = capture;
        self
    }

    /// Enable or disable visual property capture.
    pub fn with_visual_props(mut self, capture: bool) -> Self {
        self.capture_visual_props = capture;
        self
    }
}
