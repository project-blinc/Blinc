# Blinc Project Plan

## Overview

Blinc is a native UI framework powered by Zyntax, featuring:

- **Declarative DSL** (`.blinc` / `.bl`) with compile-time optimization
- **Fine-grained Reactivity** via signals (no VDOM)
- **Built-in State Machines** (Harel statecharts) for widget interactions
- **Animation-first Design** with spring physics and keyframes
- **GPU Rendering** via SDF-based primitives (wgpu/Metal)
- **Paint/Canvas System** for custom 2D drawing
- **Cross-platform** targeting Android, iOS, and Desktop

---

## Phase 1: Core Infrastructure

### 1.1 Toolchain Foundation

**Goal**: Establish the build system and cross-platform compilation pipeline.

#### Tasks

- [x] **CLI Scaffolding** (`blinc_cli`)
  - [x] Implement `blinc build` command with target selection
  - [x] Implement `blinc dev` with file watcher (notify crate) - *stub ready*
  - [x] Implement `blinc new` for project scaffolding
  - [x] Implement `blinc init` for in-place initialization
  - [x] Implement `blinc plugin build` for ZRTL plugin compilation - *stub ready*
  - [x] Implement `blinc doctor` for platform diagnostics
  - [x] Implement `blinc info` for toolchain information
  - [x] Implement `blinc check` for project validation - *stub ready*

- [ ] **Zyntax Integration**
  - [ ] Integrate `zyntax_embed` for JIT compilation
  - [ ] Configure grammar loading from `grammars/blinc.zyn`
  - [ ] Set up ZRTL plugin discovery and loading
  - [ ] Implement hot-reload via grammar recompilation

- [x] **Target Configurations**
  - [x] Create `toolchain/targets/android.toml` with NDK settings
  - [x] Create `toolchain/targets/ios.toml` with Xcode settings
  - [x] Create `toolchain/targets/macos.toml`
  - [x] Create `toolchain/targets/windows.toml`
  - [x] Create `toolchain/targets/linux.toml`

- [x] **Project Scaffolding System**
  - [x] `.blincproj` configuration schema (TOML-based)
  - [x] `src/` directory with main.blinc templates
  - [x] `plugins/` directory for local plugins
  - [x] `platforms/` directory with platform-specific files:
    - [x] Android: Gradle project, MainActivity.kt, AndroidManifest.xml
    - [x] iOS: Info.plist, AppDelegate.swift, LaunchScreen.storyboard
    - [x] macOS: Info.plist, entitlements.plist
    - [x] Windows: app.rc (resources), app.manifest
    - [x] Linux: .desktop entry, .metainfo.xml (AppStream)
  - [x] Project templates: default, minimal, counter

- [x] **CI/CD Infrastructure**
  - [x] GitHub Actions CI workflow (ci.yml)
  - [x] Android cross-compilation workflow (android.yml)
  - [x] Release workflow for CLI distribution (release.yml)
  - [x] Install script (scripts/install.sh)

- [x] **Build Optimization**
  - [x] `release-small` profile for mobile (opt-level=z, fat LTO, panic=abort)
  - [x] Android library size optimization (~530KB from 10MB+)
  - [x] Strip symbols in release builds

### 1.2 Blinc Grammar (`blinc.zyn`)

**Goal**: Define the complete Blinc DSL grammar that compiles to ZRTL function calls.

#### DSL Constructs

| Construct | Syntax | Compiles To |
|-----------|--------|-------------|
| `@widget` | `@widget Name { ... }` | Struct + init/render functions |
| `@prop` | `@prop name: Type = default` | Struct field |
| `@state` | `@state name: Type = value` | `blinc_signal_create_*()` |
| `@derived` | `@derived name: Type = expr` | `blinc_derived_create()` |
| `@machine` | `@machine name { states { ... } }` | `blinc_fsm_create()` |
| `@spring` | `@spring name { stiffness, damping, target }` | `blinc_spring_create()` |
| `@animation` | `@animation name { duration, keyframes }` | `blinc_keyframe_create()` |
| `@render` | `@render { Widget(...) { ... } }` | `blinc_widget_*()` calls |
| `@paint` | `@paint (ctx) { ... }` | `blinc_paint_*()` calls |

