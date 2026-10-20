//! The integration tests, as one binary: each file is a module, so the
//! crate and its dependencies are linked once rather than once per file.

mod aux_routing;
mod backdrop;
mod bc4_quality;
mod clip_fill_rule;
mod display_list;
mod notch_joins;
mod primitive_layout;
mod producer_opacity;
mod sdf_shaped_rect;
mod shader_prelude;
mod shadow_spread;
