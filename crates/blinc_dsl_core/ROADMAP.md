# Blinc DSL roadmap

Status of the `.blinc` surface: what it can express today, what is being
worked on, and what is queued. Each item names the concepts involved
rather than files, which move.

## Shipped

**Control flow in view bodies.** `if` / `else` emit children from either
branch. `while` runs and its side effects land, but a loop body that
emits children is still broken: the child list belongs to the entry
block and a later block cannot use it.

**Cross-module views.** ES6 `import { Comp } from "./module"`, resolved
by walking the import graph from the entry file. Only `compile_project`
does that resolution; `compile_source` and `compile_file` see one file.

**Reactive bindings.** Three mechanisms, deliberately distinguishable:

- an in-place property write, for props with a property writer
  (dimensions, colours, opacity, progress value) — a set patches render
  props, with no rebuild and no layout pass
- a `deps()` subtree rebuild, for anything structural or textual — text
  content has no property writer, and a value that gates a style branch
  cannot be patched
- `computed { … } : T`, a derived that fires when any signal it reads
  changes

A prop reads as a binding handle (`Fsm.field`) or as a value
(`Fsm.field.get()`), and the two are not interchangeable: only the
second needs a re-render, which is what a decorated component subscribes
to.

**Signals.** Declaration with an initial value (`signal name: T =
literal`), the four bridged types, and two-way binding for the widgets
that own their state. A declared default applies when the signal is
first minted, so it never overwrites a live value.

**FSMs.** `context` blocks, self transitions, event dispatch, and
`@stateful @fsm([X])` binding a component to the fields it reads.

## In progress

**Reactive props across the cn surface.** Button, Progress, Skeleton,
Separator, Badge, Label, Alert, Input, Textarea, Switch and Checkbox
accept bound values. The rest of the cn widgets still take plain values.

Two rules have to hold for every widget converted:

- content rebuilt inside a `Stateful` sets its own visual props. The
  stylesheet pass has already walked the tree by the time a callback
  builds its content, so it cannot inherit a colour from an ancestor's
  class.
- a wrapper introduced around rebuilt content must align where the node
  it replaces would have. `w_fit` / `h_fit` set `align_self: Start`,
  which overrides the parent's `align-items`.

## Next

**Hot reload for `.blinc` files.** Edit a file and see it in the running
app, so widget and CSS iteration costs seconds rather than a rebuild.

Zyntax's `hot_reload` is not the mechanism: it swaps one function's
machine code by id and cannot add or remove declarations, while a
`.blinc` edit usually moves signals, components, FSMs and the view
together. Whole-instance recompile is the path, and three properties
that make it viable are already true and pinned by tests: a live
instance can compile again, the recompile swaps what renders, and signal
values outlive the instance that declared them.

1. *The loop.* Recompile on file change, keeping the previous program if
   the new one fails to parse. The app already has a recursive file
   watcher, an invalidation queue, and a wake path for a window parked
   in `ControlFlow::Wait`.
2. *Idempotent recompiles.* Declared signals, declared FSMs and compiled
   stylesheets accumulate per compile, so a reload duplicates them and
   keeps stale entries. They need replacing per compile, and an edited
   `style { }` block must replace its previous sheet rather than append
   a second copy. Open: whether a deleted component leaves the registry,
   and whether re-registering an FSM keeps its current state.
3. *State preservation.* Signals already survive. Scroll offsets, focus
   and keyed text buffers need checking.
4. *Ergonomics.* An entry point that wires watching and recompiling, and
   parse errors surfaced in-window rather than only in the log.

**Scoped `@stateful`.** A decorated component mounts one `Stateful` at
the view root, so any transition re-renders the whole program. Anything
keyed on node identity survives only because of stable ids. The runtime
half exists — rendering a single component's view — but a component has
to mount its own `Stateful` at its call site, and the attempt so far
stalls on call-site key injection ordering and on re-entering the JIT
during the first render.

## Later

**Module system.** Export lists, a manifest, and a watcher story that
composes with hot reload.

**Router.** Route declarations, params, and nested outlets.

**Standard library.** Formatting, collections and math beyond what the
view body needs today.
