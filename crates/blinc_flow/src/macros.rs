//! The `flow!` macro.

/// Define a flow shader with Rust-like syntax, as a [`FlowGraph`](blinc_core::FlowGraph).
///
/// The macro accepts Rust identifiers and primitives, with no raw strings. A canvas draws the
/// graph it produces.
///
/// # Syntax
///
/// ```rust,ignore
/// let graph = flow!(shader_name, fragment, {
///     input uv: builtin(uv);
///     input time: builtin(time);
///     step mist: pattern_noise { scale: 3.0; detail: 5; };
///     node wave = sin(uv.x * 10.0 + time);
///     output color = vec4(wave, wave, wave, 1.0);
/// });
/// ```
///
/// # Usage
///
/// ```rust,ignore
/// canvas(|_, _| {}).w(200.0).h(200.0).flow(flow!(ripple, fragment, {
///     input uv: builtin(uv);
///     input time: builtin(time);
///     node d = distance(uv, vec2(0.5, 0.5));
///     node wave = sin(d * 20.0 - time * 4.0);
///     output color = vec4(wave, wave, wave, 1.0);
/// }))
/// ```
#[macro_export]
macro_rules! flow {
    ($name:ident, $target:ident, { $($body:tt)* }) => {{
        $crate::parse_flow_string(concat!(
            "@flow ", stringify!($name), " {\n  target: ", stringify!($target), ";\n  ",
            stringify!($($body)*),
            "\n}"
        )).expect(concat!("flow!: failed to parse flow '", stringify!($name), "'"))
    }};
}
