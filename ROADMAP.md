# Blinc Roadmap

> Last updated: 2026-04-13

## Vision

Blinc is a GPU-accelerated, cross-platform UI framework that enables developers to build production-quality desktop, mobile, and embedded applications from a single Rust codebase. The framework aims to match the quality of native platform UIs while providing a unified developer experience across all targets.

---

## Phase 1: Desktop Production Readiness

> Goal: Make Blinc viable for shipping real desktop applications.

### 1.1 System Integration (P0)

| Feature | Status | Notes |
|---------|--------|-------|
| File dialogs (open/save/folder) | **Done** | Via `rfd` crate, `blinc_app::dialog` module |
| System tray / status bar icon | **Done** | `blinc_app::tray::TrayIconBuilder` via `tray-icon` + `muda` |
| Native OS notifications | **Done** | `blinc_app::notify::Notification` via `notify-rust` |
| Drag and drop | **Done** | Window-level `on_file_drop()` + element-level `.on_file_drop()` on Div |
| Clipboard (rich content) | **Done** | Cross-platform text + image via `arboard` crate |
| Global keyboard shortcuts | **Done** | `blinc_app::hotkey::GlobalHotkey` via `global-hotkey` |

### 1.2 Window Management (P0)

| Feature | Status | Notes |
|---------|--------|-------|
| Multi-window support | **Done** | `ctx.open_window(config)`, `WindowState`, `AppCommand` event loop |
| Window min/max size constraints | **Done** | `WindowConfig::min_size()`, `max_size()` |
| Programmatic window positioning | **Done** | `set_position()`, `center()`, `set_size()` on Window trait |
| Window state persistence | **Done** | `WindowStateStore` with JSON save/load |
| Modal window API | **Done** | `WindowConfig::modal()`, input blocking on non-modal windows |
| Custom title bar / drag regions | **Done** | `.drag_region()` on Div, `window_actions` module |

### 1.3 Input (P1)

| Feature | Status | Notes |
|---------|--------|-------|
| IME / compose input | **Done** | Winit Ime::Commit routed as Key::Char events, `set_ime_allowed(true)` |
| Context menu wiring | **Done** | `.on_right_click()` / `.on_context_menu()` on Div |
| Trackpad gestures (pinch/rotate) | **Done** | `.on_pinch()` / `.on_rotate()` on Div, winit gesture events |

### 1.4 Missing Widgets (P1)

> **Layering rule** (applies to every widget in this section and §1.5):
> all `blinc_layout::widgets::*` widgets must render with theme tokens
> by default — colours from `ColorTokens`, radii from `RadiusTokens`,
> shape from `ShapeTokens`, spacing from `SpacingTokens`, typography
> from `TypographyTokens`, easings from `AnimationTokens`. A bare
> `text_input(...)`, `slider(...)`, `number_input(...)` built against
> the platform default bundle should look right out of the box —
> theme-aware, dark-mode-aware, variant-aware (Restrained / Hybrid /
> Expressive). `blinc_cn` then layers shadcn-flavoured surface
> styling and micro-interactions on top via CSS classes and themed
> wrappers (the same way `cn::input` wraps `blinc_layout::text_input`
> today: the layout widget already paints itself from the active
> theme; cn contributes the `.cn-input` border-shape / focus-ring /
> hover-bg overlay and a few interaction polishes like the
> outline-offset transition). This keeps the layout widgets useful
> outside a `cn_bundle()` install — anyone building a custom theme
> bundle gets cohesive defaults without depending on `blinc_cn`.

| Widget | Status | Notes |
|--------|--------|-------|
| Date picker | Planned | Calendar grid + input. See §1.5 for the cn component sizing. |
| Time picker | Planned | Clock face or dropdown. See §1.5. |
| Color picker | Planned | HSL/RGB wheel + swatches. See §1.5. |
| Range slider (dual thumb) | Planned | Extend existing slider. See §1.5. |
| Number input (stepper) | **Done** | First-class `blinc_layout::widgets::number_input` plus `cn::number_input` themed wrapper with steppers, bounds, precision, keyboard stepping, and stable width sizing. |
| Data grid | Planned | Sortable, filterable table. See §1.5 (depends on cn::table). |
| Virtualized list | **Done** | `virtual_list(count, builder)` — variable-height items, CSS classes, flexbox layout |
| Rich text editor | **Done** | `rich_text_editor()` — formatting toolbar, undo/redo, selection, clipboard |

### 1.5 cn Component Library — shadcn/ui parity gaps (P1)

