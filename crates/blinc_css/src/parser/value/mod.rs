//! Value grammars.
//!
//! One module per family of CSS value: colors, gradients, lengths,
//! transforms, shadows, filters, clip paths, and the rest. Each parses a
//! declaration's right-hand side into the typed form the style stores.

mod brush;
mod clip_path;
mod color;
mod filter;
mod gradient;
mod length;
mod shadow;
mod shape3d;
mod svg_path;
mod text;
mod transform;

pub(crate) use brush::*;
pub use clip_path::*;
pub(crate) use color::*;
pub(crate) use filter::*;
pub use gradient::*;
pub(crate) use length::*;
pub(crate) use shadow::*;
pub use shape3d::*;
pub(crate) use svg_path::*;
pub(crate) use text::*;
pub(crate) use transform::*;