#### Tasks

- [ ] Define grammar metadata (`@language` block)
- [ ] Implement widget definition parsing
- [ ] Implement property declarations
- [ ] Implement reactive state (`@state`, `@derived`)
- [ ] Implement state machines (`@machine`)
- [ ] Implement animations (`@spring`, `@animation`)
- [ ] Implement render tree (`@render`)
- [ ] Implement paint context (`@paint`)
- [ ] Add semantic actions to emit ZRTL function calls

### 1.3 Reactive System (`blinc_core`)

**Goal**: Fine-grained signal-based reactivity inspired by Leptos/SolidJS.

#### Architecture

```
Signal → Subscribers → Effects/Derived
         (lazy)        (push invalidation, pull values)
```

#### Tasks

- [x] Implement `Signal<T>` with version tracking
- [x] Implement `Derived<T>` (memoized computed values)
- [x] Implement `Effect` (side effects on signal change)
- [x] Implement automatic dependency tracking
- [x] Implement batched updates
- [x] Implement reactive graph topological sorting
- [ ] Export ZRTL C-ABI functions

### 1.4 State Machine Runtime (`blinc_core`)

**Goal**: Harel statecharts for complex widget interactions.

#### Features

- Hierarchical states (nested)
- Parallel states (concurrent regions)
- Guards (conditional transitions)
- Entry/exit actions
- Transition actions

#### Tasks

- [x] Implement `StateMachine` with transition table
- [x] Implement state entry/exit callbacks
- [x] Implement guard conditions
- [x] Implement parallel state regions
- [x] Implement hierarchical state resolution
- [ ] Export ZRTL C-ABI functions

---

## Phase 2: Animation & Layout

### 2.1 Animation System (`blinc_animation`)

**Goal**: Framer Motion-quality animations with spring physics.

#### Spring Physics

- RK4 integration for accuracy
- Configurable stiffness, damping, mass
- Interruptible with velocity inheritance
- Auto-settle detection

#### Keyframe Animations

- Timed sequences with easing
- Multi-property support
- Wildcard keyframes (from current value)

#### Timeline Orchestration

- Sequential/parallel composition
- Relative offsets (`-=`, `+=`)
- Stagger functions for lists

#### Tasks

- [x] Implement `Spring` with RK4 integration
- [x] Implement `KeyframeAnimation` with interpolation
- [x] Implement `Timeline` with offsets
- [x] Implement `AnimationScheduler` for frame updates
- [x] Add easing function library (cubic bezier support)
- [x] Implement stagger utilities
- [x] Spring presets (gentle, wobbly, stiff, snappy, molasses)
- [ ] Export ZRTL C-ABI functions

### 2.2 Layout Engine (`blinc_layout`)

**Goal**: Flexbox layout via Taffy with GPUI-style builder API.

#### Tasks

- [x] Integrate Taffy layout engine
- [x] Map Blinc style properties to Taffy styles
- [x] Implement layout tree management
- [x] Implement GPUI-style builder API (`div()`, `text()`, `svg()`)
- [x] Support percentage, pixel, and auto sizing
- [x] Implement RenderTree for layout-to-rendering bridge
- [x] Implement layer separation (Background/Glass/Foreground)
- [x] 100+ Tailwind-style builder methods (`.flex_col()`, `.p()`, `.gap()`, etc.)
- [x] Implement dirty tracking for incremental layout
- [ ] Export ZRTL C-ABI functions

---

## Phase 3: Rendering

### 3.1 GPU Renderer (`blinc_gpu`)

