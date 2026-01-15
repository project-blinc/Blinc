//! Blinc Fuchsia Async - Port-Based Async Runtime
//!
//! This crate provides an async executor for Fuchsia using Zircon ports
//! for event-driven wakeups.
//!
//! # Architecture
//!
//! On Fuchsia, async I/O works via:
//! - **Ports**: Event aggregation points where multiple objects can signal
//! - **Async Wait**: Register a handle + signals with a port
//! - **Packets**: Port delivers packets when signals are raised
//!
//! # Example
//!
//! ```ignore
//! use blinc_fuchsia_async::Executor;
//!
//! let mut executor = Executor::new()?;
//! executor.run_until_stalled(async {
//!     // Your async code here
//! });
//! ```
//!
//! # Platform Support
//!
//! - **Fuchsia**: Full functionality using Zircon ports
//! - **Other platforms**: Simple polling executor (for development/testing)

mod executor;
mod timer;
mod waker;

pub use executor::{Executor, LocalExecutor};
pub use timer::{Timer, Timeout};
pub use waker::WakeToken;

/// Re-export common futures types
pub mod prelude {
    pub use super::{Executor, LocalExecutor, Timer, Timeout};
    pub use futures::prelude::*;
}
