# Event Handling

Blinc provides event handling through closures attached to elements. Events bubble up from child to parent elements.

## Available Events

### Pointer Events

```rust
div()
    .on_click(|ctx| {
        println!("Clicked at ({}, {})", ctx.local_x, ctx.local_y);
    })
    .on_mouse_down(|ctx| {
        println!("Mouse button pressed");
    })
    .on_mouse_up(|ctx| {
        println!("Mouse button released");
    })
```

### Hover Events

```rust
div()
    .on_hover_enter(|ctx| {
        println!("Mouse entered element");
    })
    .on_hover_leave(|ctx| {
        println!("Mouse left element");
    })
```

### Focus Events

```rust
div()
    .on_focus(|ctx| {
        println!("Element focused");
    })
    .on_blur(|ctx| {
        println!("Element lost focus");
    })
```

### Keyboard Events

```rust
div()
    .on_key_down(|ctx| {
        println!("Key pressed: code={}", ctx.key_code);
        if ctx.ctrl && ctx.key_code == 83 {  // Ctrl+S
            println!("Save shortcut triggered!");
        }
    })
    .on_key_up(|ctx| {
        println!("Key released");
    })
    .on_text_input(|ctx| {
        if let Some(ch) = ctx.key_char {
            println!("Character typed: {}", ch);
        }
    })
```

### Scroll Events

```rust
div()
    .on_scroll(|ctx| {
        println!("Scrolled: dx={}, dy={}", ctx.scroll_delta_x, ctx.scroll_delta_y);
    })
```

### Drag Events

```rust
div()
    .on_drag(|ctx| {
        println!("Dragging: delta=({}, {})", ctx.drag_delta_x, ctx.drag_delta_y);
    })
    .on_drag_end(|ctx| {
        println!("Drag ended");
    })
```

### Lifecycle Events

```rust
div()
    .on_mount(|ctx| {
        println!("Element added to tree");
    })
    .on_unmount(|ctx| {
        println!("Element removed from tree");
    })
    .on_resize(|ctx| {
        println!("Element resized");
    })
```

---

## EventContext

All event handlers receive an `EventContext` with information about the event:

```rust
pub struct EventContext {
    pub event_type: EventType,       // Type of event
    pub node_id: LayoutNodeId,       // Element that received the event

    // Mouse position (global coordinates)
    pub mouse_x: f32,
    pub mouse_y: f32,

    // Mouse position (relative to element)
    pub local_x: f32,
    pub local_y: f32,

    // Scroll deltas (for SCROLL events)
    pub scroll_delta_x: f32,
    pub scroll_delta_y: f32,

    // Drag deltas (for DRAG events)
    pub drag_delta_x: f32,
    pub drag_delta_y: f32,

    // Keyboard (for KEY_DOWN, KEY_UP, TEXT_INPUT)
    pub key_char: Option<char>,      // Character for TEXT_INPUT
    pub key_code: u32,               // Virtual key code

    // Modifier keys
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,                  // Cmd on macOS, Win on Windows
}
```

---

## Event Patterns

### Toggle on Click

Use `ToggleState` for toggle buttons - it handles click transitions automatically:

```rust
use blinc_layout::stateful::stateful;

fn toggle_button(ctx: &WindowedContext) -> impl ElementBuilder {
    let handle = ctx.use_state(ToggleState::Off);

    stateful(handle)
        .w(100.0)
        .h(40.0)
        .rounded(8.0)
        .flex_center()
        .on_state(|state, div| {
            let bg = match state {
                ToggleState::Off => Color::rgba(0.3, 0.3, 0.35, 1.0),
                ToggleState::On => Color::rgba(0.2, 0.8, 0.4, 1.0),
            };
            div.set_bg(bg);
        })
        .on_click(|_| {
            println!("Toggled!");
            // ToggleState transitions automatically on click
        })
        .child(text("Toggle").color(Color::WHITE))
}
```

### Drag to Move

