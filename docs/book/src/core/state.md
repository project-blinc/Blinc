# State Management

A Blinc UI reacts to signals in place. Pass a signal to a property setter and
the property follows it; give a list or a condition to a container and its
children follow it. When the signal changes, only what depends on it is
updated, and nothing around it is rebuilt.

```rust
use blinc_core::reactive::{computed, signal};
use blinc_layout::prelude::*;

let count = signal(0_i32);
let label_color = computed(move |g| {
    if g.get(count).unwrap_or(0) > 10 { Color::RED } else { Color::WHITE }
});

div()
    .flex_col()
    .gap(2.0)
    .child(text("Count").color(label_color))
    .child(button("Add").on_click(move |_| count.update(|n| n + 1)))
```

Reach for these in order:

1. **A stylesheet** for looks that follow the pointer: `:hover`, `:active`,
   `:focus`. See [CSS Styling](./css-styling.md).
2. **A bound property** (`.bg(signal)`, `.w(&computed)`,
   `text(..).color(computed)`) for one property that follows a value.
3. **A class that follows a signal** (`.class_when("open", signal)`) to
   switch a set of stylesheet rules on and off.
4. **A list or a branch** (`for_each`, `show`, `show_or`) when the children
   themselves change. See [Lists & Conditional Content](./lists-branches.md).
5. **A `Stateful` element** for a finite state machine whose states change
   what is built. Each transition rebuilds its subtree.

---

## Reactive Property Bindings

Every Blinc element has **reactive property setters**: each takes either a
value or a signal at the same call site. With a signal, the property is
updated whenever the signal changes, and only that property. There is no
rebuild, no `on_state` callback and no `.deps([...])`.

### Signal vs State — what to reach for

Two flavours of reactive value, both wired into the same property-binding
registry. Pick by creation semantics, not capability — they support the
same `.get / .set / .update` operations and both work in every reactive
setter.

| Type | Created via | Lifetime | When to reach for it |
| --- | --- | --- | --- |
| **`Signal<T>`** | `signal(initial)`, returned by `use_signal_keyed(...)` | Slotmap-keyed in the process-global graph | `Copy` — capture by value in closures without `.clone()`. Use anywhere; the primitive. |
| **`State<T>`** | `use_state(initial)`, `use_state_keyed(k, init)` | Hook-keyed slot persisted across rebuilds | UI-component-local state where call-site keying matters. `Clone`. |

Both can be passed to `.bg(...)` etc. interchangeably:

```rust
let count: Signal<i32> = signal(0);          // bare primitive
let theme: State<Theme> = use_state(Theme::Dark);  // hook-keyed

div().bg(&theme_color_for(theme)).rounded(&radius_for(count));
```

> **Migration note:** older code shows `State<T>` everywhere because
> `Signal<T>` only got its rich API in this release. They're
> interoperable — mix freely.

### `Reactive<T>` and `IntoReactive<T>`

Reactive setters take `impl IntoReactive<T>`. Four impls cover the
common cases:

| Pass in | Resolves to | What happens |
| --- | --- | --- |
| A value of `T` | `Reactive::Const(T)` | Direct write at build time — no subscription |
| `&Signal<T>` or `Signal<T>` | `Reactive::Bound(state)` | Registers a subscription on the signal's id; fires on every `.set(...)` |
| `&State<T>` or `State<T>` | `Reactive::Bound(state)` | Same as `Signal<T>` — same channel |
| `&Computed<T>` or `Computed<T>` | `Reactive::Computed(c)` | Registers a subscription on the derived id; fires when *any* tracked dependency of the computed changes |

The call site doesn't change — the type of the argument selects the
behaviour:

```rust
use blinc_core::reactive::signal;            // bare reactive primitive
use blinc_core::context_state::use_state;    // hook-keyed
use blinc_layout::prelude::*;

let bg = signal(Color::from_hex(0x1a1a1a));  // Copy
let w  = use_state(120.0_f32);                // Clone

div()
    .bg(bg)         // Signal is Copy — pass by value
    .w(&w)          // State needs reference (Clone, not Copy)
    .rounded(8.0)   // eager — no subscription
```

There is no separate "bound" setter. The eager and bound forms share
one method name, so you can swap a constant for a signal (or vice
versa) by changing the argument alone.

### Free functions: `signal()` / `computed()` / `derived()` / `effect()`

