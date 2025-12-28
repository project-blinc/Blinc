# Introduction

**Blinc** is a GPU-accelerated, reactive UI framework for Rust. It provides a declarative, component-based approach to building high-performance user interfaces with smooth animations and modern visual effects.

## Why Blinc?

- **GPU-Accelerated Rendering** - All rendering is done on the GPU via wgpu, enabling smooth 60fps animations and complex visual effects like glass materials and shadows.

- **Declarative UI** - Build interfaces using a fluent, composable API inspired by SwiftUI and modern web frameworks. No manual DOM manipulation.

- **Reactive State** - Automatic UI updates when state changes, with fine-grained reactivity for optimal performance.

- **Spring Physics** - Natural, physics-based animations using spring dynamics instead of fixed durations.

- **Cross-Platform** - Runs on macOS, Windows, Linux, and Android (iOS coming soon).

## Key Features

### Flexbox Layout
All layout is powered by [Taffy](https://github.com/DioxusLabs/taffy), a high-performance flexbox implementation. Use familiar CSS-like properties:

```rust
div()
    .flex_col()
    .gap(16.0)
    .p(24.0)
    .child(text("Hello"))
    .child(text("World"))
```

### Material Effects
Built-in support for glass, metallic, and other material effects:

```rust
div()
    .glass()
    .rounded(16.0)
    .p(24.0)
    .child(text("Frosted Glass"))
```

### Type-Safe Animations
The `BlincComponent` derive macro generates type-safe animation hooks:

```rust
#[derive(BlincComponent)]
struct MyCard {
    #[animation]
    scale: f32,
    #[animation]
    opacity: f32,
}

// Usage
let scale = MyCard::use_scale(ctx, 1.0, SpringConfig::snappy());
let opacity = MyCard::use_opacity(ctx, 0.0, SpringConfig::gentle());
```

### Event Handling
Intuitive event handling with closures:

```rust
div()
    .on_click(|_| println!("Clicked!"))
    .on_hover_enter(|_| println!("Hovered"))
```

## Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│                   Your Application                   │
├─────────────────────────────────────────────────────┤
│  blinc_app   │  WindowedApp, Context, State Hooks   │
├──────────────┼──────────────────────────────────────┤
│  blinc_layout│  Elements, Flexbox, Event Routing    │
├──────────────┼──────────────────────────────────────┤
│  blinc_animation │  Springs, Timelines, Motion      │
├──────────────┼──────────────────────────────────────┤
│  blinc_gpu   │  Render Pipeline, Materials          │
├──────────────┼──────────────────────────────────────┤
│  wgpu        │  GPU Abstraction Layer               │
└─────────────────────────────────────────────────────┘
```

## Quick Example

Here's a minimal Blinc application:

```rust
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};

fn main() -> Result<()> {
    WindowedApp::run(WindowConfig::default(), |ctx| {
        div()
            .w(ctx.width)
            .h(ctx.height)
            .bg(Color::rgba(0.1, 0.1, 0.15, 1.0))
            .flex_center()
            .child(
                div()
                    .glass()
                    .rounded(16.0)
                    .p(32.0)
                    .child(text("Hello, Blinc!").size(24.0).color(Color::WHITE))
            )
    })
}
```

## Next Steps

- [Installation](./getting-started/installation.md) - Set up your development environment
- [Your First App](./getting-started/first-app.md) - Build a complete application step by step
- [Elements & Layout](./core/elements-layout.md) - Learn about available UI elements
