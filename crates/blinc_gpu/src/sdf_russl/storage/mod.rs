//! The shaders still written for the tier with storage buffers in every stage only.

#[path = "../shared/access.rs"]
pub(crate) mod access;
#[path = "../shared/clip.rs"]
mod clip;
#[path = "../shared/common.rs"]
mod common;
#[path = "../shared/primitive.rs"]
pub(crate) mod primitive;

pub(crate) mod sdf_core;
