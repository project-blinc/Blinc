//! The tier whose vertex stage reads a vertex buffer and whose fragment stage reads storage
//! buffers.

#[path = "../access/buffers.rs"]
pub(crate) mod access;
#[path = "../shared/clip.rs"]
mod clip;
#[path = "../shared/common.rs"]
mod common;
#[path = "../shared/primitive.rs"]
pub(crate) mod primitive;
#[path = "../shared/shadow.rs"]
mod shadow;

#[path = "../shared/shadow_attrs.rs"]
pub(crate) mod sdf_shadow;
