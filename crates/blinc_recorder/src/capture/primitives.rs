//! Core primitive types for recording.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// A timestamp relative to the recording session start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Timestamp(u64);

impl Timestamp {
    /// Create a timestamp from microseconds.
    pub fn from_micros(micros: u64) -> Self {
        Self(micros)
    }

    /// Create a timestamp from a duration.
    pub fn from_duration(duration: Duration) -> Self {
        Self(duration.as_micros() as u64)
    }

    /// Get the timestamp as microseconds.
    pub fn as_micros(&self) -> u64 {
        self.0
    }

    /// Get the timestamp as milliseconds.
    pub fn as_millis(&self) -> u64 {
        self.0 / 1000
    }

    /// Get the timestamp as seconds (f64 for precision).
    pub fn as_secs_f64(&self) -> f64 {
        self.0 as f64 / 1_000_000.0
    }

    /// Create a zero timestamp.
    pub fn zero() -> Self {
        Self(0)
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Self::zero()
    }
}

impl std::ops::Sub for Timestamp {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        Duration::from_micros(self.0.saturating_sub(rhs.0))
    }
}

/// A 2D point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl From<(f32, f32)> for Point {
    fn from((x, y): (f32, f32)) -> Self {
        Self { x, y }
    }
}

impl From<blinc_core::Point> for Point {
    fn from(p: blinc_core::Point) -> Self {
        Self { x: p.x, y: p.y }
    }
}

/// A rectangle with position and size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Get the center point of the rectangle.
    pub fn center(&self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Check if a point is inside the rectangle.
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.width
            && point.y >= self.y
            && point.y <= self.y + self.height
    }
}

impl From<blinc_core::Rect> for Rect {
    fn from(r: blinc_core::Rect) -> Self {
        Self {
            x: r.x(),
            y: r.y(),
            width: r.width(),
            height: r.height(),
        }
    }
}

/// Clock for generating timestamps relative to session start.
#[derive(Debug)]
pub struct RecordingClock {
    start: Instant,
}

impl RecordingClock {
    /// Create a new clock starting now.
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    /// Get the current timestamp relative to session start.
    pub fn now(&self) -> Timestamp {
        Timestamp::from_duration(self.start.elapsed())
    }

    /// Reset the clock to start now.
    pub fn reset(&mut self) {
        self.start = Instant::now();
    }
}

impl Default for RecordingClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_conversion() {
        let ts = Timestamp::from_micros(1_500_000);
        assert_eq!(ts.as_millis(), 1500);
        assert!((ts.as_secs_f64() - 1.5).abs() < 0.001);
    }

    #[test]
    fn test_rect_contains() {
        let rect = Rect::new(10.0, 10.0, 100.0, 50.0);
        assert!(rect.contains(Point::new(50.0, 30.0)));
        assert!(!rect.contains(Point::new(5.0, 30.0)));
    }
}
