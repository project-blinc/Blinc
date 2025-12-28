# BlincComponent Macro

The `BlincComponent` derive macro generates type-safe hooks for **state and animations**, eliminating manual string keys and reducing boilerplate. Use it to define component-scoped state that persists across UI rebuilds.

## Overview

`BlincComponent` is designed for two primary use cases:

1. **State Management** - Generate `State<T>` hooks for component data (counters, toggles, form values)
2. **Animations** - Generate `SharedAnimatedValue` hooks for spring-based animations

## Basic Usage

```rust
use blinc_app::prelude::*;

#[derive(BlincComponent)]
struct MyComponent;
```

This generates:
- `MyComponent::COMPONENT_KEY` - Unique compile-time key
- `MyComponent::use_animated_value(ctx, initial, config)` - Spring animation
- `MyComponent::use_animated_value_with(ctx, suffix, initial, config)` - Named spring
- `MyComponent::use_animated_timeline(ctx)` - Keyframe timeline
- `MyComponent::use_animated_timeline_with(ctx, suffix)` - Named timeline

---

## State Fields

Fields without `#[animation]` generate state hooks:

```rust
#[derive(BlincComponent)]
struct Counter {
    count: i32,              // Generates: use_count(ctx, initial) -> State<i32>
    step: i32,               // Generates: use_step(ctx, initial) -> State<i32>
}
```

### Using State Fields

```rust
fn counter_demo(ctx: &WindowedContext) -> impl ElementBuilder {
    // BlincComponent generates type-safe state hooks
    let count = Counter::use_count(ctx, 0);
    let step = Counter::use_step(ctx, 1);

    // Create persistent button state handle
    let button_handle = ctx.use_state(ButtonState::Idle);

    // Use stateful(handle) with .deps() to react to state changes
    stateful(button_handle)
        .flex_col()
        .gap(16.0)
        .p(16.0)
        .deps(&[count.signal_id(), step.signal_id()])
        .on_state(move |state, container| {
            // Read current values inside on_state
            let current_count = count.get();
            let current_step = step.get();

            let bg = match state {
                ButtonState::Idle => Color::rgba(0.15, 0.15, 0.2, 1.0),
                ButtonState::Hovered => Color::rgba(0.18, 0.18, 0.25, 1.0),
                _ => Color::rgba(0.15, 0.15, 0.2, 1.0),
            };

            // Update container with dynamic content
            container.merge(
                div()
                    .bg(bg)
                    .child(text(&format!("Count: {}", current_count)).color(Color::WHITE))
                    .child(text(&format!("Step: {}", current_step)).color(Color::WHITE))
            );
        })
        .on_click(move |_| {
            let current_step = step.get();
            count.update(|v| v + current_step);
        })
        .child(increment_button(ctx))
}

fn increment_button(ctx: &WindowedContext) -> impl ElementBuilder {
    let handle = ctx.use_state(ButtonState::Idle);

    stateful(handle)
        .px(16.0)
        .py(8.0)
        .rounded(8.0)
        .on_state(|state, div| {
            let bg = match state {
                ButtonState::Idle => Color::rgba(0.3, 0.5, 0.9, 1.0),
                ButtonState::Hovered => Color::rgba(0.4, 0.6, 1.0, 1.0),
                ButtonState::Pressed => Color::rgba(0.2, 0.4, 0.8, 1.0),
                _ => Color::rgba(0.3, 0.5, 0.9, 1.0),
            };
            div.set_bg(bg);
        })
        .child(text("Increment").color(Color::WHITE))
}
```

**Key point:** When UI content depends on state values that can change, use `stateful(handle)` with `.deps()` to declare the dependency. The `on_state` callback re-runs whenever those signals change, and you update the display via `container.merge()` or `div.set_*()` methods.

### Common State Patterns

```rust
#[derive(BlincComponent)]
struct TodoList {
    items: Vec<String>,      // List of items
    filter: Filter,          // Current filter mode
    selected_index: Option<usize>,  // Currently selected item
}

#[derive(BlincComponent)]
struct FormData {
    username: String,
    email: String,
    is_valid: bool,
}

#[derive(BlincComponent)]
struct Settings {
    theme: Theme,
    notifications_enabled: bool,
    volume: f32,
}
```

---

## Animation Fields

Fields with `#[animation]` generate spring animation hooks:

```rust
#[derive(BlincComponent)]
struct PullToRefresh {
    #[animation]
    content_offset: f32,    // Generates: use_content_offset(ctx, initial, config)

    #[animation]
    icon_scale: f32,        // Generates: use_icon_scale(ctx, initial, config)

    #[animation]
    icon_opacity: f32,      // Generates: use_icon_opacity(ctx, initial, config)
}
```

### Using Animation Fields