**Goal**: High-performance SDF-based GPU rendering.

#### Render Pipeline

1. Collect primitives from widget tree
2. Sort by z-order
3. Batch by primitive type
4. Render: shadows → backgrounds → borders → content → glass → foreground → text

#### SDF Shaders

- Rounded rectangles with variable corner radii
- Circles and ellipses
- Gaussian blur shadows (via erf approximation)
- Gradients (linear, radial, conic)

#### Tasks

- [x] Set up wgpu device and surface
- [x] Implement rounded rectangle SDF shader
- [x] Implement shadow shader (Gaussian blur)
- [x] Implement gradient shader (linear, radial, conic)
- [x] Implement primitive batching
- [x] Implement glass/vibrancy shader (Apple-style glassmorphism)
- [x] Implement backdrop blur rendering
- [x] Implement path tessellation via lyon
- [x] Implement MSAA support (4x anti-aliasing for SVG)
- [x] Implement dynamic MSAA pipeline creation
- [x] Implement composite shader with blend modes
- [x] Implement backbuffer for glass effects
- [ ] Implement texture atlas for caching
- [ ] Optimize draw call batching

### 3.2 Paint/Canvas System (`blinc_paint`)

**Goal**: Full 2D drawing API for custom graphics.

#### API Design

```rust
ctx.fill_rect(rect, corner_radius, brush);
ctx.stroke_rect(rect, corner_radius, brush, stroke_width);
ctx.fill_circle(center, radius, brush);
ctx.stroke_circle(center, radius, brush, stroke_width);
ctx.draw_path(path, brush);
ctx.push_clip(rect);
ctx.push_transform(matrix);
```

#### Tasks

- [x] Implement `PaintContext` with command recording
- [x] Implement path building API (PathBuilder)
- [x] Implement color and gradient types
- [x] Implement shape primitives (rect, circle, rounded rect)
- [x] Implement transform stack
- [x] Implement clip stack
- [x] Implement shadow presets (small, medium, large, xl)
- [x] Integrate with GPU renderer (GpuPaintContext)
- [ ] Export ZRTL C-ABI functions

### 3.3 Text Rendering (`blinc_text`)

**Goal**: High-quality text with proper shaping and rendering.

#### Tasks

- [x] Integrate font loading (ttf-parser)
- [x] Implement glyph rasterization
- [x] Implement glyph atlas with region tracking
- [x] Implement text shaping (rustybuzz/HarfBuzz)
- [x] Implement text layout (line breaking, alignment)
- [x] Implement text anchoring (left, center, right)
- [x] Implement optical text centering
- [x] Implement TextRenderingContext for GPU integration
- [x] System font loading (Helvetica/macOS, DejaVu/Linux, Segoe/Windows)
- [ ] Implement SDF-based glyph rendering
- [ ] Implement glyph atlas LRU eviction

### 3.4 Image Rendering (`blinc_image`)

**Goal**: Cross-platform image loading and GPU texture rendering.

#### Tasks

- [x] Implement image decoding (PNG, JPEG, GIF, WebP, BMP)
- [x] Implement ImageData with RGBA pixel access
- [x] Implement ImageSource enum (File, Bytes, Base64, URL)
- [x] Implement cross-platform asset loading via platform loader
- [x] Implement GPU texture creation and caching
- [x] Implement image element builder for layout system
- [x] Implement image rendering in glass layer pipeline
- [x] Implement layer-aware image rendering (images ON glass stay crisp)
- [x] Implement Brush::Image variant for background images
- [x] Implement ImageFit modes (Cover, Contain, Fill, Tile)
- [ ] Implement async network image loading

### 3.6 SVG Rendering (`blinc_svg`)

**Goal**: Load and render SVG graphics.

#### Tasks

- [x] Implement SVG parsing (usvg)
- [x] Implement SVG to draw commands conversion
- [x] Implement SVG path rendering
- [x] Implement SVG styling support
- [x] Implement SVG element builder for layout system
- [x] Support fill and stroke rendering

