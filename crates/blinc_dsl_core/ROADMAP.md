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

**Styling any widget.** `class = "a b"` on a `cn.*` or core widget, on
top of whatever classes the widget carries itself, so a `.blinc` rule
reaches a cn widget the same way it reaches a `Div`. Every extern
widget takes it; the arg used to parse and then be dropped, which meant
the call compiled and the rule silently did nothing.

**Hot reload for `.blinc` files.** Edit a file and see it in the running
app, so widget and CSS iteration costs seconds rather than a rebuild.
All four parts below are done, and both the single-file and the
imported-module paths are pinned by tests.

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
  which used to override the parent's `align-items`. The layout tree now
  marks that value incidental and lets a parent's stated `align_items`
  outrank it, so this is no longer something each widget has to
  remember. An `align_self` an author wrote still wins, as CSS
  promises.

**cn coverage.** 52 of cn's widgets have a DSL binding. Eight do not,
and they are the ones left:

| Widget | Lines | Shape it needs |
| --- | --- | --- |
| `DropdownMenu` | 1002 | Items as children, overlay anchored to a trigger. The overlay half already exists — Popover, Tooltip and HoverCard all use the signal-as-handle watcher. |
| `ContextMenu` | 955 | `DropdownMenu` opened by right-click instead of a trigger. |
| `Menubar` | 1100 | A row of `DropdownMenu`s sharing which one is open. |
| `NavigationMenu` | 823 | Menubar with panels rather than item lists. |
| `Table` | 262 | Smallest file, but the only one that genuinely needs a list of structs: a row is not a scalar, and the collection type stops at `Vec<T>` for scalar `T`. |
| `Tree` | 726 | Recursive children. Nothing else in the DSL nests a widget in itself yet. |
| `Toast` | 519 | Queue plus timing, and a known exit-motion race on web. No trigger-shaped anchor, so it needs a host-side entry point rather than a view-body call. |
| `Chart` | 1861 | The biggest single surface in cn, and needs the list-of-structs type as much as `Table` does. Last. |

`ToggleGroup`, `Breadcrumb`, `Select`, `Combobox` and `Pagination` are
done from this family, by three different routes: `Breadcrumb` waited for the collection type and
takes `items = [...]`, `ToggleGroup`, `Select` and `Combobox`
needed nothing new because their options are children; and `Pagination`
has no options at all, deriving its numbers from a total.

`Pagination` did need one thing, and it was a TYPE rather than a shape:
its builder wants `State<usize>` and the DSL has no usize signal, so
`PageValue` was added to accept either that or a `NumberValue` and round
at the boundary. Converting instead would have minted a second signal id
and left the binding writing to a copy. Expect the same wherever a cn
widget's state type has no DSL equivalent — add an accepting enum rather
than narrowing at the call site. Try the children shape first —
most of the ten above are option lists, and only `Table` and `Chart`
clearly need a row type.

`Select`'s option child is `cn.Option`, not `cn.SelectItem`: a combobox
offers the same value/label/disabled choice, and one widget per parent
would differ only in which bracket it sits inside. It renders standalone
rather than vanishing outside a parent. `Combobox` reuses it unchanged, which is
the case it was named for. Reach for a new child type only where the
choice genuinely differs — a menu item is a command, not a value, so
that family will need its own.

**Reactivity through the language, not around it.** *The priority.*
Signals, FSMs and components each live in a process-global registry
keyed by a string, and dependency sets are computed by a pass that walks
the AST guessing what a view reads. Zyntax has symbols, algebraic
effects and fibers; the DSL reaches around all three. Every symptom is
that one decision:

- `signal page` in two modules is ONE signal. It surfaced in the
  playground: `main.blinc` owns `page` for which tab the sidebar shows,
  a pagination demo declared `signal page` for its page number, and
  clicking a page navigated the app. Nothing warns — the second
  declaration finds the first and adopts it.
- `signal page` in two COMPONENTS of one file is also one signal, and
  that is likelier: components in a file are written together and reuse
  obvious names.
