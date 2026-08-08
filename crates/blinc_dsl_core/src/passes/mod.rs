//! Post-parse lowering passes.
//!
//! The pipeline turns the grammar's typed AST into a shape the Zyntax JIT
//! can compile: signals and FSMs become extern calls, components are
//! inlined and given call-site identity, widget bodies become child
//! lists, and styling args become overlay bindings. Each submodule owns
//! one area; `lib.rs` drives the ordering, which matters — several passes
//! document the passes they must run after.
//!
//! Submodules re-export their passes here, so call sites stay
//! `passes::<name>` regardless of which file a pass lives in.

mod args;
mod components;
mod consts;
mod fsm;
mod list_map;
mod matching;
mod misc;
mod modules;
mod reactive;
mod refs;
mod signals;
mod structs;
mod styling;
mod view;
mod widgets;
mod with_blocks;

pub(crate) use args::*;
pub(crate) use components::*;
pub(crate) use consts::*;
pub(crate) use fsm::*;
pub(crate) use list_map::*;
pub(crate) use matching::*;
pub(crate) use misc::*;
pub(crate) use modules::*;
pub(crate) use reactive::*;
pub(crate) use refs::resolve_ref_calls;
pub use signals::signal_registry_key;
pub(crate) use signals::*;
pub(crate) use structs::*;
pub(crate) use styling::*;
pub(crate) use view::*;
pub(crate) use widgets::*;
pub(crate) use with_blocks::*;