---

## Phase 4: Platform Integration

### 4.0 Platform Abstraction (`blinc_platform`)

**Goal**: Cross-platform traits and APIs for windowing, input, and assets.

#### Tasks

- [x] Implement Platform, Window, EventLoop traits
- [x] Implement WindowConfig with title, size, resizable options
- [x] Implement Event types (Window, Input, Lifecycle, Frame)
- [x] Implement InputEvent (Mouse, Keyboard, Touch, Scroll)
- [x] Implement cross-platform AssetLoader trait
- [x] Implement FilesystemAssetLoader for desktop
- [x] Implement global asset loader pattern (OnceLock)
- [x] Implement AssetPath enum (Relative, Absolute, Embedded)
- [ ] Implement clipboard access trait
- [ ] Implement system theme detection trait

### 4.1 Desktop Platform (`blinc_platform_desktop`)

**Goal**: Native windowing for macOS, Windows, Linux.

#### Tasks

- [x] Implement window creation via winit
- [x] Implement event loop integration
- [x] Implement input handling (keyboard, mouse, touch)
- [x] Implement DPI scaling (scale factor tracking, logical/physical conversion)
- [x] Implement scroll phase detection (Started, Moved, Ended, MomentumEnded)
- [ ] Implement clipboard access
- [ ] Implement system theme detection

### 4.2 Android Platform (`blinc_platform_android`)

**Goal**: Native Android integration.

#### Tasks

- [x] Implement NativeActivity integration
- [x] Implement JNI bridge for system APIs
- [x] Implement touch input handling
- [x] Implement Vulkan/GLES surface creation
- [x] Implement AndroidAssetLoader (NDK AssetManager)
- [ ] Implement lifecycle management (pause/resume)
- [ ] Implement soft keyboard handling
- [x] Create Gradle project template

#### Build Infrastructure

- [x] Android NDK cross-compilation (API 35)
- [x] aarch64-linux-android target
- [x] x86_64-linux-android target (for emulator)
- [x] Optimized library size (~530KB)

### 4.3 iOS Platform (`blinc_platform_ios`)

**Goal**: Native iOS integration.

#### Tasks

- [x] Implement UIKit integration via objc2
- [x] Implement Metal rendering support
- [x] Implement touch input handling
- [ ] Implement safe area insets
- [ ] Implement keyboard handling
- [ ] Implement lifecycle management
- [x] Create Xcode project template

---

## Phase 5: Application Framework

### 5.1 App Delegate (`blinc_app`)

**Goal**: High-level API for building Blinc applications.

#### Tasks

- [x] Implement BlincApp with configuration
- [x] Implement single render() function API
- [x] Implement RenderContext for unified rendering
- [x] Implement automatic glass backdrop handling
- [x] Implement automatic text/SVG positioning from layout
- [x] Implement MSAA configuration
- [x] Implement comprehensive visual test suite

### 5.2 Widget Library (`blinc_widgets`)

**Goal**: Core UI widgets with FSM-driven interactions.

| Widget | States | Animations |
|--------|--------|------------|
| Button | idle, hovered, pressed, focused, disabled | ripple, scale |
| Checkbox | unchecked, checking, checked, unchecking | checkmark draw |
| Toggle | off, transitioning, on | thumb slide |
| TextField | empty, focused, filled, error | label float |
| Dropdown | closed, opening, open, closing | height expand |
| Modal | hidden, entering, visible, exiting | fade + scale |
| Tabs | idle, switching | underline slide |
| Accordion | collapsed, expanding, expanded | height spring |
| Tooltip | hidden, delay, showing, visible | fade + offset |
| Slider | idle, dragging | thumb scale |
| ScrollView | idle, scrolling, momentum | content offset |

#### Tasks