```rust
fn pull_to_refresh_demo(ctx: &WindowedContext) -> impl ElementBuilder {
    // Each field gets its own type-safe hook
    let content_offset = PullToRefresh::use_content_offset(ctx, 0.0, SpringConfig::wobbly());
    let icon_scale = PullToRefresh::use_icon_scale(ctx, 0.5, SpringConfig::snappy());
    let icon_opacity = PullToRefresh::use_icon_opacity(ctx, 0.0, SpringConfig::snappy());

    // Use with motion() for animated rendering
    motion()
        .translate_y(content_offset.lock().unwrap().get())
        .child(/* content */)
}
```

---

## Combining State and Animation

A component can have both state and animation fields:

```rust
#[derive(BlincComponent)]
struct ExpandableCard {
    // State fields
    is_expanded: bool,
    content: String,

    // Animation fields
    #[animation]
    height: f32,
    #[animation]
    arrow_rotation: f32,
}

fn expandable_card(ctx: &WindowedContext) -> impl ElementBuilder {
    let is_expanded = ExpandableCard::use_is_expanded(ctx, false);
    let height = ExpandableCard::use_height(ctx, 60.0, SpringConfig::snappy());
    let arrow_rotation = ExpandableCard::use_arrow_rotation(ctx, 0.0, SpringConfig::snappy());

    let expanded = is_expanded.get();

    motion()
        .h(height.lock().unwrap().get())
        .on_click(move |_| {
            is_expanded.update(|v| !v);
            let target_height = if !expanded { 200.0 } else { 60.0 };
            let target_rotation = if !expanded { 180.0 } else { 0.0 };
            height.lock().unwrap().set_target(target_height);
            arrow_rotation.lock().unwrap().set_target(target_rotation);
        })
        .child(/* card content */)
}
```

---

## Multiple Values per Component

Use `_with` suffix methods for multiple values of the same type:

```rust
#[derive(BlincComponent)]
struct DraggableBox;

fn draggable(ctx: &WindowedContext) -> impl ElementBuilder {
    // Multiple animated values with suffixes
    let x = DraggableBox::use_animated_value_with(ctx, "x", 100.0, SpringConfig::wobbly());
    let y = DraggableBox::use_animated_value_with(ctx, "y", 100.0, SpringConfig::wobbly());

    // ...
}
```

---

## Timelines with BlincComponent

```rust
#[derive(BlincComponent)]
struct SpinningLoader;

fn loader(ctx: &WindowedContext) -> impl ElementBuilder {
    let timeline = SpinningLoader::use_animated_timeline(ctx);

    let entry_id = timeline.lock().unwrap().configure(|t| {
        let id = t.add(0, 1000, 0.0, 360.0);
        t.set_loop(-1);
        t.start();
        id
    });

    // ...
}
```

---

## How It Works

The macro generates a unique key from `module_path!()` and the struct name:

```rust
impl MyCard {
    pub const COMPONENT_KEY: &'static str = concat!(module_path!(), "::", stringify!(MyCard));
    // e.g., "my_app::components::MyCard"
}
```

This ensures:
- **Uniqueness** - Keys are unique across your entire codebase
- **Stability** - Keys don't change unless you move/rename the struct
- **No collisions** - Different modules can have same-named components

---

## Generated Methods

### For Unit Structs

```rust
#[derive(BlincComponent)]
struct MyComponent;

// Generates:
impl MyComponent {
    pub const COMPONENT_KEY: &'static str;

    pub fn use_animated_value(
        ctx: &WindowedContext,
        initial: f32,
        config: SpringConfig,
    ) -> SharedAnimatedValue;

    pub fn use_animated_value_with(
        ctx: &WindowedContext,
        suffix: &str,
        initial: f32,
        config: SpringConfig,
    ) -> SharedAnimatedValue;

    pub fn use_animated_timeline(
        ctx: &WindowedContext,
    ) -> SharedAnimatedTimeline;

    pub fn use_animated_timeline_with(
        ctx: &WindowedContext,
        suffix: &str,
    ) -> SharedAnimatedTimeline;
}
```

### For Structs with Fields

```rust
#[derive(BlincComponent)]
struct MyComponent {
    #[animation]
    scale: f32,
    count: i32,
}

// Additionally generates:
impl MyComponent {
    pub fn use_scale(
        ctx: &WindowedContext,
        initial: f32,
        config: SpringConfig,
    ) -> SharedAnimatedValue;

    pub fn use_count(
        ctx: &WindowedContext,
        initial: i32,
    ) -> State<i32>;
}
```

---

## Best Practices

1. **Group related state and animations** - A component should represent one logical UI element with its related state and animations.

2. **Use fields for named values** - Prefer `#[animation] scale: f32` over `use_animated_value_with(ctx, "scale", ...)`.

3. **Combine state and animations** - Use state fields for data, animation fields for visual transitions.

4. **Document fields** - Add doc comments to fields for generated method documentation.

```rust
#[derive(BlincComponent)]
struct ExpandableSection {
    /// Whether the section is currently expanded
    is_expanded: bool,

    /// Animated height for smooth expand/collapse
    #[animation]
    height: f32,
}
```

5. **Use `motion()` with animated values** - Wrap content using animated values in `motion()` for proper redraws.
