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

```
┌─────────────────────────────────────────────────────────────┐
│  WindowedApp Event Loop (Platform abstraction)              │
├─────────────────────────────────────────────────────────────┤
│ • Receives pointer, keyboard, lifecycle events              │
│ • Routes through EventRouter -> StateMachines               │
│ • Triggers reactive signal updates                          │
│ • Checks signal dependencies for rebuilds                   │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  ReactiveGraph & Stateful Element Updates                   │
├─────────────────────────────────────────────────────────────┤
│ • Signals change -> Effects run -> Rebuilds queued          │
│ • Stateful elements transition -> Subtree rebuild queued    │
│ • Diff algorithm determines change categories               │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  RenderTree Update (Incremental)                            │
├─────────────────────────────────────────────────────────────┤
│ • incremental_update() compares hashes                      │
│ • VisualOnly: apply prop updates only                       │
│ • ChildrenChanged: rebuild subtrees + mark layout dirty     │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  Layout Computation (Taffy Flexbox)                         │
├─────────────────────────────────────────────────────────────┤
│ • compute_layout() on dirty nodes only                      │
│ • Returns (x, y, width, height) for all nodes               │
│ • Cached in LayoutTree                                      │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  Animation Scheduler (Background Thread @ 120fps)           │
├─────────────────────────────────────────────────────────────┤
│ • Ticks springs, keyframes, timelines                       │
│ • Stores current values in AnimatedValue                    │
│ • Sets needs_redraw flag, wakes main thread                 │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│  GPU Rendering (DrawContext -> GpuRenderer)                 │
├─────────────────────────────────────────────────────────────┤
│ • RenderTree traversal, samples animation values            │
│ • Emits SDF primitives to batches                           │
│ • Multi-pass rendering: Background -> Glass -> Foreground  │
└─────────────────────────────────────────────────────────────┘
```

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
