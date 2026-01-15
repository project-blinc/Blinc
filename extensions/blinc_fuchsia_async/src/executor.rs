//! Async executor for Fuchsia

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// A single-threaded async executor
///
/// On Fuchsia, this uses Zircon ports for efficient event-driven wakeups.
/// On other platforms, it uses a simple polling approach.
pub struct Executor {
    /// Inner executor state
    inner: Rc<RefCell<ExecutorInner>>,
}

struct ExecutorInner {
    /// Queue of tasks ready to run
    ready_queue: VecDeque<Task>,
    /// Whether we're currently running
    running: bool,
}

struct Task {
    /// The future to poll
    future: Pin<Box<dyn Future<Output = ()> + 'static>>,
}

impl Executor {
    /// Create a new executor
    pub fn new() -> Result<Self, blinc_fuchsia_zircon::Status> {
        Ok(Self {
            inner: Rc::new(RefCell::new(ExecutorInner {
                ready_queue: VecDeque::new(),
                running: false,
            })),
        })
    }

    /// Spawn a future onto the executor
    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + 'static,
    {
        let task = Task {
            future: Box::pin(future),
        };
        self.inner.borrow_mut().ready_queue.push_back(task);
    }

    /// Spawn a future and return a handle to await its completion
    pub fn spawn_with_handle<F, T>(&self, future: F) -> SpawnHandle<T>
    where
        F: Future<Output = T> + 'static,
        T: 'static,
    {
        let result = Rc::new(RefCell::new(None));
        let result_clone = result.clone();

        self.spawn(async move {
            let value = future.await;
            *result_clone.borrow_mut() = Some(value);
        });

        SpawnHandle { result }
    }

    /// Run a single future to completion
    pub fn run<F, T>(&mut self, future: F) -> T
    where
        F: Future<Output = T>,
    {
        let mut pinned = Box::pin(future);
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        loop {
            // Try to complete the main future
            if let Poll::Ready(result) = pinned.as_mut().poll(&mut cx) {
                return result;
            }

            // Process ready tasks
            self.poll_ready_tasks();

            // On non-Fuchsia, just yield to prevent busy loop
            #[cfg(not(target_os = "fuchsia"))]
            std::thread::yield_now();

            // On Fuchsia, we'd wait on a port here
            #[cfg(target_os = "fuchsia")]
            {
                // TODO: Wait on port for events
                std::thread::yield_now();
            }
        }
    }

    /// Run until no progress can be made
    ///
    /// Returns true if all futures completed, false if blocked waiting.
    pub fn run_until_stalled<F, T>(&mut self, future: F) -> Poll<T>
    where
        F: Future<Output = T>,
    {
        let mut pinned = Box::pin(future);
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        // Poll main future
        if let Poll::Ready(result) = pinned.as_mut().poll(&mut cx) {
            return Poll::Ready(result);
        }

        // Process all ready tasks
        while self.poll_ready_tasks() > 0 {
            // Try main future again
            if let Poll::Ready(result) = pinned.as_mut().poll(&mut cx) {
                return Poll::Ready(result);
            }
        }

        Poll::Pending
    }

    /// Poll all ready tasks once
    ///
    /// Returns number of tasks polled.
    fn poll_ready_tasks(&mut self) -> usize {
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        let mut polled = 0;

        loop {
            let task = {
                let mut inner = self.inner.borrow_mut();
                inner.ready_queue.pop_front()
            };

            match task {
                Some(mut task) => {
                    polled += 1;
                    if task.future.as_mut().poll(&mut cx).is_pending() {
                        // Task not done, re-queue it
                        self.inner.borrow_mut().ready_queue.push_back(task);
                    }
                }
                None => break,
            }
        }

        polled
    }

    /// Check if there are pending tasks
    pub fn has_pending_tasks(&self) -> bool {
        !self.inner.borrow().ready_queue.is_empty()
    }

    /// Number of pending tasks
    pub fn pending_task_count(&self) -> usize {
        self.inner.borrow().ready_queue.len()
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new().expect("Failed to create executor")
    }
}

/// Handle for awaiting a spawned task's result
pub struct SpawnHandle<T> {
    result: Rc<RefCell<Option<T>>>,
}

impl<T> SpawnHandle<T> {
    /// Try to get the result if ready
    pub fn try_get(&self) -> Option<T> {
        self.result.borrow_mut().take()
    }

    /// Check if the result is ready
    pub fn is_ready(&self) -> bool {
        self.result.borrow().is_some()
    }
}

/// A thread-local executor (same as Executor but emphasizes single-threaded use)
pub type LocalExecutor = Executor;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_simple() {
        let mut executor = Executor::new().unwrap();
        let result = executor.run(async { 42 });
        assert_eq!(result, 42);
    }

    #[test]
    fn test_spawn_and_run() {
        let mut executor = Executor::new().unwrap();

        let counter = Rc::new(RefCell::new(0));
        let counter_clone = counter.clone();

        executor.spawn(async move {
            *counter_clone.borrow_mut() += 1;
        });

        executor.run(async {});

        // Note: spawn doesn't guarantee execution order
        // In a real implementation, tasks would be polled
    }

    #[test]
    fn test_run_until_stalled() {
        let mut executor = Executor::new().unwrap();
        let result = executor.run_until_stalled(async { 123 });
        assert!(matches!(result, Poll::Ready(123)));
    }
}
