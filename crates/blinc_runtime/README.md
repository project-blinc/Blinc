# blinc_runtime

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

Core runtime for Blinc UI applications.

## Overview

`blinc_runtime` is the embedding SDK for integrating Blinc into Rust applications. It re-exports the essential crates needed for building Blinc applications.

## Features

- **Modular**: Enable only the features you need
- **Full Feature**: `full` feature enables all components
- **Prelude**: Convenient re-exports for common usage

## Quick Start

```toml
[dependencies]
blinc_runtime = { version = "0.1", features = ["full"] }
```

```rust
use blinc_runtime::prelude::*;

fn main() {
    // Initialize runtime
    blinc_runtime::init();

    // Build your UI
    let ui = div()
        .w_full()
        .h_full()
        .child(text("Hello from Blinc Runtime!"));
}
```

## Feature Flags

| Feature | Description | Default |
|---------|-------------|---------|
| `full` | Enable all features | No |
| `core` | Core reactivity and events | Yes |
| `animation` | Animation system | Yes |
| `layout` | Layout engine | Yes |
| `gpu` | GPU rendering | No |
| `paint` | 2D painting API | Yes |

## Re-exports

When using the `full` feature:

```rust
// Core types
pub use blinc_core::*;

// Animation
pub use blinc_animation::*;

// Layout
pub use blinc_layout::*;

// GPU (with gpu feature)
pub use blinc_gpu::*;

// Paint
pub use blinc_paint::*;
```

## Initialization

```rust
use blinc_runtime;

fn main() {
    // Initialize global state, font registry, etc.
    blinc_runtime::init();

    // Your application code
}
```

## Use Cases

- **Embedding**: Integrate Blinc UI into existing Rust applications
- **Custom Shells**: Build custom application shells around Blinc
- **Testing**: Create test harnesses for Blinc components
- **Headless**: Render Blinc UI without a window

## License

MIT OR Apache-2.0
