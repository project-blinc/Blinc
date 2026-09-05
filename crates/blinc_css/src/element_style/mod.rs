//! Unified element styling
//!
//! Provides `ElementStyle` - a consistent style schema for all visual properties
//! that can be applied to layout elements. This enables:
//!
//! - Consistent API across `Div`, `StatefulDiv`, and other elements
//! - State-dependent styling with full property support
//! - Style composition and merging
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//! use blinc_core::Color;
//!
//! // Create a style
//! let style = ElementStyle::new()
//!     .bg(Color::BLUE)
//!     .rounded(8.0)
//!     .shadow_md()
//!     .scale(1.0);
//!
//! // Use with stateful elements
//! stateful_button()
//!     .idle(ElementStyle::new().bg(Color::BLUE))
//!     .hovered(ElementStyle::new().bg(Color::LIGHT_BLUE).scale(1.02))
//!     .pressed(ElementStyle::new().bg(Color::DARK_BLUE).scale(0.98));
//! ```
//!
//! # Layout
//!
//! - [`schema`] — the [`ElementStyle`] struct itself and its constructor.
//! - [`builder`] — the builder methods, one module per family of property.
//! - [`keywords`] — the enums for keyword-valued properties.
//! - [`dynamic`] — properties held as an unevaluated `calc(env(...))`.
//! - [`filter`] — CSS filter functions.
//! - `macros` — the `css!`, `style!` and `flow!` macros.

mod builder;
pub mod dynamic;
pub mod filter;
pub mod keywords;
mod macros;
pub mod schema;
#[cfg(test)]
mod tests;

pub use dynamic::*;
pub use filter::*;
pub use keywords::*;
pub use schema::*;
