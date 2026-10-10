# Reactive State System

Blinc implements a **push-pull hybrid reactive system** for fine-grained state management without virtual DOM overhead. This is inspired by modern reactive frameworks like Leptos and SolidJS.

## Core Concepts

### `State<T>`

A `State<T>` is a reactive value handle — a cheap clone-able wrapper
around a slot in the process-wide [`ReactiveGraph`]. When the value
changes, every subscriber (signal-bound property binding, stateful
element with `.deps([…])`, derived computation) is notified.

```rust
use blinc_core::context_state::use_state;

// Auto-keyed by source location (#[track_caller]).
let count = use_state(0_i32);

// Read the current value.
let value = count.get();

// Update the value.
count.set(5);
count.update(|v| v + 1);
```

`State<T>` is internally an `Arc`-of-graph-handle, so cloning it is
cheap. Pass it by reference (`&count`) or by value — both work.

### `Signal<T>` (low-level)

Inside the reactive graph the underlying slot is a `Signal<T>` keyed
by `SignalId`. `State<T>` is the public binding-friendly form;
`Signal<T>` / `SignalId` is what the registry indexes by. Most
application code only touches `State<T>`.

```rust
// Get the underlying SignalId — used by .deps([...]) on Stateful elements.
let id = count.signal_id();
```

## Automatic Dependency Tracking

When code accesses a signal's value inside an `on_state` callback, the
dependency is automatically recorded:

```rust
// Stateful element with signal dependency
stateful::<ButtonState>()
    .deps([count.signal_id()])  // Declare dependency
    .on_state(move |ctx| {
        // Reading count.get() here is tracked
        let value = count.get();
        div().bg(color_for_value(value))
    })
```

When `count` changes, only elements depending on it re-run their callbacks.

## ReactiveGraph Internals

The `ReactiveGraph` manages all reactive state:

```rust
struct ReactiveGraph {
    signals: SlotMap<SignalId, SignalNode>,
    deriveds: SlotMap<DerivedId, DerivedNode>,
    effects: SlotMap<EffectId, EffectNode>,
    pending_effects: Vec<EffectId>,
    batch_depth: u32,
}
```

### Data Structures

| Type | Purpose |
|------|---------|
| `SignalNode` | Stores value + list of subscribers |
| `DerivedNode` | Cached computed value + dirty flag |
| `EffectNode` | Side-effect function + dependencies |

### Subscription Flow

```
Signal.set(new_value)
    │
    ├── Mark all subscribers dirty
    │
    ├── Propagate to derived values
    │
    └── Queue effects for execution
```

## Derived Values (`Computed<T>`)

Derived values compute from other signals and cache the result.
`use_computed` returns a [`Computed<T>`] — a binding-friendly wrapper
around a `Derived<T>` handle plus the reactive graph it lives in:

```rust
use blinc_core::context_state::{use_state, use_computed};

let count = use_state(0_i32);

let doubled = {
    let count = count.clone();
    use_computed(move |_g| count.get() * 2)
};

// Value is cached until any tracked dependency changes.
let value = doubled.get();  // Computed once
let again = doubled.get();  // Returns cached value
```

### Auto-tracked dependencies

The closure passed to `use_computed` auto-tracks every signal it
reads. There is no `.deps([…])` for computed values — touching a
`State<T>` inside the closure subscribes the derived to that signal.
When any tracked dependency fires `.set(...)`, the derived's dirty
bit flips and the `PropertyBindingRegistry` notifies every property
binding subscribed to that derived.

### Lazy Evaluation

Derived values only compute when:
1. First accessed after creation
2. Accessed after a dependency changed
3. Their value is explicitly needed

This prevents wasted computation for unused values.

### Binding to properties

`Computed<T>` plugs into the same `IntoReactive<T>` channel as
`State<T>`. Pass `&computed` to any reactive setter:

```rust
div()
    .child(text("Doubled").color(&doubled_color))
    .w(&width_computed)
```