> blinc_cn currently ships 45 components. The remaining gaps to reach
> full shadcn/ui parity, plus the §1.4 widgets re-framed as their
> intended `cn::*` landing surface. Effort is rough: **XS** < 1 day,
> **S** 1–3 days, **M** ~1 week, **L** 2+ weeks. Priority follows
> shadcn parity + common request frequency.

| Component | Priority | Effort | Prereq | Notes |
|-----------|----------|--------|--------|-------|
| `cn::toggle` | P0 | XS | — | Single binary toggle button. Reuse `button` + Stateful with `ToggleState`. Pairs with `:aria-pressed` styling. |
| `cn::toggle_group` | P0 | XS | `cn::toggle` | Radio-style toggle bar; single- or multi-select variants. |
| `cn::number_input` | P0 | XS | `blinc_layout::number_input`, `cn::input` | Themed wrapper around the first-class `number_input` widget — applies cn surface (`.cn-input`-shaped border/focus ring, hover bg) plus the chevron up/down stepper buttons. Parse / clamp / commit logic lives in the layout widget; cn only contributes look + the stepper affordance. Roadmap §1.4 entry. |
| `cn::table` | P0 | S | `blinc_layout::table` | Themed wrapper exposing `Table` / `TableHeader` / `TableBody` / `TableRow` / `TableCell` / `TableCaption` builders matching the shadcn surface. No sort/filter — that's `data_grid`. |
| `cn::input_otp` | P0 | S | `cn::input` | **Done** — segmented PIN / OTP input (N digits). Focus chain across boxes with auto-advance on type, empty-Backspace rewind, paste distribution, masking, grouping, and error styling. |
| `cn::calendar` | P0 | M | — | Month grid + day cells + range / single selection. Hand-rolled date math (chrono is overkill for a UI calendar). Prereq for date picker. |
| `cn::date_picker` | P0 | M | `cn::calendar`, `cn::popover` | Input + popover-mounted calendar. Roadmap §1.4 entry. |
| `cn::form` | P0 | M | — | Typed schema + field-binding wrapper. Field `State`s register into a form context; submit collects + validates. Should integrate with `cn::input` / `cn::textarea` / `cn::select` / `cn::checkbox` field variants surfacing error state via CSS class (e.g. `.cn-input--invalid`). |
| `cn::range_slider` | P1 | S | `cn::slider` | Dual-thumb extension. Most of the work is hit-testing the nearest thumb on drag start + clamping the trailing thumb against the leading one. Roadmap §1.4 entry. |
| `cn::carousel` | P1 | S | `cn::scroll_area` | Horizontal scroll-snap container + prev/next buttons + dot indicators + optional autoplay. A non-cn `carousel_demo` example exists as a reference. |
| `cn::command` | P1 | M | `cn::combobox` | `⌘K`-style searchable command palette: fuzzy match, keyboard nav, grouped items, action shortcuts. Lifts the keyboard-driven half of `combobox` into a richer overlay surface. |
| `cn::time_picker` | P1 | M | `cn::popover`, `cn::input` | Clock face or scrollable hours/minutes/seconds columns. Roadmap §1.4 entry. |
| `cn::color_picker` | P1 | M | `cn::popover`, `cn::slider` | HSL/RGB wheel (canvas-rendered), saturation/value square, hue slider, hex input, recently-used swatches. Roadmap §1.4 entry. |
| `cn::data_grid` | P2 | L | `cn::table`, `cn::virtual_list`, `cn::form`, `cn::checkbox`, `cn::dropdown_menu` | Sortable, filterable, paginated, column-resizable, row-selectable table on top of `virtual_list` for performance. Roadmap §1.4 entry. Pulls in column-header sort indicators + filter dropdowns + selection state + per-row context-menu. |

---

## Phase 2: Mobile Production Readiness

> Goal: Bring mobile targets to feature parity with desktop.

### 2.1 Input & Interaction (P0)

| Feature | Status | Notes |
|---------|--------|-------|
| Soft keyboard show/hide | **Done** | Atomic flags polled in frame loop |
| IME / compose input (mobile) | Partial | Basic text input works, full compose candidates need InputConnection/UITextInput |
| Gesture recognizers | **Done** | `GestureExt` trait: `.on_tap()`, `.on_long_press()`, `.on_swipe()` + pinch/rotate |
| Pull-to-refresh | **Done** | `pull_to_refresh(callback)` widget with threshold, max pull, indicator |
| Safe area insets | **Done** | `ctx.safe_area`, `safe_top/right/bottom/left()`, `safe_width/height()` |
| Haptic feedback | Done | Native bridge handlers on both platforms |

### 2.2 Platform Integration (P1)

