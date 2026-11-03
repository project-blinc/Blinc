//! A clock a flow shader's `time` can follow, which code can play, pause, seek and step.

use std::sync::{Arc, Mutex};

use web_time::Instant;

struct ClockState {
    time: f32,
    playing: bool,
    rate: f32,
    /// When `time` last advanced from the wall clock; `None` while paused.
    resumed: Option<Instant>,
}

impl ClockState {
    /// Fold the wall time since the last read into `time`.
    fn advance(&mut self) {
        if let Some(since) = self.resumed {
            let now = Instant::now();
            self.time += now.duration_since(since).as_secs_f32() * self.rate;
            self.resumed = Some(now);
        }
    }
}

/// The `time` of a flow shader, under a developer's control.
///
/// Give one to a canvas with `FlowLoop::Clock`. While it plays the canvas runs every frame and
/// `time` follows the wall clock, scaled by the rate; paused, `time` holds still and the canvas
/// draws again only when the clock moves, so [`step`](Self::step) and [`seek`](Self::seek)
/// advance a shader one chosen interval at a time. Clones share the clock.
///
/// ```rust,ignore
/// let clock = FlowClock::paused();
/// let view = canvas(|_, _| {}).flow(RIPPLE).flow_loop(FlowLoop::Clock(clock.clone()));
/// // in a key handler:
/// clock.step(1.0 / 60.0);
/// ```
#[derive(Clone)]
pub struct FlowClock {
    state: Arc<Mutex<ClockState>>,
}

impl FlowClock {
    fn with(playing: bool) -> Self {
        Self {
            state: Arc::new(Mutex::new(ClockState {
                time: 0.0,
                playing,
                rate: 1.0,
                resumed: playing.then(Instant::now),
            })),
        }
    }

    /// A clock at zero that runs from now.
    pub fn playing() -> Self {
        Self::with(true)
    }

    /// A clock at zero that holds still until played or stepped.
    pub fn paused() -> Self {
        Self::with(false)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ClockState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The current time in seconds.
    pub fn time(&self) -> f32 {
        let mut state = self.lock();
        state.advance();
        state.time
    }

    /// Whether the clock is running.
    pub fn is_playing(&self) -> bool {
        self.lock().playing
    }

    /// Run from the current time.
    pub fn play(&self) {
        let mut state = self.lock();
        if !state.playing {
            state.playing = true;
            state.resumed = Some(Instant::now());
        }
        drop(state);
        crate::request_redraw();
    }

    /// Hold at the current time.
    pub fn pause(&self) {
        let mut state = self.lock();
        state.advance();
        state.playing = false;
        state.resumed = None;
        drop(state);
        crate::request_redraw();
    }

    /// Jump to `seconds` and draw that frame.
    pub fn seek(&self, seconds: f32) {
        let mut state = self.lock();
        state.advance();
        state.time = seconds;
        drop(state);
        crate::request_redraw();
    }

    /// Move `seconds` forward (or back, if negative) and draw that frame. A running clock keeps
    /// running from the new time.
    pub fn step(&self, seconds: f32) {
        let mut state = self.lock();
        state.advance();
        state.time += seconds;
        drop(state);
        crate::request_redraw();
    }

    /// Scale the wall clock a running clock follows: 0.5 is half speed.
    pub fn set_rate(&self, rate: f32) {
        let mut state = self.lock();
        state.advance();
        state.rate = rate;
    }
}

impl Default for FlowClock {
    fn default() -> Self {
        Self::playing()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paused_clock_holds_still_until_stepped() {
        let clock = FlowClock::paused();
        assert_eq!(clock.time(), 0.0);
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert_eq!(clock.time(), 0.0);
        clock.step(0.25);
        clock.step(0.25);
        assert_eq!(clock.time(), 0.5);
        clock.seek(2.0);
        assert_eq!(clock.time(), 2.0);
        assert!(!clock.is_playing());
    }

    #[test]
    fn a_playing_clock_follows_the_wall_clock_until_paused() {
        let clock = FlowClock::playing();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let t = clock.time();
        assert!(t > 0.0);
        clock.pause();
        let held = clock.time();
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert_eq!(clock.time(), held);
        clock.play();
        assert!(clock.is_playing());
    }
}
