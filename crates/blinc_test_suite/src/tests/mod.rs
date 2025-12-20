//! Visual test cases organized by category

pub mod shapes;
pub mod transforms;
pub mod opacity;
pub mod gradients;
pub mod paths;
pub mod text;
pub mod shadows;
pub mod clipping;
pub mod sdf;
pub mod paint_context;

use crate::runner::TestSuite;

/// Create all test suites
pub fn all_suites() -> Vec<TestSuite> {
    vec![
        shapes::suite(),
        transforms::suite(),
        opacity::suite(),
        gradients::suite(),
        paths::suite(),
        text::suite(),
        shadows::suite(),
        clipping::suite(),
        sdf::suite(),
        paint_context::suite(),
    ]
}
