//! The tier with storage buffers in every stage.

#[path = "../access/buffers.rs"]
pub(crate) mod access;
#[path = "../shared/clip.rs"]
mod clip;
#[path = "../shared/common.rs"]
mod common;
#[path = "../shared/notch.rs"]
mod notch;
#[path = "../shared/primitive.rs"]
pub(crate) mod primitive;
#[path = "../shared/shadow.rs"]
mod shadow;
#[path = "../shared/shapes3d.rs"]
mod shapes3d;
#[path = "../shared/three_d.rs"]
mod three_d;

pub(crate) mod sdf_3d;
pub(crate) mod sdf_core;
pub(crate) mod sdf_notch;
pub(crate) mod sdf_shadow;
