//! Flow shaders.
//!
//! A flow is a shader graph: a target, typed inputs, computation nodes and outputs, written in a
//! small statement language and held as a [`blinc_core::FlowGraph`] the GPU layer compiles. The
//! [`flow!`] macro writes one inline, and a canvas draws it:
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

mod macros;
mod parse;
mod util;

pub use parse::{parse_flow_string, parse_flow_string_with};
pub use util::{ParseError, Severity};

#[cfg(test)]
mod tests {
    use blinc_core::FlowTarget;

    #[test]
    fn the_macro_builds_a_graph_from_rust_tokens() {
        let graph = crate::flow!(ripple, fragment, {
            input uv: builtin(uv);
            input time: builtin(time);
            node d = distance(uv, vec2(0.5, 0.5));
            node wave = sin(d * 20.0 - time * 4.0);
            output color = vec4(wave, wave, wave, 1.0);
        });
        assert_eq!(graph.name, "ripple");
        assert_eq!(graph.target, FlowTarget::Fragment);
        assert_eq!(graph.inputs.len(), 2);
        assert_eq!(graph.outputs.len(), 1);
    }
}