- FSM state enums are already mangled per module, whose own comment says
  same-named cross-file FSMs would otherwise collide. An admission that
  name keys collide, patched for one case.
- The dependency pass guesses because reads are not observable. A
  resolved symbol, or an effect, would say.

Mangling is not the fix: it leaves one global registry with longer keys,
addressable from anywhere, merely harder to hit by accident.

### The design

**Two jobs, and only one of them is already solved.** Keep them apart —
conflating them once already produced a wrong conclusion.

*Observing* which signals a region read does NOT need effects. A `with`
region opens a read scope, reads record at the getter externs, and the
accumulated set is the dependency set. There is one interception with
one behaviour, so a `perform` adds nothing. Measured in
`tests/observed_deps.rs`.

*Scoping* is what effects are for, and it is the harder half. A handler
installed for a dynamic extent decides what a name RESOLVES TO inside
it. That is the mechanism that makes two components able to each declare
`page` without colliding: not longer names, but no shared string space
to collide in — the read is a `perform`, and which signal it reaches is
whatever handler the enclosing scope installed. `with H { … }` is the
extent, and the backends already carry it (`pending_with_scopes`,
`lower_with_scopes`).

So the read stays observed as it is today; what changes is that a signal
DECLARATION installs a handler for its scope rather than minting into a
process-global registry, and a read resolves through the handler stack
rather than through a baked global id.

**Attempted and reverted, twice — read this before trying again.**
Qualifying a signal's registry key by module was built and taken back
out (ecd9a3b9, bdb2110f, reverted in b2e3cd7c). Both failures came from
the same place: a signal resolves BY NAME in five separate sites, and
the answer to "which module is this name in?" is not available at all of
them.

- Keying only the mint site left bound props, styling args and both dep
  lists resolving the bare name. A write went to `mod$$x` while the
  widget bound to it had resolved `x`, so every bound control in an
  imported module went dead. Shipped, and found by clicking, not by
  tests.
- Routing all five through a thread-local module context fixed compile
  time and broke render time: `with` region deps, `@stateful` deps and
  host lookups run long after every module has compiled, and the
  thread-local then holds whichever module compiled LAST. Two overlays
  in different modules resolved to each other's signal and opened
  together.

The shape that would work: resolve a signal's key ONCE, at declaration,
where the module is known — and carry the resolved ID to every consumer
instead of the name. Render-time name lookup has no module and cannot
acquire one, so any design that still looks a signal up by name after
compilation will fail the same way a third time.

**What the tests missed, which is the more useful lesson.** Every test
written for this asserted on the registry: minted here, looked up there.
All of them passed through both regressions, because all five resolution
paths were invisible to that shape of assertion. The test that would
have caught it is a module signal BOUND TO A WIDGET and then written —
the thing an author actually does. Assert on behaviour through a
binding, not on registry contents.

**What scoping does to the dependency set.** Observed deps are a
`Vec<SignalId>` of process-global raw ids, and every consumer — a
`Stateful`'s `deps()`, `check_stateful_deps`, the ~74 host call sites —
assumes a name or an id means the same thing everywhere. Scoping breaks
that assumption, and there are two readings:

- *Resolution changes, identity does not.* A read performs, the handler
  resolves it to a concrete signal INSTANCE, and that instance has an
  id. Two components' `page` resolve to two different instances with two
  different ids. Dep lists stay lists of ids and nothing downstream
  moves. This is what `EffectTypeId` naming the OPERATION while the
  handler decides who answers actually buys.
- *Identity changes too.* A signal is meaningful only relative to a
  handler stack, and there is no stable id to put in a dep list at all.

Take the first. It keeps the reactive substrate intact and confines the
change to declaration and read. But it carries one requirement that has
to hold or the whole thing churns: **a handler must resolve to the SAME
instance across renders.** Resolve freshly each time and the dep set
changes every frame, so the `Stateful` re-subscribes to new ids
continuously and nothing ever matches a write.

**And the API already makes that a choice rather than a hazard.**
`host_fiber_api.rs`'s `a_bound_handler_carries_state_across_steps` shows
both modes side by side:

