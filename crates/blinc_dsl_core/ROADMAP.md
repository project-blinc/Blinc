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

**`with` blocks. Done.** A decorated component is the right tool when
there is state behaviour worth naming. For the common case there is now
an inline form:

    with @fsm([Play, Change]) {
        if Play.busy.get() { ... } else { ... }
    }

Bare names work too, and are what a region usually wants:

    with count { ... }
    with count, Play { ... }

They are classified at registration against the declared signals and
FSMs, NOT by capitalisation — guessing there would leave a misjudged
region silently subscribed to nothing. A name matching neither warns.

A reactive region placed directly in a parent view, no component
needed, and only that region rebuilds when a listed dependency changes.
The playground's `BusyPanel` was exactly the shape this replaces: a
component that existed only to carry a decorated view, whose decoration
wrapped the whole program.

The lowering lifts the block's body into a synthetic component — a
`Class` plus an inherent `impl { fn view() }`, the same pair the folded
`component` form emits — and rewrites the site to
`__blinc_with__(<id>, __component_call__("__blinc_with_<id>"))`. Two
things fall out of lifting to a real component rather than keeping the
body inline:

- component-call lowering, children expansion and the value-returning
  promotion all key on an impl method named `view`, and NONE of them
  descend into lambda bodies. A body left inline would have needed all
  three taught about a new shape;
- argument order renders the region before the builtin runs, so the
  builtin adopts an already-built widget. `on_state` invokes its
  callback during construction, so a region that re-rendered on its
  first call would re-enter the JIT with the outer view still executing.
  The mounted `Stateful` serves the pre-built handle once and
  re-renders only from the second call on.

Neither recorded stall applies: there is no existing call site to
rewrite, and the id is minted process-wide so it is unique per block and
across recompiles.

A body that does not itself end in a widget call — a bare `if`, a loop
— is wrapped in a container, which both gives the region something to
return and gives the branches a child list to push onto. A body that
already ends in one is left alone, so existing regions keep exactly the
boxes they had.

Two limits worth knowing. A `with` nested inside another `with` does
not lift — the synthetic views are appended after the walk, and an inner
region has no meaning the outer one doesn't already cover. And a region
reads its deps from the decorators alone: a bare `with { }` falls back
to every declared signal, the same as a bare `@stateful`.

Still open: whether a bare `Play.busy` inside a `with` should read as a
value. Everywhere else the distinction between `Play.busy` (binding
handle) and `Play.busy.get()` (value) is load bearing, and a block that
declares its deps up front is the one place the shorthand could be
unambiguous.

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

**Signals and FSMs as algebraic effects.** Zyntax has a real effect
system — `effect_system.rs` carries `EffectHandler` /
`EffectHandlerOperation`, `TypedFunction.with_handlers` wraps a body in
`HandleEffect` at HIR lowering, and there is a whole
`passes/algebraic_effects` crate with continuation and dispatch
support. None of it is used by the DSL, which instead reaches around
the language: signals live in a process-global registry keyed by name,
dependency sets are computed by a pass that walks the AST guessing what
a view reads, and FSM dispatch is a host function.

Effect handlers are dynamically scoped, which is exactly the shape
dependency tracking wants. If a signal READ were an effect operation, a
handler installed at a reactive boundary would observe precisely the
signals a render actually touched — no declared `deps([…])`, no pass
inferring them. That would delete a class of bug this file already
records twice: `stateful_ctx_value_reads` exists only because
subscribing to every declared signal re-rendered the program on
unrelated writes, and the binding-handle vs `.get()` distinction exists
because the pass cannot tell a read from a reference. A handler can.

Likewise a signal WRITE as an operation gives batching for free — the
`batch_stateful_deps` guard around FSM dispatch is a hand-rolled
version of what a handler scope does natively — and an FSM transition
as an operation puts guards, actions and effects under one handler
rather than split between the registry and host callbacks.

**First, a harness.** Checked before writing any of this down: none of
zyntax's three effect test files executes JIT'd code. Not one
`get_function_ptr` between `effect_compilation_tests`,
`effect_emission_tests` and `llvm_effect_parity_tests` — they cover
effect analysis, handler resolution and emission, and stop at the IR.
So the runtime behaviour of a perform/handle round-trip is, as far as
the test suite goes, unverified.

That reorders the spike. Before asking what a handler costs, ask
whether one runs:

1. Build an HIR function that performs an operation under a handler,
   JIT it, call it, assert the returned value. The existing
   `create_simple_effect` / `create_simple_handler` helpers in
   `effect_compilation_tests.rs` give the IR shape; what is missing is
   `compile_function` + `get_function_ptr` + a transmuted call, the
   pattern `cranelift_backend_tests.rs` already uses.
2. Then cost: time that call against the same function with the
   operation inlined, at a call count that matters for a signal read.
   A handler per read is only viable if the overhead is small against a
   `HashMap` lookup, which is what the read costs today.
3. Then composition across the JIT boundary. Less of a risk than it
   first looks: a handler need not BE the reactive boundary, it can
   delegate to whatever `Stateful` is active, or to nothing when there
   is none. `set_stateful_deps_notifier` is already that shape in the
   other direction — a process-global hook the boundary registers and
   compiled code reaches through without knowing `Stateful` exists. A
   read handler accumulates the reads in its scope and hands the set
   over on exit, which is what `check_stateful_deps` receives today,
   except exact instead of inferred.

Whether continuations are needed at all, or only the handler-scope
part, falls out of (1). The payoff if it holds: reactivity stops being
bolted onto the language and starts being expressed in it.


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
| L | Scoped `@stateful` | One `@stateful` anywhere rebuilds the whole program on every signal write, which also kills in-flight animation. Blocked on the `Root$view` wall — see the section above. `with` blocks now deliver most of the benefit without it, so this is only worth doing for the case where the state behaviour deserves a name. |
| L | Router | Route declarations, params, nested outlets, and how a route change interacts with subtree rebuilds. |
| L | Standard library | Open-ended by nature; scope it against what view bodies actually reach for. |

**Suggested order.** Hot reload, the four exposed-but-static widgets,
the collection type and `with` blocks are done. The item-driven widgets
are unblocked and can land one at a time. `while` with children and
scoped `@stateful` are independent and can slot in whenever the
compiler work is worth the context switch.
