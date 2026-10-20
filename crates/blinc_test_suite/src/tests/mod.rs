//! Visual test cases organized by category

pub mod aux_routing;
pub mod blur_cost;
pub mod clip_fill_rule;
pub mod clipping;
pub mod display_list;
pub mod glass;
pub mod gradients;
pub mod layout;
pub mod notch;
pub mod opacity;
pub mod overlay_layers;
pub mod paint_context;
pub mod paths;
pub mod sdf;
pub mod shadows;
pub mod shapes;
pub mod svg;
pub mod text;
pub mod transforms;

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
        glass::suite(),
        svg::suite(),
        layout::suite(),
        notch::suite(),
    ]
}
