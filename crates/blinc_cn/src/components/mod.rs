//! Themed components built on blinc_layout primitives
//!
//! Each component follows a consistent pattern:
//! - Builder function (e.g., `button("Label")`)
//! - Variant enum (e.g., `ButtonVariant`)
//! - Size enum (e.g., `ButtonSize`)
//! - Implements `ElementBuilder` for rendering

pub mod button;

pub use button::{button, Button, ButtonSize, ButtonVariant};
