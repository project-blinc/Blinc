# Custom State Machines

For complex interactions beyond hover/press, define custom state types with the `StateTransitions` trait.

## Defining Custom States

```rust
use blinc::layout::stateful::StateTransitions;
use blinc::core::events::event_types::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
enum PlayerState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

impl StateTransitions for PlayerState {
    fn on_event(&self, event: u32) -> Option<Self> {
        match (self, event) {
            // Click cycles through states
            (PlayerState::Stopped, POINTER_UP) => Some(PlayerState::Playing),
            (PlayerState::Playing, POINTER_UP) => Some(PlayerState::Paused),
            (PlayerState::Paused, POINTER_UP) => Some(PlayerState::Playing),
            _ => None,
        }
    }
}
```

## Using Custom States

```rust
use blinc::layout::prelude::*;

fn player_button() -> impl ElementBuilder {
    stateful::<PlayerState>()
        .w(60.0)
        .h(60.0)
        .rounded_full()
        .flex_center()
        .on_state(|ctx| {
            let bg = match ctx.state() {
                PlayerState::Stopped => Color::rgba(0.3, 0.3, 0.35, 1.0),
                PlayerState::Playing => Color::rgba(0.2, 0.8, 0.4, 1.0),
                PlayerState::Paused => Color::rgba(0.9, 0.6, 0.2, 1.0),
            };
            div().bg(bg).child(text("▶").color(Color::WHITE))
        })
}
```

## Event Types

Available event types for state transitions:

```rust
use blinc::core::events::event_types::*;

POINTER_ENTER    // Mouse enters element
POINTER_LEAVE    // Mouse leaves element
POINTER_DOWN     // Mouse button pressed
POINTER_UP       // Mouse button released (click)
POINTER_MOVE     // Mouse moved over element

KEY_DOWN         // Keyboard key pressed
KEY_UP           // Keyboard key released
TEXT_INPUT       // Character typed

FOCUS            // Element gained focus
BLUR             // Element lost focus

SCROLL           // Scroll event
DRAG             // Drag motion
DRAG_END         // Drag completed
```

## Multi-Phase Interactions

### Drag State Machine

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
enum DragPhase {
    #[default]
    Idle,
    Hovering,
    Pressing,
    Dragging,
}

impl StateTransitions for DragPhase {
    fn on_event(&self, event: u32) -> Option<Self> {
        match (self, event) {
            // Enter hover
            (DragPhase::Idle, POINTER_ENTER) => Some(DragPhase::Hovering),
            (DragPhase::Hovering, POINTER_LEAVE) => Some(DragPhase::Idle),

            // Start press
            (DragPhase::Hovering, POINTER_DOWN) => Some(DragPhase::Pressing),

            // Transition to drag on move while pressed
            (DragPhase::Pressing, DRAG) => Some(DragPhase::Dragging),

            // Release
            (DragPhase::Pressing, POINTER_UP) => Some(DragPhase::Hovering),
            (DragPhase::Dragging, DRAG_END) => Some(DragPhase::Idle),

            _ => None,
        }
    }
}

fn draggable_card() -> impl ElementBuilder {
    stateful::<DragPhase>()
        .w(120.0)
        .h(80.0)
        .rounded(8.0)
        .on_state(|ctx| {
            let (bg, cursor) = match ctx.state() {
                DragPhase::Idle => (Color::BLUE, "default"),
                DragPhase::Hovering => (Color::CYAN, "grab"),
                DragPhase::Pressing => (Color::YELLOW, "grabbing"),
                DragPhase::Dragging => (Color::GREEN, "grabbing"),
            };
            div().bg(bg).cursor(cursor)
        })
}
```

### Focus State Machine

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
enum InputFocus {
    #[default]
    Idle,
    Hovered,
    Focused,
    FocusedHovered,
}

impl StateTransitions for InputFocus {
    fn on_event(&self, event: u32) -> Option<Self> {
        match (self, event) {
            // Hover transitions
            (InputFocus::Idle, POINTER_ENTER) => Some(InputFocus::Hovered),
            (InputFocus::Hovered, POINTER_LEAVE) => Some(InputFocus::Idle),
            (InputFocus::Focused, POINTER_ENTER) => Some(InputFocus::FocusedHovered),
            (InputFocus::FocusedHovered, POINTER_LEAVE) => Some(InputFocus::Focused),

            // Focus transitions
            (InputFocus::Idle, FOCUS) => Some(InputFocus::Focused),
            (InputFocus::Hovered, FOCUS) => Some(InputFocus::FocusedHovered),
            (InputFocus::Hovered, POINTER_UP) => Some(InputFocus::FocusedHovered),
            (InputFocus::Focused, BLUR) => Some(InputFocus::Idle),
            (InputFocus::FocusedHovered, BLUR) => Some(InputFocus::Hovered),

            _ => None,
        }
    }
}

fn focusable_input() -> impl ElementBuilder {
    stateful::<InputFocus>()
        .w(200.0)
        .h(40.0)
        .rounded(4.0)
        .on_state(|ctx| {
            let (border_color, border_width) = match ctx.state() {
                InputFocus::Idle => (Color::GRAY, 1.0),
                InputFocus::Hovered => (Color::LIGHT_GRAY, 1.0),
                InputFocus::Focused => (Color::BLUE, 2.0),
                InputFocus::FocusedHovered => (Color::BLUE, 2.0),
            };
            div().border(border_width, border_color)
        })
}
```

