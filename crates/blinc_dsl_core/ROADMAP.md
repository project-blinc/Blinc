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

**Collections.** `[a, b, c]` literals and `xs[i]` indexing, plus
`Vec<T>` props on extern widgets for String / bool / i32 / i64 / f64.
Zyntax lowers a literal to `List<T> { data, len, capacity }`, so a prop
crosses as one pointer and nothing marshals per element. The stride
follows the element type — `bool` is one byte, `i32` four — so the
decoder is selected from the declared `Vec<T>` rather than assumed. A
list of structs needs the element layout and is not supported yet.

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
Separator, Badge, Label, Alert, Input, Textarea, Switch, Checkbox, Kbd
and Avatar accept bound values. Spinner's colours are bindable from
Rust but not from the DSL, where the prop is a hex string and no colour
type crosses the FFI. Card has no scalar props to bind. The rest of the
cn widgets still take plain values.

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
4. *Ergonomics.* **Done.** `watch_sources` settles a save on the watcher
   thread and raises one flag, so a host drains a flag and recompiles
   instead of hand-rolling debounce and retry. A failed compile renders
   as a source snippet — the offending line, a caret, a hint — and
   `error_banner` shows it over the UI until a compile succeeds.

Two bugs this shook out, both worth knowing about outside hot reload:
queued subtree rebuilds outlive the tree that queued them, since a full
build restarts the layout slotmap and hands the same ids out again (now
dropped by build epoch); and mixed int/float arithmetic reached
Cranelift with an integer operand under a float instruction, which the
verifier rejected, silently dropping the function.

**Scoped `@stateful`.** `has_stateful_view` is a single global flag:
ANY view carrying `@stateful` makes `view_widget` wrap the whole
program in one `Stateful` whose `on_state` calls `render_main` — a
re-run of the entire entry view. So one decorated component means every
signal write tears down and rebuilds every node.

That is not just a cost. A rebuilt subtree loses anything mid-flight:
`cn.Switch`'s thumb spring is reconstructed on every toggle, which is
why it jumped in the playground and animates correctly in `cn_demo`,
where nothing rebuilds it. Widgets can defend themselves by persisting
state across rebuilds, and the switch now does, but every animated
widget would have to.

**`with` blocks — the sugar to build first.** A decorated component is
the right tool when there is real state behaviour to name. For the
common case there should be an inline form:

    with @fsm([Play, Change]) {
        if Play.busy.get() { ... } else { ... }
    }

A reactive region placed directly in a parent view, no component
needed. It is also the easier thing to implement, because it has no
existing call site to rewrite — which is exactly what broke the
component-scoped attempt. Desugaring:

- lift the block into a generated view function, `__blinc_with_<n>$view`,
  registered like any component view so `render_component` can call it
  by name;
- emit `__scoped_stateful__("__blinc_with_<n>")` where the block stood,
  with the `@fsm` / `@stateful` lists as its deps.

Nothing about user component lowering changes, so the `Root$view` wall
below does not apply. The generated name is unique per block, so
call-site keying is not needed either — both recorded stalls are
side-stepped rather than solved.

Worth settling while designing it: whether a bare `Play.busy` inside a
`with` should read as a value. Everywhere else the distinction between
`Play.busy` (binding handle) and `Play.busy.get()` (value) is load
bearing, and a block that declares its deps up front is the one place
the shorthand could be unambiguous.

**The shape the fix takes.** A component call lowers to
`<Name>$view(...)`. Wrapping a decorated one as

    __scoped_stateful__("Name", <Name>$view(...))

does the whole job: argument evaluation runs the inner call FIRST, so
the builtin receives an already-rendered widget handle and adopts it as
the `Stateful`'s first content. Later refreshes call
`render_component(renderer, "Name")`, which runs outside any render.
Nothing re-enters the JIT during build, and it is a plain nested call —
no `Block`-as-expression, so neither stall applies to it.

