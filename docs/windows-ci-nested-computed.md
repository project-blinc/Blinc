# Windows CI: nested `Computed` panics with "RefCell already borrowed"

**Status:** open. Only failing job on `main`; every other job is green.
**Owner:** Blinc (`blinc_core::reactive`). Not a Zyntax issue — see
"Why this is not the DSL" below before routing it.

## Symptom

`Test (windows-latest)` fails; `Test (macos-latest)` and
`Test (ubuntu-latest)` pass on the same commit. Four tests, all in
`blinc_cn_dsl`, all with the same panic:

```
playground_builds_a_subtree_without_deadlocking
busy_still_re_renders
grow_does_not_re_render_the_program
playground_interaction_cost

thread panicked at library\core\src\cell.rs:1431:14:
RefCell already borrowed
```

The run is cancelled on first failure, so the four are one fault, not
four.

## The path

Backtrace, innermost first, trimmed to the frames that matter:

```
RefCell<Option<Vec<SignalId>>>::take
ReactiveGraph::get_derived::<f64>          <- inner, panics here
Computed<f64>::try_get
blinc_layout::binding::bind_f64_computed_as_f32::{closure#0}
Computed<f32>::try_get
Div::bind_transform_from::<f32, Computed<f32>, ...>
cn::progress::Progress::from_config
```

A `Computed<f32>` whose compute closure evaluates a `Computed<f64>`.
That nesting comes from the f64-to-f32 narrowing bridge: the DSL types
every number as f64, layout stores f32, so `bind_f64_computed_as_f32`
wraps the source derived in a second one.

It is reached from the playground's
`cn.Progress(value = computed { Play.pct / 2.0 })`.

## Mechanism

In `ReactiveGraph::get_derived` (`crates/blinc_core/src/reactive.rs`):

1. `self.tracking.replace(Some(Vec::new()))` opens dependency tracking.
2. `(*compute)(self)` runs the closure.
3. `self.tracking.take()` collects what the closure read.

Step 2 re-enters `get_derived` for the inner derived, and that nested
call reaches its own step 3 while the outer tracking cell is live. The
inner `take()` is what panics.

Two things make this worse than a plain re-entrancy bug:

- **The tracking cell is single-slot.** Even without the panic, the
  inner call's `take()` would consume the OUTER call's dependency list,
  so the outer derived would subscribe to whatever the inner read, or to
  nothing. Dependency tracking for nested computed values is wrong
  independently of the crash.
- **The nested call reaches the graph through a raw pointer.**
  `IN_FLIGHT_GRAPH` is set to `self as *const _` around the compute call
  so nested reads take a re-entrant fast path instead of deadlocking on
  the global mutex. The outer frame holds `&mut self` while the inner
  frame derives another mutable path to the same graph. That aliasing is
  why the behaviour is platform-dependent: it is not that Windows is
  special, it is that the layout/codegen there observes the borrow state
  the other targets happen not to.

The `// Note: For now, we don't track derived -> derived dependencies`
comment at the top of `get_derived` is the same gap seen from the other
side: nesting was never supported, and the narrowing bridge introduced
it without the graph being ready.

## Reproduce

```
cargo test -p blinc_cn_dsl --test pg_grow_cost busy_still_re_renders
```

Passes on macOS and Linux, so reproducing the panic needs a Windows
runner or a target where the aliasing is observed. The dependency-
tracking half (the outer derived losing its deps to the inner `take`)
is inspectable anywhere: assert on the outer derived's `dependencies`
after evaluating a nested computed.

## Fix directions

Not yet chosen. In rough order of preference:

1. **Make tracking a stack, not a slot.** Push a frame per
   `get_derived`, pop it at the end, and attribute reads to the
   innermost frame. Fixes the panic and the dep-attribution bug
   together, and matches how the DSL read scopes already work
   (`blinc_dsl_core::read_scope`).
2. **Evaluate the inner derived before opening the outer's tracking.**
   Narrower, but only works when the nesting is known statically, which
   the narrowing bridge is; it leaves general nesting broken.
3. **Flatten the bridge.** Have `bind_f64_computed_as_f32` keep the
   upstream `DerivedId` and narrow on read instead of creating a second
   derived. Removes this instance of nesting without making nesting
   work; `bind_f64_as_f32` already does exactly this for the signal
   path, and its doc comment explains why the wrapper shape was avoided
   there.

Whichever lands, the four tests above are the acceptance check, and a
non-Windows regression test asserting the outer derived's dependency
list should land with it.

## Why this is not the DSL

The failing tests live in `blinc_cn_dsl` and run DSL source, which makes
this look like a compiler or JIT problem. It is not:

- The panic is in `blinc_core`, in ordinary Rust, on the host side of
  the FFI boundary. No JIT frame appears between `Progress::from_config`
  and the panic.
- The same nesting is reachable from hand-written Rust:
  `div().bind_transform_from(computed_f32_wrapping_a_computed_f64)`.
- Zyntax CI is green on `main`.

Zyntax does own a separate, unrelated defect blocking the DSL `for`
loop. That is written up in the Zyntax repo as
`ZYNTAX_LOOP_SUBSTRATE_BUG.md` and shares nothing with this issue.