- [x] Implement base Widget trait and WidgetId
- [x] Implement Button widget with click handling
- [x] Implement Container widget
- [x] Implement Text widget
- [x] Implement ScrollView with spring physics bounce-back
  - [x] FSM states (Idle, Scrolling, Decelerating, Bouncing)
  - [x] Webkit-style rubber-band overscroll
  - [x] Configurable spring bounce (stiff, gentle, no-bounce presets)
  - [x] Momentum and deceleration
  - [x] Vertical, horizontal, and bidirectional scrolling
- [x] Implement TextInput with validation
  - [x] Input types (text, number, email, password, url, tel, search)
  - [x] Character filtering and constraints
  - [x] Cursor with smooth blink animation
  - [x] Selection support (Shift+arrows, Ctrl+A)
  - [x] Visual states (idle, hovered, focused, error)
- [x] Implement TextArea (multi-line text editing)
  - [x] Row/column sizing
  - [x] Cursor and selection
  - [x] Full keyboard navigation
- [x] Implement Code/Pre elements with syntax highlighting
  - [x] Regex-based token matching
  - [x] Optional line numbers in gutter
  - [x] Read-only and editable modes
  - [x] Canvas-based cursor animation
- [x] Implement Motion container for enter/exit animations
  - [x] Fade, scale, bounce, pop, slide presets
  - [x] Stagger animations (forward, reverse, from-center)
  - [x] Enter/exit animation support
- [x] Implement Typography helpers
  - [x] Headings: h1-h6 with semantic sizing
  - [x] Inline text: b, strong, span, small, label, muted
  - [x] Block text: p (paragraph), caption, inline_code
- [x] Implement Table elements
  - [x] HTML-like API: table, thead, tbody, tfoot, tr, th, td
  - [x] TableBuilder for declarative data tables
  - [x] Striped rows support
  - [x] Cell styling and alignment
- [ ] Implement Checkbox with animation
- [ ] Implement Toggle with spring animation
- [ ] Implement Dropdown with expand animation
- [X] Implement Modal with backdrop
- [ ] Implement Tabs with indicator animation
- [ ] Implement Slider with drag handling

### 5.3 Theming System

#### Tasks

- [ ] Define theme schema (colors, typography, spacing)
- [ ] Implement theme provider pattern
- [ ] Implement dark/light mode switching
- [ ] Implement theme inheritance

---

## Phase 6: Developer Experience

### 6.1 Hot Reload

**Goal**: Sub-second iteration during development.

#### Architecture

```
File Change → Grammar Recompile → JIT Update → State Preserved
```

#### Tasks

- [ ] Implement file watcher with debouncing
- [ ] Implement incremental grammar compilation
- [ ] Implement widget tree diffing
- [ ] Implement state preservation across reloads

### 6.2 Developer Tools

#### Tasks

- [ ] Implement widget inspector overlay
- [ ] Implement state machine visualizer
- [ ] Implement animation timeline debugger
- [ ] Implement reactive graph viewer
- [ ] Implement performance profiler

### 6.3 IDE Integration

#### Tasks

- [ ] Create VS Code extension with syntax highlighting
- [ ] Implement LSP server for autocomplete
- [ ] Implement error diagnostics
- [ ] Implement go-to-definition

---

## Phase 7: Production Hardening

### 7.1 Performance

#### Tasks

- [ ] Profile and optimize hot paths
- [ ] Implement layout caching
- [ ] Implement render tree diffing
- [ ] Optimize memory allocations (arena allocators)
- [ ] Implement GPU texture atlasing

### 7.2 Testing

#### Tasks

- [x] Unit tests for reactive system
- [x] Unit tests for state machines
- [x] Unit tests for animation
- [x] Integration tests for blinc_core
- [x] Visual test suite (blinc_test_suite) with 14 test categories
  - [x] Clipping tests
  - [x] Glass/vibrancy tests
  - [x] Gradient tests
  - [x] Layout tests
  - [x] Opacity tests
  - [x] Paint context tests
  - [x] Path rendering tests
  - [x] SDF primitive tests
  - [x] Shadow tests
  - [x] Shape tests
  - [x] SVG tests
  - [x] Text tests
  - [x] Transform tests
