//! Blinc's SDF shaders in russl, compiled at build time.
//!
//! The shader sources live in `shared`; each tier module includes them (`#[path]`) beside its own
//! `access` module, so the same file compiles against the way that tier reads a primitive and the
//! auxiliary data. `sdf_core` is the full core pipeline and `simple` the one for plain solid
//! shapes: `sdf_core` with the blocks they never use switched off. `sdf_shadow`, `sdf_3d` and
//! `sdf_notch` are the shadow, raymarching and notch pipelines. `russl_parity` checks that simple
//! and full draw the same pixels for the shapes `simple` takes, and that each tier draws what the
//! storage tier draws.

// One source file is a module of each tier, which is the point.
#![allow(clippy::duplicate_mod)]

pub(crate) mod data_texture;
pub(crate) mod storage;
pub(crate) mod vertex_buffer;

pub(crate) mod simple {
    russl::russl! {
        #![specialize(super::storage::sdf_core)]
        const FEAT_GRADIENT: bool = false;
        const FEAT_MASK: bool = false;
        const FEAT_FILTER: bool = false;
        const FEAT_SHADOW: bool = false;
        const FEAT_3D: bool = false;
        const FEAT_CLIP_SHAPE: bool = false;
    }
}
