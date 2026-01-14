# blinc_macros

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

Procedural macros for Blinc UI.

## Overview

`blinc_macros` provides derive macros for generating boilerplate code in Blinc applications.

## Macros

### BlincComponent

Generate component infrastructure including unique keys, animation hooks, and state management:

```rust
use blinc_macros::BlincComponent;

#[derive(BlincComponent)]
struct MyButton {
    label: String,
    #[animate]
    scale: f32,
    #[animate]
    opacity: f32,
}

impl MyButton {
    fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            scale: 1.0,
            opacity: 1.0,
        }
    }

    fn render(&self) -> impl ElementBuilder {
        button(&self.label)
            .transform(Transform::scale(self.scale, self.scale))
            .opacity(self.opacity)
    }
}
```

### Generated Code

The `BlincComponent` derive generates:

```rust
impl MyButton {
    // Unique component key for diffing
    fn component_key() -> &'static str {
        "MyButton_abc123"  // Compile-time unique ID
    }

    // Instance key for multiple instances
    fn instance_key(&self) -> String {
        format!("{}_{}", Self::component_key(), /* instance id */)
    }

    // Animation state accessors
    fn animated_scale(&self) -> AnimatedValue<f32> { ... }
    fn animated_opacity(&self) -> AnimatedValue<f32> { ... }
}
```

## Attributes

### `#[animate]`

Mark a field as animatable:

```rust
#[derive(BlincComponent)]
struct Card {
    #[animate]
    height: f32,  // Generates AnimatedValue<f32>

    #[animate(spring = "bouncy")]
    scale: f32,   // Use bouncy spring preset

    #[animate(duration = 300)]
    opacity: f32, // 300ms duration
}
```

### `#[state]`

Mark a field as reactive state:

```rust
#[derive(BlincComponent)]
struct Counter {
    #[state]
    count: i32,  // Generates Signal<i32>
}

// Usage:
counter.count.set(5);
let value = counter.count.get();
```

### `#[key]`

Customize instance key generation:

```rust
#[derive(BlincComponent)]
#[key(field = "id")]
struct ListItem {
    id: String,
    label: String,
}

// Instance key will use the `id` field
```

## Instance Key Variants

```rust
// Single instance (default)
#[derive(BlincComponent)]
struct Header { ... }

// Multiple instances with auto key
#[derive(BlincComponent)]
#[key(auto)]
struct ListItem { ... }

// Multiple instances with explicit key field
#[derive(BlincComponent)]
#[key(field = "item_id")]
struct ListItem {
    item_id: String,
    ...
}
```

## Requirements

- Rust 1.65+ (for proc-macro2 features)
- `syn` and `quote` for macro implementation

## License

MIT OR Apache-2.0