Four free functions provide the bare reactive-primitive surface, all
operating against the process-global reactive graph:

```rust
use blinc_core::reactive::{signal, computed, derived, effect};

let count: Signal<i32> = signal(0);

// Computed (alias: `derived`). Auto-tracks every signal read inside
// the closure. Re-fires bindings when any tracked dep changes.
let doubled = computed(move |g| g.get(count).unwrap_or(0) * 2);

// Side effect — logging, IO, custom integrations.
let _e = effect(move |g| {
    println!("count = {}", g.get(count).unwrap_or(0));
});

// Drives both: bindings re-paint, effect re-prints.
count.set(5);
```

`Signal<T>` is `Copy`, so closures capture by value without `.clone()`
ceremony:

```rust
let n = signal(0_i32);
let plus  = button("+").on_click(move |_| n.update(|v| v + 1));
let minus = button("-").on_click(move |_| n.update(|v| v - 1));
// Both closures captured `n` by copy — no boilerplate.
```

### Reactive-aware setters

These take `impl IntoReactive<T>`: a value, a signal, a state or a computed.

**`Div`**

| Setter | `T` | What a change does |
| --- | --- | --- |
| `.bg(value)` | `Color` | Repaints, no layout |
| `.opacity(value)` | `f32` | Repaints |
| `.rounded(value)` | `f32` | Repaints |
| `.border_color(value)` | `Color` | Repaints |
| `.border_width(value)` | `f32` | Repaints |
| `.shadow(value)` | `Shadow` | Repaints |
| `.shadows(value)` | `Vec<Shadow>` | Repaints the whole shadow stack |
| `.transform(value)` | `Transform` | Repaints |
| `.scale(value)` | `f32` | Repaints; composes with the existing transform |
| `.rotate(value)` / `.rotate_deg(value)` | `f32` | Repaints |
| `.transform_width(value)` | `f32` (0..=1) | Repaints; a GPU scale-x about the left edge, for fills such as a progress bar |
| `.bind_transform_from(source, \|v\| Transform::…)` | any `T` | Repaints; maps a signal to a transform |
| `.w(value)` / `.h(value)` | `f32` | Runs layout |
| `.p(value)` | `f32` | Runs layout |
| `.gap(value)` | `f32` | Runs layout |
| `.visible(value)` | `bool` | Flips `display: none`, runs layout |
| `.absolute_when(value)` | `bool` | Takes the element out of flow and back, runs layout |
| `.class_when(name, value)` | `bool` | Adds or removes a class; see below |
| `.when(value, \|d\| ..)` | `bool` | Shows or hides the children `f` adds; see below |

**`Text`**

| Setter | `T` |
| --- | --- |
| `Text::bound(source)` | `String`: the content, measured again on each change |
| `.size(value)` | `f32` |
| `.color(value)` | `Color` |
| `.weight(value)` | `FontWeight` |
| `.letter_spacing(value)` | `f32` |
| `.line_height(value)` | `f32` |

`Text::bound` takes a `Reactive<String>`: make one from a signal, state or
computed with `.into_reactive()` (`blinc_layout::IntoReactive`).

**Others**

| Setter | `T` |
| --- | --- |
| `svg(..).color(value)` | `Color` |
| `button(label)` | `String`: the label |
| `button(..).disabled(value)` | `bool` |

Visual-only updates skip layout and just repaint. Layout-affecting updates
patch the live layout style and run one layout pass on the next frame.

```rust
use blinc_core::reactive::{computed, signal};
use blinc_layout::prelude::*;
use blinc_layout::IntoReactive;

let name = signal("Ada".to_string());
let greeting = computed(move |g| format!("Hello, {}", g.get(name).unwrap_or_default()));
let accent = signal(Color::WHITE);

div()
    .flex_row()
    .gap(2.0)
    .child(svg(CHECK_ICON).size(16.0, 16.0).color(accent))
    .child(Text::bound(greeting.into_reactive()).size(16.0).color(accent))
```

### Classes that follow a signal

`.class_when(name, condition)` gives the element a class while the
condition holds. The stylesheet's rules for the class apply when it is
added and stop applying when it is removed, in place:

```rust
let open = signal(false);

div()
    .class("panel")
    .class_when("panel--open", open)
    .on_click(move |_| open.update(|v| !v))
```

```css
.panel { height: 48px; background: #1a1a1a; }
.panel--open { height: 240px; background: #2a2a2a; }
```