- [x] blinc_app API tests
- [ ] Visual regression tests (reference image comparison)
- [ ] Performance benchmarks

### 7.3 Documentation

#### Tasks

- [X] API reference documentation
- [X] Tutorial: Getting Started
- [X] Tutorial: Building Your First App
- [X] Guide: Custom Widgets
- [X] Guide: Animations
- [X] Guide: Paint/Canvas
- [ ] Guide: Platform Integration

---

## Current Status Summary

### Crates Overview

| Crate | Lines | Tests | Status |
|-------|-------|-------|--------|
| **blinc_core** | ~3,500 | ✓ | Reactive signals, FSM runtime, draw context, Brush types, events |
| **blinc_animation** | ~1,500 | ✓ | Springs (RK4), keyframes, timelines, easing |
| **blinc_layout** | ~8,000 | ✓ | Taffy, GPUI-style builder, widgets (scroll, text input, code, tables, typography, motion) |
| **blinc_gpu** | ~4,000 | ✓ | SDF rendering, glass, MSAA, compositing, image textures |
| **blinc_paint** | ~1,500 | ✓ | Canvas API, paths, shapes, transforms |
| **blinc_text** | ~2,000 | ✓ | Font loading, shaping, atlas, layout, text wrapping |
| **blinc_image** | ~500 | ✓ | Image loading, decoding, cross-platform assets |
| **blinc_svg** | ~800 | ✓ | SVG parsing and rendering |
| **blinc_platform** | ~1,200 | ✓ | Cross-platform traits, asset loading, scroll phases |
| **blinc_app** | ~1,500 | ✓ | High-level app framework, windowed runner, DPI scaling |
| **blinc_widgets** | ~400 | - | Button, container, text (basic) |
| **blinc_runtime** | ~200 | - | Embedding SDK |
| **blinc_cli** | ~2,000 | - | CLI tool |
| **blinc_test_suite** | ~3,000 | 107 | Comprehensive visual testing |

### Platform Extensions

| Extension | Status |
|-----------|--------|
| **blinc_platform_desktop** | Window creation, input handling, filesystem assets, DPI scaling, scroll phases via winit |
| **blinc_platform_android** | NativeActivity, JNI, Vulkan, NDK AssetManager (~530KB binary) |
| **blinc_platform_ios** | UIKit, Metal, touch input |

### Completed ✓

| Component | Features |
|-----------|----------|
| **blinc_core** | Reactive signals, FSM runtime, draw context, layer model, Brush::Image, event system |
| **blinc_animation** | Springs (RK4), keyframes, timelines, easing, presets |
| **blinc_layout** | Taffy integration, GPUI-style builder, RenderTree, materials, image element |
| **blinc_layout widgets** | Scroll (spring physics), TextInput, TextArea, Code/Pre (syntax highlighting), Motion (enter/exit animations), Typography (h1-h6, span, muted), Tables (TableBuilder) |
| **blinc_gpu** | SDF shaders, gradients, glass/blur, MSAA, path tessellation, image textures |
| **blinc_paint** | Paint context, paths, shapes, transforms, shadows |
| **blinc_text** | Font loading, text shaping, glyph atlas, layout, text wrapping |
| **blinc_image** | Image decoding (PNG/JPEG/GIF/WebP/BMP), cross-platform asset loading |
| **blinc_svg** | SVG parsing, rendering, element builder |
| **blinc_platform** | Platform traits, event system, cross-platform AssetLoader, scroll phases |
| **blinc_app** | BlincApp, RenderContext, WindowedApp runner, DPI scaling, visual tests |
| **blinc_cli** | Full CLI with new/init/build/dev/doctor/info |
| **blinc_platform_desktop** | winit integration, filesystem assets, DPI scaling, scroll phase detection |
| **blinc_platform_android** | NDK integration, JNI bridge, Vulkan, NDK AssetManager |
| **blinc_platform_ios** | UIKit, Metal, touch input |
| **CI/CD** | GitHub Actions for CI, Android, releases |
| **Test Suite** | 107 tests across 14 categories |

