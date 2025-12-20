//! Blinc Visual Test Suite
//!
//! Comprehensive testing framework for the Blinc UI framework rendering capabilities.
//! Includes both headless tests and interactive window tests.
//!
//! # Features
//!
//! - `interactive` - Enable interactive window tests (requires display)
//!
//! # Test Categories
//!
//! - **Headless Tests**: Run without display, render to textures
//! - **Visual Regression**: Compare rendered output to reference images
//! - **Interactive Tests**: Manual testing with live windows
//! - **Benchmarks**: Performance testing of rendering pipeline

pub mod harness;
pub mod runner;
pub mod tests;

#[cfg(feature = "interactive")]
pub mod window;

pub use harness::{TestContext, TestHarness, TestResult};
pub use runner::TestRunner;