Remaining to build: the `__scoped_stateful__` builtin (mount a
`Stateful` with the component's deps, re-render on refresh), one rewrite
in `lower_component_calls` for names in `stateful_components`, and
gating `view_widget`'s whole-program wrap so it only fires for a
decorated ENTRY view.

An attempt got all four pieces compiling and hit one wall, recorded
here so the next one starts past it:

- **Attribution.** `detect_and_strip_stateful_views` sees a component
  view as an `impl <Component> { fn view() }` — the owner name is
  `"view"`, NOT `"<Component>$view"`. Take the component from
  `imp.for_type` (`Type::Unresolved(name)`) and match the method named
  `view`. Stripping a `$view` suffix from the method name silently
  matches nothing, and the entry-view fallback then swallows every
  decoration.
- **The wall.** With the call-site rewrite active, compiling
  `view { Root() }` fails with *"Call to undefined function
  'Root$view'"* — an UNRELATED component. Replacing the decorated
  component's call appears to stop some later pass from emitting view
  functions for other components. Whatever emits `<Name>$view` needs
  understanding before the rewrite can land; the rewrite itself is
  three lines.
- **Testing it.** "The sibling's node survived" passes vacuously when
  nothing re-renders at all — which is exactly the state the gating
  change produces if the rewrite is inert. Any test here must first
  assert the decorated component DID re-render.

First cut can key the `Stateful` on the component name, which is
sufficient while a decorated component appears once, and leaves
multi-instance keying to the call-id work below.

Two known stalls, and what is now known about each:

1. *Call-site key injection.* A scoped `Stateful` needs a stable key
   per call site. The primitives exist (`__push_call_id__` /
   `__pop_call_id__` / `__pop_call_id_and_return__`) but NOTHING emits
   them — `lower_component_calls` defers the bracket because a
   `Block`-as-expression at the trailing-statement position trips
   Zyntax's SSA value map. A Block-free route: rely on left-to-right
   argument evaluation and emit one call whose first argument does the
   push. `__pop_call_id_and_return__` is already designed on exactly
   that assumption — but since nothing calls it, the assumption is
   unverified. Confirm it with a two-argument host call before building
   on it.
2. *JIT reentrancy on the first render.* `Stateful::on_state` runs
   during build, so a scoped one would call the component's JIT
   function while the parent's render already holds the runtime lock. A
   possible route: let the parent's render produce the first subtree as
   it does today, have the `Stateful` adopt that already-built result,
   and invoke the JIT only on later refreshes.

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
| ✅ | Hot reload, ergonomics | Done. `watch_sources` settles the save; parse errors render as snippets and banner over the UI. |
| ✅ | Reactive props on the four exposed-but-static widgets | Done. Kbd + Avatar bindable from the DSL, Spinner from Rust, Card has no scalar props. |
| S each | Expose a container-shaped widget | Dialog, Popover, Tooltip, Sheet, Drawer, Collapsible, Accordion, ScrollArea, AspectRatio, Toggle. Children blocks and named slots already work, so these are wrappers plus scalars. |
| M | `while` with children | The child list belongs to the entry block and a later block cannot use it. A lowering change, not a widget change. |
| ✅ | A collection type across the FFI | Done. `[a, b, c]` literals, `xs[i]` indexing, `Vec<T>` props for String / bool / i32 / i64 / f64, and `cn.Breadcrumb` as the first consumer. A list of structs still needs the element layout. |
| M | Module system | Export lists and a manifest. Composes with hot reload, so worth doing after it. |
| L | Item-driven widgets | Select, Combobox, DropdownMenu, Menubar, ContextMenu, NavigationMenu, Breadcrumb, Pagination, ToggleGroup, Table, Tree, Chart. Each is large on its own and every one waits on the collection type. Chart is the biggest single surface in cn. |
| M | `with @fsm([…]) { … }` blocks | An inline reactive region: no component, no call site to rewrite, so neither scoped-`@stateful` stall applies. Desugars to a generated view + `__scoped_stateful__`. Do this before the component-scoped form. |
| L | Scoped `@stateful` | One `@stateful` anywhere rebuilds the whole program on every signal write, which also kills in-flight animation. Blocked on the `Root$view` wall — see the section above. `with` blocks deliver most of the benefit without it. |
| L | Router | Route declarations, params, nested outlets, and how a route change interacts with subtree rebuilds. |
| L | Standard library | Open-ended by nature; scope it against what view bodies actually reach for. |

**Suggested order.** Hot reload, the four exposed-but-static widgets and
the collection type are done. The item-driven widgets are unblocked and
can land one at a time. `while`
with children and scoped `@stateful` are independent and can slot in
whenever the compiler work is worth the context switch.
