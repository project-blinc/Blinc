# blinc_core

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

Core runtime for the Blinc UI framework - reactive signals, state machines, and event dispatch.

## Overview

`blinc_core` provides the foundational building blocks for the Blinc UI framework:

- **Reactive Signals**: Fine-grained reactivity without Virtual DOM overhead
- **State Machines**: Harel statecharts for widget interaction states
- **Event Dispatch**: Unified event handling across platforms
- **Layer Model**: Unified visual content representation
- **Draw Context**: Unified rendering API for 2D content

## Features

### Reactive Signals

```rust
use blinc_core::{Signal, Derived, Effect};

// Create a signal
let count = Signal::new(0);

// Derive a computed value
let doubled = Derived::new(|| count.get() * 2);

// Create an effect that runs when dependencies change
Effect::new(|| {
    println!("Count is now: {}", count.get());
});

// Update the signal
count.set(5); // Effect runs, prints "Count is now: 5"
```

### State Machines

```rust
use blinc_core::{StateMachine, FsmRuntime};

// Define states and transitions for interactive widgets
let fsm = StateMachine::new()
    .state("idle")
    .state("hover")
    .state("pressed")
    .transition("idle", "mouseenter", "hover")
    .transition("hover", "mouseleave", "idle")
    .transition("hover", "mousedown", "pressed")
    .build();
```

### Draw Context

```rust
use blinc_core::{DrawContext, Color, Rect, CornerRadius, Brush};

fn render(ctx: &mut dyn DrawContext) {
    // Fill a rounded rectangle
    ctx.fill_rect(
        Rect::new(0.0, 0.0, 100.0, 50.0),
        CornerRadius::uniform(8.0),
        Brush::Solid(Color::BLUE),
    );
}
```

## Main Types

| Type | Description |
|------|-------------|
| `Signal<T>` | Reactive state container |
| `Derived<T>` | Computed value that auto-updates |
| `Effect` | Side effect that runs on dependency changes |
| `StateMachine` | FSM for interaction states |
| `DrawContext` | Trait for 2D drawing operations |
| `Color` | RGBA color type |
| `Brush` | Fill type (solid, gradient, glass) |
| `Rect` | Rectangle geometry |
| `CornerRadius` | Per-corner border radius |
| `Shadow` | Drop shadow configuration |
| `Transform` | 2D transformation matrix |

## Architecture

```
blinc_core
├── signals/      # Reactive primitives (Signal, Derived, Effect)
├── fsm/          # State machine infrastructure
├── events/       # Event types and dispatch
├── draw/         # Drawing context and primitives
├── geometry/     # Rect, Point, Size, Transform
├── color/        # Color and brush types
└── store/        # Key-value state persistence
```

## License

MIT OR Apache-2.0
