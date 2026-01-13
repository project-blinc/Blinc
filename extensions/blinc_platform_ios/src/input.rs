//! iOS touch input handling
//!
//! Converts UITouch events to Blinc input events.

use blinc_platform::{InputEvent, TouchEvent};

/// Touch phase from UITouch
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    /// Touch began
    Began,
    /// Touch moved
    Moved,
    /// Touch ended
    Ended,
    /// Touch cancelled
    Cancelled,
}

/// A single touch point
#[derive(Clone, Debug)]
pub struct Touch {
    /// Unique identifier for this touch
    pub id: u64,
    /// X position in logical pixels
    pub x: f32,
    /// Y position in logical pixels
    pub y: f32,
    /// Touch phase
    pub phase: TouchPhase,
    /// Force of the touch (0.0 - 1.0 on 3D Touch devices)
    pub force: f32,
}

impl Touch {
    /// Create a new touch event
    pub fn new(id: u64, x: f32, y: f32, phase: TouchPhase) -> Self {
        Self {
            id,
            x,
            y,
            phase,
            force: 0.0,
        }
    }

    /// Create a touch with force (3D Touch)
    pub fn with_force(id: u64, x: f32, y: f32, phase: TouchPhase, force: f32) -> Self {
        Self {
            id,
            x,
            y,
            phase,
            force,
        }
    }
}

/// Convert an iOS touch to a Blinc input event
pub fn convert_touch(touch: &Touch) -> InputEvent {
    match touch.phase {
        TouchPhase::Began => InputEvent::Touch(TouchEvent::Started {
            id: touch.id,
            x: touch.x,
            y: touch.y,
            pressure: touch.force,
        }),
        TouchPhase::Moved => InputEvent::Touch(TouchEvent::Moved {
            id: touch.id,
            x: touch.x,
            y: touch.y,
            pressure: touch.force,
        }),
        TouchPhase::Ended => InputEvent::Touch(TouchEvent::Ended {
            id: touch.id,
            x: touch.x,
            y: touch.y,
        }),
        TouchPhase::Cancelled => InputEvent::Touch(TouchEvent::Cancelled { id: touch.id }),
    }
}

/// Convert multiple touches to Blinc input events
pub fn convert_touches(touches: &[Touch]) -> Vec<InputEvent> {
    touches.iter().map(convert_touch).collect()
}

/// Gesture detector for common iOS gestures
#[derive(Debug, Default)]
pub struct GestureDetector {
    /// Active touches
    active_touches: Vec<Touch>,
    /// Whether a tap is in progress
    tap_in_progress: bool,
    /// Start position of potential tap
    tap_start: Option<(f32, f32)>,
}

impl GestureDetector {
    /// Create a new gesture detector
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a touch event and detect gestures
    pub fn process(&mut self, touch: &Touch) -> Option<Gesture> {
        match touch.phase {
            TouchPhase::Began => {
                self.active_touches.push(touch.clone());
                self.tap_in_progress = true;
                self.tap_start = Some((touch.x, touch.y));
                None
            }
            TouchPhase::Moved => {
                // Update touch position
                if let Some(existing) = self.active_touches.iter_mut().find(|t| t.id == touch.id) {
                    // Check if moved too far for a tap
                    if let Some((start_x, start_y)) = self.tap_start {
                        let dx = touch.x - start_x;
                        let dy = touch.y - start_y;
                        if dx * dx + dy * dy > 100.0 {
                            // 10pt threshold
                            self.tap_in_progress = false;
                        }
                    }
                    existing.x = touch.x;
                    existing.y = touch.y;
                }
                None
            }
            TouchPhase::Ended => {
                self.active_touches.retain(|t| t.id != touch.id);

                // Check for tap
                if self.tap_in_progress {
                    self.tap_in_progress = false;
                    self.tap_start = None;
                    return Some(Gesture::Tap {
                        x: touch.x,
                        y: touch.y,
                    });
                }

                self.tap_start = None;
                None
            }
            TouchPhase::Cancelled => {
                self.active_touches.retain(|t| t.id != touch.id);
                self.tap_in_progress = false;
                self.tap_start = None;
                None
            }
        }
    }

    /// Get the number of active touches
    pub fn active_touch_count(&self) -> usize {
        self.active_touches.len()
    }
}

/// Detected gestures
#[derive(Clone, Debug)]
pub enum Gesture {
    /// Single tap
    Tap { x: f32, y: f32 },
    /// Long press
    LongPress { x: f32, y: f32 },
    /// Pan/drag gesture
    Pan {
        dx: f32,
        dy: f32,
        velocity: (f32, f32),
    },
    /// Pinch gesture (for zoom)
    Pinch { scale: f32, center: (f32, f32) },
    /// Rotation gesture
    Rotation { angle: f32, center: (f32, f32) },
}
