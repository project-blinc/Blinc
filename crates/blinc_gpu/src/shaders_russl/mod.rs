//! The renderer's single-variant shaders in russl, compiled at build time: the fullscreen
//! post-process passes, the shadow depth pass and the backdrop blur. The SDF family, which has
//! one variant per device tier, is in `sdf_russl`.

pub(crate) mod backdrop_rows;
pub(crate) mod bloom;
pub(crate) mod blur;
pub(crate) mod clear_quad;
pub(crate) mod color_matrix;
pub(crate) mod composite;
pub(crate) mod drop_shadow;
#[cfg(test)]
mod effect_tests;
mod fullscreen;
pub(crate) mod glow;
#[cfg(test)]
mod harness;
pub(crate) mod layer_composite;
mod layer_quad;
pub(crate) mod mask_image;
pub(crate) mod oit_composite;
pub(crate) mod shadow_depth;
pub(crate) mod skybox;
#[cfg(test)]
mod tests;
pub(crate) mod tonemap;
