//! Procedural macros for Blinc UI.
//!
//! `BlincComponent` generates reactive props, named Signal hooks and animation
//! hooks. Signal and Computed props use the layout binding channel; hooks are
//! generic over context traits and preserve per-field, per-instance identity.
//! `flow!` compiles canvas flow shaders to WGSL.

mod component;
mod flow;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

/// Generate reactive props, persistent signal hooks and animation hooks.
///
/// A named struct `Counter { count: i32 }` generates `CounterProps` with
/// a `Reactive<i32>` field and a fluent `count(impl IntoReactive<i32>)` setter.
/// `Counter::props(value)` constructs it from a value, `Signal` or `Computed`.
/// The declared struct is unchanged.
///
/// `Counter::use_count(ctx, initial)` returns a persisted `Signal<i32>`.
/// `_for` accepts an explicit instance key; `_auto` keys by the call site.
/// Fields marked `#[animation]` generate spring hooks and are omitted from props.
/// All structs also receive `COMPONENT_KEY`, instance keys, and shared spring
/// and timeline hooks. Named props require a direct `blinc_layout` dependency;
/// hook consumers depend on `blinc_core` and `blinc_animation`.
///
/// ```ignore
/// #[derive(BlincComponent)]
/// struct Counter {
///     label: String,
///     count: i32,
/// }
///
/// let count = Counter::use_count(ctx, 0);
/// let label = computed(move |g| format!("Count: {}", g.get(count).unwrap_or(0)));
/// let props = Counter::props(label, count);
/// let view = Text::bound(props.label);
/// ```
#[proc_macro_derive(BlincComponent, attributes(animation))]
pub fn derive_blinc_component(input: TokenStream) -> TokenStream {
    match component::expand(parse_macro_input!(input as DeriveInput)) {
        Ok(expanded) => expanded.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Transpile a flow shader to WGSL at compile time.
///
/// Evaluates to a `blinc_core::FlowShader`: the WGSL and what a pipeline binds for it. It is a
/// constant expression, so a `const` can hold one, and nothing is parsed or generated at run
/// time. Draw it with `Canvas::flow`.
///
/// ```ignore
/// let ripple = flow!(ripple, fragment, {
///     input uv: builtin(uv);
///     input time: builtin(time);
///     node d = distance(uv, vec2(0.5, 0.5));
///     node wave = sin(d * 20.0 - time * 4.0);
///     output color = vec4(wave, wave, wave, 1.0);
/// });
/// ```
///
/// The body is the flow language written as Rust tokens: `input`, `node`, `step`, `chain`,
/// `use` and `output` statements. Step types take their underscore names (`pattern_noise`,
/// `color_ramp`, `adjust_falloff`) and colours are hex integers (`0x1a3a5a`, or
/// `0xrrggbbaa` with alpha).
///
/// ```ignore
/// let terrain = flow!(terrain, fragment, {
///     input uv: builtin(uv);
///     input time: builtin(time);
///     step noise: pattern_noise { scale: 4.0; detail: 6; animation: time * 0.3; }
///     step palette: color_ramp {
///         source: noise;
///         stops: 0x1a3a5a 0.0, 0x88cc66 0.55, 0xffffff 1.0;
///     }
///     output color = palette;
/// });
/// ```
///
/// A flow that does not parse or generate is a compile error at the macro call. The expansion
/// names `::blinc_core`, so the calling crate depends on it.
#[proc_macro]
pub fn flow(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as flow::FlowInput);
    TokenStream::from(flow::expand(input))
}
