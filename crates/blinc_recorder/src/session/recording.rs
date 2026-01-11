//! Recording session state machine.

use super::config::RecordingConfig;
use crate::capture::{
    RecordedEvent, RecordingClock, Timestamp, TimestampedEvent, TreeDiff, TreeSnapshot,
};
use parking_lot::RwLock;
use std::collections::VecDeque;

/// State of the recording session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    /// Not recording.
    Idle,
    /// Actively recording events and snapshots.
    Recording,
    /// Recording paused (can resume).
    Paused,
    /// Recording stopped (cannot resume, only export).
    Stopped,
}

/// A recording session that captures events and tree snapshots.
pub struct RecordingSession {
    /// Configuration for this session.
    config: RecordingConfig,
    /// Current session state.
    state: SessionState,
    /// Clock for timestamps.
    clock: RecordingClock,
    /// Ring buffer of recorded events.
    events: VecDeque<TimestampedEvent>,
    /// Ring buffer of tree snapshots.
    snapshots: VecDeque<TreeSnapshot>,
    /// Last snapshot for diff computation.
    last_snapshot: Option<TreeSnapshot>,
    /// Accumulated pause duration (for accurate timestamps).
    pause_duration: std::time::Duration,
    /// When the current pause started (if paused).
    pause_start: Option<std::time::Instant>,
    /// Statistics.
    stats: SessionStats,
}

/// Statistics for a recording session.
#[derive(Clone, Debug, Default)]
pub struct SessionStats {
    /// Total events recorded.
    pub total_events: u64,
    /// Total snapshots taken.
    pub total_snapshots: u64,
    /// Events dropped due to buffer overflow.
    pub events_dropped: u64,
    /// Snapshots dropped due to buffer overflow.
    pub snapshots_dropped: u64,
    /// Last event timestamp.
    pub last_event_time: Option<Timestamp>,
    /// Last snapshot timestamp.
    pub last_snapshot_time: Option<Timestamp>,
}

impl RecordingSession {
    /// Create a new recording session with the given configuration.
    pub fn new(config: RecordingConfig) -> Self {
        Self {
            config,
            state: SessionState::Idle,
            clock: RecordingClock::new(),
            events: VecDeque::new(),
            snapshots: VecDeque::new(),
            last_snapshot: None,
            pause_duration: std::time::Duration::ZERO,
            pause_start: None,
            stats: SessionStats::default(),
        }
    }

    /// Get the current session state.
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Get the configuration.
    pub fn config(&self) -> &RecordingConfig {
        &self.config
    }

    /// Get session statistics.
    pub fn stats(&self) -> &SessionStats {
        &self.stats
    }

    /// Check if currently recording.
    pub fn is_recording(&self) -> bool {
        self.state == SessionState::Recording
    }

    /// Start recording.
    pub fn start(&mut self) {
        match self.state {
            SessionState::Idle => {
                self.clock.reset();
                self.events.clear();
                self.snapshots.clear();
                self.last_snapshot = None;
                self.pause_duration = std::time::Duration::ZERO;
                self.stats = SessionStats::default();
                self.state = SessionState::Recording;
            }
            SessionState::Paused => {
                // Resume from pause
                if let Some(pause_start) = self.pause_start.take() {
                    self.pause_duration += pause_start.elapsed();
                }
                self.state = SessionState::Recording;
            }
            _ => {}
        }
    }

    /// Pause recording (can resume later).
    pub fn pause(&mut self) {
        if self.state == SessionState::Recording {
            self.pause_start = Some(std::time::Instant::now());
            self.state = SessionState::Paused;
        }
    }

    /// Stop recording (cannot resume).
    pub fn stop(&mut self) {
        if self.state == SessionState::Recording || self.state == SessionState::Paused {
            self.state = SessionState::Stopped;
            self.pause_start = None;
        }
    }

    /// Reset the session to idle state.
    pub fn reset(&mut self) {
        self.state = SessionState::Idle;
        self.events.clear();
        self.snapshots.clear();
        self.last_snapshot = None;
        self.pause_duration = std::time::Duration::ZERO;
        self.pause_start = None;
        self.stats = SessionStats::default();
    }

    /// Get the current timestamp (accounting for pause time).
    pub fn current_timestamp(&self) -> Timestamp {
        let raw = self.clock.now();
        let pause_micros = self.pause_duration.as_micros() as u64;
        Timestamp::from_micros(raw.as_micros().saturating_sub(pause_micros))
    }

    /// Record an event.
    pub fn record_event(&mut self, event: RecordedEvent) {
        if self.state != SessionState::Recording {
            return;
        }

        let timestamp = self.current_timestamp();
        let timestamped = TimestampedEvent::new(timestamp, event);

        // Ring buffer: remove oldest if at capacity
        if self.events.len() >= self.config.max_events {
            self.events.pop_front();
            self.stats.events_dropped += 1;
        }

        self.events.push_back(timestamped);
        self.stats.total_events += 1;
        self.stats.last_event_time = Some(timestamp);
    }