See [Reactive Property Bindings](../core/state.md#reactive-property-bindings)
in the State Management chapter for the call-site surface.

## Effects

Effects are side-effects that run when dependencies change:

```rust
// Conceptual - effects
effect(|| {
    let value = count.get();  // Tracks dependency on count
    println!("Count changed to {}", value);
});
```

Effects are:
- Queued when dependencies change
- Executed after the current batch completes
- Run in topological order (respecting dependency depth)

## Batching

Multiple signal updates can be batched to prevent redundant recomputation:

```rust
// Without batching: 3 separate updates, 3 effect runs
count.set(1);
name.set("Alice");
enabled.set(true);

// With batching: 1 combined update, 1 effect run
ctx.batch(|g| {
    g.set(count, 1);
    g.set(name, "Alice");
    g.set(enabled, true);
});
```

### How Batching Works

1. `batch_start()` increments batch depth counter
2. Signal updates mark subscribers dirty but don't run effects
3. `batch_end()` decrements counter
4. When counter reaches 0, all pending effects execute

## Property Bindings (Unified Property Channel)

A second integration path runs alongside `.deps()` + `on_state`:
**signal-bound element properties**. Calling
`div().bg(&state)` or `div().w(&computed)` registers a subscription
against the minted `LayoutNodeId` in the process-wide
[`PropertyBindingRegistry`][reg]. When the signal (or any tracked
dependency of the computed) fires, the registry walks subscribers and
queues a [`PartialPropertyUpdate`] — the same channel CSS animations,
transitions, and stateful refreshes already use.

[reg]: https://docs.rs/blinc_layout/latest/blinc_layout/binding/struct.PropertyBindingRegistry.html

```
State<T>::set(new_value)
    │
    ▼
notify_property_bindings(signal_id)
    │
    ▼  (registry walks subscribers for signal_id)
    │
    ├── render-targeting:  queue_prop_update_partial(node, prop, write)
    └── layout-targeting:  queue_layout_update_partial(node, prop, write)
            │
            ▼
        platform runner drains queue next frame:
        - patches RenderProps / taffy::Style
        - schedules compute_layout if needs_layout = true
        - request_redraw()
```

Two parallel indexes — one keyed by `SignalId`, one by `DerivedId`,
both feeding the same `Subscriber` dispatch path. `Computed<T>` rides
the second; `State<T>` rides the first.

### Lifecycle

Registration happens inside the element's `build()` after the layout
node is minted. Unregistration happens automatically in
`remove_subtree_nodes` — `PropertyBindingRegistry::unregister_node`
evicts every subscription belonging to that node, so a structural
rebuild can't leak stale subscribers.

### When to reach for which channel

| Goal | Channel |
| --- | --- |
| Update one property on one element | Reactive setter (e.g. `.bg(&state)`), the cheapest |
| Switch a set of stylesheet rules | `.class_when(name, signal)`: a class toggle on the same queue |
| Change the children | `for_each`, `show`, `show_or`: a region update on the same queue |
| A state machine whose states build different structure | `Stateful<S>` + `.deps([signal_id])` + `on_state` |
| Side effect (logging, IO) when a signal changes | `effect(..)` |

The queue carries more than property writes. A class toggle adds or removes
a class and re-applies that element's rules. A text update replaces a text
node's content and has it measured again. A region update reads a list or
condition again and adds, removes and reorders the rows of a `for_each`,
`show` or `show_or`. Each update reports what it affects (paint, layout,
text measurement), and the runner runs layout only when one of them asks
for it.

### Ownership

A row or a branch is built inside an `Owner`, a scope that records the
signals, computeds and effects created inside it. When the row goes, its
owner disposes them and runs any cleanups registered with
`Owner::on_cleanup`. Keyed state (`use_state_keyed`) is meant to outlive a
rebuild and is never owned.

See the [Reactive Property Bindings](../core/state.md#reactive-property-bindings)
section for the call-site surface.

## Integration with Stateful Elements

The reactive system integrates with stateful elements via `.deps()`:

```rust
fn counter_display(count: State<i32>) -> impl ElementBuilder {
    stateful::<NoState>()
        // Declare signal dependencies
        .deps([count.signal_id()])
        .on_state(move |_ctx| {
            // This callback re-runs when count changes
            let current = count.get();
            div().child(text(&format!("{}", current)).color(Color::WHITE))
        })
}
```

### Dependency Registry

The system maintains a registry of signal dependencies:

```rust
// Internal tracking
struct DependencyEntry {
    signal_ids: Vec<SignalId>,
    node_id: LayoutNodeId,
    refresh_callback: Box<dyn Fn()>,
}
```

When signals change, the registry triggers rebuilds for dependent nodes.

## Performance Characteristics

### O(1) Signal Access

Reading a signal is a simple memory lookup:

```rust
fn get(&self) -> T {
    self.value.clone()  // Direct access, no computation
}
```

### O(subscribers) Propagation

Updates only touch direct subscribers:

```rust
fn set(&mut self, value: T) {
    self.value = value;
    for subscriber in &self.subscribers {
        subscriber.mark_dirty();
    }
}
```

### Minimal Allocations

- `SignalId` is a 64-bit handle (Copy)
- Subscriber lists use `SmallVec<[_; 4]>` (inline for small counts)
- SlotMap provides dense storage without gaps

## Comparison to Virtual DOM

| Aspect | Virtual DOM | Blinc Reactive |
|--------|-------------|----------------|
| State change | Rebuild entire component | Update only affected nodes |
| Diffing | O(tree size) | O(1) per signal |
| Memory | VDOM objects per render | Fixed signal storage |
| Dependency tracking | Manual (useEffect deps) | Automatic |

## Best Practices

1. **Prefer the auto-keyed form** — `use_state(initial)` derives a slot
   from the source location. Use `use_state_keyed("key", || value)` only
   when one source line produces multiple instances (loops, repeated
   factories).

2. **Bind properties directly when you can** — `div().bg(&state)` /
   `div().w(&computed)` is cheaper than rebuilding a Stateful subtree
   for a single-property update.

3. **Reach for `.deps()` + `on_state` when the subtree shape changes** —
   reactive setters patch one property; restructuring children needs
   the callback path.

4. **Batch related updates** — Group multiple signal changes to avoid
   redundant work.

5. **Use `Computed<T>` for multi-signal reactivity** — `use_computed`
   auto-tracks every signal touched inside its closure, so a derived
   value bound via `.opacity(&computed)` fires when any of its inputs
   change.

6. **Keep signals granular** — Fine-grained signals enable more precise
   updates. A separate `State<f32>` for `width` and `opacity` re-paints
   less than a single `State<UiConfig>` carrying both.
