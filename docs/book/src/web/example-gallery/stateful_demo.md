# Stateful API + Signal-bound modifiers demo.

Two complementary examples:

1. **Stateful counter / event display** — the original demo of
   `stateful::<S>()`, `ctx.use_signal()`, `ctx.use_animated_value()`
   (declarative spring animations + scoped signals).

2. **Signal-bound modifiers** (reactive-architecture-v2 Phase 2) —
   `.bg(&state)` / `.opacity(&state)` / `.rounded(&state)` /
   `.border_color(&state)` / `.w(&state)` patch a single
   `RenderProps` (or taffy `Style`) cell on `state.set(...)` without
   a `Stateful` wrap or closure re-run.

<div class="blinc-example" data-example="stateful_demo" data-title="Blinc stateful_demo example"></div>

> **Tip:** Some demos are best viewed in a full browser window. Click "Open in a new tab" below for the full experience.

[Open in a new tab](../../examples/stateful_demo/index.html) · [View source on GitHub](https://github.com/project-blinc/Blinc/blob/main/examples/blinc_app_examples/examples/stateful_demo.rs)