- `resume_fiber_within(token, &["Seq"])` opens a fresh handler scope per
  step, so handler state is reconstructed each time — the machine
  observes 1, 1, 1.
- `get_handler("Seq")` resolves a handler token ONCE and
  `bind_fiber_handler(token, handler)` binds it for the machine's
  lifetime — 1, 2, 3.

A component's signal scope is the bound case: resolved when the
component mounts, carried across every render, so the instances a read
resolves to are stable and a dep list built from them does not churn.
Per-step is there for when fresh state is what you want. The failure I
was worried about is the one you get by choosing the wrong mode, not one
lying in wait.

The same file also pins that a partial handler install unwinds —
`resume_fiber_within(token, &["Feed", "NoSuchHandler"])` fails with the
handler stack at the depth it started, and the machine still drivable.
`handler_stack_depth()` is observable, and `drop_fiber` exists, so a
component unmounting has a defined way to let go.

It also changes what a question means. "What is signal `page`" stops
having an answer; the answerable question is "what is `page` IN THIS
SCOPE". That applies to the host, to cross-module reads, and to tests —
`tests/observed_deps.rs` currently resolves ids with a global
`signal::lookup(name)`, which is the very mechanism being removed, so
that helper becomes "resolve in scope" or the tests lose their footing.

Fibers do the same job for FSM state: a machine's state lives in the
fiber, the host holds a token in component state, and there is no
registry entry to collide with another component's machine. Scope by
construction rather than by convention. AEL's `replay` and
`reversibility` modes sit on the same descriptor and are the first
credible answer this design has had for undo.

**An FSM is an `@effect(E) fiber def`.** Calling one constructs a fiber;
`yield` is a suspension point, and suspending IS waiting in a state. An
event resumes it. The current state stops being an enum a registry
tracks and becomes the fiber's program counter; transitions become
ordinary control flow rather than a table plus a host dispatch function.

**Identity is the symbol, scope is the block.** `SymbolTable` resolves
names to `HirId`s per compilation unit, so the string stops being what
two declarations agree on at runtime. Scope follows blocks, which the
backends already support, so a component body scopes its own signals.

**Hot reload rides OSR rather than rebuilding.** Today a reload builds a
fresh instance and swaps it wholesale — which is exactly why values must
survive in a global registry. That registry IS the state preservation,
so it cannot be removed until the reload stops needing it.

### Why this is adaptation, not research

`zyntax/crates/zynml/tests/hot_reload_effect_fibers.rs` runs the whole
composition and calls it "the observing FSM": each transition performs
an effect to read an event, folds it into loop-carried state, and
yields, with the handler installed by `with` around the pump loop. Under
`enable_osr` + `enable_hot_reload` it proves, in one test, that editing
a SUSPENDED machine preserves its state, its handler-stack segment and
its dispatch path together. A companion test edits the event SOURCE
under a running machine: the machine never reloads, and from its next
perform it observes the edited events. Both proofs rest on a count
landing strictly between the all-old and all-new extremes, which is only
reachable if state crossed the edit.

So both halves of an FSM reload independently while suspended — what it
does, and what it responds to. That is more than Blinc manages today
with a full instance rebuild.

### Order, and the one blocker

1. ~~Signal reads as effect operations at one boundary.~~ **Already
   done, and effects are not the mechanism.** `read_scope` accumulates
   reads at the `__signal_get_by_id_*` choke point and `with_regions::
   mount` prefers that set over the registered one. `host.rs` argues the
   case where the interception happens: a `perform` exists so a handler
   can intercept, and there is exactly one interception with one
   behaviour, so the indirection buys nothing here.

   Measured rather than assumed (`tests/observed_deps.rs`): a region
   reading one of two signals subscribes to exactly that one. The
   distinction that decides whether observation happens at all is
   BINDING vs READ — `Text("{x}")` hands the widget a handle and the JIT
   body never reads, so such a region observes nothing and falls back to
   the registered set, correctly. Control flow forces a real read and
   the set comes back exact.

   So the AST-walking guess is already gone for `with` regions. What
   remains of that complaint is `@stateful` views, which still take the
   marker-arg path in `detect_and_strip_stateful_views` and subscribe to
   ALL declared signals when no deps are named. Pointing those at a read
   scope is the actual step, and it is smaller than what was written
   here.

   None of this displaces effects from the plan: they are how SCOPING
   works, not how observation works, and scoping is still ahead. A
   declaration installs a handler for its extent; a read resolves
   through the handler stack. That is the step that makes two
   components' `page` two different signals.
