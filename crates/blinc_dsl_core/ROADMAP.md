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

**Hot reload for `.blinc` files** — *the loop and state preservation are
done; recompile hygiene turned out to be moot.* Edit a file and see it in the running
app, so widget and CSS iteration costs seconds rather than a rebuild.

Zyntax's `hot_reload` is not the mechanism: it swaps one function's
machine code by id and cannot add or remove declarations, while a
`.blinc` edit usually moves signals, components, FSMs and the view
together. Whole-instance recompile is the path, and three properties
that make it viable are already true and pinned by tests: a live
instance can compile again, the recompile swaps what renders, and signal
values outlive the instance that declared them.

1. *The loop.* **Done.** A reload builds a fresh instance and the host
   swaps it in; a failure keeps the running one. Compiling into a live
   runtime is not an option: it re-runs the changed module, but the
   entry's call to `<Module>$view` keeps binding to the symbol
   registered first, so only entry-file edits ever appeared to reload.
2. *Idempotent recompiles.* **Moot.** A fresh instance starts with empty
   accumulators, so there is nothing to deduplicate. The registry
   questions go with it.
3. *State preservation.* **Done for signals**, which is what an editing
   session mostly cares about. The reload now goes through the same
   incremental update every other frame does rather than throwing the
   tree away, so scroll offsets, focus and node identity survive too.
   A stylesheet edit is the one thing the diff can't see — it changes
   no element hash — so a reload forces one stylesheet pass.
4. *Ergonomics.* **Mostly done.** `watch_sources` settles a save on the
   watcher thread and raises one flag, so a host drains a flag and
   recompiles instead of hand-rolling debounce and retry. Still open:
   parse errors surfaced in-window rather than only in the log.

Two bugs this shook out, both worth knowing about outside hot reload:
queued subtree rebuilds outlive the tree that queued them, since a full
build restarts the layout slotmap and hands the same ids out again (now
dropped by build epoch); and mixed int/float arithmetic reached
Cranelift with an integer operand under a float instruction, which the
verifier rejected, silently dropping the function.

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

## Sizing

Ordered smallest first. Sizes are relative: S is a sitting, M is a day
or two, L is a week or more and usually hides a design question.

| Size | Item | Notes |
| --- | --- | --- |
| ✅ | Hot reload, the loop | Done. Fresh instance per reload, swapped in by the host. |
| ✅ | Hot reload, state checks | Done. Signals survive, and the reload updates the tree incrementally rather than replacing it. |
| S | Hot reload, ergonomics | `watch_sources` settles the save and raises one flag. Parse errors in-window still open. |
| S | Reactive props on the four exposed-but-static widgets | Card, Kbd, Spinner are mechanical. Avatar needs its content enum to carry a boxed builder. |
| S each | Expose a container-shaped widget | Dialog, Popover, Tooltip, Sheet, Drawer, Collapsible, Accordion, ScrollArea, AspectRatio, Toggle. Children blocks and named slots already work, so these are wrappers plus scalars. |
| M | `while` with children | The child list belongs to the entry block and a later block cannot use it. A lowering change, not a widget change. |
| M | A collection type across the FFI | The gate on roughly half the remaining surface. Props today are String, i32, i64, f64, `Reactive<T>` or a non-generic custom type; a list of items is not expressible. |
| M | Module system | Export lists and a manifest. Composes with hot reload, so worth doing after it. |
| L | Item-driven widgets | Select, Combobox, DropdownMenu, Menubar, ContextMenu, NavigationMenu, Breadcrumb, Pagination, ToggleGroup, Table, Tree, Chart. Each is large on its own and every one waits on the collection type. Chart is the biggest single surface in cn. |
| L | Scoped `@stateful` | A decorated component must mount its own `Stateful` at its call site. Two attempts stalled, on call-site key injection ordering and on re-entering the JIT during the first render. |
| L | Router | Route declarations, params, nested outlets, and how a route change interacts with subtree rebuilds. |
| L | Standard library | Open-ended by nature; scope it against what view bodies actually reach for. |

**Suggested order.** Hot reload first: it is the only item that makes
every later item cheaper to test, and three of its four phases are S.
Then the four exposed-but-static widgets, then the collection type,
since it gates more of the remaining surface than anything else. `while`
with children and scoped `@stateful` are independent and can slot in
whenever the compiler work is worth the context switch.