The rules that change are the ones whose subject is this element. A rule
about another element that depends on the class (a descendant or sibling
selector) catches up on the next full style pass. A constant condition is
the same as `.class(name)` or nothing.

### Showing, hiding and taking out of flow

- `.visible(signal)` takes the element out of layout (`display: none`)
  while the signal is false. When shown again it has the display it was
  built with.
- `.when(signal, |d| d.child(..))` builds the children `f` adds either way
  and shows them while the signal holds. Only the children follow the
  signal; anything else `f` sets applies as if it were true, so bind those
  properties directly.
- `.absolute_when(signal)` positions the element absolutely while the
  signal holds, keeping its insets.

To build content only while a condition holds, use `show` or `show_or`; see
[Lists & Conditional Content](./lists-branches.md).

### Pointer state as signals

When a stylesheet can't say it, `Interaction` gives an element's hover,
press and focus as signals:

```rust
use blinc_core::reactive::computed;
use blinc_layout::prelude::*;

let pointer = Interaction::keyed("save-card");
let hovered = pointer.hovered().signal();
let bg = computed(move |g| {
    if g.get(hovered).unwrap_or(false) {
        Color::from_hex(0x2a2a2a)
    } else {
        Color::from_hex(0x1a1a1a)
    }
});

div().track(&pointer).bg(bg).p(4.0).child(text("Save"))
```

`.track(&pointer)` feeds the signals from the element's events. Hovered
covers the element and everything under it. Pressed is set by a press and
cleared by a release or by the pointer leaving. `Interaction::keyed(key)`
keeps the signals across rebuilds under `key`; `Interaction::new` takes
signals you own.

### Computed (derived) values

`computed(compute)` returns a `Computed<T>` that evaluates the closure
lazily and tracks every signal it reads through the graph it is given. Pass
it to a reactive setter like a signal:

```rust
use blinc_core::reactive::{computed, signal};

let count = signal(0_i32);
let label_color = computed(move |g| {
    if g.get(count).unwrap_or(0) > 10 { Color::RED } else { Color::WHITE }
});

div()
    .child(text("Count").color(label_color))
    .on_click(move |_| count.update(|n| n + 1))
```

When `count` changes, every computed that read it is marked dirty and every
binding to those computeds fires. Here only the text's colour is patched.

Read signals inside the closure through the graph it is given
(`g.get(signal)`). For a `State<T>`, read `g.get(state.signal())`.

`use_computed(compute)` is the hook-keyed form, persisted across rebuilds.

### Which tool for which change

| Use | When |
| --- | --- |
| A bound setter (`.bg(signal)`, `.w(&computed)`, ...) | One property follows a value |
| `.class_when(..)` | A set of stylesheet rules follows a condition |
| `for_each`, `show`, `show_or` | The children change |
| `Stateful` with `.deps(..)` and `on_state` | A state machine whose states build different structure |

### Lifecycle

Reactive bindings belong to the node they were made on. When the node is
removed, every binding on it is dropped with it, so stale subscribers can't
fire. A list row or a branch also disposes the signals, computeds and
effects created while it was built. You never unsubscribe by hand.

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

Prefer the bare auto-keyed form when each call site holds one slot —
`use_state` (for `State<T>`) and `use_fsm` (for `SharedState<S>`)
both derive their keys from the caller's source location via
`#[track_caller]`, so you don't have to invent + thread a string per
slot. Keyed variants stay around for the cases auto-keying can't
cover (loops, reusable factories instantiated multiple times from
the same line).

---

## Stateful Elements

