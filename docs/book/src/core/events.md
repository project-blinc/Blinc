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

fn toggle_button() -> impl ElementBuilder {
    stateful::<ToggleState>()
        .w(100.0)
        .h(40.0)
        .rounded(8.0)
        .flex_center()
        .on_state(|ctx| {
            let bg = match ctx.state() {
                ToggleState::Off => Color::rgba(0.3, 0.3, 0.35, 1.0),
                ToggleState::On => Color::rgba(0.2, 0.8, 0.4, 1.0),
            };
            div().bg(bg)
        })
        .on_click(|_| {
            println!("Toggled!");
            // ToggleState transitions automatically on click
        })
        .child(text("Toggle").color(Color::WHITE))
}
```

### Drag to Move

`offset` is a `State<(f32, f32)>` carrying the drag accumulator.
`bind_transform_from` maps it into a `Transform::translate`, so each
`.set(...)` patches the GPU translate directly — no Stateful rebuild,
no relayout.

```rust
use blinc_core::context_state::use_state;
use blinc_core::Transform;

fn draggable_box() -> impl ElementBuilder {
    let offset = use_state((100.0_f32, 100.0_f32));

    div()
        .w(80.0)
        .h(80.0)
        .rounded(8.0)
        .bg(Color::rgba(0.4, 0.6, 1.0, 1.0))
        .bind_transform_from(offset.clone(), |(x, y)| Transform::translate(x, y))
        .on_drag({
            let offset = offset.clone();
            move |evt| {
                offset.update(|(x, y)| (x + evt.drag_delta_x, y + evt.drag_delta_y));
            }
        })
}
```

### Keyboard Shortcuts

```rust
fn keyboard_handler(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .w_full()
        .h_full()
        .on_key_down(|evt| {
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

fn hover_card() -> impl ElementBuilder {
    stateful::<ButtonState>()
        .w(200.0)
        .h(120.0)
        .rounded(12.0)
        .on_state(|ctx| {
            let (bg, scale) = match ctx.state() {
                ButtonState::Hovered => (Color::rgba(0.2, 0.2, 0.3, 1.0), 1.02),
                _ => (Color::rgba(0.15, 0.15, 0.2, 1.0), 1.0),
            };
            div().bg(bg).transform(Transform::scale(scale, scale))
        })
        .child(text("Hover me!").color(Color::WHITE))
}
```

---

## Keyboard Navigation

Tab and Shift+Tab move focus through the controls that mark themselves as tab
stops, in tree order, wrapping at the ends. The page scrolls to show the
control that takes focus. Enter and Space click the focused control. A press
anywhere clears focus set by Tab.

When nothing is focused, Tab starts at the first visible control and
Shift+Tab at the last. Scrolling the focused control offscreen makes the
next Tab start within the current viewport as well. Once focus is visible,
traversal continues in tree order and scrolls the next control into view.

Mark any element as a tab stop with the `tabindex` attribute:

```rust
use blinc_layout::prelude::div;

div()
    .attr("tabindex", "0")
    .on_click(|_| println!("activated"))
```

An element with a `disabled` attribute, or with no size, is skipped. The
`:focus` selector matches the element that holds focus, so a rule draws the
ring:

```css
.card:focus {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
}
```

A text input marks itself `tabindex=text`: Tab focuses it as a press does and
leaves it on the next Tab. A code or rich-text editor keeps Tab for its own
use. The `cn` buttons, checkboxes, switches, toggles, radios, tabs, selects,
menu triggers, accordion and collapsible triggers, links, sliders and text
fields are tab stops already.

## Capturing State in Closures

Event handlers are `Fn` closures. `State<T>` is cheap to `clone()`
(it's an Arc-of-handle internally), so the common pattern is to clone
the handle into each closure that needs it:

```rust
use blinc_core::context_state::use_state;
use blinc_layout::stateful::{NoState, stateful};

fn counter_buttons() -> impl ElementBuilder {
    let count = use_state(0_i32);

    div()
        .flex_row()
        .gap(16.0)
        .child(
            div()
                .on_click({
                    let count = count.clone();
                    move |_| count.update(|v| v - 1)
                })
                .child(text("-"))
        )
        // The label re-renders when count changes — wrap in Stateful
        // with .deps() so it picks up the new value.
        .child(
            stateful::<NoState>()
                .deps([count.signal_id()])
                .on_state({
                    let count = count.clone();
                    move |_ctx| div().child(text(&format!("{}", count.get())))
                })
        )
        .child(
            div()
                .on_click(move |_| count.update(|v| v + 1))
                .child(text("+"))
        )
}
```

### Thread Safety

`BlincContextState` is a thread-safe global singleton — the reactive
graph and hook state both live behind `Arc<Mutex<...>>`. `State<T>`
handles wrap that graph, so calling `.set` / `.update` from any
thread is safe.

```rust
let my_state = use_state(0_i32);

div()
    .on_click({
        let my_state = my_state.clone();
        move |_| {
            // Safe: State<T> is thread-safe through the shared graph.
            my_state.update(|v| v + 1);

            // BlincContextState exposes the rest of the global APIs.
            BlincContextState::get().set_focus(Some("my-input"));
            BlincContextState::get().request_rebuild();
        }
    })
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

2. **Use `stateful::<S>()` for hover/press** - Instead of manually tracking hover state, use `stateful::<ButtonState>()` which handles state transitions automatically.

3. **Clone before closures** - Clone `Arc`, signals, or context references before moving them into closures.

4. **Avoid nested event handlers** - Events bubble up, so you rarely need deeply nested handlers.

5. **Use local coordinates** - For hit testing within an element, use `ctx.local_x` and `ctx.local_y`.
