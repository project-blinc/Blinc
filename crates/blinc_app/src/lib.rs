//! Blinc Application Framework
//!
//! Clean API for building Blinc applications with layout and rendering.
//!
//! # Example (Headless Rendering)
//!
//! ```ignore
//! use blinc_app::prelude::*;
//!
//! fn main() -> Result<()> {
//!     let app = BlincApp::new()?;
//!
//!     let ui = div()
//!         .w(400.0).h(300.0)
//!         .flex_col().gap(4.0).p(4.0)
//!         .child(
//!             div().glass()
//!                 .w_full().h(100.0)
//!                 .rounded(16.0)
//!                 .child(text("Hello Blinc!").size(24.0))
//!         );
//!
//!     app.render(&ui, &target_view, 400.0, 300.0)?;
//! }
//! ```
//!
//! # Example (Windowed Application)
//!
//! ```ignore
//! use blinc_app::prelude::*;
//! use blinc_app::windowed::{WindowedApp, WindowedContext};
//!
//! fn main() -> Result<()> {
//!     WindowedApp::run(WindowConfig::default(), |ctx| {
//!         div()
//!             .w(ctx.width).h(ctx.height)
//!             .bg([0.1, 0.1, 0.15, 1.0])
//!             .flex_center()
//!             .child(
//!                 div().glass().rounded(16.0).p(24.0)
//!                     .child(text("Hello Blinc!").size(32.0))
//!             )
//!     })
//! }
//! ```

mod app;
mod context;
mod error;

#[cfg(feature = "windowed")]
pub mod windowed;

#[cfg(test)]
mod tests;

pub use app::{BlincApp, BlincConfig};
pub use context::RenderContext;
pub use error::{BlincError, Result};

// Re-export layout API for convenience
pub use blinc_layout::prelude::*;
pub use blinc_layout::RenderTree;

// Re-export platform types for windowed applications
pub use blinc_platform::WindowConfig;

/// Prelude module - import everything commonly needed
pub mod prelude {
    pub use crate::app::{BlincApp, BlincConfig};
    pub use crate::context::RenderContext;
    pub use crate::error::{BlincError, Result};

    // Layout builders
    pub use blinc_layout::prelude::*;
    pub use blinc_layout::RenderTree;

    // Core types
    pub use blinc_core::{Color, Point, Rect, Size};

    // Platform types
    pub use blinc_platform::WindowConfig;
}
