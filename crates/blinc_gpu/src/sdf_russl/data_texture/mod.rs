//! The tier with no storage buffers: the vertex stage reads a vertex buffer, the fragment stage
//! data textures.

#[path = "../access/textures.rs"]
pub(crate) mod access;
#[path = "../shared/clip.rs"]
mod clip;
#[path = "../shared/common.rs"]
mod common;
#[path = "../shared/primitive.rs"]
pub(crate) mod primitive;
#[path = "../shared/shadow.rs"]
mod shadow;
#[path = "../shared/shapes3d.rs"]
mod shapes3d;
#[path = "../shared/three_d.rs"]
mod three_d;

#[path = "../shared/three_d_attrs.rs"]
pub(crate) mod sdf_3d;
#[path = "../shared/shadow_attrs.rs"]
pub(crate) mod sdf_shadow;
