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

**Symbols instead of name-keyed registries.** *The priority.* Signals,
FSMs and components each live in a process-global registry keyed by a
string. Zyntax has symbols — with identity and scope, resolved by the
compiler, in both the Cranelift and LLVM backends — and the DSL reaches
around all of it to look things up by name at runtime.

Every symptom below is the same mistake wearing different clothes:

- `signal page` in two modules is ONE signal. It surfaced in the
  playground: `main.blinc` owns `page` for which tab the sidebar shows,
  a pagination demo declared `signal page` for its page number, and
  clicking a page navigated the app. Nothing warns — the second
  declaration finds the first and adopts it, and behaviour is the only
  report.
- `signal page` in two COMPONENTS of one module is also one signal, and
  that is likelier: a file's components are written together and reuse
  obvious names. A module is only the coarsest scope; if scoping follows
  blocks, a component body is one too.
- FSMs have their own global registry with the same shape. State enums
  are already mangled per module by `apply_module_namespace_prefix`,
  whose comment says same-named cross-file FSMs would otherwise collide
  — an admission that name keys collide, patched for one case.
- Dependency sets are computed by a pass that WALKS THE AST GUESSING
  what a view reads, because reads are not tracked. A resolved symbol
  would say.

Mangling is not the fix. Adding signals to the module-namespace pass
stops the accidental collision but leaves one global registry with
longer keys: a signal stays addressable from anywhere, merely harder to
hit by accident.

What changes when symbols carry it instead: a signal belongs to the
scope that declares it, two scopes cannot collide because there is no
shared string space to collide in, and the compiler resolves a reference
rather than the runtime matching a name. Sharing inverts from the
default to a request.

**The one thing that must land with it.** `blinc_runtime::signal::
set_str("page", …)` is how Rust drives a `.blinc` program, and ~74 call
sites pass bare names. That works today only because nothing is private.
Scoping needs an export — a deliberate way to make a symbol reachable
from outside — or the host loses its grip on the program. Design it
alongside, not after.

**What is actually there (verified).** Zyntax's `SymbolTable` in
`compiler/lowering` holds functions, globals, types, effects and
handlers, each an `IndexMap` from `InternedString` to a `HirId` /
`TypeId`. So the table is name-keyed too — but per compilation unit, and
resolved BY THE COMPILER to an id. That is the difference that matters:
identity becomes the `HirId`, and the string stops being what two
declarations agree on at runtime. It also has `with`-scopes for effect
handlers (`pending_with_scopes`, `lower_with_scopes`) and block scoping
for locals in both backends.

**Still to explore before this is a plan.** How a signal declaration
should appear in that table (a global? an effect?); whether component
bodies can introduce a scope in the table or only inside a function;
what the FSM registry keys on and whether it can hold ids; how a
resolved id survives hot reload, which today relies on values outliving
an instance in a name-keyed registry; and what an export looks like so
the ~74 host call sites keep a grip. None of that is settled here, and
guessing it would produce the kind of plan that reads well and does not
survive contact.

**How AEL does it, as a worked precedent.** The sibling project drives
Zyntax effects through a public handler API rather than a name registry,
and the shape maps onto signals almost directly:

- An `EffectHandlerDescriptor` declares the operation: `effect_type:
  EffectTypeId` — an ID, not a string — plus `operation`,
  `parameter_types`, `result_type`, an `abi_version`, and `replay` /
  `reversibility` modes.
- `NativeEffectHandler::new(descriptor, closure)` wraps a Rust closure
  taking `&EffectInvocation` and returning `HandlerOutcome::Completed(
  EffectDatum)`.
- `runtime.register_effect_handler(Arc<dyn UserEffectHandler>)` installs
  it, then `runtime.compile_effect(fragment, effect)` compiles the code
  that performs it.

The identity is `EffectTypeId`; the handler is installed for a dynamic
extent rather than looked up by name at the point of use. That is the
whole difference from `mint_or_get(name)`.

For signals this lines up with what the effects section below already
argues: a signal READ as an effect operation means a handler installed
at a reactive boundary observes exactly the reads in that extent — no
AST-walking pass guessing what a view touched. `replay` and
`reversibility` are also suggestive for hot reload and for undo, neither
of which the current design has an answer for.

Read `ael_cli/src/main.rs` around the `register_effect_handler` call and
`ael_effects`'s `EffectHandlerDescriptor` before designing this; they are
short and concrete.

**Fibers are the shape of an FSM.** Zyntax has `fiber def NAME(...)`:
calling one constructs a fiber (`FiberNew`) rather than running the
body, `yield expr` inside it is a suspension point (`FiberYield`, lowered
to `krio_fiber_yield`), and `FiberResume` continues it. An FSM is
precisely a computation suspended between events, so the correspondence
is direct:

- The FSM body becomes a `fiber def`. It runs until it yields, and
  yielding IS waiting in a state.
- An event resumes the fiber, carrying the event as the resume value.
- The current state stops being an enum the host tracks in a registry
  and becomes the fiber's own suspension point — the program counter.
- Transitions become ordinary control flow. A state machine is written
  as straight-line code with yields rather than as a transition table
  plus a dispatch function.

Paired with effects, what the FSM DOES at each step (writing signals,
dispatching) becomes effect operations resolved by whichever handler is
installed. That makes an FSM testable by installing different handlers
rather than by driving the real runtime, and it removes the host
dispatch function the DSL currently reaches for.

**Both of those are already solved upstream**, and there is a test that
is very nearly the UI case:
`zyntax/crates/zynml/tests/hot_reload_effect_fibers.rs`. Its own summary
calls an effectful fiber "the observing FSM": each transition performs
an effect to read an event, folds it into loop-carried state, and
yields; the handler is the machine's event source, installed with `with`
around the pump loop.

The shape, verbatim from that test:

```
effect Event { def next_event(): i64 }
handler Feed for Event { def next_event(): i64 { return 3 } }

@effect(Event)
fiber def machine(): i64 {
    let mut state: i64 = 0
    while state < CAP {
        let e = next_event()
        state = state + e * WEIGHT
        yield state
    }
    return state
}

def drive(): i64 {
    with Feed {
        let f = machine()
        while let Some(x) = f.next() { count = count + 1 }
    }
}
```

Under `TieredConfig` with `enable_osr` and `enable_hot_reload`,
`reload_typed_program` edits `machine` WHILE IT IS SUSPENDED MID-RUN and
the test asserts all three legs survive together: the fiber's state, its
handler-stack segment, and the dispatch path. The proof is a pump count
landing strictly between the all-old and all-new extremes — it could
only do that by carrying state across the edit and then folding events
with the new weight. So a fiber's state does outlive the code that
created it, and the reload does not have to reconstruct it.

**Editing the event source works too.** Op-table patching has landed
upstream (`patch effect dispatch tables in place and surface reload as a
runtime event`), and the tests moved with it: a reload report now
carries `dispatch_patched`, and a fresh perform reaches the edited
handler rather than the body baked at module compile.

The test added alongside it is the one that matters most here — the
event SOURCE is edited under a machine that is mid-run, and the machine
itself never reloads. Its state and handler segment are untouched, and
from its next perform the events it observes carry the edited value.
Again proven by a count landing strictly between the extremes, which
only happens if dispatch retargeted mid-flight.

So both halves reload: an FSM's transitions, and the source feeding it,
independently and while suspended. For a UI that is the whole editing
loop — change what a machine does, or change what it responds to,
without losing where it had got to.

Related: `SymbolTable.fiber_fn_names` already tracks which functions are
fibers, so the plumbing is present rather than hypothetical.

**Zyntax's own hot-swap is the mechanism to drive.** `compiler/src/osr.rs`
exists and `tiered_backend` already does "threads, atomic code-pointer
swap, generations", wiring `osr::osr_runtime_symbols()` into the
Cranelift backend so JIT'd back-edge code can resolve them. Read the
phase comments before relying on it: they say OSR lands in phase 2/3 and
deopt in phase 4, so the swap machinery is further along than the
on-stack replacement itself. Check what is actually complete rather than
taking the module's existence as the feature.

This matters because Blinc's hot reload currently works by building a
FRESH instance and swapping it wholesale, which is why signal values
have to survive in a process-global registry — the registry IS the
state-preservation mechanism. An atomic code-pointer swap with
generations replaces the instance rebuild, and OSR is what would let a
SUSPENDED FIBER carry its state across an edit rather than being
reconstructed. That is the missing piece for FSMs-as-fibers: without it,
an FSM mid-flight is lost on reload; with it, the fiber resumes into new
code.

So the three threads converge, and upstream has already proven the hard
part. Effects give reads an observable boundary, fibers give FSMs their
suspension, and OSR carries both across an edit — demonstrated together
in one test rather than three separate hopes. What is left for Blinc is
mapping its own vocabulary onto it: a pointer event as an effect
operation, the app's event loop as the installed handler, and an FSM
declaration as an `@effect(...) fiber def`.

Related: the algebraic-effects section below argues the same thing about
reactivity, which is not a coincidence. Both come from the DSL treating
Zyntax as a backend to emit into rather than a language with a semantics
to use.

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