2. **An export mechanism.** THE BLOCKER, and it must land before
   scoping. `blinc_runtime::signal::set_str("page", …)` is how Rust
   drives a `.blinc` program, across ~74 call sites, and it works only
   because nothing is private. Scoping without an export takes the
   host's grip away.
3. **Signals become scoped symbols.** The breaking change, deliberately
   third: by then the effect path is proven and the host has a supported
   way to reach in.
4. **FSMs become fibers**, once effects carry their events. Nothing is
   unknown on the Zyntax side: `zynml/tests/host_fiber_api.rs` is the
   host-driven contract, and its header describes this use case — "a
   framework constructs a machine from a compiled `fiber def`, holds a
   token, and steps it on its own schedule".

   ```rust
   let token = rt.get_fiber("machine")?;        // per mounted FSM
   match rt.resume_fiber(token)? {              // once per event
       HostFiberStep::Yielded(v) => /* v drives the re-render */,
       HostFiberStep::Done => /* machine finished */,
   }
   ```

   `resume_fiber` takes NO value. Events do not go in through resume —
   they arrive through the effect handler the host installs around each
   step, which is the host equivalent of a source-level `with Feed { … }`
   and is what a native handler backed by the winit queue plugs into.
   Stepping past completion stays `Done`.

   The two edges an edit creates surface as values rather than traps,
   which is what lets a token live in component state:

   - the machine's function is DELETED — the next resume fails as a
     value, the test calls it "the UI's cue to drop and remount", and
     dropping the fiber still works
   - the yield SHAPE changes — the handle carries a generation and
     `fiber_info` reports staleness, so a stale token is detected rather
     than misread. A running fiber keeps yielding the shape its handle
     was created against; one created after the edit carries the new
     generation.

   So the Blinc-side work is only: a pointer event as an effect
   operation, a token in component state, and `Done`-or-failure meaning
   remount.
5. **Hot reload switches to OSR reload**, last, because it is the step
   that lets the global registry finally go.

Steps 1 and 2 are independent and can go in parallel. Nothing before
step 5 removes the registry, so each earlier step is reversible.

## Next

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

That reordered the spike: before asking what a handler costs, ask
whether one runs.

1. **Answered: it runs.** `effect_execution_tests.rs` JITs a module and
   calls it, and a `perform` dispatches to its handler and returns the
   handler's value. Two things came out of writing it. Tier 1 lowers a
   perform to a direct call to `<Handler>$<op>`, so the handler BODY has
   to exist as an ordinary function under that mangled name — the
   `HirEffectHandler` entry declares the dispatch, it does not supply
   the code. And even a purely static handler needs the effect-runtime
   symbols registered, because regional dispatch lowers the site to
   `select(lookup_op(..) != 0, dyn, static)`; without them
   `finalize_definitions` panics on the unresolved relocation rather
   than failing the compile, which reads as a Cranelift crash rather
   than a missing host dependency.

   What is NOT proven: resumable handlers (Tier 3 pads the call with a
   `Resume<T>` sentinel) and `CaptureContinuation`, which the backend
   logs as unimplemented. So the non-resumable, non-capturing path is
   the only one with evidence behind it.
