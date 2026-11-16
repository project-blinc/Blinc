//! Sources that failed to load, and when each is tried again, by the retry of the image that asked
//! for it.

use std::collections::HashMap;

use blinc_layout::ImageRetry;
use web_time::Instant;

/// A load to start again: the source, its fade-in and its retry.
pub(super) struct Due {
    pub source: String,
    pub fade_duration_ms: u32,
    pub retry: ImageRetry,
}

struct Failure {
    failures: u32,
    /// When the source may be tried again, `None` when it has no tries left.
    retry_at: Option<Instant>,
    fade_duration_ms: u32,
    retry: ImageRetry,
}

#[derive(Default)]
pub(super) struct ImageFailures {
    failed: HashMap<String, Failure>,
}

impl ImageFailures {
    /// Whether `source` may be loaded now: it has not failed, or its wait is over.
    pub fn may_try(&self, source: &str) -> bool {
        self.failed
            .get(source)
            .is_none_or(|f| f.retry_at.is_some_and(|at| Instant::now() >= at))
    }

    /// Record a failed load of `source`, waiting as `retry` says before the next try.
    pub fn failed(&mut self, source: &str, fade_duration_ms: u32, retry: ImageRetry) {
        let failures = self.failed.get(source).map_or(0, |f| f.failures) + 1;
        let retry_at = retry.wait(failures).map(|wait| Instant::now() + wait);
        self.failed.insert(
            source.to_owned(),
            Failure {
                failures,
                retry_at,
                fade_duration_ms,
                retry,
            },
        );
    }

    /// Record that `source` loaded.
    pub fn loaded(&mut self, source: &str) {
        self.failed.remove(source);
    }

    /// The sources whose wait is over, for a loader that starts them again by itself.
    pub fn due(&self) -> Vec<Due> {
        let now = Instant::now();
        self.failed
            .iter()
            .filter(|(_, f)| f.retry_at.is_some_and(|at| now >= at))
            .map(|(source, f)| Due {
                source: source.clone(),
                fade_duration_ms: f.fade_duration_ms,
                retry: f.retry,
            })
            .collect()
    }

    /// Forget the failures of the sources `matches` picks, so they are tried at once, as after
    /// the file behind them changed.
    pub fn forget(&mut self, matches: impl Fn(&str) -> bool) {
        self.failed.retain(|source, _| !matches(source));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_default_waits_longer_each_time_up_to_half_a_minute() {
        let r = ImageRetry::default();
        let ms = |n| r.wait(n).unwrap().as_millis();
        assert_eq!([ms(1), ms(2), ms(3), ms(4)], [500, 1000, 2000, 4000]);
        assert_eq!(ms(7), 30_000);
        assert_eq!(ms(40), 30_000);
    }

    #[test]
    fn a_retry_with_attempts_stops_after_them() {
        let r = ImageRetry::backoff(Duration::from_millis(100), Duration::from_secs(1)).attempts(2);
        assert!(r.wait(1).is_some() && r.wait(2).is_some());
        assert_eq!(r.wait(3), None);
        assert_eq!(ImageRetry::NEVER.wait(1), None);
    }

    #[test]
    fn a_source_with_no_tries_left_is_not_tried_until_forgotten() {
        let mut f = ImageFailures::default();
        f.failed("a.png", 0, ImageRetry::NEVER);
        assert!(!f.may_try("a.png"));
        assert!(f.due().is_empty());
        f.forget(|s| s == "a.png");
        assert!(f.may_try("a.png"));
    }
}
