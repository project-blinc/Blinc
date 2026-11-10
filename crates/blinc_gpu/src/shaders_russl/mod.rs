//! The renderer's single-variant shaders in russl, compiled at build time: the fullscreen
//! post-process passes, the shadow depth pass and the backdrop blur. The SDF family, which has
//! one variant per device tier, is in `sdf_russl`.

pub(crate) mod backdrop_rows;
pub(crate) mod bloom;
pub(crate) mod clear_quad;
mod fullscreen;
pub(crate) mod oit_composite;
pub(crate) mod shadow_depth;
pub(crate) mod skybox;
#[cfg(test)]
mod tests;
pub(crate) mod tonemap;
