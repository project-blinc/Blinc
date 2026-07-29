# Reactivity as an algebraic effect

Dependency tracking today is **declared or inferred**. A `Stateful`
carries a `deps` list; the DSL fills it by scraping the view body for
`.get()` calls and mapping those to signal ids. That inference is the
source of a whole class of bugs, because it is a guess about what the
body will read, made without running it.

This design replaces the guess with an observation. A view body runs
under a handler for a `Reactive` effect; every signal read performs an
operation; the handler records the id and returns the value. When the
body finishes, the accumulated set **is** the dependency set — exact,
per render, with nothing to keep in sync.

The three feasibility questions are answered and live as tests in
zyntax's effect-execution suite: a performed operation reaches its
handler, it costs about what a direct call costs (well under a signal
read), and a handler can delegate to host state. What follows is design,
not exploration.

## The operations

`Reactive` declares read operations only, one per bridged signal type,
mirroring the getters the DSL already lowers to:

    read_i32(id) -> i32
    read_i64(id) -> i64
    read_f64(id) -> f64
    read_bool(id) -> bool
    read_string(id) -> ptr

One op per type rather than a single untyped one, because the existing
signal FFI is already split that way and a uniform `i64` payload would
put a cast on both sides of every read.

**Writes stay as they are.** The write path already notifies correctly;
making it an effect would buy batching and transactional semantics, not
correctness. It is the natural second increment, not part of this one.
Deferring it also keeps the first version to a handler that cannot
change observable behaviour: recording is a side effect on host state,
and the value returned is the value that would have been read anyway.

## Where a scope begins and ends

A scope is one render of one reactive region. The regions already exist:
a `with` block's generated view, a decorated component's view, and the
entry view.

The host installs the handler; the region's IR is untouched. This works
because a region is already a separate function called from outside, and
because runtime handler installation is what regional dispatch is for —
a perform site lowers to "if a handler is installed at run time, call
it, otherwise call the static one".

The scope has to open **before** the region's view runs. A `with` site
lowers to a call whose arguments are evaluated left to right, so the
scope opens in an argument:

    __blinc_with__(__blinc_scope_enter__(<id>), <the region's view call>)

`__blinc_scope_enter__` pushes a recording scope and returns the id;
the view renders, and its reads land in that scope; `__blinc_with__`
pops the scope and mounts the `Stateful` with exactly the ids that were
read. The deps are known at the moment they are needed, which is mount.

This uses only argument evaluation order — no block-as-expression, no
re-entering the JIT mid-render. Both are shapes that have already
stalled work here once.

## Nesting: the innermost scope wins

Scopes form a stack, and only the top records. A read inside a nested
region belongs to that region alone, because re-rendering the inner
region is sufficient when the value changes — which is the entire point
of having regions.

This falls out of the stack discipline rather than needing a rule.

## A read with no scope open

The static handler is the fallback: it performs the plain read and
records nothing. So a read outside any region — in an event handler, an
`init` block, a host call — behaves exactly as it does today, plus the
dispatch check, which measured at about a nanosecond.

That makes the interesting case correct by construction rather than by
care: **a closure created during a render but invoked later does not
pollute the render's dep set.** By the time a click handler runs, the
scope it was built under is long popped, so its reads find no scope and
record nothing. Under AST inference this is a known hazard; here it
cannot happen.

## What this removes

- The AST scrape for context-field value reads. Its whole job is
  approximating what the body reads.
- Author-written dep lists as a *correctness* requirement.
  `@stateful([a, b])`, `@fsm([X])` and the bare `with a, b` form stay
  meaningful — an `@fsm` list still selects which FSM's shared state a
  `Stateful` binds — but they stop being how dependencies are found.
- The rule that a `Stateful`'s deps must exclude bound props. A bound
  prop is passed as a handle and never read, so it is never recorded.
  The recorded case where one FSM action wrote five bound fields and
  cost five full re-renders is not reachable: none of those writes was
  ever read as a value inside the region.

## Consequences to design against

**Deps change per render.** A body that reads one signal on one render
and another on the next is correctly tracked, which is the improvement —
but it means the `Stateful`'s subscription must be re-registered on every
render, not just at mount. Today's mount-time registration is a
simplification that only worked because the dep list was static.

**Derived bodies need their own scope.** Reads inside `computed { }`
belong to the derived, not to the enclosing region: the region binds to
the derived, and the derived tracks its own inputs. A nested scope gives
this for free, and matches how the current code already skips computed
bodies when scraping.

**String reads cross as pointers.** `read_string` returning a pointer
puts an ownership question on the perform boundary that the other four
types do not have. Worth settling before the string op is wired, not
after.

**One process-wide handler registry.** Regional dispatch resolves
handlers through a global lookup, so two coexisting DSL instances share
it. That is the same trade the FSM guard dispatcher and the view
renderer already make, and it should be made knowingly rather than
discovered.

## Why this is worth doing

The current design cannot be made correct by trying harder. Inference
approximates what a body reads, and every gap between the approximation
and the truth is either a missed update or a wasted re-render — both of
which have been shipped and fixed here individually. Observation has no
gap to close.

The secondary payoff is that reactivity stops being a mechanism bolted
beside the language and becomes something the language expresses.
