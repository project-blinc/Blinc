//! The `ElementStyle` builder, one module per family of property.
//!
//! Each module adds its own `impl ElementStyle` block. Rust allows an
//! inherent impl to be split across modules of the defining crate, so the
//! builder reads as one API while its ~200 methods stay grouped by what
//! they set.

mod border;
mod effects;
mod interaction;
mod layout;
mod merge;
mod paint;
mod surface;
mod svg;
mod text;
mod transform;
