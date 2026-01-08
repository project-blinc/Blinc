# State Management

Blinc uses **Stateful elements** as the primary way to manage UI state. Stateful elements handle state transitions automatically without rebuilding the entire UI tree.

## Stateful Elements

`Stateful` is a wrapper element that manages visual states (hover, press, focus, etc.) efficiently. When state changes, only the affected element updates - not the entire UI.

### Basic Usage

```rust
use blinc_layout::prelude::*;

fn feature_card(label: &str, accent: Color) -> impl ElementBuilder {
    let label = label.to_string();

    stateful::<ButtonState>()
        .w_fit()
        .p(4.0)
        .rounded(14.0)
        .on_state(move |ctx| {
            let bg = match ctx.state() {
                ButtonState::Idle => accent,
                ButtonState::Hovered => Color::rgba(
                    (accent.r * 1.15).min(1.0),
                    (accent.g * 1.15).min(1.0),
                    (accent.b * 1.15).min(1.0),
                    accent.a,
                ),
                ButtonState::Pressed => Color::rgba(
                    accent.r * 0.85,
                    accent.g * 0.85,
                    accent.b * 0.85,
                    accent.a,
                ),
                ButtonState::Disabled => Color::GRAY,
            };

            div()
                .bg(bg)
                .on_click({
                    let label = label.clone();
                    move |_| println!("'{}' clicked!", label)
                })
                .child(text(&label).color(Color::WHITE))
        })
}
```

### How It Works

1. `stateful::<S>()` creates a StatefulBuilder for state type S
2. `.on_state(|ctx| ...)` defines the callback that receives a `StateContext`
3. Events (hover, click, etc.) trigger automatic state transitions
4. `ctx.state()` returns the current state for pattern matching
5. Return a `Div` from the callback - it's merged onto the container

---

## StateContext

The `StateContext` provides access to state and scoped utilities within your callback:

```rust
stateful::<ButtonState>()
    .on_state(|ctx| {
        // Get current state
        let state = ctx.state();

        // Create scoped signals (persist across rebuilds)
        let counter = ctx.use_signal("counter", || 0);

        // Create scoped animated values
        let opacity = ctx.use_animated_value("opacity", 1.0);

        // Access dependency values
        let value: i32 = ctx.dep(0).unwrap_or_default();

        // Dispatch events to trigger state transitions
        // ctx.dispatch(CUSTOM_EVENT);

        div().bg(color_for_state(state))
    })
```

### StateContext Methods

| Method | Description |
|--------|-------------|
| `ctx.state()` | Get the current state value |
| `ctx.event()` | Get the event that triggered this callback (if any) |
| `ctx.use_signal(name, init)` | Create/retrieve a scoped signal |
| `ctx.use_spring(name, target, config)` | Declarative spring animation (recommended) |
| `ctx.spring(name, target)` | Declarative spring with default stiff config |
| `ctx.use_animated_value(name, initial)` | Low-level animated value handle |
| `ctx.use_timeline(name)` | Create/retrieve an animated timeline |
| `ctx.dep::<T>(index)` | Get dependency value by index |
| `ctx.dep_as_state::<T>(index)` | Get dependency as State<T> handle |
| `ctx.dispatch(event)` | Trigger a state transition |

---

## Event Access

Use `ctx.event()` to access the event that triggered the callback:

```rust
use blinc_core::events::event_types::*;

stateful::<ButtonState>()
    .on_state(|ctx| {
        // ctx.event() returns Some(EventContext) when triggered by user event
        // Returns None when triggered by dependency changes
        if let Some(event) = ctx.event() {
            match event.event_type {
                POINTER_UP => {
                    println!("Clicked at ({}, {})", event.local_x, event.local_y);
                }
                POINTER_ENTER => {
                    println!("Mouse entered!");
                }
                KEY_DOWN => {
                    if event.ctrl && event.key_code == 83 {  // Ctrl+S
                        println!("Save shortcut pressed!");
                    }
                }
                _ => {}
            }
        }

        let bg = match ctx.state() {
            ButtonState::Idle => Color::BLUE,
            ButtonState::Hovered => Color::CYAN,
            ButtonState::Pressed => Color::DARK_BLUE,
            _ => Color::GRAY,
        };

        div().bg(bg)
    })
```

### EventContext Fields