A `Stateful` element runs a finite state machine and rebuilds its subtree
from `on_state` on each transition. Use it when the states change what is
built. For colours, sizes and other properties that follow hover, press or
data, a stylesheet or a [bound property](#reactive-property-bindings)
patches the element in place, which is cheaper.

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
| `ctx.use_signal(name, init)` | Create/retrieve a scoped signal (auto-subscribes) |
| `ctx.subscribe(&signal)` | Subscribe this stateful to an externally-created signal's changes |
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
let count_signal: State<i32> = use_state_keyed("count", || 0);
let name_signal: State<String> = use_state_keyed("name", || "".to_string());

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

Define your own state enum for complex interactions. The `StateTransitions` trait has two transition methods:

- `on_event(&self, event: u32) -> Option<Self>` — fires on a discrete user input (pointer, keyboard, custom events). Required.
- `on_tick(&self) -> Option<Self>` — fires when a registered signal dependency changes, giving the state machine a chance to transition based on data without an event. Default returns `None`; override only when you need a data-guarded transition.

### Event-driven transitions (`on_event`)

```rust
use blinc_layout::stateful::StateTransitions;
use blinc_core::events::event_types::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
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

### Data-guarded transitions (`on_tick`)

`on_tick` is the Harel-statechart-style guard path: the state machine re-evaluates itself when a registered signal dependency changes, and may transition without an explicit event. The framework calls it during the deps-driven rebuild path; you read the relevant signals inside the impl and return `Some(NextState)` when a value condition is met.

Use it for state that should follow data automatically (loading completion, threshold crossings, timeouts driven by an external clock signal) rather than user input.

```rust
use blinc_layout::stateful::StateTransitions;
use blinc_core::use_state_keyed;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
enum LoaderState {
    #[default]
    Loading,
    Done,
    Failed,
}

// A signal the state machine reads inside on_tick. Anything that
// can be read globally works — a keyed state, a singleton value,
// or a thread-local context.
use blinc_core::State;
fn progress_signal() -> State<f32> {
    use_state_keyed("loader_progress", || 0.0)
}

impl StateTransitions for LoaderState {
    // No user-input transitions for this loader.
    fn on_event(&self, _event: u32) -> Option<Self> { None }

    fn on_tick(&self) -> Option<Self> {
        let progress = progress_signal().get();
        match self {
            LoaderState::Loading if progress >= 1.0 => Some(LoaderState::Done),
            LoaderState::Loading if progress < 0.0  => Some(LoaderState::Failed),
            _ => None,
        }
    }
}

fn loader_view() -> impl ElementBuilder {
    let progress = progress_signal();

    stateful::<LoaderState>()
        .deps([progress.signal_id()])  // tick fires when this signal changes
        .on_state(|ctx| {
            match ctx.state() {
                LoaderState::Loading => div().child(text("Loading...")),
                LoaderState::Done    => div().child(text("Done!")),
                LoaderState::Failed  => div().child(text("Error")),
            }
        })
}
```

The state machine's `on_tick` re-runs every time `progress` changes (because we registered it via `.deps([...])`), and the transition fires automatically when `progress` crosses one of the guard thresholds — no event handler needed.

**`on_event` and `on_tick` compose:** an FSM can use both. Events drive user-input transitions, `on_tick` drives data-conditioned transitions. The framework calls them on the appropriate paths.

---

## Persistent Stateful Handles (`SharedState<S>`)

Blinc has two distinct persistent-state abstractions and the names
get confusing without context — picking the right one comes down
to **what you're storing**:

| Abstraction | Returns | Use for | Constructors |
| --- | --- | --- | --- |
| **`State<T>`** | A signal-backed slot with `.get()` / `.set()` / `.update()` and a `signal_id()` for `.deps([…])` | Fine-grained reactive values — counters, flags, form fields, anything one place writes and another reads via signals | `use_state(initial)` (bare, `#[track_caller]`), `use_state_keyed(key, init)` |
| **`SharedState<S>`** | An `Arc<Mutex<StatefulInner<S>>>` — the handle a `Stateful<S>` widget hangs its FSM off of | Stateful UI elements with discrete states (hover / press / drag / custom state machines), shared across call sites or driven from external events | `use_fsm(initial)` (bare, `#[track_caller]`), `use_fsm_keyed(key, initial)` |

> **TL;DR:** `use_state` returns `State<T>` (a signal). `use_fsm`
> returns `SharedState<S>` (an FSM handle). They are not interchangeable.
> If you reach for one and the type-checker rejects it, you almost
> certainly want the other.

When you build a `Stateful<S>` widget outside an `on_state` closure —
typically because you want to share its FSM with multiple call sites
or drive it from external events — you need a `SharedState<S>`
handle that survives UI rebuilds. Two factory functions cover this:

```rust
use blinc_layout::prelude::*;
use blinc_layout::stateful::{ButtonState, use_fsm, use_fsm_keyed};

// Bare — keyed by the source location of THIS call via `#[track_caller]`.
// One slot per source line. Don't use inside a loop.
let modal_btn   = use_fsm(ButtonState::Idle);
let toast_btn   = use_fsm(ButtonState::Idle);
let dialog_btn  = use_fsm(ButtonState::Idle);

// Explicit — keyed by anything `Hash` (string, integer, tuple,
// InstanceKey, ...). Use this for loops, list items, or reusable
// component factories called multiple times from the same line.
for entry in items.iter() {
    let entry_btn = use_fsm_keyed(entry.id, ButtonState::Idle);
    // …
}
```

The same `Hash` key plus the same `S` always returns the same
`Arc<Mutex<StatefulInner<S>>>`, so state survives subtree rebuilds —
the slot lives in the process-wide `BlincContextState` reactive
graph, not in the layout tree.

### `#[track_caller]` and widget wrappers

`#[track_caller]` is forwarding, not generating. If a widget factory
is tagged `#[track_caller]` and calls `use_fsm` internally,
`Location::caller()` returns the **user's** call site, not the
factory's body. Two distinct call sites give two distinct slots —
which is what you want for the common case:

```rust
#[track_caller]
fn my_button(label: &str) -> impl ElementBuilder {
    // Forwarded — `use_fsm` sees the caller's source line.
    let handle = use_fsm(ButtonState::Idle);
    stateful_from_handle(handle).on_state(/* … */)
}

fn settings_page() -> impl ElementBuilder {
    div()
        .child(my_button("Save"))    // line 42 → unique slot
        .child(my_button("Cancel"))  // line 43 → unique slot
}
```

The loop case is the trap. `#[track_caller]` still forwards the
caller's location, but every iteration of a loop calls from the
same line, so every iteration collides on the same slot:

```rust
// 🚫 BUG — all 5 buttons share one ButtonState handle:
for i in 0..5 {
    col = col.child(my_button("Item"));  // line 47, every iteration
}

// ✅ Pass an explicit per-instance key:
for i in 0..5 {
    col = col.child(my_button_keyed(i, "Item"));
}

#[track_caller]
fn my_button_keyed(id: u32, label: &str) -> impl ElementBuilder {
    let handle = use_fsm_keyed(id, ButtonState::Idle);
    stateful_from_handle(handle).on_state(/* … */)
}
```

### Mental model

| Scenario | API | Key |
| --- | --- | --- |
| One widget per source line | `use_fsm(initial)` | `(file, line, column)` via `#[track_caller]` |
| Loop body / `.map()` / repeated factory call | `use_fsm_keyed(k, initial)` | Per-iteration data: index, id, tuple, `InstanceKey` |
| Different widget types from the same line | `use_fsm(initial)` works | Key is also typed on `SharedState<S>`, so two calls with different `S` from one line still get distinct slots |

The same split exists for plain reactive cells (`State<T>`):

| State type | Bare auto-keyed | Explicit key |
| --- | --- | --- |
| `State<T>` (basic reactive value) | `use_state(initial)` | `use_state_keyed(key, init)` |
| `SharedState<S>` (FSM handle) | `use_fsm(initial)` | `use_fsm_keyed(key, initial)` |

### Why this works across rebuilds

A subtree rebuild replaces layout nodes, but `LayoutNodeId`s aren't
the identity stateful state hangs off of — that lives in the
process-wide hooks store, keyed by `(call_site, S)` (or
`(explicit_key, S)`). Rebuilds tear down and re-mint layout nodes
but the source location of `use_fsm` doesn't change, so the
same slot is found on the next call. Combined with `StableNodeId`s
(which make event routing survive rebuilds), Stateful widgets keep
their internal FSM state, scoped signals, and registered springs /
keyframes across every rebuild.

---

## Best Practices

1. **Bind, don't rebuild.** A property that follows a value takes the signal
   directly: `.bg(signal)`, `.w(&computed)`, `text(..).color(computed)`.

2. **Let the stylesheet handle pointer looks.** `:hover`, `:active` and
   `:focus` need no Rust. Use `Interaction` when a stylesheet can't say it.

3. **Use `class_when` for a set of rules.** Several properties that change
   together belong in a class.

4. **Use `for_each` and `show` for children that change.** Key a list by
   the item's identity, not its position.

5. **Read through the graph inside a `computed`.** `g.get(signal)` tracks
   the read.

6. **Keep `Stateful` for state machines.** Use it when a transition has to
   build different structure.

7. **Prefer the bare auto-keyed variant.** `use_state(initial)` for
   `State<T>`, `use_fsm(initial)` for `SharedState<S>`. Reach for the
   `_keyed` variants only when one source line produces several instances
   (loops, reusable factories).
