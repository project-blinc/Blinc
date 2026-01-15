//! Time types for Zircon

use std::ops::{Add, Sub};

/// A duration of time (in nanoseconds)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Duration(i64);

impl Duration {
    /// Zero duration
    pub const ZERO: Duration = Duration(0);

    /// Infinite duration
    pub const INFINITE: Duration = Duration(i64::MAX);

    /// Create a duration from nanoseconds
    pub const fn from_nanos(nanos: i64) -> Self {
        Duration(nanos)
    }

    /// Create a duration from microseconds
    pub const fn from_micros(micros: i64) -> Self {
        Duration(micros * 1_000)
    }

    /// Create a duration from milliseconds
    pub const fn from_millis(millis: i64) -> Self {
        Duration(millis * 1_000_000)
    }

    /// Create a duration from seconds
    pub const fn from_seconds(secs: i64) -> Self {
        Duration(secs * 1_000_000_000)
    }

    /// Get the duration in nanoseconds
    pub const fn into_nanos(self) -> i64 {
        self.0
    }

    /// Get the duration in microseconds
    pub const fn into_micros(self) -> i64 {
        self.0 / 1_000
    }

    /// Get the duration in milliseconds
    pub const fn into_millis(self) -> i64 {
        self.0 / 1_000_000
    }

    /// Get the duration in seconds
    pub const fn into_seconds(self) -> i64 {
        self.0 / 1_000_000_000
    }
}

impl From<std::time::Duration> for Duration {
    fn from(d: std::time::Duration) -> Self {
        Duration(d.as_nanos() as i64)
    }
}

impl Add for Duration {
    type Output = Duration;
    fn add(self, rhs: Duration) -> Duration {
        Duration(self.0.saturating_add(rhs.0))
    }
}

impl Sub for Duration {
    type Output = Duration;
    fn sub(self, rhs: Duration) -> Duration {
        Duration(self.0.saturating_sub(rhs.0))
    }
}

/// A point in time (nanoseconds since boot)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Time(i64);

impl Time {
    /// The epoch (time 0)
    pub const ZERO: Time = Time(0);

    /// Infinite future
    pub const INFINITE: Time = Time(i64::MAX);

    /// Infinite past
    pub const INFINITE_PAST: Time = Time(i64::MIN);

    /// Create a time from nanoseconds
    pub const fn from_nanos(nanos: i64) -> Self {
        Time(nanos)
    }

    /// Get the time in nanoseconds
    pub const fn into_nanos(self) -> i64 {
        self.0
    }

    /// Get the current monotonic time
    #[cfg(target_os = "fuchsia")]
    pub fn get_monotonic() -> Self {
        crate::sys::clock_get_monotonic()
    }

    #[cfg(not(target_os = "fuchsia"))]
    pub fn get_monotonic() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64;
        Time(nanos)
    }
}

impl Add<Duration> for Time {
    type Output = Time;
    fn add(self, rhs: Duration) -> Time {
        Time(self.0.saturating_add(rhs.0))
    }
}

impl Sub<Duration> for Time {
    type Output = Time;
    fn sub(self, rhs: Duration) -> Time {
        Time(self.0.saturating_sub(rhs.0))
    }
}

impl Sub for Time {
    type Output = Duration;
    fn sub(self, rhs: Time) -> Duration {
        Duration(self.0.saturating_sub(rhs.0))
    }
}

/// An instant in time (alias for Time for compatibility)
pub type Instant = Time;
