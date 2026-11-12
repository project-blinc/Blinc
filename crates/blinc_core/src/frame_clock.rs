//! The frame clock: one monotonic time, set once at the start of each frame and read by
//! everything that animates that frame.
//!
//! A runner calls [`FrameClock::begin_frame`] before it ticks anything, or
//! [`FrameClock::begin_frame_at`] when it keeps its own time, as a recorder stepping a
//! simulated clock does. Consumers read [`FrameClock::now`] and friends instead of taking a
//! time of their own. The clock never runs backwards, and [`FrameClock::delta`] is the gap
//! since the previous frame, uncapped: a consumer that wants a cap applies its own.
//!
//! The clock is process-wide and is set from the thread that runs the frame.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static ORIGIN: OnceLock<web_time::Instant> = OnceLock::new();
static NOW_US: AtomicU64 = AtomicU64::new(0);
static DELTA_US: AtomicU64 = AtomicU64::new(0);
static FRAME: AtomicU64 = AtomicU64::new(0);

/// The time of the frame being run. See the [module](self) docs.
pub struct FrameClock;

impl FrameClock {
    /// Start a frame at the wall clock's time.
    pub fn begin_frame() {
        let origin = ORIGIN.get_or_init(web_time::Instant::now);
        Self::begin_frame_at(origin.elapsed());
    }

    /// Start a frame at `now`, a time since the clock's origin that the caller keeps. A time
    /// before the previous frame's is held at it, so the clock does not run backwards.
    pub fn begin_frame_at(now: Duration) {
        let now_us = u64::try_from(now.as_micros()).unwrap_or(u64::MAX);
        let previous = NOW_US.fetch_max(now_us, Ordering::AcqRel);
        DELTA_US.store(now_us.saturating_sub(previous), Ordering::Release);
        FRAME.fetch_add(1, Ordering::AcqRel);
    }

    /// The time of this frame, since the clock's origin.
    pub fn now() -> Duration {
        Duration::from_micros(NOW_US.load(Ordering::Acquire))
    }

    /// [`Self::now`] in seconds, as the GPU reads it. An `f32` keeps about a millisecond for
    /// the first four and a half hours; a shader that runs longer wraps it.
    pub fn now_secs() -> f32 {
        (NOW_US.load(Ordering::Acquire) as f64 / 1_000_000.0) as f32
    }

    /// The time from the previous frame to this one, zero on the first.
    pub fn delta() -> Duration {
        Duration::from_micros(DELTA_US.load(Ordering::Acquire))
    }

    /// How many frames have begun.
    pub fn frame() -> u64 {
        FRAME.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test, because the clock is process-wide.
    #[test]
    fn the_clock_follows_the_frames_and_never_runs_backwards() {
        let base = Duration::from_secs(1_000);
        let frames = FrameClock::frame();
        FrameClock::begin_frame_at(base);
        assert_eq!(FrameClock::now(), base);
        assert_eq!(FrameClock::frame(), frames + 1);

        FrameClock::begin_frame_at(base + Duration::from_millis(16));
        assert_eq!(FrameClock::now(), base + Duration::from_millis(16));
        assert_eq!(FrameClock::delta(), Duration::from_millis(16));

        // A late-arriving earlier time leaves the clock where it was.
        FrameClock::begin_frame_at(base);
        assert_eq!(FrameClock::now(), base + Duration::from_millis(16));
        assert_eq!(FrameClock::delta(), Duration::ZERO);

        assert!((FrameClock::now_secs() - 1_000.016).abs() < 0.001);

        // The wall clock moves it on from wherever it is.
        let before = FrameClock::now();
        FrameClock::begin_frame();
        assert!(FrameClock::now() >= before);
    }
}