```rust
fn draggable_box(ctx: &WindowedContext) -> impl ElementBuilder {
    let pos_x = ctx.use_signal(100.0f32);
    let pos_y = ctx.use_signal(100.0f32);

    let x = ctx.get(pos_x).unwrap_or(100.0);
    let y = ctx.get(pos_y).unwrap_or(100.0);
    let ctx_clone = ctx.clone();

    div()
        .absolute()
        .left(x)
        .top(y)
        .w(80.0)
        .h(80.0)
        .rounded(8.0)
        .bg(Color::rgba(0.4, 0.6, 1.0, 1.0))
        .on_drag(move |evt| {
            ctx_clone.update(pos_x, |v| v + evt.drag_delta_x);
            ctx_clone.update(pos_y, |v| v + evt.drag_delta_y);
        })
}
```

### Keyboard Shortcuts

```rust
fn keyboard_handler(ctx: &WindowedContext) -> impl ElementBuilder {
    let ctx_clone = ctx.clone();

    div()
        .w_full()
        .h_full()
        .on_key_down(move |evt| {
            // Ctrl+S or Cmd+S to save
            if (evt.ctrl || evt.meta) && evt.key_code == 83 {
                println!("Save triggered!");
            }
            // Escape to close
            if evt.key_code == 27 {
                println!("Escape pressed!");
            }
        })
}
```

### Hover Preview

```rust
use blinc_layout::stateful::stateful;

fn hover_card(ctx: &WindowedContext) -> impl ElementBuilder {
    let handle = ctx.use_state(ButtonState::Idle);

    stateful(handle)
        .w(200.0)
        .h(120.0)
        .rounded(12.0)
        .on_state(|state, div| {
            let (bg, scale) = match state {
                ButtonState::Hovered => (Color::rgba(0.2, 0.2, 0.3, 1.0), 1.02),
                _ => (Color::rgba(0.15, 0.15, 0.2, 1.0), 1.0),
            };
            div.set_bg(bg);
            div.set_transform(Transform::scale(scale, scale));
        })
        .child(text("Hover me!").color(Color::WHITE))
}
```

---

## Capturing State in Closures

Event handlers are `Fn` closures, so you need to clone any values you want to use inside:

```rust
fn counter_buttons(ctx: &WindowedContext) -> impl ElementBuilder {
    let count = ctx.use_signal(0i32);

    // Clone what you need for the closures
    let ctx_inc = ctx.clone();
    let ctx_dec = ctx.clone();

    div()
        .flex_row()
        .gap(16.0)
        .child(
            div()
                .on_click(move |_| {
                    ctx_dec.update(count, |v| v - 1);
                })
                .child(text("-"))
        )
        .child(text(&format!("{}", ctx.get(count).unwrap_or(0))))
        .child(
            div()
                .on_click(move |_| {
                    ctx_inc.update(count, |v| v + 1);
                })
                .child(text("+"))
        )
}
```

For shared mutable state, use `Arc<Mutex<T>>`:

```rust
use std::sync::{Arc, Mutex};

fn shared_state_example() -> impl ElementBuilder {
    let data = Arc::new(Mutex::new(Vec::<String>::new()));
    let data_click = Arc::clone(&data);

    div()
        .on_click(move |_| {
            data_click.lock().unwrap().push("clicked".to_string());
        })
}
```

---

## Best Practices

1. **Keep handlers lightweight** - Do minimal work in event handlers. For heavy operations, queue work or update state.

2. **Use stateful(handle) for hover/press** - Instead of manually tracking hover state, use `ctx.use_state()` with `stateful(handle)` which handles state transitions automatically.

3. **Clone before closures** - Clone `Arc`, signals, or context references before moving them into closures.

4. **Avoid nested event handlers** - Events bubble up, so you rarely need deeply nested handlers.

5. **Use local coordinates** - For hit testing within an element, use `ctx.local_x` and `ctx.local_y`.