| Feature | Status | Notes |
|---------|--------|-------|
| Deep linking / URL schemes | **Done** | `blinc_router::handle_deep_link()` + platform `DeepLink` parser |
| App lifecycle | **Done** | Resumed/Suspended/LowMemory events, Android full lifecycle mapping |
| Native bridge API (Rust side) | **Done** | `native_call()`, `native_register()`, `PlatformAdapter` trait in blinc_core |
| Native bridge streams | **Done** | `native_stream()` — bidirectional data (camera, audio, sensors) with auto-cleanup |

> **Push notifications, camera, location, biometrics, status bar theming** are implemented as **example native bridge extensions** — not in the framework core, but as documented templates that demonstrate the bridge API:
> - **Android**: `BlincNativeBridge.register("camera", "capture", handler)` in Kotlin
> - **iOS**: `BlincNativeBridge.shared.register(namespace:name:handler:)` in Swift
> - **Rust**: `native_call("camera", "capture", args)` (planned API)
>
> Blinc provides the bridge transport. Platform features ship as reference implementations users can copy and adapt.

### 2.3 Media (P1)

| Feature | Status | Notes |
|---------|--------|-------|
| Audio playback | **Done** | `AudioPlayer` — Desktop: rodio (Vorbis/WAV/FLAC), Mobile: native bridge |
| Video decoding | **Done** | `VideoDecoder` — Desktop: OpenH264 (H.264 NAL → RGBA), Mobile: native bridge |
| AV frame utilities | **Done** | `Frame` (RGBA/RGB/BGRA/YUV420/Gray, scale, convert) + `AudioSamples` (f32/i16/u8, resample, mono) |
| Audio recording | **Done** | `AudioRecorder` — Desktop: cpal, Mobile: native bridge stream |
| Video player | **Done** | `VideoPlayer` — play/pause/seek/volume, frame push API |
| Camera stream | **Done** | `CameraStream` — RTC-like reactive capture, RGBA frame delivery |
| Audio widget | **Done** | `audio_player()` — waveform canvas, `MediaControls` via `Player` trait |
| Video widget | **Done** | `video_player()` — canvas surface, shared controls, dimensions |
| Player trait | **Done** | Shared `Player` trait for `AudioPlayer` + `VideoPlayer` + live streams |
| Live streaming | **Done** | `Player::is_live()`, LIVE badge, seek-less controls |

