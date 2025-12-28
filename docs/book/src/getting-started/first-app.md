# Your First App

Let's build a simple counter application to learn Blinc fundamentals.

## The Basic Structure

Every Blinc windowed application follows this pattern:

```rust
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};

fn main() -> Result<()> {
    WindowedApp::run(WindowConfig::default(), |ctx| {
        // Your UI goes here
        build_ui(ctx)
    })
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .w(ctx.width)
        .h(ctx.height)
        // ... children
}
```

The `WindowedApp::run` function:
1. Creates a window with the given configuration
2. Sets up the GPU renderer
3. Calls your UI builder function when needed
4. Handles events and animations automatically

## Building a Counter

Let's create a counter with increment and decrement buttons.

### Step 1: Window Configuration

```rust
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};

fn main() -> Result<()> {
    let config = WindowConfig {
        title: "Counter App".to_string(),
        width: 400,
        height: 300,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}
```

### Step 2: Creating State

Use `use_state_keyed` to create reactive state that persists across UI rebuilds:

```rust
fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    // Create keyed state for the count - persists across rebuilds
    let count = ctx.use_state_keyed("counter", || 0i32);

    // State will be read inside stateful elements via .deps()
    // ... rest of UI
}
```

### Step 3: Building the Layout with Stateful Elements

The key insight in Blinc is that UI doesn't rebuild on every state change. Instead, we use `stateful(handle)` with `.deps()` to react to state changes:

```rust
use blinc_layout::stateful::stateful;

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    let count = ctx.use_state_keyed("counter", || 0i32);
    let container_handle = ctx.use_state(ButtonState::Idle);

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
        .flex_col()
        .justify_center()
        .items_center()
        .gap(24.0)
        // Title
        .child(
            text("Counter")
                .size(32.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE)
        )
        // Count display - uses stateful with deps to update when count changes
        .child(count_display(ctx, count.clone()))
        // Buttons row
        .child(
            div()
                .flex_row()
                .gap(16.0)
                .child(counter_button(ctx, count.clone(), "-", -1))
                .child(counter_button(ctx, count.clone(), "+", 1))
        )
}
```

### Step 4: Creating the Count Display

The count display needs to update when the count changes. We use `stateful(handle)` with `.deps()`:

```rust
fn count_display(ctx: &WindowedContext, count: State<i32>) -> impl ElementBuilder {
    let handle = ctx.use_state(ButtonState::Idle);

    stateful(handle)
        .deps(&[count.signal_id()])
        .on_state(move |_state, container| {
            let current = count.get();
            container.merge(
                div()
                    .child(
                        text(&format!("{}", current))
                            .size(64.0)
                            .weight(FontWeight::Bold)
                            .color(Color::rgba(0.4, 0.6, 1.0, 1.0))
                    )
            );
        })
}
```

### Step 5: Creating Interactive Buttons

For interactive buttons with hover and press states, use `stateful(handle)`:

```rust
fn counter_button(
    ctx: &WindowedContext,
    count: State<i32>,
    label: &'static str,
    delta: i32,
) -> impl ElementBuilder {
    // Use use_state_for for reusable components with a unique key
    let handle = ctx.use_state_for(label, ButtonState::Idle);

    stateful(handle)
        .w(60.0)
        .h(60.0)
        .rounded(12.0)
        .flex_center()
        .on_state(|state, div| {
            // Apply different styles based on current state
            let bg = match state {
                ButtonState::Idle => Color::rgba(0.2, 0.2, 0.25, 1.0),
                ButtonState::Hovered => Color::rgba(0.3, 0.3, 0.35, 1.0),
                ButtonState::Pressed => Color::rgba(0.15, 0.15, 0.2, 1.0),
                ButtonState::Disabled => Color::rgba(0.1, 0.1, 0.12, 0.5),
            };
            div.set_bg(bg);
        })
        .on_click(move |_| {
            count.update(|v| v + delta);
        })
        .child(
            text(label)
                .size(28.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE)
        )
}
```

## Complete Example

Here's the full counter application:

```rust
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_layout::stateful::stateful;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Counter App".to_string(),
        width: 400,
        height: 300,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    let count = ctx.use_state_keyed("counter", || 0i32);

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
        .flex_col()
        .justify_center()
        .items_center()
        .gap(24.0)
        .child(
            text("Counter")
                .size(32.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE)
        )
        .child(count_display(ctx, count.clone()))
        .child(
            div()
                .flex_row()
                .gap(16.0)
                .child(counter_button(ctx, count.clone(), "-", -1))
                .child(counter_button(ctx, count.clone(), "+", 1))
        )
}

fn count_display(ctx: &WindowedContext, count: State<i32>) -> impl ElementBuilder {
    let handle = ctx.use_state(ButtonState::Idle);

    stateful(handle)
        .deps(&[count.signal_id()])
        .on_state(move |_state, container| {
            let current = count.get();
            container.merge(
                div()
                    .child(
                        text(&format!("{}", current))
                            .size(64.0)
                            .weight(FontWeight::Bold)
                            .color(Color::rgba(0.4, 0.6, 1.0, 1.0))
                    )
            );
        })
}

fn counter_button(
    ctx: &WindowedContext,
    count: State<i32>,
    label: &'static str,
    delta: i32,
) -> impl ElementBuilder {
    let handle = ctx.use_state_for(label, ButtonState::Idle);

    stateful(handle)
        .w(60.0)
        .h(60.0)
        .rounded(12.0)
        .flex_center()
        .on_state(|state, div| {
            let bg = match state {
                ButtonState::Idle => Color::rgba(0.2, 0.2, 0.25, 1.0),
                ButtonState::Hovered => Color::rgba(0.3, 0.3, 0.35, 1.0),
                ButtonState::Pressed => Color::rgba(0.15, 0.15, 0.2, 1.0),
                ButtonState::Disabled => Color::rgba(0.1, 0.1, 0.12, 0.5),
            };
            div.set_bg(bg);
        })
        .on_click(move |_| {
            count.update(|v| v + delta);
        })
        .child(
            text(label)
                .size(28.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE)
        )
}
```

> **Tip:** For more examples, explore the `crates/blinc_app/examples/` directory which includes
> `windowed.rs`, `canvas_demo.rs`, `motion_demo.rs`, and more.

## Key Concepts Learned

1. **WindowedApp::run** - Entry point for desktop applications
2. **WindowedContext** - Provides window dimensions and state hooks
3. **use_state_keyed** - Creates reactive state with a string key
4. **stateful(handle)** - Creates elements that react to state changes
5. **deps()** - Declares signal dependencies for reactive updates
6. **on_state** - Callback that runs when state or dependencies change
7. **Fluent Builder API** - Chain methods like `.w()`, `.h()`, `.child()`
8. **Flexbox Layout** - Use `.flex_col()`, `.flex_center()`, `.gap()`

## Next Steps

- Learn about all available [Elements & Layout](../core/elements-layout.md)
- Add [Spring Animations](../animation/springs.md) to your counter
- Explore [Styling & Materials](../core/styling-materials.md) for visual polish
