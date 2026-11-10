# Architecture Overview

Blinc is a high-performance UI framework built from the ground up for GPU-accelerated rendering without virtual DOM overhead. This chapter explains how the major systems work together.

## Design Philosophy

Blinc follows several key principles:

1. **Fine-grained Reactivity** - Signal-based state management with automatic dependency tracking eliminates the need for virtual DOM diffing
2. **Layout as Separate Concern** - Tree structure is independent from visual properties, enabling visual-only updates without layout recomputation
3. **GPU-First Rendering** - SDF shaders provide resolution-independent, smooth rendering with glass/blur effects
4. **Incremental Updates** - Hash-based diffing with change categories minimizes recomputation
5. **Background Thread Animations** - Animation scheduler runs independently from the UI thread

## System Architecture

Each frame runs through five named phases. The phase numbering is
load-bearing: code, traces, and runbooks all reference it.

```
┌─────────────────────────────────────────────────────────────┐
│  WindowedApp Event Loop (Platform abstraction)              │
├─────────────────────────────────────────────────────────────┤
│ • Receives pointer, keyboard, lifecycle events              │
│ • Routes through EventRouter -> StateMachines               │
│ • Triggers reactive signal updates                          │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  Phase 1: Diff                                              │
├─────────────────────────────────────────────────────────────┤
│ • incremental_update() compares hashes                      │
│ • VisualOnly: apply prop updates                            │
│ • ChildrenChanged: rebuild subtree, invalidate cache        │
│ • Most signal updates skip rebuild; they land as            │
│   dynamic bindings the compositor patches in Phase 4        │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  Phase 2: Layout (Taffy Flexbox)                            │
├─────────────────────────────────────────────────────────────┤
│ • compute_layout() on dirty nodes only                      │
│ • Stylesheet overrides applied before compute_layout        │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  Phase 3: Tick                                              │
├─────────────────────────────────────────────────────────────┤
│ • Spring physics, CSS keyframes, CSS transitions            │
│ • Scroll physics, motion bindings                           │
│ • Layout-prop animations re-run compute_layout              │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  Phase 4: Paint                                             │
├─────────────────────────────────────────────────────────────┤
│ • try_fast_paint? (cache + no rebuild / layout / scroll)    │
│     yes → apply_binding_deltas + apply_css_deltas           │
│          render_static_layer_damaged (scissor + redraw)     │
│     no  → full walker → repopulate cache                    │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  Phase 5: Composite & Present                               │
├─────────────────────────────────────────────────────────────┤
│ • Blit static cache → swapchain                             │
│ • Overlay pass: dynamic batch + motion subtree blits        │
│ • Submit, present                                           │
│ • request_redraw() if any visible animation mid-flight      │
└─────────────────────────────────────────────────────────────┘
```

### What makes this fast

After the first paint of a tree state, the static layer lives in a
GPU-side texture cache. Most subsequent frames (hover, focus, CSS
animation tick, spring tick) never re-walk the tree. The compositor
patches a small set of primitive fields in place (translate, scale,
opacity, colour, corner radius) and the GPU pass scissors a damage rect,
re-dispatching only the SDF primitives that intersect.

Signal-bound props (`bg(my_signal)`, `w(my_signal)`) go through the same
in-place patch path. A signal update doesn't necessarily rebuild any
subtree; it can simply patch the GPU primitive's relevant field.

A signal *read* inside a branched expression is the case that rebuilds the
reading component's subtree (not the whole UI). And off-screen content
opted into viewport culling emits zero primitives, so it costs nothing.

The full re-walk only fires when the cache invalidates: structural
rebuilds, layout-affecting changes, scroll, overlay transitions. See [GPU
Rendering](./gpu-rendering.md) for the cache, damage, and culling details.

The static layer is a window-sized texture. A desktop window that has drawn
nothing for two seconds frees it, with the offscreen targets, pooled layer
textures, flow scene copy and mesh targets, and the next frame paints in full
and makes them again. The window-sized intermediates also go once no frame has
used them for 60 frames, while the app keeps drawing. An app
that drives its own loop can do the same with `BlincApp::release_idle_memory`.

## Core Crates

| Crate | Purpose |
|-------|---------|
| `blinc_core` | Reactive signals, FSM, core types, event system |
| `blinc_layout` | Element builders, Taffy integration, diff system, stateful elements |
| `blinc_animation` | Spring physics, keyframe timelines, animation scheduler |
| `blinc_gpu` | wgpu renderer, SDF shaders, glass effects, text rendering |
| `blinc_text` | Font loading, glyph shaping, text atlas |
| `blinc_app` | WindowedApp, render context, platform integration |

## Why No Virtual DOM?

Traditional frameworks (React, Vue) use a virtual DOM to diff the entire component tree on every state change. This has overhead:

1. Creating VDOM objects for every render
2. Diffing the full tree to find changes
3. Patching the real DOM with changes

Blinc avoids this with:

1. **Fine-grained signals** - Only dependent code re-runs when state changes
2. **Stateful elements** - UI state managed at the element level, not rebuilt from scratch
3. **Hash-based diffing** - Quick equality checks without deep comparison
4. **Change categories** - Visual vs layout vs structural changes handled differently

The result: updates proportional to what changed, not to tree size.

---

## Chapter Contents

- [GPU Rendering](./gpu-rendering.md) - SDF primitives, glass effects, text rendering
- [Reactive State](./reactive-state.md) - Signal system, dependency tracking, effects
- [Layout & Diff](./layout-diff.md) - Taffy integration, incremental updates
- [Animation](./animation.md) - Spring physics, timelines, scheduler
- [Stateful Elements](./stateful.md) - FSM-driven interactive widgets
