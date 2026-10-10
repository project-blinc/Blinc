# Lists & Conditional Content

A `Div` can hold children that come and go while the app runs: one per item
of a list, or a branch shown while a condition holds. When the source
changes, rows are added, removed and reordered in place. The tree around
them is not rebuilt, and rows that stay keep their elements and their state.

## Lists: `for_each`

```rust
use blinc_core::reactive::signal;
use blinc_layout::prelude::*;

#[derive(Clone)]
struct Todo {
    id: u32,
    title: String,
}

let todos = signal(vec![
    Todo { id: 1, title: "Write the docs".into() },
    Todo { id: 2, title: "Ship".into() },
]);

div()
    .flex_col()
    .gap(2.0)
    .for_each(
        todos,
        |todo: &Todo| todo.id,
        |todo: Todo| text(todo.title).size(14.0),
    )
```

`for_each(each, key, item)` takes three things:

- **`each`**, the items: a `Signal<Vec<T>>`, a `State<Vec<T>>`, a
  `Computed<Vec<T>>`, or a plain `Vec<T>` for a list that never changes. A
  list derived from other signals is a `computed`.
- **`key`**, which names an item. An item whose key was there before keeps
  its element. A new key gets `item(value)` built. A key that has gone takes
  its element with it.
- **`item`**, which builds the element for one item. It runs once per key,
  not on every change of the list.

Because a row is built from the value its key first had, an item that
changes while its key stays shows its old value. Bind the part that changes
to a signal inside the row, or make it part of the key.

The element is the container. Rows lay out as its own children, with its
direction, gap and alignment, and children added before or after the call
stay where they are. An element holds one list or branch.

Two items with the same key show the first; the later one is skipped with a
warning.

### What a row owns

Each row is built inside a scope of its own (an [`Owner`](#scopes-owner)).
Signals, computeds and effects created while the row is built belong to it
and are disposed when the row goes, so a list that churns does not leak.
Keyed state (`use_state_keyed`) is meant to outlive a rebuild and is never
owned by a row.

## Branches: `show` and `show_or`

```rust
let signed_in = signal(false);

div().show_or(
    signed_in,
    || text("Welcome back"),
    move || button("Sign in").on_click(move |_| signed_in.set(true)),
)
```

`show(when, then)` builds `then()` while `when` holds and tears it down,
with what it created, when it stops. Nothing is built while the condition
is false. `show_or(when, then, otherwise)` adds an element for the other
case.

Three calls hide content. They differ in what exists while it is hidden:

| Call | While the condition is false | When it flips |
| --- | --- | --- |
| `.show(cond, \|\| ..)` | Nothing is built | The branch is built, or torn down with its state |
| `.visible(cond)` | The element exists, with `display: none` | Its display flips and layout runs |
| `.when(cond, \|d\| ..)` | The children `f` added exist, hidden | Their display flips and layout runs |
| `.collapsed_when(cond)` | The element and its content exist, at no height | Its height changes and layout runs, which a height animation can follow |

Use `show` when the hidden content is expensive to keep or should start
fresh each time. Use `visible` when it should keep its state and toggles
often.

## Leaving with an animation: `Row::on_leave`

A row normally goes the moment its key does. To animate it out, wrap the
element in a `Row` and say how it leaves:

```rust
use std::sync::{Arc, Mutex};

use blinc_animation::{AnimatedValue, SpringConfig, get_scheduler};
use blinc_layout::region::Row;

div().flex_col().gap(2.0).for_each(
    todos,
    |todo: &Todo| todo.id,
    |todo: Todo| {
        let fade = Arc::new(Mutex::new(AnimatedValue::new(
            get_scheduler(),
            1.0,
            SpringConfig::snappy(),
        )));
        let (out, settled) = (fade.clone(), fade.clone());
        Row::new(motion().opacity(fade).child(text(todo.title)))
            .on_leave(
                move || out.lock().unwrap().set_target(0.0),
                move || !settled.lock().unwrap().is_animating(),
            )
    },
)
```

`on_leave(start, done)` keeps the row mounted after its key goes. `start`
runs once, when the key goes. `done` is then asked once per frame, and the
row is removed when it returns true. Until then the row keeps its place
among the rows and everything it owns. `done` must come true eventually, or
the row stays for as long as the list does.

If the key comes back while the old row is leaving, a new row is built for
it and the old one still finishes leaving.

`show` and `show_or` take a `Row` too, so a branch can animate out the same
way.

### Leaving out of flow

A leaving row still takes up space. When the new content should take its
place at once, as in a page transition, take the leaving row out of flow
while it goes:

```rust
let leaving = signal(false);
Row::new(page.absolute_when(leaving)).on_leave(
    move || leaving.set(true),
    move || exit_finished(),
)
```

`absolute_when` positions the element absolutely while the signal holds,
with a layout pass and no rebuild. With no insets it sits at the start of
its container, and the content that replaces it lays out as if it were
gone.

## Scopes: `Owner`

Rows and branches use `blinc_core::owner::Owner` to give back what they
create. You can use it directly for anything that is built, used for a
while, then thrown away:

```rust
use blinc_core::owner::Owner;
use blinc_core::reactive::signal;

let owner = Owner::new();
let count = owner.run(|| {
    Owner::on_cleanup(|| tracing::debug!("panel closed"));
    signal(0)
});

// ...

owner.dispose(); // `count` is disposed and the cleanup runs
```

A signal, computed or effect made inside `Owner::run` belongs to the owner.
`dispose` disposes them, then any owners created inside it, then runs the
cleanups registered with `Owner::on_cleanup`. Owners are per thread.
