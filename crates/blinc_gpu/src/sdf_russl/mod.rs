//! The SDF core shader in russl, compiled at build time. `simple` is the pipeline for plain solid
//! shapes: `sdf_core` with the blocks they never use switched off. The full pipeline is still
//! built from `sdf_core.wgsl`; `russl_parity` checks that the two draw the same pixels.

mod clip;
mod common;
mod sdf_core;

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