## Combining with External State

Use `.deps()` to combine state machine transitions with external signals:

```rust
fn smart_button() -> impl ElementBuilder {
    let enabled = use_state_keyed("enabled", || true);

    stateful::<ButtonState>()
        .px(16.0)
        .py(8.0)
        .rounded(8.0)
        .deps([enabled.signal_id()])
        .on_state(move |ctx| {
            let is_enabled = enabled.get();

            let bg = if !is_enabled {
                Color::rgba(0.2, 0.2, 0.25, 0.5)  // Disabled
            } else {
                match ctx.state() {
                    ButtonState::Idle => Color::rgba(0.3, 0.5, 0.9, 1.0),
                    ButtonState::Hovered => Color::rgba(0.4, 0.6, 1.0, 1.0),
                    ButtonState::Pressed => Color::rgba(0.2, 0.4, 0.8, 1.0),
                    _ => Color::rgba(0.3, 0.5, 0.9, 1.0),
                }
            };

            div().bg(bg).child(text("Submit").color(Color::WHITE))
        })
}
```

## Accessing Dependencies via Context

Use `ctx.dep()` for cleaner dependency access:

```rust
fn counter_button(count: State<i32>) -> impl ElementBuilder {
    stateful::<ButtonState>()
        .deps([count.signal_id()])
        .on_state(|ctx| {
            // Access by index - no need to capture in closure
            let value: i32 = ctx.dep(0).unwrap_or_default();

            // Or get a State handle for reading/writing
            if let Some(count_state) = ctx.dep_as_state::<i32>(0) {
                // count_state.set(value + 1);
            }

            let bg = match ctx.state() {
                ButtonState::Hovered => Color::CYAN,
                _ => Color::BLUE,
            };

            div()
                .bg(bg)
                .child(text(&format!("Count: {}", value)))
        })
}
```

## Using Scoped State

StateContext provides scoped signals and animated values:

```rust
fn interactive_counter() -> impl ElementBuilder {
    stateful::<ButtonState>()
        .on_state(|ctx| {
            // Scoped signal - persists across rebuilds
            let clicks = ctx.use_signal("clicks", || 0);

            // Scoped animated value with spring physics
            let scale = ctx.use_animated_value("scale", 1.0);

            // Animate based on state
            match ctx.state() {
                ButtonState::Pressed => {
                    scale.lock().unwrap().set_target(0.95);
                }
                _ => {
                    scale.lock().unwrap().set_target(1.0);
                }
            }

            let s = scale.lock().unwrap().get();

            div()
                .transform(Transform::scale(s, s))
                .child(text(&format!("Clicks: {}", clicks.get())))
                .on_click(move |_| {
                    clicks.update(|n| n + 1);
                })
        })
}
```

## State Debugging

Log state transitions for debugging:

```rust
impl StateTransitions for MyState {
    fn on_event(&self, event: u32) -> Option<Self> {
        let next = match (self, event) {
            // ... transitions ...
            _ => None,
        };

        if let Some(ref new_state) = next {
            println!("State: {:?} -> {:?} (event: {})", self, new_state, event);
        }

        next
    }
}
```

## Setting Initial State

Use `.initial()` when you need a non-default starting state:

```rust
fn initially_disabled_button(disabled: bool) -> impl ElementBuilder {
    stateful::<ButtonState>()
        .initial(if disabled { ButtonState::Disabled } else { ButtonState::Idle })
        .on_state(|ctx| {
            let bg = match ctx.state() {
                ButtonState::Disabled => Color::GRAY,
                ButtonState::Idle => Color::BLUE,
                ButtonState::Hovered => Color::CYAN,
                ButtonState::Pressed => Color::DARK_BLUE,
            };
            div().bg(bg)
        })
}
```

## NoState for Dependency-Only Containers

When you only need dependency tracking without state transitions:

```rust
fn data_display(data: State<Vec<String>>) -> impl ElementBuilder {
    stateful::<NoState>()
        .deps([data.signal_id()])
        .on_state(|ctx| {
            // Access data via context
            let items: Vec<String> = ctx.dep(0).unwrap_or_default();

            div()
                .flex_col()
                .gap(4.0)
                .children(items.iter().map(|item| {
                    div().child(text(item))
                }))
        })
}
```

## Best Practices

1. **Keep states minimal** - Only include states you need to distinguish visually.

2. **Handle all paths** - Consider every possible event in each state.

3. **Use descriptive names** - State names should clearly indicate the UI appearance.

4. **Return None for no-ops** - If an event doesn't cause a transition, return `None`.

5. **Test transitions** - Verify all state paths work as expected.

6. **Use `.deps()` for external dependencies** - When combining with signals.

7. **Use `ctx.dep()` over closures** - Cleaner access to dependency values.

8. **Implement Default** - Mark the default state with `#[default]` attribute.

9. **Use scoped signals** - `ctx.use_signal()` for state local to the stateful.

10. **Use animated values** - `ctx.use_animated_value()` for smooth transitions.
