# Custom State Machines

For complex interactions beyond hover/press, define custom state types with the `StateTransitions` trait.

## Defining Custom States

```rust
use blinc_layout::stateful::{stateful, StateTransitions};
use blinc_core::events::event_types::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum PlayerState {
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
fn player_button(ctx: &WindowedContext) -> impl ElementBuilder {
    let handle = ctx.use_state(PlayerState::Stopped);

    stateful(handle)
        .w(60.0)
        .h(60.0)
        .rounded_full()
        .flex_center()
        .on_state(|state, div| {
            let bg = match state {
                PlayerState::Stopped => Color::rgba(0.3, 0.3, 0.35, 1.0),
                PlayerState::Playing => Color::rgba(0.2, 0.8, 0.4, 1.0),
                PlayerState::Paused => Color::rgba(0.9, 0.6, 0.2, 1.0),
            };
            div.set_bg(bg);
        })
        .child(text("▶").color(Color::WHITE))
}
```

## Event Types

Available event types for state transitions:

```rust
use blinc_core::events::event_types::*;

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
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum DragPhase {
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
```

### Focus State Machine

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum InputFocus {
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
```

## Combining with External State

Use `.deps()` to combine state machine transitions with external signals:

```rust
fn smart_button(ctx: &WindowedContext) -> impl ElementBuilder {
    let enabled = ctx.use_state_keyed("enabled", || true);
    let handle = ctx.use_state(ButtonState::Idle);

    stateful(handle)
        .px(16.0)
        .py(8.0)
        .rounded(8.0)
        .deps(&[enabled.signal_id()])
        .on_state(move |state, div| {
            let is_enabled = enabled.get();

            let bg = if !is_enabled {
                Color::rgba(0.2, 0.2, 0.25, 0.5)  // Disabled
            } else {
                match state {
                    ButtonState::Idle => Color::rgba(0.3, 0.5, 0.9, 1.0),
                    ButtonState::Hovered => Color::rgba(0.4, 0.6, 1.0, 1.0),
                    ButtonState::Pressed => Color::rgba(0.2, 0.4, 0.8, 1.0),
                    _ => Color::rgba(0.3, 0.5, 0.9, 1.0),
                }
            };

            div.set_bg(bg);
        })
        .child(text("Submit").color(Color::WHITE))
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

## Best Practices

1. **Keep states minimal** - Only include states you need to distinguish visually.

2. **Handle all paths** - Consider every possible event in each state.

3. **Use descriptive names** - State names should clearly indicate the UI appearance.

4. **Return None for no-ops** - If an event doesn't cause a transition, return `None`.

5. **Test transitions** - Verify all state paths work as expected.

6. **Combine with .deps()** - For states that depend on external signals.
