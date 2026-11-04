# BlincComponent Macro

`BlincComponent` generates reactive props and persistent Signal hooks from a
struct. Props accept ordinary values, `Signal<T>` and `Computed<T>` through
`IntoReactive<T>`. Forward them to UI bindings so changes patch mounted elements.

## Declare the Fields

```rust
use blinc_app::prelude::*;

#[derive(BlincComponent)]
pub struct Counter {
    /// Text displayed before the count.
    pub label: String,
    pub count: i32,
    pub disabled: bool,
}
```

This generates `CounterProps`, `Counter::props(label, count, disabled)` and
fluent setters on the props. The original `Counter` struct stays unchanged;
its fields describe the prop types and named hooks.

The companion has the same visibility and generic parameters as the declared
struct. Each prop keeps its field visibility and documentation, and stores
`Reactive<T>`. Cloning props retains their Signal and Computed sources.

## Supply Reactive Props

```rust
use blinc_app::prelude::*;
use blinc_core::reactive::{computed, signal};

#[derive(BlincComponent)]
pub struct Status {
    pub label: String,
    pub disabled: bool,
}

fn status(props: StatusProps) -> Div {
    div()
        .class("status")
        .attr_when("disabled", "", props.disabled)
        .child(Text::bound(props.label).class("status-label"))
}

fn example() -> Div {
    let busy = signal(false);
    let label = computed(move |g| {
        if g.get(busy).unwrap_or(false) {
            "Working".to_string()
        } else {
            "Ready".to_string()
        }
    });
    let props = Status::props("Initial", false)
        .label(label)
        .disabled(busy);
    status(props)
}
```

The constructor accepts every ordinary field in declaration order. Setters
accept the same `impl IntoReactive<T>` inputs, including borrowed signals,
borrowed computeds and an existing `Reactive<T>` from another props value.

`Status::props("Ready", false)` uses constants. Passing `busy` keeps its signal
identity; passing `label` keeps its computation. No value is read or copied
into a new signal while constructing props.

The `status` class styles the root, `status-label` styles its text, and the
root's `disabled` attribute exposes the bound state for restyling:

```css
.status { padding: 12px; border-radius: var(--radius-md); }
.status-label { color: var(--text-primary); }
.status[disabled] { opacity: 0.5; }
```

The derive generates data APIs. Declare classes and CSS rules in the component
that consumes the props.

## Own Mutable Values with Signal Hooks

```rust
use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;

#[derive(BlincComponent)]
pub struct Counter {
    pub label: String,
    pub count: i32,
    pub disabled: bool,
}

fn counter(ctx: &WindowedContext) -> Div {
    let count: Signal<i32> = Counter::use_count(ctx, 0);
    let caption = blinc_core::reactive::computed(move |g| {
        format!("Count: {}", g.get(count).unwrap_or(0))
    });
    let props = Counter::props(caption, count, false);
    div()
        .class("counter")
        .attr("tabindex", "0")
        .attr_when("disabled", "", props.disabled)
        .child(Text::bound(props.label))
        .on_click(move |_| count.update(|value| value + 1))
}
```

`use_count` creates a writable `Signal<i32>` on its first call and retrieves
that same signal on later builds. Its `initial` argument is a plain `i32`,
used only to seed owned data. External Signal or Computed inputs belong in
`Counter::props`, whose fields are read-only binding sources.

| Generated hook | Identity |
|---|---|
| `Counter::use_count(ctx, initial)` | Component type and field |
| `Counter::use_count_for(ctx, key, initial)` | Component type, field and explicit instance key |
| `Counter::use_count_auto(ctx, initial)` | Component type, field and caller location |

Use `_for` in lists and loops. Two calls to `_auto` from one loop body share
a call site. Different fields, modules, component types and generic
instantiations have separate hook identities.

Hooks work with any `BlincContext`; they do not require a native window.

## Combine Props in Computed and Effect

Use `Reactive::read(g)` inside computations so dependencies stay tracked.
Use `now()` only when taking a snapshot, such as reading a prop in an event
handler. Reading a prop while building a view does not create a binding.

```rust
use blinc_app::prelude::*;
use blinc_core::reactive::{computed, effect};
use blinc_layout::binding::IntoReactive;

#[derive(BlincComponent)]
pub struct Counter {
    pub label: String,
    pub count: i32,
}

fn counter(props: CounterProps) -> Div {
    let count = props.count.clone();
    let _observer = effect(move |g| {
        tracing::debug!(count = count.read(g).unwrap_or(0), "Counter changed");
    });
    let caption = computed(move |g| {
        format!("{}: {}",
            props.label.read(g).unwrap_or_default(),
            props.count.read(g).unwrap_or(0))
    });
    div().class("counter").child(Text::bound(caption.into_reactive()))
}
```

Use Computed for values derived from props, Effect for side effects, and UI
bindings for visual updates. Components built under a build scope share its
cleanup lifetime. Ordinary prop changes do not need `.deps()` or an
`on_state` rebuild.

## Animation Hooks and Unit Structs

`#[animation]` fields retain the existing spring hooks and are omitted from
props. For example, a component with `label: String` and
`#[animation] scale: f32` receives `Component::props(label)` and
`use_scale(ctx, initial, config)` returning `SharedAnimatedValue`.

Unit and tuple structs generate component keys and shared spring/timeline
hooks. A unit struct has no companion props type:

```rust
use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;

#[derive(BlincComponent)]
struct LoadingIndicator;

fn animation(ctx: &WindowedContext) {
    let _spring = LoadingIndicator::use_animated_value(ctx, 1.0, SpringConfig::snappy());
    let _timeline = LoadingIndicator::use_animated_timeline(ctx);
}
```

All component types expose `COMPONENT_KEY`, `instance_key()` and
`instance_key_for(suffix)`. Shared spring and timeline methods also have
`_with(ctx, suffix, ...)` variants. Prefer CSS transitions, keyframes and
`motion()` for component appearance and interaction states.

## Dependencies and Migration

Consumers depend directly on `blinc_macros` (or use the derive reexport from
`blinc_app`), `blinc_core` and `blinc_animation`. Named reactive fields also
require `blinc_layout`, which owns `IntoReactive` and `Reactive`.

Existing field hooks now return `Signal<T>`. Use its `get`, `set` and `update`
methods and forward the signal to a binding. Hooks retain their existing
keys for module-level non-generic component types. Ordinary Rust struct literals
keep their declared field types; use the generated `Name::props(...)` factory
when supplying reactive inputs.
