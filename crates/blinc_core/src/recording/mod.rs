//! What a recording holds: user events, element-tree snapshots and the
//! primitives they are made of.
//!
//! These types live here so the layout, the runners and `blinc_recorder` share
//! one definition: a runner sends them through the hooks on
//! [`crate::BlincContextState`], and the recorder stores, replays and shows
//! them. They are plain data and serialise, so a recording can be saved and
//! opened in the debugger.

mod events;
mod primitives;
mod tree;

pub use events::*;
pub use primitives::*;
pub use tree::*;
