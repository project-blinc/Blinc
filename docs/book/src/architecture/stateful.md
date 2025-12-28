# Stateful Elements & FSM

Blinc uses **Finite State Machines (FSM)** to manage interactive UI state. This provides predictable state transitions for widgets like buttons, checkboxes, and text fields.

## Finite State Machines

### Core Concepts

An FSM defines:
- **States**: Discrete conditions the element can be in
- **Events**: Inputs that trigger transitions
- **Transitions**: Rules mapping (state, event) -> new_state

```rust
// State IDs and Event IDs are u32
type StateId = u32;
type EventId = u32;

struct Transition {
    from_state: StateId,
    event: EventId,
    to_state: StateId,
    guard: Option<Box<dyn Fn() -> bool>>,  // Conditional transition
    action: Option<Box<dyn Fn()>>,          // Side effect
}
```

### FSM Builder

```rust
let fsm = StateMachine::builder(initial_state)
    .on(State::Idle, Event::PointerEnter, State::Hovered)
    .on(State::Hovered, Event::PointerLeave, State::Idle)
    .on(State::Hovered, Event::PointerDown, State::Pressed)
    .on(State::Pressed, Event::PointerUp, State::Hovered)
    .on_enter(State::Pressed, || {
        println!("Button pressed!");
    })
    .build();
```

### Entry/Exit Callbacks

```rust
.on_enter(state, || { /* called when entering state */ })
.on_exit(state, || { /* called when leaving state */ })
```

### Guard Conditions

Transitions can be conditional:

```rust
.transition(
    Transition::new(State::Idle, Event::Click, State::Active)
        .with_guard(|| is_enabled())
)
```

---

## StateTransitions Trait

For type-safe state definitions, implement `StateTransitions`:

```rust
use blinc_layout::stateful::StateTransitions;
use blinc_core::events::event_types::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ButtonState {
    Idle,
    Hovered,
    Pressed,
    Disabled,
}

impl StateTransitions for ButtonState {
    fn on_event(&self, event: u32) -> Option<Self> {
        match (self, event) {
            (ButtonState::Idle, POINTER_ENTER) => Some(ButtonState::Hovered),
            (ButtonState::Hovered, POINTER_LEAVE) => Some(ButtonState::Idle),
            (ButtonState::Hovered, POINTER_DOWN) => Some(ButtonState::Pressed),
            (ButtonState::Pressed, POINTER_UP) => Some(ButtonState::Hovered),
            _ => None,
        }
    }
}
```

### Available Event Types

```rust
// Pointer events
POINTER_ENTER    // Mouse enters element
POINTER_LEAVE    // Mouse leaves element
POINTER_DOWN     // Mouse button pressed
POINTER_UP       // Mouse button released
POINTER_MOVE     // Mouse moved over element

// Keyboard events
KEY_DOWN         // Key pressed
KEY_UP           // Key released
TEXT_INPUT       // Character typed

// Focus events
FOCUS            // Element gained focus
BLUR             // Element lost focus

// Other
SCROLL           // Scroll event
DRAG             // Drag motion
DRAG_END         // Drag completed
```

---

## Stateful Elements

### Creating Stateful Elements

```rust
use blinc_layout::stateful::stateful;

fn interactive_card(ctx: &WindowedContext) -> impl ElementBuilder {
    let handle = ctx.use_state(ButtonState::Idle);

    stateful(handle)
        .w(200.0)
        .h(120.0)
        .rounded(12.0)
        .on_state(|state, div| {
            let bg = match state {
                ButtonState::Idle => Color::rgba(0.15, 0.15, 0.2, 1.0),
                ButtonState::Hovered => Color::rgba(0.18, 0.18, 0.25, 1.0),
                ButtonState::Pressed => Color::rgba(0.12, 0.12, 0.16, 1.0),
                ButtonState::Disabled => Color::rgba(0.1, 0.1, 0.12, 0.5),
            };
            div.set_bg(bg);
        })
        .child(text("Hover me").color(Color::WHITE))
}
```

### How It Works

1. **Handle creation**: `ctx.use_state()` creates a persistent state handle
2. **Element creation**: `stateful(handle)` creates a Stateful wrapper
3. **Event routing**: Pointer/keyboard events are routed to the FSM
4. **State transition**: FSM computes new state from (current_state, event)
5. **Callback invocation**: `on_state` callback runs with new state
6. **Visual update**: Callback updates the element's appearance

### Keyed State for Reusable Components

When using stateful elements in loops or reusable components:

```rust
fn list_item(ctx: &WindowedContext, id: &str) -> impl ElementBuilder {
    // Use id as key to avoid state collisions
    let handle = ctx.use_state_for(id, ButtonState::Idle);

    stateful(handle)
        .on_state(|state, div| { /* ... */ })
        .child(text(id).color(Color::WHITE))
}
```

