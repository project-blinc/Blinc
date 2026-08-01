# Element Refs

Signals describe *what* the UI shows. A ref reaches an element and *does
something to it*: focus this field, scroll that container back to the
top, select the text the user just pasted.

Those are not values, so no amount of state modelling expresses them.
There is no string you can write into a signal that means "put the
cursor here".

```rust,ignore
use blinc_layout::selector::InputRef;

let email = InputRef::new();

// Bind it to an element while building.
text_input(&data).bind(&email)

// Drive it from anywhere, later.
email.focus();
email.select_all();
```

## Binding is the identity

A ref knows which element it belongs to because it was **bound** to one.
Nothing is named, and nothing has to stay unique.

```rust,ignore
let first = DivRef::new();
let second = DivRef::new();

div().bind(&first)      // these two cannot be confused,
div().bind(&second)     // whatever they are called
```

This is the difference from `ctx.query("some-id")`, which addresses an
element by a string you invent and have to keep unique across the whole
program. Two components that both chose `"panel"` collide silently, and
a component used twice collides with itself.

The renderer resolves each ref to its element's node while building, so
a ref is live from the first frame the element exists.

## The kinds

Each kind exposes what that element can actually do.

| Kind | Bind with | Does |
|---|---|---|
| `DivRef` | `div().bind(&r)` | `focus`, `blur`, `scroll_into_view`, `bounds`, `exists` |
| `ScrollRef` | `scroll().bind(&r)`, `div().bind_scroll(&r)` | `scroll_to_top`, `scroll_to_bottom`, `scroll_by`, `scroll_to(id)`, `offset` |
| `InputRef` | `text_input(&data).bind(&r)` | the above, plus `value`, `set_value`, `clear`, `select_all` |
| `TextareaRef` | `text_area(&state).bind(&r)` | the same, over lines rather than one string |

`ScrollRef` has more than the table shows — scroll options, behaviours
and offsets are covered in [Scroll](../widgets/scroll.md).

`InputRef` and `TextareaRef` are separate types on purpose. A text area
keeps `Vec<String>` lines and a `(line, column)` cursor where an input
keeps one string and a byte offset, so one shared type would mean
`set_value` did something different depending on what happened to be
bound.

## Two halves, two moments

A field ref binds twice, and the timing differs:

- **The state** is handed over immediately. A text field is built from a
  `SharedTextInputData` its caller already holds, so `value()` and
  `set_value()` work before anything renders.
- **The element** is resolved by the renderer when the field is built,
  like any other bound ref.

So `is_bound()` and `exists()` answer different questions. The first
asks whether a field is behind this ref at all; the second asks whether
its element is currently in the tree. A field scrolled out of view still
knows what was typed into it.

## A ref goes dead when its element does

`LayoutNodeId`s are reissued as the tree rebuilds. A ref that simply
remembered its node would, after the element stopped being built, start
addressing whatever inherited that slot — `exists()` would say true,
`bounds()` would return stale geometry, and `focus()` would land on an
unrelated element.

Refs check the binding still holds on every read. Once the element stops
being built, the ref reports nothing and its commands become no-ops:

```rust,ignore
card.exists();     // false
card.bounds();     // None
card.focus();      // no-op, with a warning
```

Commands on an unbound ref are also no-ops rather than panics, which
matters because a handler can fire before its element is built or after
a rebuild dropped it.

## Using one from a handler

Refs are `Send + Sync` and cheap to clone, so a handler can capture one:

```rust,ignore
let list = ScrollRef::new();
let list_for_click = list.clone();

button("Jump to latest").on_click(move |_| {
    list_for_click.scroll_to_bottom();
})
```

Clones share one binding — a ref handed to a builder and the clone kept
by a handler are the same handle.

## Writing state reaches the screen

A keystroke repaints because the event carrying it drives a frame; a
signal write repaints because the write notifies. A ref reaches past
both and changes state directly, so the mutating methods run the
widget's own refresh rather than merely asking for a redraw. Requesting
a frame marks intent without re-running the callback that builds the
visible text, which leaves a field holding the new value and showing the
old one.

You do not have to do anything for this — it is what `set_value`,
`clear` and `select_all` already do — but it explains why a ref is not
simply "mutate the state and hope".

## Refs in the DSL

`.blinc` sources declare refs, and the compiler supplies the identity
from the declaration's own source position, so nothing asks the author
for a key:

```
ref search: Input

view {
    cn.Input(ref = search, placeholder = "search")
    cn.Button("Clear", on_click = || search.clear())
}
```

Two instances of a component that declares a ref get two handles. See
the DSL chapter for the surface; the machinery is the types above.