### In Progress

| Component | Status |
|-----------|--------|
| **Zyntax Integration** | Waiting for Grammar2/Runtime2 |
| **ZRTL C-ABI exports** | Pending Zyntax integration |

### Next Priorities

1. **Zyntax Grammar2 Integration** - Enable .blinc file parsing
2. **ZRTL Function Exports** - Bridge Rust runtime to Zyntax
3. **Remaining Widgets** - Checkbox, Toggle, Dropdown, Modal, Tabs, Slider
4. **Theming System** - Colors, typography, dark/light mode
5. **Hot Reload** - File watcher + JIT recompilation

---

## Technical Decisions

### Why Zyntax?

- **AOT Compilation**: Native binaries without runtime overhead
- **JIT for Development**: Instant hot-reload during development
- **Custom DSL**: Grammar-defined language without forking a compiler
- **ZRTL Plugins**: Modular runtime with dynamic/static linking

### Why SDF Rendering?

- **Resolution Independent**: Sharp at any scale
- **GPU Efficient**: Simple fragment shaders
- **Smooth Edges**: Built-in anti-aliasing
- **Flexible**: Combine shapes with boolean operations

### Why Built-in State Machines?

- **Explicit States**: No impossible state combinations
- **Visual Debugging**: Generate statechart diagrams
- **Animation Triggers**: Entry/exit actions drive animations
- **Testable**: State machines are easily unit tested

### Why Fine-Grained Reactivity?

- **No VDOM Diffing**: Direct updates to affected widgets
- **Minimal Re-renders**: Only dependent computations update
- **Predictable**: Clear dependency graph
- **Performant**: O(1) signal updates

---

## Success Metrics

1. **Performance**: 120 FPS on target devices
2. **Hot Reload**: < 100ms from save to update
3. **Binary Size**: < 5MB for minimal app (Android ~530KB achieved ✓)
4. **Memory**: < 50MB for typical app
5. **Developer Experience**: Intuitive DSL, helpful errors

---

## Demo Applications

### Music Player UI

A complete music player interface demonstrating:

- Glass/vibrancy effects with backdrop blur
- SVG icons (rewind, pause, forward)
- Text rendering with proper anchoring
- Progress bar with nested glass elements
- GPUI-style builder API usage

Location: `crates/blinc_test_suite/src/tests/layout.rs` (music_player test)

### Glass Card UI

Demonstrates glassmorphism with:

- Multiple layered glass panels
- Background elements visible through blur
- Foreground children rendered on top
- Automatic layer separation

Location: `crates/blinc_test_suite/src/tests/layout.rs` (glass_card test)

### Typography Demo

Demonstrates typography helpers:

- Heading hierarchy (h1-h6) with semantic sizing
- Inline text elements (bold, span, small, label, muted)
- Block text (paragraphs, captions, inline code)
- Text wrapping and layout

Location: `crates/blinc_app/examples/typography_demo.rs`

### Table Demo

Demonstrates table components:

- TableBuilder for declarative data tables
- Manual table construction (thead, tbody, tr, td, th)
- Striped rows for better readability
- Cell styling and alignment options

Location: `crates/blinc_app/examples/table_demo.rs`

### Animation Demo

Demonstrates motion animations:

- Enter/exit animations (fade, scale, bounce, slide)
- Stagger animations with configurable direction
- Spring-based smooth transitions

Location: `crates/blinc_app/examples/animation_demo.rs`
