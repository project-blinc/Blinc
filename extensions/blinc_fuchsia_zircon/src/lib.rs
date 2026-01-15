//! Blinc Fuchsia Zircon - Standalone Zircon Kernel Bindings
//!
//! This crate provides Rust bindings for the Zircon kernel, allowing
//! Blinc applications to run on Fuchsia OS without requiring the full
//! Fuchsia source tree.
//!
//! ## Platform Support
//!
//! - **Fuchsia**: Full functionality via real syscalls
//! - **Other platforms**: Types compile but operations panic at runtime
//!
//! ## Core Types
//!
//! - [`Handle`] - Generic kernel object handle
//! - [`Channel`] - IPC channel for FIDL communication
//! - [`EventPair`] - Paired events for signaling
//! - [`Vmo`] - Virtual Memory Object for buffer sharing
//! - [`Status`] - Kernel error codes
//!
//! ## Example
//!
//! ```ignore
//! use blinc_fuchsia_zircon::{Channel, Status};
//!
//! let (client, server) = Channel::create()?;
//! client.write(b"hello", &[])?;
//! ```

mod status;
mod handle;
mod channel;
mod eventpair;
mod vmo;
mod time;
mod rights;

#[cfg(target_os = "fuchsia")]
mod sys;

#[cfg(not(target_os = "fuchsia"))]
mod sys_stub;

#[cfg(not(target_os = "fuchsia"))]
use sys_stub as sys;

pub use status::{Status, ok};
pub use handle::{Handle, HandleBased, HandleRef, AsHandleRef};
pub use channel::{Channel, ChannelReadResult, MessageBuf};
pub use eventpair::EventPair;
pub use vmo::Vmo;
pub use time::{Duration, Time, Instant};
pub use rights::Rights;

/// Raw handle type (u32)
pub type RawHandle = u32;

/// Invalid handle constant
pub const HANDLE_INVALID: RawHandle = 0;

/// Result type for Zircon operations
pub type Result<T> = std::result::Result<T, Status>;

/// Prelude for common imports
pub mod prelude {
    pub use super::{
        Status, Handle, HandleBased, HandleRef, AsHandleRef,
        Channel, EventPair, Vmo, Duration, Time, Rights,
        Result, ok,
    };
}
