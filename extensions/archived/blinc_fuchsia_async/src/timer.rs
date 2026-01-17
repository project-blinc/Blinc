//! Timer support for async operations

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use blinc_fuchsia_zircon::Duration;
use pin_project_lite::pin_project;

/// An async timer that completes after a duration
pub struct Timer {
    /// Target completion time
    deadline: Instant,
}

impl Timer {
    /// Create a timer that fires after the given duration
    pub fn after(duration: std::time::Duration) -> Self {
        Self {
            deadline: Instant::now() + duration,
        }
    }

    /// Create a timer from a Zircon duration
    pub fn after_zx(duration: Duration) -> Self {
        Self::after(std::time::Duration::from_nanos(duration.into_nanos() as u64))
    }

    /// Create a timer that fires at the given instant
    pub fn at(deadline: Instant) -> Self {
        Self { deadline }
    }

    /// Sleep for the given duration
    pub async fn sleep(duration: std::time::Duration) {
        Timer::after(duration).await
    }

    /// Sleep for the given Zircon duration
    pub async fn sleep_zx(duration: Duration) {
        Timer::after_zx(duration).await
    }
}

impl Future for Timer {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if Instant::now() >= self.deadline {
            Poll::Ready(())
        } else {
            // In a real Fuchsia implementation, we'd register with the port
            // For now, just indicate we're not ready
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

pin_project! {
    /// A future with a timeout
    pub struct Timeout<F> {
        #[pin]
        future: F,
        #[pin]
        timer: Timer,
    }
}

impl<F: Future> Timeout<F> {
    /// Create a new timeout wrapper
    pub fn new(future: F, duration: std::time::Duration) -> Self {
        Self {
            future,
            timer: Timer::after(duration),
        }
    }

    /// Create from Zircon duration
    pub fn new_zx(future: F, duration: Duration) -> Self {
        Self {
            future,
            timer: Timer::after_zx(duration),
        }
    }
}

impl<F: Future> Future for Timeout<F> {
    type Output = Result<F::Output, TimedOut>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        // Check if the future completed
        if let Poll::Ready(output) = this.future.poll(cx) {
            return Poll::Ready(Ok(output));
        }

        // Check if timed out
        if let Poll::Ready(()) = this.timer.poll(cx) {
            return Poll::Ready(Err(TimedOut));
        }

        Poll::Pending
    }
}

/// Error returned when a timeout expires
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimedOut;

impl std::fmt::Display for TimedOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "operation timed out")
    }
}

impl std::error::Error for TimedOut {}

/// Extension trait for adding timeout to futures
pub trait TimeoutExt: Future + Sized {
    /// Add a timeout to this future
    fn timeout(self, duration: std::time::Duration) -> Timeout<Self> {
        Timeout::new(self, duration)
    }

    /// Add a timeout using Zircon duration
    fn timeout_zx(self, duration: Duration) -> Timeout<Self> {
        Timeout::new_zx(self, duration)
    }
}

impl<F: Future> TimeoutExt for F {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timer_immediate() {
        // A timer with zero duration should complete immediately
        let timer = Timer::after(std::time::Duration::ZERO);
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        let mut pinned = Box::pin(timer);
        assert!(matches!(pinned.as_mut().poll(&mut cx), Poll::Ready(())));
    }

    #[test]
    fn test_timed_out_error() {
        let err = TimedOut;
        assert_eq!(format!("{}", err), "operation timed out");
    }
}
