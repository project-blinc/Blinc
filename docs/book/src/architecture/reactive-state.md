# Reactive State System

Blinc implements a **push-pull hybrid reactive system** for fine-grained state management without virtual DOM overhead. This is inspired by modern reactive frameworks like Leptos and SolidJS.

## Core Concepts

### Signals

A `Signal<T>` is a reactive container for a value. When the value changes, all dependent computations automatically update.

```rust
// Create a signal
let count = ctx.use_state_keyed("count", || 0i32);

// Read the current value
let value = count.get();

// Update the value
count.set(5);
count.update(|v| v + 1);
```

### Signal IDs

Signals are identified by `SignalId`, a cheap-to-copy handle:

```rust
// Get the signal's ID for dependency tracking
let id = count.signal_id();
```

## Automatic Dependency Tracking

When code accesses a signal's value, the dependency is automatically recorded:

```rust
// Stateful element with signal dependency
stateful(handle)
    .deps(&[count.signal_id()])  // Declare dependency
    .on_state(move |state, div| {
        // Reading count.get() here is tracked
        let value = count.get();
        div.set_bg(color_for_value(value));
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

## Derived Values

Derived values compute from other signals and cache the result:

```rust
// Conceptual - derived values
let doubled = derived(|| count.get() * 2);

// Value is cached until count changes
let value = doubled.get();  // Computed once
let again = doubled.get();  // Returns cached value
```

### Lazy Evaluation

Derived values only compute when:
1. First accessed after creation
2. Accessed after a dependency changed
3. Their value is explicitly needed

This prevents wasted computation for unused values.

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

## Integration with Stateful Elements

The reactive system integrates with stateful elements via `.deps()`:

```rust
fn counter_display(ctx: &WindowedContext, count: State<i32>) -> impl ElementBuilder {
    let handle = ctx.use_state(ButtonState::Idle);

    stateful(handle)
        // Declare signal dependencies
        .deps(&[count.signal_id()])
        .on_state(move |_state, container| {
            // This callback re-runs when count changes
            let current = count.get();
            container.merge(
                div().child(text(&format!("{}", current)).color(Color::WHITE))
            );
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

1. **Use keyed state for persistence** - `use_state_keyed("key", || value)` survives rebuilds

2. **Batch related updates** - Group multiple signal changes to avoid redundant work

3. **Declare dependencies explicitly** - Use `.deps()` for stateful elements that read signals

4. **Prefer stateful for visual changes** - Use stateful elements instead of signals for hover/press effects

5. **Keep signals granular** - Fine-grained signals enable more precise updates
