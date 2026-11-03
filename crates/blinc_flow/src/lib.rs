//! Flow shaders.
//!
//! A flow is a shader graph: a target, typed inputs, computation nodes and outputs, written in a
//! small statement language and held as a [`blinc_core::FlowGraph`]. This crate parses the
//! language and generates WGSL from the graph, with no GPU dependency, so `flow!` (in
//! `blinc_macros`) runs both at compile time and emits a [`blinc_core::FlowShader`] a canvas
//! draws:
//!
//! ```rust,ignore
//! canvas(|_, _| {}).w(200.0).h(200.0).flow(flow!(ripple, fragment, {
//!     input uv: builtin(uv);
//!     input time: builtin(time);
//!     node d = distance(uv, vec2(0.5, 0.5));
//!     node wave = sin(d * 20.0 - time * 4.0);
//!     output color = vec4(wave, wave, wave, 1.0);
//! }))
//! ```

mod codegen;
mod parse;
mod util;

use blinc_core::FlowShader;

pub use codegen::{compile_flow, flow_needs_scene_texture, flow_to_wgsl};
pub use parse::{parse_flow_string, parse_flow_string_with};
pub use util::{ParseError, Severity};

/// Compile `@flow` source text into a shader at run time. `flow!` does the same at compile time.
pub fn flow_shader(source: &str) -> Result<FlowShader, String> {
    let graph = parse_flow_string(source)?;
    compile_flow(&graph).map_err(|e| format!("codegen error: {e}"))
}
