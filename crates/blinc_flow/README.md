# blinc_flow

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

Flow shaders: a small shader-graph language, parsed and turned into WGSL without a GPU.

## Overview

A flow is a target, typed inputs, computation nodes and outputs. `blinc_flow` parses the
language into a `blinc_core::FlowGraph` and generates WGSL from it. The `flow!` macro in
`blinc_macros` (re-exported by `blinc_layout`) runs both at compile time and evaluates to a
`blinc_core::FlowShader` that a canvas draws with `Canvas::flow`.

## Run-time compilation

For source that is not known when the app is built:

```rust
let shader = blinc_flow::flow_shader(
    "@flow solid { target: fragment; output color = vec4(1.0, 0.0, 0.0, 1.0); }",
)?;
```

`flow_to_wgsl` and `compile_flow` give the generated WGSL and the `FlowShader` from a
`FlowGraph`.

## License

Apache-2.0
