//! Math utilities for 3D graphics

mod bounds;
mod extensions;
mod quat;

pub use bounds::{BoundingBox, BoundingSphere};
pub use extensions::{Mat4Ext, Vec3Ext, mat4_mul};
pub use quat::Quat;

// Re-export common math types from blinc_core
pub use blinc_core::{Mat4, Vec2, Vec3};