| Field | Type | Description |
|-------|------|-------------|
| `event_type` | `u32` | Event type (POINTER_UP, POINTER_ENTER, etc.) |
| `node_id` | `LayoutNodeId` | The node that received the event |
| `mouse_x`, `mouse_y` | `f32` | Absolute mouse position |
| `local_x`, `local_y` | `f32` | Position relative to element bounds |
| `bounds_x`, `bounds_y` | `f32` | Element position (top-left corner) |
| `bounds_width`, `bounds_height` | `f32` | Element dimensions |
| `scroll_delta_x`, `scroll_delta_y` | `f32` | Scroll delta (for SCROLL events) |
| `drag_delta_x`, `drag_delta_y` | `f32` | Drag offset (for DRAG events) |
| `key_char` | `Option<char>` | Character (for TEXT_INPUT events) |
| `key_code` | `u32` | Key code (for KEY_DOWN/KEY_UP events) |
| `shift`, `ctrl`, `alt`, `meta` | `bool` | Modifier key states |

---

## Setting Initial State

Use `.initial()` to set the initial state:

```rust
stateful::<ButtonState>()
    .initial(if disabled { ButtonState::Disabled } else { ButtonState::Idle })
    .on_state(|ctx| {
        // ...
        div()
    })
```

---

## Signal Dependencies with `.deps()`

When a Stateful element needs to react to external signal changes (not just hover/press events), use `.deps()` to declare dependencies:

```rust
fn direction_toggle() -> impl ElementBuilder {
    // External state that affects the element's appearance
    let direction = use_state_keyed("direction", || Direction::Horizontal);

    stateful::<ButtonState>()
        .w(120.0)
        .h(40.0)
        .rounded(8.0)
        // Declare dependency - on_state re-runs when this signal changes
        .deps([direction.signal_id()])
        .on_state(move |ctx| {
            // Read the current direction value
            let dir = direction.get();
            let label = match dir {
                Direction::Horizontal => "Horizontal",
                Direction::Vertical => "Vertical",
            };

            let bg = match ctx.state() {
                ButtonState::Idle => Color::rgba(0.3, 0.5, 0.9, 1.0),
                ButtonState::Hovered => Color::rgba(0.4, 0.6, 1.0, 1.0),
                _ => Color::rgba(0.3, 0.5, 0.9, 1.0),
            };

            div()
                .bg(bg)
                .on_click(move |_| {
                    // Toggle direction
                    direction.update(|d| match d {
                        Direction::Horizontal => Direction::Vertical,
                        Direction::Vertical => Direction::Horizontal,
                    });
                })
                .child(text(label).color(Color::WHITE))
        })
}
```

### Accessing Dependencies via StateContext

You can access dependency values directly from the context using `ctx.dep()`:

```rust
let count_signal: State<i32> = use_state(|| 0);
let name_signal: State<String> = use_state(|| "".to_string());

stateful::<ButtonState>()
    .deps([count_signal.signal_id(), name_signal.signal_id()])
    .on_state(|ctx| {
        // Access by index (matches order in .deps())
        let count: i32 = ctx.dep(0).unwrap_or_default();
        let name: String = ctx.dep(1).unwrap_or_default();

        // Or get a full State<T> handle for reading and writing
        if let Some(count_state) = ctx.dep_as_state::<i32>(0) {
            let value = count_state.get();
            // count_state.set(value + 1);
        }

        div().child(text(&format!("{}: {}", name, count)))
    })
```

### When to Use `.deps()`

Use `.deps()` when your `on_state` callback reads values from signals that can change independently of the element's internal state transitions.

Without `.deps()`, the `on_state` callback only runs when:
- The element's state changes (Idle → Hovered, etc.)

With `.deps()`, it also runs when:
- Any of the declared signal dependencies change

---

## Scoped Signals

Use `ctx.use_signal()` for state that's scoped to the stateful container:

```rust
stateful::<ButtonState>()
    .on_state(|ctx| {
        // This signal is keyed to this specific stateful container
        // Format: "{stateful_key}:signal:click_count"
        let click_count = ctx.use_signal("click_count", || 0);

        div()
            .child(text(&format!("Clicks: {}", click_count.get())))
            .on_click(move |_| {
                click_count.update(|n| n + 1);
            })
    })
```

---

## Animated Values

### Declarative API (Recommended)

Use `ctx.use_spring()` for declarative spring animations - specify the target and get the current animated value:

```rust
stateful::<ButtonState>()
    .on_state(|ctx| {
        // Declarative: specify target, get current value
        let target_scale = match ctx.state() {
            ButtonState::Hovered => 1.1,
            _ => 1.0,
        };
        let current_scale = ctx.use_spring("scale", target_scale, SpringConfig::wobbly());

        // For default stiff spring, use ctx.spring()
        let opacity = ctx.spring("opacity", if ctx.state() == ButtonState::Idle { 0.8 } else { 1.0 });

        div()
            .transform(Transform::scale(current_scale, current_scale))
            .opacity(opacity)
    })
```