2. **Answered: a perform is about a call.** Release build, 2M
   iterations: direct call 1.57ns, perform 2.64ns, `HashMap` lookup
   9.89ns. So the dispatch check costs ~1.7x a direct call and ~0.27x
   what a signal read costs today. Cost is not the objection to a
   handler per read.

   Measured with the loop in Rust, not in the JIT: both variants pay an
   identical Rust->native call per iteration, so the difference isolates
   the perform overhead. Run it in RELEASE — the perform-vs-call ratio
   holds either way, but a debug `HashMap` is ~20x slower than it should
   be and makes a perform look far cheaper against a signal read than it
   is.
3. **Answered: a handler can delegate to host state.** JIT'd code
   performs two reads, a host-side handler records both ids and returns
   their values, and the computation sums what it got back. That is the
   shape a reactive read takes: the handler sees every read in its
   scope, exactly, instead of the write side inferring it — which is
   what `check_stateful_deps` does today.

**Continuations are not needed for the read case**, which was the open
risk after (1). A read handler is TAIL-RESUMPTIVE: `perform read(id)`
lowers to a plain call that returns the value and lets execution
continue, and that is "resume with a value" without capturing anything.
So the read path sits entirely on the non-resumable lowering, the only
one with evidence behind it. `Resume<T>` and `CaptureContinuation`
would only come up for a handler that wants to run the body more than
once or abandon it — neither of which a dependency-tracking read does.

All three questions are answered, and none of them is the reason not to
do this. **The design is written up in `docs/effects-reactivity.md`.**

In short: `Reactive` declares one read operation per bridged signal
type; a scope is one render of one region, opened in an argument of the
`with` call site so it precedes the region's view and closes when the
`Stateful` is mounted with exactly the ids that were read; scopes stack
and the innermost records; a read with no scope open falls to a static
handler that reads and records nothing, so everything outside a region
behaves as it does today.

What it removes: the AST scrape for context-field value reads, dep
lists as a correctness requirement, and the rule that deps must exclude
bound props — a bound prop is passed as a handle and never read, so it
is never recorded. It also makes one hazard unreachable rather than
merely avoided: a closure built during a render but invoked later finds
no scope open and cannot pollute the render's dep set.

What it costs: little. Subscriptions are re-registered per render rather
than at mount, because an exact dep set changes with the branch taken —
but `Stateful` already registers after its callback runs, and the
registry already tolerates re-entry from a refresh, so this is one hash
insert per region render on a path that exists. Writes stay as they are
for now.


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
| ✅ | Expose a container-shaped widget | Done. Dialog, Popover, Tooltip, Sheet, Drawer, Collapsible, Accordion, ScrollArea, AspectRatio, Toggle. Three shapes emerged: bound state, body-as-trigger, and named slots; the overlay three share a signal-as-handle watcher. |
| M | `while` with children | The child list belongs to the entry block and a later block cannot use it. A lowering change, not a widget change. |
| ✅ | A collection type across the FFI | Done. `[a, b, c]` literals, `xs[i]` indexing, `Vec<T>` props for String / bool / i32 / i64 / f64, and `cn.Breadcrumb` as the first consumer. A list of structs still needs the element layout. |
| M | Module system | Export lists and a manifest. Composes with hot reload, so worth doing after it. |
| L | The eight cn widgets with no DSL binding | See "cn coverage" above. The menu family next, Chart last. |
| L | Scoped `@stateful` | One `@stateful` anywhere rebuilds the whole program on every signal write, which also kills in-flight animation. Blocked on the `Root$view` wall — see the section above. `with` blocks now deliver most of the benefit without it, so this is only worth doing for the case where the state behaviour deserves a name. |
| L | Router | Route declarations, params, nested outlets, and how a route change interacts with subtree rebuilds. |
| L | Standard library | Open-ended by nature; scope it against what view bodies actually reach for. |

**Suggested order.** Hot reload, the four exposed-but-static widgets,
the collection type and `with` blocks are done, as is styling any widget
by class. Of the eight cn widgets still unbound, the menu
family shares one overlay shape and can go together; `Table` and `Chart` should wait for a
list of structs, and everything else can land one at a time using the
option-as-child shape. `while` with children and scoped `@stateful` are
independent and can slot in whenever the compiler work is worth the
context switch.
