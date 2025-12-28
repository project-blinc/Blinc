# State Management

Blinc uses **Stateful elements** as the primary way to manage UI state. Stateful elements handle state transitions automatically without rebuilding the entire UI tree.

## Stateful Elements

`Stateful` is a wrapper element that manages visual states (hover, press, focus, etc.) efficiently. When state changes, only the affected element updates - not the entire UI.

### Basic Usage

```rust
use blinc_layout::stateful::stateful;

fn feature_card(ctx: &WindowedContext, label: &str, accent: Color) -> impl ElementBuilder {
    // Create persistent state handle (keyed by label for reusable components)
    let handle = ctx.use_state_for(label, ButtonState::Idle);

    stateful(handle)
        .w_fit()
        .p(4.0)
        .rounded(14.0)
        .on_state(move |state, div| match state {
            ButtonState::Idle => {
                div.set_bg(accent);
                div.set_rounded(14.0);
            }
            ButtonState::Hovered => {
                let hover_color = Color::rgba(
                    (accent.r * 1.15).min(1.0),
                    (accent.g * 1.15).min(1.0),
                    (accent.b * 1.15).min(1.0),
                    accent.a,
                );
                div.set_bg(hover_color);
                div.set_transform(Transform::scale(1.05, 1.05));
            }
            ButtonState::Pressed => {
                div.set_bg(Color::rgba(accent.r * 0.85, accent.g * 0.85, accent.b * 0.85, accent.a));
                div.set_transform(Transform::scale(0.95, 0.95));
            }
            ButtonState::Disabled => {
                div.set_bg(Color::GRAY);
            }
        })
        .on_click(move |_| println!("'{}' clicked!", label))
        .child(text(label).color(Color::WHITE))
}
```

### How It Works

1. `ctx.use_state(InitialState)` creates a persistent state handle
2. `stateful(handle)` creates a Stateful element from the handle
3. Events (hover, click, etc.) trigger automatic state transitions
4. `on_state` callback is called when state changes
5. Use `div.set_*()` methods or `div.swap()` to update the element

---

## State Handle Functions

Blinc provides two ways to create state handles:

### `use_state()` - Auto-keyed

For unique call sites (not in loops or reusable components):

```rust
fn image_showcase(ctx: &WindowedContext) -> impl ElementBuilder {
    // Auto-keyed by source location - works for unique call sites
    let handle = ctx.use_state(ButtonState::Idle);

    stateful(handle)
        .on_state(|state, div| {
            match state {
                ButtonState::Idle | ButtonState::Disabled => {
                    div.set_shadow(Shadow::new(0.0, 4.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.2)));
                }
                ButtonState::Hovered | ButtonState::Pressed => {
                    div.set_shadow(Shadow::new(0.0, 12.0, 24.0, Color::rgba(0.4, 0.6, 1.0, 0.5)));
                    div.set_transform(Transform::scale(1.03, 1.03));
                }
            }
        })
        .child(img("path/to/image.webp").w(200.0).h(150.0))
}
```

### `use_state_for()` - Explicit key

For reusable components or loops where you need unique keys:

```rust
fn item_list(ctx: &WindowedContext, items: &[String]) -> impl ElementBuilder {
    div()
        .flex_col()
        .gap(4.0)
        .child(
            items.iter().map(|item| {
                // Clone item for the key (use_state_for takes ownership)
                let handle = ctx.use_state_for(item.clone(), ButtonState::Idle);

                stateful(handle)
                    .p(12.0)
                    .rounded(8.0)
                    .on_state(|state, div| {
                        let bg = match state {
                            ButtonState::Idle => Color::rgba(0.15, 0.15, 0.2, 1.0),
                            ButtonState::Hovered => Color::rgba(0.2, 0.2, 0.28, 1.0),
                            ButtonState::Pressed => Color::rgba(0.3, 0.5, 0.9, 1.0),
                            _ => Color::rgba(0.15, 0.15, 0.2, 1.0),
                        };
                        div.set_bg(bg);
                    })
                    .child(text(item).color(Color::WHITE))
            })
        )
}
```

---

## Updating State in `on_state`

There are two patterns for updating the div in the `on_state` callback:

### Pattern 1: Direct setters (Recommended)

Use `div.set_*()` methods for individual property updates:

```rust
.on_state(|state, div| {
    match state {
        ButtonState::Idle => {
            div.set_bg(Color::BLUE);
            div.set_rounded(8.0);
            div.set_shadow(Shadow::new(0.0, 2.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.2)));
        }
        ButtonState::Hovered => {
            div.set_bg(Color::CYAN);
            div.set_rounded(12.0);
            div.set_transform(Transform::scale(1.05, 1.05));
        }
        // ...
    }
})
```

### Pattern 2: Merge

Use `container.merge()` to apply partial updates with children:

```rust
.on_state(move |state, container| {
    let bg = match state {
        ButtonState::Idle => Color::BLUE,
        ButtonState::Hovered => Color::CYAN,
        _ => Color::BLUE,
    };
    // merge() can also add/update children
    container.merge(
        div()
            .bg(bg)
            .child(text(&format!("Count: {}", count.get())).color(Color::WHITE))
    );
})
```

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

### ToggleState

```rust
ToggleState::Off   // Toggle is off
ToggleState::On    // Toggle is on
```

Transitions:
- `Off` → `On` (on click)
- `On` → `Off` (on click)

### CheckboxState

```rust
CheckboxState::UncheckedIdle
CheckboxState::UncheckedHovered
CheckboxState::CheckedIdle
CheckboxState::CheckedHovered
```

### TextFieldState

```rust
TextFieldState::Idle
TextFieldState::Hovered
TextFieldState::Focused
TextFieldState::FocusedHovered
TextFieldState::Disabled
```

### ScrollState

```rust
ScrollState::Idle
ScrollState::Scrolling
ScrollState::Decelerating
ScrollState::Bouncing
```

---

## Shorthand Constructors

For simple cases without persistent state:

```rust
use blinc_layout::stateful::{stateful_button, toggle, stateful_checkbox};

// Creates Stateful<ButtonState> starting at Idle
stateful_button()
    .on_state(|state, div| { /* ... */ })

// Creates Stateful<ToggleState>
toggle(false)  // Start in Off state
    .on_state(|state, div| { /* ... */ })

// Creates Stateful<CheckboxState>
stateful_checkbox(false)  // Start unchecked
    .on_state(|state, div| { /* ... */ })
```

---

## Custom State Types

Define your own state enum for complex interactions:

```rust
use blinc_layout::stateful::{Stateful, StateTransitions};
use blinc_core::events::event_types::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum DragState {
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

fn draggable_item(ctx: &WindowedContext) -> impl ElementBuilder {
    let handle = ctx.use_state(DragState::Idle);

    stateful(handle)
        .w(100.0)
        .h(100.0)
        .rounded(8.0)
        .on_state(|state, div| {
            let bg = match state {
                DragState::Idle => Color::BLUE,
                DragState::Hovering => Color::CYAN,
                DragState::Dragging => Color::GREEN,
            };
            div.set_bg(bg);
        })
}
```

---

## Signal Dependencies with `.deps()`

When a Stateful element needs to react to external signal changes (not just hover/press events), use `.deps()` to declare dependencies:

```rust
fn direction_toggle(ctx: &WindowedContext) -> impl ElementBuilder {
    // External state that affects the element's appearance
    let direction = ctx.use_state_keyed("direction", || Direction::Horizontal);
    let button_handle = ctx.use_state(ButtonState::Idle);

    stateful(button_handle)
        .w(120.0)
        .h(40.0)
        .rounded(8.0)
        // Declare dependency - on_state re-runs when this signal changes
        .deps(&[direction.signal_id()])
        .on_state(move |state, div| {
            // Read the current direction value
            let dir = direction.get();
            let label = match dir {
                Direction::Horizontal => "Horizontal",
                Direction::Vertical => "Vertical",
            };

            let bg = match state {
                ButtonState::Idle => Color::rgba(0.3, 0.5, 0.9, 1.0),
                ButtonState::Hovered => Color::rgba(0.4, 0.6, 1.0, 1.0),
                _ => Color::rgba(0.3, 0.5, 0.9, 1.0),
            };

            div.set_bg(bg);
        })
        .on_click(move |_| {
            // Toggle direction
            direction.update(|d| match d {
                Direction::Horizontal => Direction::Vertical,
                Direction::Vertical => Direction::Horizontal,
            });
        })
        .child(text("Toggle Direction").color(Color::WHITE))
}
```

### When to Use `.deps()`

Use `.deps()` when your `on_state` callback reads values from signals or keyed state that can change independently of the element's internal state transitions.

Without `.deps()`, the `on_state` callback only runs when:
- The element's state changes (Idle → Hovered, etc.)

With `.deps()`, it also runs when:
- Any of the declared signal dependencies change

---

## Keyed State

For state persisted across UI rebuilds with a string key:

```rust
let is_expanded = ctx.use_state_keyed("sidebar_expanded", || false);

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

1. **Use `stateful(handle)` with `use_state()`** - This is the primary pattern for stateful UI elements.

2. **Use `use_state_for(key, ...)` in loops** - When creating multiple stateful elements in a loop, use explicit keys.

3. **Use `div.set_*()` or `div.swap()`** - Both patterns work; choose based on your preference.

4. **Keep state close to usage** - Define state handles in the component that needs them.

5. **Prefer built-in state types** - They have correct transitions already defined.

6. **Custom states for complex flows** - Define your own when built-in types don't fit.

7. **Use `.deps()` for external dependencies** - When `on_state` reads from other signals.
