//! Blinc's SDF shaders in russl, compiled at build time.
//!
//! The sources live in `shared`. A shader written once serves every device tier: `TIER` is a
//! pipeline constant (0 storage buffers, 1 a vertex buffer for the vertex stage, 2 data
//! textures), `#if` in the source chooses what each tier declares and reads, and a one-line
//! specialisation builds the variant. `simple` is `sdf_core` with the blocks plain solid shapes
//! never use switched off. `russl_parity` checks that simple and full draw the same pixels for the
//! shapes `simple` takes, and that each tier draws what the storage tier draws.

// One source file is a module in more than one place.
#![allow(clippy::duplicate_mod)]

#[path = "shared/access.rs"]
pub(crate) mod access;
#[path = "shared/clip.rs"]
mod clip;
#[path = "shared/common.rs"]
mod common;
#[path = "shared/notch.rs"]
mod notch;
#[path = "shared/primitive.rs"]
pub(crate) mod primitive;
#[path = "shared/shadow.rs"]
mod shadow;
#[path = "shared/shapes3d.rs"]
mod shapes3d;
#[path = "shared/three_d.rs"]
mod three_d;

/// The compact shader for plain rects and circles, storage tier only.
#[path = "shared/sdf_simple.rs"]
pub(crate) mod sdf_simple;

/// The glyph shader: storage tier and data texture tier.
#[path = "shared/text.rs"]
pub(crate) mod text;
pub(crate) mod text_dt {
    russl::russl! { #![specialize(super::text)] const TIER: u32 = 2; }
}

/// The core shader: storage tier, vertex buffer tier and data texture tier, and `simple`, the
/// storage tier with the blocks plain solid shapes never use switched off.
#[path = "shared/sdf_core.rs"]
pub(crate) mod sdf_core;
pub(crate) mod sdf_core_vb {
    russl::russl! { #![specialize(super::sdf_core)] const TIER: u32 = 1; }
}
pub(crate) mod sdf_core_dt {
    russl::russl! { #![specialize(super::sdf_core)] const TIER: u32 = 2; }
}

/// The shadow shader: the same three tiers.
#[path = "shared/sdf_shadow.rs"]
pub(crate) mod sdf_shadow;
pub(crate) mod sdf_shadow_vb {
    russl::russl! { #![specialize(super::sdf_shadow)] const TIER: u32 = 1; }
}
pub(crate) mod sdf_shadow_dt {
    russl::russl! { #![specialize(super::sdf_shadow)] const TIER: u32 = 2; }
}

/// The notch shader: the same three tiers.
#[path = "shared/sdf_notch.rs"]
pub(crate) mod sdf_notch;
pub(crate) mod sdf_notch_vb {
    russl::russl! { #![specialize(super::sdf_notch)] const TIER: u32 = 1; }
}
pub(crate) mod sdf_notch_dt {
    russl::russl! { #![specialize(super::sdf_notch)] const TIER: u32 = 2; }
}

/// The 3D shader: the same three tiers.
#[path = "shared/sdf_3d.rs"]
pub(crate) mod sdf_3d;
pub(crate) mod sdf_3d_vb {
    russl::russl! { #![specialize(super::sdf_3d)] const TIER: u32 = 1; }
}
pub(crate) mod sdf_3d_dt {
    russl::russl! { #![specialize(super::sdf_3d)] const TIER: u32 = 2; }
}

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