    /// Record a tree snapshot.
    pub fn record_snapshot(&mut self, mut snapshot: TreeSnapshot) -> Option<TreeDiff> {
        if self.state != SessionState::Recording {
            return None;
        }

        let timestamp = self.current_timestamp();
        snapshot.timestamp = timestamp;

        // Compute diff from last snapshot
        let diff = self
            .last_snapshot
            .as_ref()
            .map(|last| crate::capture::diff_trees(last, &snapshot));

        // Ring buffer: remove oldest if at capacity
        if self.snapshots.len() >= self.config.max_snapshots {
            self.snapshots.pop_front();
            self.stats.snapshots_dropped += 1;
        }

        self.last_snapshot = Some(snapshot.clone());
        self.snapshots.push_back(snapshot);
        self.stats.total_snapshots += 1;
        self.stats.last_snapshot_time = Some(timestamp);

        diff
    }

    /// Get recorded events.
    pub fn events(&self) -> &VecDeque<TimestampedEvent> {
        &self.events
    }

    /// Get recorded snapshots.
    pub fn snapshots(&self) -> &VecDeque<TreeSnapshot> {
        &self.snapshots
    }

    /// Get the last recorded snapshot.
    pub fn last_snapshot(&self) -> Option<&TreeSnapshot> {
        self.last_snapshot.as_ref()
    }

    /// Get events in a time range.
    pub fn events_in_range(&self, start: Timestamp, end: Timestamp) -> Vec<&TimestampedEvent> {
        self.events
            .iter()
            .filter(|e| e.timestamp >= start && e.timestamp <= end)
            .collect()
    }

    /// Get the snapshot closest to a given timestamp.
    pub fn snapshot_at(&self, timestamp: Timestamp) -> Option<&TreeSnapshot> {
        self.snapshots
            .iter()
            .filter(|s| s.timestamp <= timestamp)
            .last()
    }

    /// Get the recording duration.
    pub fn duration(&self) -> Timestamp {
        self.stats
            .last_event_time
            .or(self.stats.last_snapshot_time)
            .unwrap_or_default()
    }

    /// Export all recorded data.
    pub fn export(&self) -> RecordingExport {
        RecordingExport {
            config: self.config.clone(),
            events: self.events.iter().cloned().collect(),
            snapshots: self.snapshots.iter().cloned().collect(),
            stats: self.stats.clone(),
        }
    }
}

/// Exported recording data for serialization.
#[derive(Clone, Debug)]
pub struct RecordingExport {
    pub config: RecordingConfig,
    pub events: Vec<TimestampedEvent>,
    pub snapshots: Vec<TreeSnapshot>,
    pub stats: SessionStats,
}

/// Thread-safe wrapper around RecordingSession.
pub struct SharedRecordingSession {
    inner: RwLock<RecordingSession>,
}

impl SharedRecordingSession {
    pub fn new(config: RecordingConfig) -> Self {
        Self {
            inner: RwLock::new(RecordingSession::new(config)),
        }
    }

    pub fn state(&self) -> SessionState {
        self.inner.read().state()
    }

    pub fn is_recording(&self) -> bool {
        self.inner.read().is_recording()
    }

    pub fn start(&self) {
        self.inner.write().start();
    }

    pub fn pause(&self) {
        self.inner.write().pause();
    }

    pub fn stop(&self) {
        self.inner.write().stop();
    }

    pub fn reset(&self) {
        self.inner.write().reset();
    }

    pub fn record_event(&self, event: RecordedEvent) {
        self.inner.write().record_event(event);
    }

    pub fn record_snapshot(&self, snapshot: TreeSnapshot) -> Option<TreeDiff> {
        self.inner.write().record_snapshot(snapshot)
    }

    pub fn stats(&self) -> SessionStats {
        self.inner.read().stats().clone()
    }

    pub fn export(&self) -> RecordingExport {
        self.inner.read().export()
    }

    pub fn with_session<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&RecordingSession) -> R,
    {
        f(&self.inner.read())
    }

    pub fn with_session_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut RecordingSession) -> R,
    {
        f(&mut self.inner.write())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_state_transitions() {
        let mut session = RecordingSession::new(RecordingConfig::minimal());
        assert_eq!(session.state(), SessionState::Idle);

        session.start();
        assert_eq!(session.state(), SessionState::Recording);

        session.pause();
        assert_eq!(session.state(), SessionState::Paused);

        session.start(); // Resume
        assert_eq!(session.state(), SessionState::Recording);

        session.stop();
        assert_eq!(session.state(), SessionState::Stopped);

        // Can't resume after stop
        session.start();
        assert_eq!(session.state(), SessionState::Stopped);

        session.reset();
        assert_eq!(session.state(), SessionState::Idle);
    }

    #[test]
    fn test_event_recording() {
        use crate::capture::{MouseButton, MouseEvent, Modifiers, Point};

        let mut session = RecordingSession::new(RecordingConfig::minimal().with_max_events(5));
        session.start();

        for i in 0..7 {
            session.record_event(RecordedEvent::Click(MouseEvent {
                position: Point::new(i as f32 * 10.0, 0.0),
                button: MouseButton::Left,
                modifiers: Modifiers::none(),
                target_element: None,
            }));
        }

        // Should only have 5 events (max), dropped 2
        assert_eq!(session.events().len(), 5);
        assert_eq!(session.stats().events_dropped, 2);
        assert_eq!(session.stats().total_events, 7);
    }
}
