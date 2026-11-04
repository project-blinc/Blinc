# blinc_macros

Procedural macros for Blinc UI. See the
[component guide](https://blinc.rs/docs/components/blinc-component.html).

## BlincComponent

Declare ordinary field types. The derive generates a companion props type,
fluent `IntoReactive<T>` setters, and persistent `Signal<T>` hooks:

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
        .child(Text::bound(props.label))
}

let busy = signal(false);
let label = computed(move |g| {
    if g.get(busy).unwrap_or(false) { "Working" } else { "Ready" }.to_string()
});
let props = Status::props("Initial", false).label(label).disabled(busy);
let view = status(props);
```

`StatusProps` stores `Reactive<String>` and `Reactive<bool>`. Constants,
Signal, Computed, borrowed sources and existing reactive props all use the
same setters. Their source identities are retained, so forward fields to
bindings instead of reading a value during construction. Field visibility,
documentation, generic parameters and bounds are preserved. Props can be cloned.

The original struct is unchanged. The constructor takes all ordinary fields
in declaration order; every field also has a fluent setter on the props.

For component-owned mutable data, `Status::use_label(ctx, initial)` returns
a persisted Signal. `_for(ctx, instance_key, initial)` isolates list items;
`_auto(ctx, initial)` keys by caller location. Initial values seed hooks once.
Use Computed to combine prop values with `prop.read(g)`; use Effect for side
effects. Visual changes belong in property bindings and CSS rules.

`#[animation]` fields generate spring hooks returning `SharedAnimatedValue`
and are excluded from props. Unit structs retain the shared spring/timeline
hooks and instance-key methods without generating a props type.

Consumers need `blinc_core` and `blinc_animation`; named reactive fields also
need `blinc_layout`. The derive is exported by this crate and reexported as
`BlincComponent` and `Component` by `blinc_app`.

## flow!

Compile a flow shader to WGSL with `flow!(name, fragment, { ... })` and draw
it with `Canvas::flow`. See the [flow shader guide](https://blinc.rs/docs/advanced/flow-shaders.html).

## License

MIT OR Apache-2.0
