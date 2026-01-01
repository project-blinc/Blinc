//! Blinc Embedding SDK
//!
//! Integrate Blinc UI into Rust applications.

#[cfg(feature = "blinc_core")]
pub use blinc_core;

#[cfg(feature = "blinc_animation")]
pub use blinc_animation;

#[cfg(feature = "blinc_layout")]
pub use blinc_layout;

#[cfg(feature = "blinc_gpu")]
pub use blinc_gpu;

#[cfg(feature = "blinc_paint")]
pub use blinc_paint;

// #[cfg(feature = "blinc_cn")]
// pub use blinc_cn;

/// Initialize the Blinc runtime
pub fn init() -> anyhow::Result<()> {
    // TODO: Initialize Zyntax runtime with Blinc grammar
    Ok(())
}
