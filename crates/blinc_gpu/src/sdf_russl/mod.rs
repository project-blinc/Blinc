//! Blinc's SDF shaders in russl, compiled at build time. `sdf_core` is the full core pipeline;
//! `simple` is the one for plain solid shapes: `sdf_core` with the blocks they never use switched
//! off. `sdf_shadow` and `sdf_3d` are the shadow and raymarching pipelines. `russl_parity` checks
//! that simple and full draw the same pixels for the shapes `simple` takes.

pub(crate) mod abi;
mod clip;
mod common;
pub(crate) mod sdf_3d;
pub(crate) mod sdf_core;
pub(crate) mod sdf_shadow;
mod shapes3d;

pub(crate) mod simple {
    russl::russl! {
        #![specialize(super::sdf_core)]
        const FEAT_GRADIENT: bool = false;
        const FEAT_MASK: bool = false;
        const FEAT_FILTER: bool = false;
        const FEAT_SHADOW: bool = false;
        const FEAT_3D: bool = false;
        const FEAT_CLIP_SHAPE: bool = false;
    }
}