### Low-Level API

For more control, use `ctx.use_animated_value()` which returns a `SharedAnimatedValue`:

```rust
stateful::<ButtonState>()
    .on_state(|ctx| {
        // Get the animated value handle
        let scale = ctx.use_animated_value("scale", 1.0);

        // With custom spring config
        let opacity = ctx.use_animated_value_with_config(
            "opacity",
            1.0,
            SpringConfig::bouncy(),
        );

        // Manually set target and get value
        match ctx.state() {
            ButtonState::Hovered => {
                scale.lock().unwrap().set_target(1.1);
            }
            _ => {
                scale.lock().unwrap().set_target(1.0);
            }
        }

        let current_scale = scale.lock().unwrap().get();
        div().transform(Transform::scale(current_scale, current_scale))
    })
```

---

## Animated Timelines

Use `ctx.use_timeline()` for complex multi-property animations with keyframes:

```rust
stateful::<ButtonState>()
    .on_state(|ctx| {
        // Persisted timeline scoped to this stateful
        let timeline = ctx.use_timeline("pulse");

        // Configure on first use, get existing entry IDs on subsequent calls
        let opacity_id = timeline.lock().unwrap().configure(|t| {
            let id = t.add(0, 1000, 0.5, 1.0);  // 0ms offset, 1000ms duration
            t.set_loop(-1);  // Loop forever
            t.start();
            id
        });

        let opacity = timeline.lock().unwrap().get(opacity_id);
        div().opacity(opacity)
    })
```

The `configure()` method is idempotent - it only runs the configuration closure on the first call and returns existing entry IDs on subsequent calls.

---

## Built-in State Types

Blinc provides common state types with automatic transitions:

### ButtonState

```rust
ButtonState::Idle      // Default state
ButtonState::Hovered   // Mouse over element
ButtonState::Pressed   // Mouse button down
ButtonState::Disabled  // Non-interactive
```

Transitions:
- `Idle` → `Hovered` (on pointer enter)
- `Hovered` → `Idle` (on pointer leave)
- `Hovered` → `Pressed` (on pointer down)
- `Pressed` → `Hovered` (on pointer up)

### NoState

For containers that only need dependency tracking without state transitions:

```rust
stateful::<NoState>()
    .deps([some_signal.signal_id()])
    .on_state(|_ctx| {
        // Rebuilds when dependencies change
        div().child(text("Content"))
    })
```

---

## Custom State Types

Define your own state enum for complex interactions:

```rust
use blinc_layout::stateful::StateTransitions;
use blinc_core::events::event_types::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
enum DragState {
    #[default]
    Idle,
    Hovering,
    Dragging,
}

impl StateTransitions for DragState {
    fn on_event(&self, event: u32) -> Option<Self> {
        match (self, event) {
            (DragState::Idle, POINTER_ENTER) => Some(DragState::Hovering),
            (DragState::Hovering, POINTER_LEAVE) => Some(DragState::Idle),
            (DragState::Hovering, POINTER_DOWN) => Some(DragState::Dragging),
            (DragState::Dragging, POINTER_UP) => Some(DragState::Idle),
            _ => None,
        }
    }
}

fn draggable_item() -> impl ElementBuilder {
    stateful::<DragState>()
        .w(100.0)
        .h(100.0)
        .rounded(8.0)
        .on_state(|ctx| {
            let bg = match ctx.state() {
                DragState::Idle => Color::BLUE,
                DragState::Hovering => Color::CYAN,
                DragState::Dragging => Color::GREEN,
            };
            div().bg(bg)
        })
}
```

---

## Keyed State (Global Signals)

For state persisted across UI rebuilds with a string key:

```rust
let is_expanded = use_state_keyed("sidebar_expanded", || false);

// Read
let expanded = is_expanded.get();

// Update
is_expanded.set(true);
is_expanded.update(|v| !v);

// Get signal ID for use with .deps()
let signal_id = is_expanded.signal_id();
```

---

## Best Practices

1. **Use `stateful::<S>()` builder** - This is the primary pattern for stateful UI elements.

2. **Return Div from callbacks** - The new API expects you to return a Div, not mutate a container.

3. **Use `.initial()` for non-default states** - Set initial state explicitly when needed.

4. **Use `ctx.use_signal()` for local state** - Scoped signals are automatically keyed.

5. **Use `ctx.dep()` for dependency access** - Cleaner than capturing signals in closures.

6. **Prefer built-in state types** - They have correct transitions already defined.

7. **Custom states for complex flows** - Define your own when built-in types don't fit.

8. **Use `.deps()` for external dependencies** - When `on_state` needs to react to signal changes.