---

## Built-in State Types

### ButtonState

```rust
enum ButtonState {
    Idle,      // Default
    Hovered,   // Mouse over
    Pressed,   // Mouse down
    Disabled,  // Non-interactive
}
```

Transitions:
- Idle → Hovered (pointer enter)
- Hovered → Idle (pointer leave)
- Hovered → Pressed (pointer down)
- Pressed → Hovered (pointer up)

### ToggleState

```rust
enum ToggleState {
    Off,
    On,
}
```

Transitions:
- Off → On (click)
- On → Off (click)

### CheckboxState

```rust
enum CheckboxState {
    UncheckedIdle,
    UncheckedHovered,
    CheckedIdle,
    CheckedHovered,
}
```

### TextFieldState

```rust
enum TextFieldState {
    Idle,
    Hovered,
    Focused,
    FocusedHovered,
    Disabled,
}
```

### ScrollState

```rust
enum ScrollState {
    Idle,
    Scrolling,
    Decelerating,
    Bouncing,
}
```

---

## Signal Dependencies

Stateful elements can depend on external signals using `.deps()`:

```rust
fn counter_display(ctx: &WindowedContext, count: State<i32>) -> impl ElementBuilder {
    let handle = ctx.use_state(ButtonState::Idle);

    stateful(handle)
        .deps(&[count.signal_id()])  // Re-run on_state when count changes
        .on_state(move |_state, container| {
            let value = count.get();
            container.merge(
                div().child(
                    text(&format!("Count: {}", value)).color(Color::WHITE)
                )
            );
        })
}
```

### When to Use `.deps()`

Use `.deps()` when your `on_state` callback reads from signals:

| Without `.deps()` | With `.deps()` |
|-------------------|----------------|
| Only runs on state transitions | Also runs when dependencies change |
| Hover/press only | External data + hover/press |

---

## Updating in on_state

### Pattern 1: Direct Setters (Recommended)

```rust
.on_state(|state, div| {
    let bg = match state {
        ButtonState::Idle => Color::BLUE,
        ButtonState::Hovered => Color::CYAN,
        _ => Color::BLUE,
    };
    div.set_bg(bg);
    div.set_transform(Transform::scale(1.0, 1.0));
})
```

### Pattern 2: Merge with Children

```rust
.on_state(move |state, container| {
    let label = match state {
        ToggleState::Off => "Off",
        ToggleState::On => "On",
    };
    container.merge(
        div()
            .bg(color_for_state(state))
            .child(text(label).color(Color::WHITE))
    );
})
```

---

## Custom State Machines

For complex interactions, define your own states:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum DragState {
    Idle,
    Hovering,
    Pressing,
    Dragging,
}

impl StateTransitions for DragState {
    fn on_event(&self, event: u32) -> Option<Self> {
        match (self, event) {
            (DragState::Idle, POINTER_ENTER) => Some(DragState::Hovering),
            (DragState::Hovering, POINTER_LEAVE) => Some(DragState::Idle),
            (DragState::Hovering, POINTER_DOWN) => Some(DragState::Pressing),
            (DragState::Pressing, DRAG) => Some(DragState::Dragging),
            (DragState::Pressing, POINTER_UP) => Some(DragState::Hovering),
            (DragState::Dragging, DRAG_END) => Some(DragState::Idle),
            _ => None,
        }
    }
}
```

---

## Event Routing

### Event Flow

```
Platform Event (pointer, keyboard)
    │
    ├── Hit test: which element?
    │
    ├── EventRouter dispatches to element
    │
    ├── StateMachine receives event
    │   └── Computes transition
    │
    └── on_state callback invoked
```

### Event Context

Handlers receive event details:

```rust
.on_click(|ctx| {
    println!("Clicked at ({}, {})", ctx.local_x, ctx.local_y);
})
.on_key_down(|ctx| {
    if ctx.ctrl && ctx.key_code == 83 {  // Ctrl+S
        save();
    }
})
```

---

## Performance

### Why FSM Over Signals?

| Signals for visual state | FSM for visual state |
|--------------------------|----------------------|
| Triggers full rebuild | Updates only affected element |
| Creates new VDOM | Mutates existing element |
| O(tree size) | O(1) |

### Minimal Updates

Stateful elements only update their own RenderProps:

```rust
// State change only affects this element
div.set_bg(new_color);  // Updates RenderProps
// No layout recomputation
// No tree diff
// Just visual update
```

### Queued Updates

State changes queue updates efficiently:

```rust
static PENDING_PROP_UPDATES: Vec<(NodeId, RenderProps)>;

// Stateful callback queues update
fn on_state(state, div) {
    div.set_bg(color);
    // Queues: (node_id, updated_props)
}

// Processed in batch by windowed app
for (node_id, props) in drain_pending() {
    render_tree.update_props(node_id, props);
}
```
