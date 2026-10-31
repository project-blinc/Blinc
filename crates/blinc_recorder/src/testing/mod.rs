//! Testing infrastructure for Blinc applications.
//!
//! - [`CapturedFrame`], [`compare_frames`], [`ScreenshotExporter`]: frames
//!   and how they compare.
//! - [`Headless`] (feature `headless`): a real UI built, driven and painted
//!   offscreen through the renderer a window uses, with the pointer and the
//!   clock under the test's control.
//! - [`Golden`] (feature `headless`): reference images recorded per machine and platform.

mod framebuffer;
#[cfg(feature = "headless")]
mod golden;
#[cfg(feature = "headless")]
mod headless;

pub use framebuffer::{
    CapturedFrame, FrameSequence, RegressionResult, ScreenshotExporter, compare_frames,
};
#[cfg(feature = "headless")]
pub use golden::{Difference, Golden, diff_image, difference, load_png};
#[cfg(feature = "headless")]
pub use headless::Headless;
