# Zyntax: loop and array codegen gaps blocking the DSL `for` loop

**Status:** open, unstarted.
**Owner:** Zyntax (`/Users/amaterasu/Vibranium/zyntax`).
**Blocks:** `for x in xs` in the Blinc DSL, the one unshipped P0 in the
language feature table. List rendering is unavailable without it.

Acceptance is mechanical: four `#[ignore]`d tests in
`crates/blinc_dsl_core/tests/loop_substrate_gaps.rs` assert the intended
behaviour today. Remove the `#[ignore]` attributes and they should pass.

## Background: why `for` is not a grammar change

The Blinc grammar can parse `for x in xs { … }` in an afternoon. It
deliberately does not, and the recorded reason is
`TypedStatement::For`: `compiler/src/cfg.rs::process_for_loop` builds an
init → header → body/exit skeleton and **emits no instructions at all**.
Both `pattern` and `iterator` are unused parameters. There is no
iterator init, no has-next test, no element bind, no advance, so the
header's `True`/`False` edges branch on nothing.

Blinc's intended workaround was to never reach that code: desugar `for`
into the `while` that already works, in a Blinc-side pass. A `while` in
a view body does emit one child per iteration, so the shape is right.

That workaround is blocked by the gaps below, which sit underneath
`for`, are reachable from plain `while`, and are not caused by `for` in
any way. Fixing `process_for_loop` alone would not unblock the feature;
fixing these four would unblock it even if `process_for_loop` stays as
it is.

## Gap 1: a local does not carry across loop iterations

The important one. A counter incremented in a loop body does not keep
its value into the next iteration, which is precisely what a desugared
`for` needs.

```blinc
signal gc: i32 = 0
let i = 0
while gc.get() < 3 { i = i + 1  gc.set(gc.get() + 1) }
if i == 3 { Text("advanced") }   // never renders; i is not 3
```

The loop is bounded by a signal so it terminates regardless; the
trailing `if` reports whether the local kept up. It does not.

Reads as missing phi nodes for loop-carried values at the loop header.
The signal-driven counter works, which is why every loop in the Blinc
test corpus uses one.

Test: `a_local_carries_across_loop_iterations`.

## Gap 2: a local read from its literal initialiser is wrong

Independent of loops, and strange enough to be worth stating as a pair:

```blinc
let i = 3       i = i + 3
if i == 3 { … }                 vs      if i == 3 { … }
// does NOT render                      // DOES render (from let i = 0)
```

A local read straight from its initialiser compares false. The same
local, written by a reassignment first, compares true. The initialiser
appears not to be materialised into the slot the later read uses;
whatever the read sees is only correct once an explicit store has
happened.

This alone would break a desugared `for`, whose counter starts life as
`let i = 0`.

Tests: `a_local_read_from_its_initialiser_is_wrong` (the gap) and
`a_local_reads_correctly_after_a_store` (the contrast, passing today).

## Gap 3: indexing an array SIGSEGVs

```blinc
let xs = ["a", "b", "c"]
Text(xs[0])          // SIGSEGV, takes the test binary with it
```

Array literals lower to `TypedExpression::Array`, which the compiler
turns into `List<T> { data, len, capacity }`, and Blinc has tests
pinning that AST shape. Nothing pinned execution, and indexing faults.

Because it is a segfault rather than a panic, this test is `#[ignore]`d
harder than the others: running it kills the whole binary.

Test: `an_array_can_be_indexed`.

## Gap 4: an array has no length

```blinc
let xs = ["a", "b"]
if xs.len() == 2 { … }
```

fails to resolve. Worth flagging for whoever picks this up: the
diagnostic names the **enclosing** function, not the missing method:

```
Call to undefined function 'C$view'. (59 known functions, 1 known externs.)
```

That is the standard failure mode for an unresolved call in this
pipeline — the enclosing function is dropped, and the error names its
caller. Expect to lose time to it if the real cause is not already
suspected. A better diagnostic here would pay for itself.

Test: `an_array_has_a_length`.

## What "done" looks like

With gaps 1 and 2 fixed, Blinc can ship `for i in 0..n` as a pure
desugar to `while`, no Zyntax `for` support required. That covers
index-driven list rendering, which is the majority of the use case.

With 3 and 4 also fixed, `for x in xs` over a collection works, which is
the full feature.

Fixing `process_for_loop` to emit a real iterator protocol is the
cleaner long-term answer and would let Blinc drop the desugar entirely.
It is not required for either milestone above, and it is strictly more
work, so it should be scheduled on its own merits rather than as a
prerequisite.

## Coordination

Blinc pins Zyntax by rev in `crates/blinc_dsl_core/Cargo.toml`,
`crates/blinc_runtime/Cargo.toml`, and `crates/blinc_cn_dsl/Cargo.toml`
— currently `48428c6ef04c735b98316d82b5a494b0479e03d8`. Push the fix,
then bump all six occurrences together; a partial bump resolves two
copies of the crate and fails with type mismatches.

Do not add a local `[patch]` to the committed `Cargo.toml`: CI's
`Guard (no local patch)` job fails on it, and the paths it points at are
gitignored so fresh clones break. Use `scripts/use-local-packages.sh`
while iterating and revert before committing.