> **Licensing**: Desktop uses royalty-free codecs only — OpenH264 (Cisco's BSD, patent costs covered by Cisco), VP9, AV1, Opus, Vorbis. No ffmpeg, no patent-encumbered codecs.
> Mobile uses platform-provided codecs (OS handles licensing).

| Example Extension | Status | Notes |
|-------------------|--------|-------|
| Push notifications | Planned | FCM/APNs example with bridge handlers |
| Camera capture | **Done** | `CameraStream` → native bridge → RGBA frames via `blinc_dispatch_stream_data` FFI |
| Location services | Planned | GPS via bridge example |
| Biometric auth | Planned | Fingerprint/Face ID bridge example |
| Status bar theming | Planned | Light/dark status bar bridge example |

### 2.3 Navigation & Router (`blinc_router` crate)

> See [docs/plans/blinc_router.md](docs/plans/blinc_router.md) for full design.
> See [docs/book/src/advanced/routing.md](docs/book/src/advanced/routing.md) for usage guide.

| Feature | Status | Notes |
|---------|--------|-------|
| Route definition & trie matching | **Done** | `/users/:id`, `*wildcard`, nested routes, O(depth) trie |
| Scoped `use_router()` hook | **Done** | Thread-local router stack, nested router support |
| History management | **Done** | `RouterHistory` with push/replace/back/forward |
| Page transitions | **Done** | `PageTransition` using `AnimationPreset` + `SpringConfig` |
| Navigation guards | **Done** | Auth guards with redirect/reject |
| Deep linking | **Done** | Auto-registered, zero-config: URI parsing + platform dispatch |
| Desktop deep links | **Done** | CLI `--deep-link=`, macOS/Windows/Linux URL scheme registration |
| iOS deep links | **Done** | `blinc_ios_handle_deep_link()` C FFI, auto-dispatched |
| Android deep links | **Done** | `dispatch_deep_link()` from JNI intent, auto-dispatched |
| System back button | **Done** | Auto-registered by `RouterBuilder::build()`, `Key::Back` dispatch |
| Named routing | **Done** | `push_named("user", &[("id", "42")])` reverse lookup |
| Route outlet | **Done** | `router.outlet()` builds current view with scoped context |
| Stack navigator | **Done** | `stack()` + `motion()` — documented integration pattern |
| Tab navigator | **Done** | `blinc_cn::tabs()` + `router.push()` — documented pattern |
| Bottom sheet navigation | **Done** | `blinc_cn::sheet()` + `router.outlet()` — documented pattern |
| Animation suspension | **Done** | `AnimatedValue::pause/resume`, `Spring::pause/resume` — old views auto-clean via Drop |
| Nested route stacks | **Done** | Sub-routers via `RouterBuilder`, `use_router()` returns innermost |

---

## Phase 3: Blinc DSL (`.blinc` files via Zyntax)

> Goal: a declarative, hot-reloadable UI language that drops onto the existing Blinc renderer without replacing the Rust builder API. Implemented by embedding Zyntax — its tiered Cranelift+LLVM JIT for development and its LLVM AOT path for shipping mobile binaries.

### 3.1 Language design

```blinc
// .blinc: declarative UI with embedded logic

import { Color, SpringConfig } from "blinc"

component Counter {
  state count: i32 = 0

  view {
    col(gap: 16, align: center, justify: center) {
      text("Count: {count}")
        .size(32)
        .color(Color::WHITE)
        .animate(spring: SpringConfig::bouncy)

      row(gap: 8) {
        button("- Decrement") { on_click: count -= 1 }
        button("+ Increment") { on_click: count += 1 }
      }
    }
  }

  style {
    .self {
      background: var(--surface);
      padding: 24px;
      border-radius: 12px;
    }
  }
}
```

### 3.2 Architecture

The DSL is **not** transpiled to Rust. The grammar lives in Zyntax (`.zyn` PEG with semantic actions), and the runtime lifts Zyntax's tiered backend whole-cloth — no parallel compiler. Two distribution modes share the same core:

```text
.blinc source
    |
    v
[Zyntax Grammar2] -> TypedProgram
    |
    v
[HIR lowering]    -> HirModule
    |
    +-----------------+----------------+
    |                                  |
    v                                  v
[Cranelift JIT]                  [LLVM backend]
  RuntimeEngine::Tiered            compile_module() -> .o
  hot-reload via                   ar / cc link -> .a or executable
  runtime.hot_reload(...)          (mobile static-link, desktop AOT)
    |                                  |
    v                                  v
[Live UI, dev hot-reload]      [Shipping binary, no JIT at runtime]
```

The join point is the `HirModule` produced by Zyntax's typed-AST → HIR lowering. Above it everything is shared (grammar, builtins, plugin registration). Below it the backend choice depends on platform and profile.

### 3.3 Crate layout

| Crate | Role |
|---|---|
| `blinc_dsl_core` | Grammar (`include_str!("../grammars/blinc.zyn")`), `RuntimeEngine` enum wrapping `ZyntaxRuntime`/`TieredRuntime`, codegen helpers. Shared by every consumer. |
| `zrtl_blinc` | Zyntax runtime library — `$Blinc$div` / `$Blinc$text` / `$Blinc$image` / `$Blinc$state_get`,`set` / `$Blinc$on_click_register` / `$Blinc$add_css` registered against existing `Div`/`Text`/`Image` builders + `BlincContextState`. Statically linked, not loaded from disk. |
| `blinc_dsl` | Rust embed API: `dsl::component(path) -> impl ElementBuilder`, `inline!` macro (P1), hot-reload integration via the existing `hot_reload::watch_dir` watcher. |
| `blinc_dsl_codegen` | LLVM AOT pipeline, lifted from `zyntax_cli/src/backends/llvm_aot.rs`. Emits `.o` via `target_machine.write_to_file(.., FileType::Object, ..)`, packages into `.a` (mobile static link) or links to executable (desktop AOT). |
| `blinc_cli` (extended) | Standalone driver — `blinc init`, `blinc dev`, `blinc build --target …`. Bundles LLVM (via inkwell) + per-platform pre-built `libblinc_<platform>.a` so non-Rust users don't need cargo on their machine. Same shape as `flutter build`. |

### 3.4 Distribution modes

**Rust embed mode** — for existing Blinc projects. `cargo add blinc_dsl`, then mix `.blinc` components with hand-written builders:

```rust
fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder {
    div()
        .child(text("Hand-written"))
        .child(blinc_dsl::component("ui/login.blinc")?)  // embed
}
```

**Standalone CLI mode** — for non-Rust users. Download the `blinc` CLI binary (no cargo install required); scaffold a pure-`.blinc` project and ship it:

```bash
blinc init my_app          # scaffolds .blinc files, no Cargo.toml
blinc dev                  # JIT runtime + watcher
blinc build --target ios   # AOT, statically linked, .ipa-ready
```

Both modes consume the same `blinc_dsl_core` and `zrtl_blinc`.

### 3.5 Platform matrix

| Target | Mode | Engine | Artifact |
|---|---|---|---|
| Desktop dev | JIT, watcher hot-reload | `TieredRuntime::development()` | `.blinc` source loaded at launch |
| Desktop release | JIT or AOT (user picks) | `TieredRuntime::production()` or LLVM AOT | JIT: `.blinc` shipped as resources / AOT: native binary, no DSL deps at runtime |
| iOS | AOT only (JIT forbidden) | LLVM AOT | `libblinc_dsl_aot.a` linked into Xcode build |
| Android | AOT only | LLVM AOT | `libblinc_dsl_aot.a` linked via NDK |
| Wasm | Deferred | — | Tracking Zyntax's wasm AOT story |

### 3.6 Type checking and diagnostics — leverage Zyntax

Zyntax already produces typed ASTs with type inference, validates call signatures against registered `NativeSignature`s, and surfaces parse / lowering / compile errors with file:line:col spans. Blinc relays those — we **do not** add a parallel check layer.

What we get for free by registering `$Blinc$*` builtins with proper signatures:

- **Call-site arity / type checking.** A `text(42)` call where the grammar declares `text(s: string) -> Element` becomes a Zyntax type error at compile time, not a runtime panic.
- **State field type inference.** `state count = 0` infers `i32`; `count -= 1` is checked against that type. Mismatches surface at `compile_typed_program` time, before any HIR lowering runs.
- **Interpolation type checking.** `"Count: {count}"` requires `count` to satisfy `ToString` (or whatever trait the interpolation builtin asks for); Zyntax's resolver enforces it.
- **Diagnostics with spans.** Parse errors carry `(file, line, col, len)`; lowering errors do too. We surface them through `BlincDslError` with the original span preserved, so the watcher / dev mode can paint a fallback overlay showing the failing file region.

What this means for the plan:

- Phase 1 prototype lists "type checker" was the wrong framing; the prototype validates that **registering builtin signatures correctly** propagates Zyntax's checks to the user.
- The risk-reduction probe should specifically exercise: (a) signature-mismatch errors land with usable spans, (b) `state` field reactivity types round-trip cleanly through the typed AST (is `count` exposed as `i32` or `State<i32>` to subsequent expressions? — needs probing), (c) diagnostics from a hot-reloaded `.blinc` file are catchable and actionable, not panics.

The `BlincDslError` type wraps `zyntax_embed::GrammarError` / `RuntimeError` and exposes a `.diagnostics() -> Vec<Diagnostic { file, span, severity, message }>` so the dev-mode overlay can render them inline. No custom error machinery for type inference itself.

### 3.7 Hot reload

Zyntax's tiered backend uses **beadie** for on-stack replacement (see `zyntax/crates/compiler/BEADIE_INTEGRATION.md`). When a `.blinc` file is recompiled via `runtime.compile_typed_program(...)`, beadie's `TieredAdapter` atomically swaps the function pointers via `Bead::swap_compiled` — including OSR for in-flight invocations of long-running functions. **No per-component `runtime.hot_reload(name, hir)` call required from Blinc; the runtime handles state preservation across the swap on its own.**

Plumbing on the Blinc side reduces to: `.blinc` file watcher (already shipped in `92d46b48`'s `hot_reload::watch_dir`) → re-parse + recompile via the same path used at first load. The frame loop calls the same component entry points; beadie hands them the new code on the next invocation.

Hot-reload is JIT-only by construction. iOS users always restart for code changes (LLVM AOT, no JIT).

### 3.8 Features

| Feature | Priority | Status | Notes |
|---------|----------|--------|-------|
| Component definitions | P0 | Shipped | `component Name { style, init, view }`, clause order free |
| Reactive state | P0 | Shipped | `signal x: T = v`; FSM `context` fields become signals |
| Template expressions | P0 | Shipped | f-string interpolation lowered to `string_concat` chains |
| Scoped CSS | P0 | Shipped | `style` block extracted and flushed into the context |
| Props / inputs | P0 | Shipped | Typed component parameters bound by `bind_component_props` |
| Conditional rendering | P0 | Shipped | `if/else` in view bodies, carried through `__cf_children__` |
| List rendering | P0 | Shipped | `items.map(\|x\| Row(x))` over a `const` list (expanded at compile time) or a `signal x: List` (walked at render). NOT `for x in xs` — see below |
| View-returning fns | P0 | Shipped | `fn Row(l: string): View { … }`; annotation optional, inferred from a body ending in a widget call |
| Event handlers | P0 | Shipped | `on_click = \|\| expr`, braces optional |
| Scoped re-render | P0 | Shipped | `with { … }` regions, deps observed from what the body actually read |
| Slot / children | P1 | Shipped | `slot Name { … }` partitions a widget body into named args |
| Import system | P1 | Partial | Named imports (`import { X } from "./m"`) work, with per-module symbol mangling so same-named components across files do not collide. Alias / glob / namespace / default forms need upstream work |
| Hot reload | P0 (dev) | Shipped | `reload_project` builds a fresh instance; signal values survive by name |
| Animation declarations | P1 | Not started | `animate` / `transition` have no DSL surface; CSS animations work |
| `for` loops | P1 | Blocked | A widget body drops `let`, so a desugared counter has nowhere to live. `map` covers list rendering, which was the motivating case |
| Visibility / `export` | P1 | Not started | `TypedVariable` carries the field; imports flat-merge, so enforcement has no layer yet |
| Standalone CLI build | P1 | Partial | `blinc_cli` has `init` / `dev` / `build`, but the AOT backend it would call does not exist |
| AOT pipeline | P0 (mobile) | Not started | `blinc_dsl_codegen` absent. Blocks iOS and Android, which forbid JIT |
| LSP server | P2 | Not started | Autocomplete, diagnostics, go-to-definition |
| VS Code extension | P2 | Not started | Syntax highlighting, inline preview |

Status verified against the tree rather than carried forward; several rows
here were stale for months. Two corrections worth stating outright:

- **List rendering does not use `for x in xs`.** Zyntax's `for` CFG
  construction is a stub, but that is not the blocker — a widget body
  drops `let` statements, so a desugared loop counter has nowhere to
  live. `map` sidesteps both and is the idiom.
- **Cross-file name collisions are already solved** by per-module symbol
  mangling. `export` would add encapsulation, not collision safety.

### 3.9 Implementation sequencing

1. **Risk-reduction prototype** (1–2 weeks). Counter component, two builtins (`$Blinc$text`, `$Blinc$on_click_register`), JIT only, Rust embed only. Validates:
    - Grammar → `TypedProgram` → `runtime.call` round-trip
    - Host-Rust closures invoking DSL functions and vice versa
    - `runtime.hot_reload` semantics for stateful UI (does swapping a `view` fn invalidate widget state in `BlincContextState`?)
    - Grammar startup cost — target <50 ms; if higher, switch to pre-compiled `.zpeg` via `include_bytes!`
    - **Diagnostic channel end-to-end.** Force a type error (`text(42)`), an arity error (`text()`), a state-field-type mismatch (`count: i32 = 0; count = "x"`), and a parse error in a hot-reloaded file. Confirm each surfaces with usable file:line:col spans, not panics. This is what tells us whether Zyntax's type-checking and diagnostics are production-grade for our use case or whether we'll need to wrap them.
    - **Reactivity round-trip through the typed AST.** Does Zyntax see `count` (declared as `state count: i32`) as `i32` or as some `State<i32>`-shaped wrapper inside expressions? Determines whether reactivity is a host-only concern or whether we need Zyntax-side type sugar.
2. **Core language + Rust embed v1** (3–4 weeks). Full grammar covering `state`/`view`/`style`/`if`/`for`/imports/events. `zrtl_blinc` covers all P0 builders. Hot-reload integration via the `.blinc` watcher branch. End state: any P0 Rust example portable to `.blinc`. **Ships first** — Rust users get value before standalone CLI lands.
3. **AOT pipeline + mobile** (3–4 weeks). LLVM AOT lifted from `zyntax_cli/src/backends/llvm_aot.rs`, cross-compile triples driven by `CARGO_CFG_TARGET_OS`, `.a` packaging via `ar`, iOS/Android runner integration. Validate iOS no-JIT compliance.
4. **Standalone CLI mode** (4–6 weeks). `blinc init`, project scaffolder, `blinc dev` for `.blinc`-only projects, `blinc build` driving the AOT pipeline, framework-prebuild CI producing per-platform `libblinc_<platform>.a`.
5. **Tooling + polish** — error reporting with file:line spans, `optimize_function` warm-up of view fns at first frame, profiling, LSP / VS Code extension.

### 3.10 Patterns lifted from Zyntax codebase

- Grammar embedding via `include_str!` — see `zyntax/crates/zynml/src/lib.rs:77`
- Stdlib resolver closure for framework modules — `zynml/src/lib.rs:186-197`, install before module load
- Plugins → grammar → module load order — `zynml/src/lib.rs:199-246`
- `RuntimeEngine` backend-agnostic enum — `zynml/src/lib.rs:141-144`
- AOT pipeline shape — `zyntax_cli/src/backends/llvm_aot.rs:103-278` (adapt `cc` linker step to `ar` for mobile `.a` output)

Patterns explicitly rejected: `.zrtl` plugin discovery from disk (Blinc statically links), env-var runtime profile dispatch, "find first function" entry-point heuristic, REPL.

---

## Phase 4: Rendering & GPU

> Goal: Complete the rendering pipeline for advanced visual content.

### 4.1 3D Mesh Rendering (P1)

| Feature | Status | Notes |
|---------|--------|-------|
| Generic mesh data | **Done** | `MeshData` + `Vertex` + `Material` — users convert from glTF/OBJ/FBX |
| draw_mesh_data | **Done** | `DrawContext::draw_mesh_data(mesh, transform)` — direct render, no registration |
| PBR materials | **Done** | `Material`: base_color, metallic, roughness, emissive, texture, alpha_mode |
| Shadow mapping | **Done** | 2048² depth pass, 4-tap PCF, front-face culling, depth bias |
| Normal / displacement maps | **Done** | Tangent-space normal maps + parallax occlusion mapping (16-layer) |
| Skeletal animation | **Done** | `Bone`/`Skeleton`/`SkinningData`, GPU skinning via storage buffer (max 256 joints) |

### 4.2 Custom Shaders (P2)

| Feature | Status | Notes |
|---------|--------|-------|
| Custom render pass API | **Done** | `CustomRenderPass` trait, PreRender/PostProcess stages, label-based removal |
| Custom bind groups | **Done** | `BindGroupBuilder` — declarative layout+bind creation for all binding types |
| Compute shader access | **Done** | `ComputeDispatch` + `create_compute_pipeline()` + `@flow` DAG |
| Post-processing pipeline | **Done** | `PostProcessChain` — ping-pong effect chain, auto texture management |

### 4.3 Performance (P1)

| Feature | Status | Notes |
|---------|--------|-------|
| draw_rgba_pixels | **Done** | `DrawContext::draw_rgba_pixels()` — GPU texture upload + render per frame |
| Dynamic image rendering | **Done** | `DynamicImage` in PrimitiveBatch, renderer uploads + draws via image pipeline |
| Virtualized list rendering | **Done** | `virtual_list()` — viewport-aware, variable-height, CSS classes |
| Texture atlas improvements | Done | SVG atlas, glyph atlas |
| Lazy image loading | **Done** | Viewport-aware load + 100px buffer, color/image/skeleton placeholders, fade-in animation, CSS `loading` property |
| Render region culling | **Done** | AABB visibility test before GPU upload, shadow/rotation/affine-aware |
| GPU memory budget | **Done** | `GpuMemoryBudget` with LRU eviction, 128 MB default, env var override |

### 4.4 Text & Fonts (P2)

| Feature | Status | Notes |
|---------|--------|-------|
| Lazy per-codepoint emoji loader | Planned | Network-fetched glyph loader for dynamic text — similar to Google Fonts' CSS2 API. Covers runtime strings (user input, fetched content, chat messages) whose codepoints weren't in the build-time emoji subset. Targets web/wasm and non-Apple platforms that don't ship a bundled Color Emoji font. Requires: (1) async `FontRegistry::load_glyph_async(codepoint)` entry point, (2) a hosted glyph service or a per-codepoint chunked font asset, (3) a pending-glyph placeholder in the shaper so layout doesn't shift when glyphs arrive. Escape hatch for the build-time emoji subsetter (which covers statically-known strings). |

---

## Phase 5: Developer Experience

> Goal: Make Blinc a joy to develop with.

### 5.1 Tooling (P1)

| Feature | Status | Notes |
|---------|--------|-------|
| Hot reload | Planned | File watcher + incremental rebuild |
| Visual inspector | Partial | blinc_debugger exists, needs UI overlay |
| Animation debugger | Planned | Timeline view, pause/step |
| Layout debugger | Planned | Flexbox visualization (like browser devtools) |
| Performance profiler | Planned | Frame time, GPU utilization, batch count |

### 5.2 IDE Integration (P2)

| Feature | Status | Notes |
|---------|--------|-------|
| VS Code extension | Planned | Zyntax syntax highlighting + preview |
| LSP for `.blinc` files | Planned | Autocomplete, diagnostics |
| Component preview | Planned | Inline rendering in editor |
| Code snippets | Planned | Common patterns and widgets |

### 5.3 Documentation (P1)

| Feature | Status | Notes |
|---------|--------|-------|
| Blinc Book | Partial | Core concepts, 3D rendering, flow shaders (vertex/material), routing, media |
| API reference (rustdoc) | Partial | Many crates need doc improvements |
| Interactive examples | **Done** | 40+ live WebGPU demos in mdBook gallery, auto-generated from cross-target examples |
| Video tutorials | Planned | Getting started, advanced topics |
| Skills.md (AI agents) | Done | Example-driven reference for LLMs |

---

## Phase 6: Accessibility

> Goal: Make Blinc apps usable by everyone.

| Feature | Priority | Notes |
|---------|----------|-------|
| Semantic element roles | P1 | Button, heading, list, etc. |
| Screen reader announcements | P1 | Platform accessibility APIs |
| Keyboard navigation (Tab order) | P1 | Focus ring, tab index |
| ARIA-like attributes | P1 | Label, description, live regions |
| High contrast mode | P2 | Theme variant |
| Reduced motion | P2 | Respect OS preference |
| Text scaling | P2 | Independent of DPI scaling |

---

## Phase 7: Platform Expansion

| Platform | Status | Notes |
|----------|--------|-------|
| macOS | Stable | wgpu (Metal) |
| Windows | Stable | wgpu (DX12/Vulkan) |
| Linux | Stable | wgpu (Vulkan) |
| Android | Stable | wgpu (Vulkan) |
| iOS | Stable | wgpu (Metal) |
| HarmonyOS | In progress | wgpu (Vulkan/OpenGL ES) |
| Web (WASM) | **Preview (Tier 2)** | wgpu WebGPU + WebGL2 fallback. Full render pipeline: SDF (split pipelines: core/shadow/3D/notch), text, mesh, @flow, motion, overlays, CSS animations/transitions, blend modes, layer effects. **WebGL2 fallback** (0.5.1): data texture path replaces storage buffers on GL adapters (Android Chrome, older browsers); VERTEX_STORAGE fallback via instance-stepped vertex buffers; glass and particle rendering pending on WebGL2. Chrome 113+, Edge 113+, Firefox 141+, Safari 18+ (macOS Tahoe), iPhone Safari (iOS 18+), Android Chrome (WebGL2). Text: Arial + FiraCode + JetBrains Mono bundled. Asset preload via `fetch()`. See [`docs/web.md`](docs/web.md). |
| Embedded (RPi) | Future | Framebuffer or Vulkan |

---

## Release Milestones

| Version | Target | Focus | Status |
|---------|--------|-------|--------|
| 0.2.0 | Q2 2026 | Desktop production readiness — system integration, multi-window, IME, clipboard, DnD, tray, hotkeys, code editor, virtual list | **Complete** |
| 0.3.0 | Q3 2026 | Mobile production readiness — gestures, safe area, haptics, navigation/router, deep linking, media (audio/video/camera), native bridge | **Complete** |
| 0.4.0 | Q3 2026 | GPU & rendering — 3D mesh pipeline (PBR, shadows, normal maps, skeletal animation), custom shaders (bind groups, compute, post-processing), render culling, memory budget | **Complete** (pulled forward from 0.5.0) |
| 0.5.0 | Q2 2026 | Web/WASM platform (WebGPU), rich text editor, virtualized list, lazy image loading, mobile soft keyboard + edit menu | **Complete** |
| 0.5.1 | Q2 2026 | WebGL2 fallback (data textures, vertex storage fallback), split SDF pipelines, video player fixes (wasm duration/pacing), platform capability detection | **Complete** |
| 0.6.0 | Q4 2026 | Blinc DSL v1 — Zyntax embed, Rust embed API (`blinc_dsl::component(path)`), JIT hot-reload, P0 grammar surface | Planned |
| 0.6.1 | Q1 2027 | LLVM AOT for mobile — iOS/Android static-lib pipeline, no-JIT compliance | Planned |
| 0.6.2 | Q1 2027 | Standalone `blinc` CLI — `blinc init`/`dev`/`build` for non-Rust users, framework-prebuild CI | Planned |
| 0.7.0 | Q1 2027 | Developer experience — hot reload, visual inspector, layout/animation debugger, performance profiler | Planned |
| 0.8.0 | Q1 2027 | Accessibility v1 — semantic roles, screen reader, keyboard navigation, ARIA-like attributes, high contrast, reduced motion | Planned |
| 0.9.0 | Q2 2027 | Platform expansion — HarmonyOS stable | In progress |
| 1.0.0 | Q3 2027 | Stable API, full documentation, Zyntax hot reload + LSP, production certification | Planned |

---

## Contributing

See individual crate READMEs for architecture details. The most impactful areas to contribute:

1. **Missing widgets** (Phase 1.4) — date picker, color picker, data grid
2. **Blinc DSL** (Phase 3) — Zyntax grammar (`grammars/blinc.zyn`), `zrtl_blinc` builtins, Rust embed API, AOT pipeline, standalone CLI
3. **Accessibility** (Phase 6) — screen reader, keyboard nav, ARIA
4. **Developer tooling** (Phase 5) — hot reload, visual inspector, debugger
5. **Documentation** — API docs, tutorials, interactive examples
