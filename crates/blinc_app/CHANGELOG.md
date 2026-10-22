# Changelog

All notable changes to `blinc_app` will be documented in this file.

## [Unreleased]

### Performance
- **Compositor fast path engages on composite-promotable CSS frames** — CSS-driven opacity / transform / single-shadow / single-filter animations now reuse the previous frame's static cache and only re-composite the overlay batch, instead of re-rasterising the whole subtree from SDF primitives. Drops per-frame CPU by 40–60% on css_features_demo's pulse / spin / drift animations.
- **Per-hit cache invalidation on pointer moves** — hover-driven CSS state changes now invalidate the static cache only for the actual hit node and its hovered ancestors, instead of flushing the whole cache. Combined with dropping DRAG / DRAG_END from the per-hit gate (drag pipelines route through their own invalidation), pointer-heavy demos (sortable, drag) keep the fast path alive between moves.
- **Render-cache invalidation hooks** wired into the spots where the previous-frame snapshot would actually go stale: stateful prop updates (`apply_state_callback`), structural subtree rebuilds (`process_pending_subtree_rebuilds`), and motion settle (when an `animate_bounds` / motion-binding FSM transitions from `Animating` → `Settled`). Pre-fix these silently kept blitting a stale snapshot for a frame after the underlying scene changed.
- **FPS-cap bypass for in-flight motion / vsync-class CSS** — when a motion FSM is mid-flight or any visible CSS animation touches a vsync-class property (transform, layout sizing, clip-path), the frame ships at native vsync regardless of `WindowConfig::animation_fps_cap`. Pairs with the `needs_vsync_for_smoothness` predicate so the cap only paces frames that genuinely tolerate sub-vsync rates.
- **Fast-path bail while overlays / bounds animations are live** — compositor fast path explicitly stands down while `OverlayStack` / `ToastTray` are dirty or any `animate_bounds` entry is mid-flight; the static cache isn't reliable across these subtree topologies.
- **`pre_warm_pipelines` on Vulkan** — desktop runners pre-compile path + SDF pipelines before the first surface present so the first user input doesn't pay a 100–300 ms shader compile spike on PowerVR / Mesa drivers.

### Added
- **Edition 2024 `+ use<>` and closure-wrapper documentation** on `WindowedApp::run` and `WindowedApp::run_with_theme`. The named-`fn`-as-builder pattern (`WindowedApp::run(config, build_ui)`) trips the higher-ranked `FnMut` bound on edition 2024 because RPIT now captures input lifetimes. The doc block lays out both fixes — closure wrapper `|ctx| build_ui(ctx)` (portable across editions) and `+ use<>` on the named fn (Rust 1.82+).
- **Asset hot-reload** under the `hot-reload` feature. New `blinc_app::hot_reload::watch_dir(path)` spawns a `notify`-backed file watcher that pushes changed paths onto the same invalidation queue dx's wire format would have populated. Image cache (`RenderContext::image_cache`, keyed by URI string) drops entries by path-suffix match against the absolute path the watcher reports — relative paths the user wrote in `image("assets/logo.png")` cleanly match against `/abs/.../assets/logo.png`. SVG document cache and SVG atlas are blanket-cleared on any invalidation since they're keyed by hash of source bytes with no path reverse-lookup. Why a Blinc-side watcher: dx's `assets: Vec<PathBuf>` field on `HotReloadMsg` is gated on Dioxus's `asset!()` macro (see `dioxus-cli/src/build/builder.rs::hotreload_bundled_assets`) and never fires for Blinc apps that don't pull in `dioxus-core`. The wire format is still parsed for forward-compat — when a future `blinc dev` driver ships its own dev server, it can populate the same channel without needing the watcher dance. Limitations: `include_bytes!` / `include_str!` assets live in rodata that subsecond can't touch, so swap to runtime `std::fs::read` paths if you want them reloadable; glyph caches and font face mappings aren't path-keyed and aren't invalidated yet (a font edit would invalidate the LRU but not the underlying fontdb entry).
- **CSS hot-reload** under the `hot-reload` feature. Inline `ctx.add_css(r#"..."#)` strings refresh on every subsecond patch — edit colors, padding, border-radius, etc. and the running window updates without resize or restart. Three pieces wire it together: (1) `WindowedContext::reset_for_hot_reload` clears the merged stylesheet, drops `css_sources`, resets `rebuild_count` to zero so the common `if ctx.rebuild_count == 0 { ctx.add_css(...) }` guard re-fires, and clears the global `ACTIVE_STYLESHEET` so widget state callbacks don't see a stale `Arc` mid-rebuild; (2) the hot-reload trigger sets `ws.render_tree = None` to route the rebuild through the no-existing-tree branch that calls `apply_all_stylesheet_styles` — `incremental_update` only diffs visual props and would skip CSS rule re-application, which is why pre-fix you had to resize the window to see the change; (3) the wake closure passed to `hot_reload::connect` now flips `frame_dirty` before calling `wake()`, mirroring the animation-scheduler wake — without it `Event::Frame` early-returned on the gate and the rebuild check never ran. Note: subsecond patches function bodies, not rodata, so a `const STYLESHEET: &str = "..."` keeps pointing at the pre-patch string after a patch — inline the literal in the `add_css` call instead. Same caveat for `static` strings and `&'static [u8]` font assets. See `docs/book/src/advanced/hot-reload.md` for the full caveat list.
- **Experimental hot-reload** via the `hot-reload` cargo feature on `blinc_app`. Wraps the user UI closure in `subsecond::call` so changes to the binary crate's UI code can be hot-patched at runtime. Currently driven by Dioxus's `dx serve --hotpatch` CLI; a native `blinc dev` driver is on the roadmap. Debug-only — release builds carry zero overhead even with the feature enabled. See `docs/book/src/advanced/hot-reload.md` for setup. Resolves issue #30 (Level 1 — minimum viable).

### Changed
- **Animation scheduler ticks on the main thread by default** (`AnimationThreadMode::Main`). Springs, keyframe animations, timelines, and `tick_callback`s now advance synchronously inside Phase 3 of each rendered frame, in lockstep with paint. Eliminates the half-frame jitter the bg-thread tick can introduce — animation values read at paint time are exactly in phase with the frame being drawn — and removes one thread from the runtime. Idle cost drops to zero (no thread to park). Apps that need fixed-rate ticking independent of rendering (game physics, audio sequencers, telemetry) opt into `AnimationThreadMode::Background` via `WindowConfig::animation_thread_mode(AnimationThreadMode::Background)` to get the prior bg-thread behaviour back.
- `AnimationScheduler::tick()` is now safe to call from the main thread under any mode — when a bg thread is running, it returns the latest activity state without advancing animations (preventing the double-tick race that existed before, where the bg thread and `RenderState::tick`'s call to `scheduler.tick()` both re-stepped springs every frame).
- `AnimationScheduler::tick()` now also invokes registered `tick_callback`s. Previously they only fired from the bg thread, which meant `Main`-mode apps never saw their callbacks run. Callbacks receive `dt` in seconds and run after the spring/keyframe/timeline pass.
- `add_spring` / `add_keyframe` / `add_timeline` / `add_tick_callback` / `set_spring_target` / `start_keyframe` / `start_timeline` now also fire `wake_callback` directly. In `Main` mode this is the only path that wakes the windowed runner when an animation registers from a non-event-handler context (custom timer thread, async task). In `Background` mode it duplicates the bg-thread's edge-trigger fire — harmless, the wake-proxy and `frame_dirty` flip on the receiving side are idempotent.

### Added
- `AnimationThreadMode` enum + `WindowConfig::animation_thread_mode(mode)` builder. Re-exported from `blinc_app::prelude`. See the docs on the enum for the trade-off.

### Fixed
- Raw-image corner-shape and overflow-fade changes participate in scratch cache fingerprints through the image-owned clip rows.
- Raw-image path clips retain changing contours and fill rules through canvas cache replay, foreground, motion and composite scratch rendering.
- Composite scratch fingerprints include raw-image shape clip payloads, so changing a polygon's fill rule invalidates the bake even when its vertices, destination and pixels are unchanged.
- Composite scratch fingerprints include raw-image affine matrices, so orientation changes invalidate a bake even when its destination and pixels are unchanged.
- Ordered foreground RGBA reaches direct, static-cache, canvas, motion and glass dispatch, including image-only groups. Foreground images are deferred with shapes during static baking and retained by cache guards. The separate-layer `render_tree` path now dispatches explicit foreground batches after its layer passes.
- Self-contained foreground shapes and paths retain group isolation in direct, canvas and motion overlay dispatch. Static-cache composition defers them until after canvas content so translucent groups blend once. Foreground-owned cache edits request a full repaint.
- Ordered RGBA batches survive dynamic and canvas dispatch, including image-only callbacks, pixel changes, paths and cache rebakes. Groups and paths stay ordered when callbacks stop emitting images, and foreground survives the glass/effect hand-off. Primitive-only cache patchers request full paint for images, and ordered content avoids duplicate path/z overlays.
- Composite scratch cache fingerprints include path geometry, command ranges, effects and 3D settings. Scratch-owned CSS indices request a full repaint before static-cache patching.
- Dynamic motion overlays retain layer commands and paths during both cached and direct dispatch. Effect and opacity groups use ordered single-sample composition; ordinary motion batches keep direct rendering. Cached opacity bindings patch the matching dynamic group and recover after zero opacity without recolouring children. CSS records routed into motion request a full repaint while CSS cache patching remains static-only.
- iOS-compile CI red: platform-gated theme + iOS overlay build errors on `aarch64-apple-ios` from a `cfg`-only path the overlay code was reaching for.
- **Residual idle CPU on Linux out-of-focus** (issue #28). The desktop platform shim's `new_events(WaitCancelled)` handler was calling `request_redraw()` on every window for every event-loop wake — including spurious wakes from Wayland / X11 compositors (focus subscriptions, configure events, raw input shifts). Each wake fired a redraw the windowed app's frame gate immediately threw away because `frame_dirty` was false; the wakeup-and-skip cycle showed up as a few percent of a CPU on a static, out-of-focus hello-world. The blanket `request_redraw` was redundant — `wake_proxy.wake()` arrives via `user_event(AppCommand::Wake)` which has its own `request_redraw`, and real input events flow through the windowed-app prelude which decides per-event whether to flip `frame_dirty`. Fix is to drop the `new_events` `request_redraw` entirely.
- **Bare-mouse-move CPU spike on Linux high-rate mice** (issue #28 follow-up, observed on Hyprland + RTX 5090 with a gaming mouse delivering 1 kHz `CursorMoved` events). The `Event::Input` branch's prelude unconditionally allocated a scratch `Vec<PendingEvent>`, a second scratch `Vec` for keyboard events, and a `Box<dyn FnMut>` event-router callback for every mouse-move — including bare moves over a `hello_blinc` UI with no handlers, no `:hover`/`:active` rules, and no `cursor:` styles. The `MouseEvent::Moved` branch had an early-return guard with the right predicate but only reached it after that allocation. At 1 kHz mouse rate the allocator overhead alone burned ~60% of one core. Fix is a hoisted early-return in the closure prelude that consults a new cached predicate on `RenderTree`. Static UIs now skip the entire `Event::Input` branch on bare moves.
- `tracing::trace!` target `blinc_platform_desktop::wakes` added — one line per event-loop wake with the `StartCause`. Use `RUST_LOG=blinc_platform_desktop::wakes=trace` to count wakeups/sec on a static UI when diagnosing idle-CPU regressions.
- `tracing::trace!` target `blinc_app::events` added — one line per windowed event with a `kind=` field (`input.mouse.moved`, `input.scroll`, `frame`, `window`, etc.). Pipe through `grep -oE 'kind=[^ ]+' | sort | uniq -c` over a sampling window to see counts per event-kind.

### Added
- `RenderTree::mouse_move_pipeline_needed()` — cached predicate behind a relaxed atomic that combines `has_any_pointer_handler() || stylesheet pointer-state-rules || has_any_cursor_style()` into a single load. Lazily recomputed on first read after a mutation invalidates it. Used by the windowed app's input prelude to short-circuit bare mouse-moves before any allocator work runs.
- `RenderTree::invalidate_mouse_move_pipeline_cache()` — public invalidator. Called by every mutation site that could change handler registration, stylesheet pointer-state rules, or per-node `cursor:` props (`set_stylesheet*`, `update_if_changed`, `incremental_update`, `process_pending_subtree_rebuilds`, `rebuild_children`, `apply_stylesheet_base_styles`, `apply_stylesheet_state_styles`).

### Added
- Display refresh rate detection at window resume — feeds `WindowConfig.max_frame_latency`-clamped `set_target_fps` so the scheduler doesn't burn 120 ticks/sec on a 60 Hz panel.
- `WindowConfig::max_frame_latency` is honoured for the primary surface (cap captured pre-move as `primary_max_frame_latency`).
- `WindowConfig::animation_fps_cap` is honoured by the redraw chain. When the only active redraw signals are animation progress (CSS keyframes / transitions / motion / theme / flow), the chain calls `WakeProxy::wake_at(1000/N ms)` instead of `request_redraw()` — pacing animation-only frames at the configured rate while leaving input / scroll / drag / cursor frames at native vsync. Halved idle CPU on `styling_demo` at `Some(30)`.
- Per-property smoothness gate sits on top of the animation FPS cap. Even when the cap would otherwise apply, the next frame ships at native vsync if any visible animation is touching a vsync-class property — transforms, 3D rotation, layout sizing (width / height / padding / margin / gap / inset), font-size, clip-path geometry — or any visible FLIP / `animate_bounds` entry is active. Backed by a new `KeyframeProperties::needs_vsync_for_smoothness` predicate.
- `tracing::trace!` target `blinc_app::redraw_signals` logs which of the nine end-of-frame signals (animation / cursor / motion / scroll / overlay / theme / css / pointer-query / flow) drove a redraw — silent on a quiet idle.

### Changed
- Removed the perpetual no-op tick callback that kept the animation scheduler thread alive at 120 fps. Combined with the scheduler park/wake change, idle apps now use ~0% CPU (issue #28).
- `needs_overlay_redraw` derived from `has_animating_overlays()` instead of `has_visible_overlays()` — a static popover no longer pins the redraw chain at vsync.
- `SvgAtlas` allocates its 1024×1024 RGBA texture and CPU shadow buffer (~8 MB total) lazily on first SVG insert. Apps that never render an SVG never pay it.
- Scrollbar-state diagnostic logs in `render_node` downgraded from `info` to `trace` (was firing every frame for every scroll container).
- **Default `windowed` feature is now lean** (issue #29). Convenience surfaces — file dialogs (`rfd`), tray + menus (`tray-icon`, `muda`), notifications (`notify-rust`), global hotkeys (`global-hotkey`) — moved out of the `windowed` umbrella into their own opt-in features: `dialogs`, `tray`, `notifications`, `hotkeys`. Saves ~130 transitive crates on Linux for apps that don't use them (404 → 270 deps via `cargo tree`). Existing code keeps compiling thanks to the no-op stubs in each module — calls become silent no-ops until the corresponding feature is enabled. To restore the pre-split behaviour, depend on `blinc_app` with `features = ["windowed-full"]`.

### Added
- `BlincApp::has_storage_buffers()` API for platform capability detection

### Changed
- Web surface configuration: COPY_SRC conditionally based on adapter support
- Web surface format: non-sRGB on WebGL2 GL adapter

## [0.5.0] - 2026-04-10

### Added

- **Web/WASM platform support** (`WebApp` runner):
  - Desktop-parity frame loop: overlays, motion containers, CSS animations/transitions, FLIP, pointer query, blend modes, @flow shaders, theme tick, focus sync
  - `WebAssetLoader` for image loading via browser `fetch()` API + `insert_asset()` for bundled assets
  - `preload_assets(&[urls])` async method for runtime image fetching
  - Per-frame wheel delta coalescing with sub-linear damping for smooth scroll
  - Scroll bounce-back physics with wheel-idle debounce (32ms overscroll / 120ms in-bounds)
  - Touch gesture-end fires `on_gesture_end()` immediately (no momentum tail)
  - `set_blend_target()` wired for CSS `mix-blend-mode` support
  - `OverlayContext::init()` for blinc_cn component overlay support
  - Fixed canvas viewport via `data-width`/`data-height`/`data-dpr` attributes
  - `BlincConfig { max_primitives: 20_000 }` default for wasm (up from 10k)
  - `RenderState` created in `WebApp::new` for full render path
  - `process_pending_scroll_refs()` wired for `ScrollRef::scroll_to`
  - Global animation scheduler registered (`blinc_animation::set_global_scheduler`)
  - Animation-driven stateful rebuilds (`check_stateful_animations`)
  - CSS stylesheet layout overrides on subtree rebuilds
- **Auto-generated web example wrappers** (`tools/build-web-examples`):
  - Cross-target convention: `pub fn build_ui(ctx) -> impl ElementBuilder`
  - 40+ examples auto-discovered, wrapper crates generated
  - mdBook gallery with lazy-loaded iframes
  - `--build` flag with git-diff incremental planner
  - `--force-rebuild` escape hatch
  - Image asset auto-detection and `preload_assets` codegen
  - CI staging copies per-example assets alongside wasm output
  - Gallery sub-pages with "best viewed in full window" tip
- **Monospace font resolution**: JetBrains Mono bundled, generic font cache invalidation after font load
- **SVG color tint**: post-rasterization `apply_tint()` for non-currentColor sources
- **SVG atlas eviction**: per-frame mark-and-sweep prevents animated SVG atlas overflow
- Mobile iOS edit menu visibility fix, paste round-trip, long-press word select
- Soft keyboard + text input cursor positioning on mobile

### Fixed

- Glass shader `textureSample` → `textureSampleLevel` for WebGPU uniform-control-flow compliance
- Image shader `fwidth` → constant AA width for WebGPU compliance
- Composite shader (layer effects) `textureSample` → `textureSampleLevel`
- Drop shadow + glow shader `textureSample` → `textureSampleLevel`
- `std::time::SystemTime` → `web_time::SystemTime` in code editor double-click detection
- SVG atlas blink on animated SVGs (eviction moved to `begin_frame`)
- CSS styling lost after stateful interaction (missing `apply_stylesheet_layout_overrides` on subtree rebuild)
- 13 examples moved `ctx.add_css()` from `fn main` into `build_ui` for web compatibility
- CI: removed fragile pkg/ cache, widened paths trigger, fresh binaryen for cn_demo wasm-opt crash
- Adapter limits: wasm uses `supported.clone()` for browser-safe device creation
- VERTEX_STORAGE capability check with descriptive error for Safari
- Surface `COPY_SRC` usage for blend mode two-pass compositing on Chrome

## [Unreleased]

### Added

- Lazy image loading completion:
  - Skeleton shimmer placeholder rendering with auto-redraw animation
  - Image placeholder rendering (type 2) using preloaded thumbnail/blur-hash
  - Fade-in animation when images finish loading (configurable per element)
  - `image_load_times` HashMap tracks when each image was first cached
  - Placeholder images are eagerly preloaded so they're ready before main render
  - CSS overrides for `loading_strategy`, `placeholder_type`, `placeholder_color`, `placeholder_image`, `fade_duration_ms` flow through `RenderProps`

## [0.4.0] - 2026-04-05

### Added

- Multi-window support (desktop):
  - `open_window(config)` / `open_window_with(config, builder)` — global APIs
  - `WindowId` type for platform-agnostic window identification
  - `WindowState` struct bundles all per-window state
  - `AppCommand` enum for cross-thread window creation via `EventLoopProxy`
  - `GpuRenderer::create_surface()` for shared-device surface creation
  - Secondary windows render custom UI via builder closures with DPI scaling
- Window management:
  - `WindowConfig::min_size()`, `max_size()` — size constraints
  - `WindowConfig::position()`, `center()` — initial positioning
  - `Window::set_position()`, `center_on_screen()`, `set_size()` — runtime positioning
  - `Window::drag_window()`, `minimize()`, `maximize()`, `close()` — window controls
- Custom title bars:
  - `.drag_region()` on Div — OS window drag on mouse down
  - `window_actions` module — `drag_window()`, `minimize_window()`, `maximize_window()`, `close_window()` callable from anywhere
  - `DesktopApp` manages `HashMap<WinitWindowId, DesktopWindow>`
  - Events tagged with `WindowId` for per-window routing
- Native file dialogs: `open_file()`, `save_file()`, `pick_folder()` with builder API
  - File type filters via `FileFilter::new("Images").ext("png").ext("jpg")`
  - Multi-file selection via `pick_many()`
  - Starting directory and default file name options
  - Desktop-only (behind `windowed` feature, uses `rfd` crate)
- Soft keyboard show/hide on mobile text widget focus (Android + iOS)

### Fixed

- Combobox dropdown scroll by using registered scroll physics directly
- Checkbox background not updating on check/uncheck
- Toast slide-in entering from middle instead of right edge
- Toast positioning via absolute layout for full viewport coverage

## [0.1.15] - 2026-03-22

### Fixed

- Combobox dropdown scroll by using registered scroll physics directly from the windowed context

## [0.1.14] - 2026-02-24

### Added

- `css_debug` example updated with stateful container tests for CSS var(), percentage, color inheritance, and inner child click persistence

## [0.1.13] - 2026-02-18

### Added

#### Pointer Query Pressure & Touch Physics

- Desktop: mouse press → binary 0/1 pressure, touch events → hardware pressure via `Force::Normalized`
- Desktop: `HashSet<u64>` active touch ID tracking for accurate `pointer-touch-count`
- iOS: forward `touch.force` to `pointer_query.set_pressure()`, track active touch count
- iOS: `blinc_handle_touch_with_force` FFI for Swift callers to pass force data (backward-compatible)
- Android: forward primary pointer pressure and touch count from `MotionEvent`

#### Flow Shader Direct Graph Support

- `FlowElement` carries optional `Arc<FlowGraph>` for direct graph rendering (bypasses stylesheet lookup)
- Render loop prefers direct graph over stylesheet-defined flow when both are available
- `semantic_flow_demo` example: added `flow!` macro plasma card demonstrating direct `div().flow(graph)` API

#### SVG CSS Animations

- SVG fill, stroke, stroke-width animatable via `@keyframes` and CSS transitions
- Stroke-dasharray/dashoffset animation for SVG line-drawing effects
- SVG path morphing via `d: path("...")` CSS animation (cubic bezier interpolation)
- SVG sub-element metadata extraction (`extract_element_metadata`) for future per-element targeting
- `svg_animation_demo` example demonstrating all SVG animation phases

### Fixed

- Double border on CSS-transformed image containers: removed redundant `parent_border` overlay from image rendering (border from `render_layer_with_motion` is sufficient and transform-aware)
- Text in stacked/absolute elements now clips correctly within ancestor scroll containers (sharp clip intersects with existing scroll clip instead of replacing it)
- Text decorations now render for all z-layers in the fast path (was only rendering z=0, dropping decorations when blend mode layers activated the fast path)
- Text and SVG elements now clip to scroll container boundaries (regression from dual-clip refactor)
- SVG own-transform applied correctly (not just inherited parent transforms)
- CSS `transform: rotate()` animation uses original angle values instead of lossy atan2 decomposition
- Performance: SVG string manipulation only runs on cache miss (not every frame)

#### 3D SDF & Styling Demo

- Expanded `styling_demo` example with 3D shape showcases (box, sphere, cylinder, torus, capsule)
- 3D boolean operations demo (union, subtract, intersect, smooth variants)
- 3D group composition examples with compound shapes
- UV-mapped gradient backgrounds on 3D surfaces
- `translate-z` depth positioning examples
- Blinn-Phong lighting configuration examples

#### Music Player Glass Card Demo

- `music_player` example: iOS-style "Now Playing" card with liquid glass morphism
- All visual styling driven by CSS via `ctx.add_css()`
- Glass card with `backdrop-filter: liquid-glass()` refracted bevel borders
- Album art, song info, progress bar with track glow animation
- Playback controls with glass icon wrappers
- Hover effects: icon/badge scale + glass brightening + shadow deepening + SVG tint transitions
- Progress bar hover-reveal: height transition with overflow clip, opacity-faded time labels

#### SVG CSS Transform Propagation

- SVGs now inherit CSS transforms from ancestor elements
- Affine transform decomposed into scale (applied to SVG bounds) + rotation (GPU shader)
- Layout recomputed after state style changes that affect layout (visibility, display, height, etc.)

#### Stylesheet Runtime Integration

- CSS animation support wired through the app runtime
- `backdrop-filter` property support in windowed runner
- Stylesheet base styles applied after tree construction
- CSS transition ticking and application in frame loop
- Animated layout property support with per-frame `compute_layout()` recomputation

#### Styling Demo Enhancements

- CSS filter hover demo (`.filter-card:hover` with brightness, saturate, contrast)
- Filter blur & drop-shadow demos (static blur, hover transition, keyframe animation, combo)
- Backdrop-filter animation demos (static blur, hover transition, blur+saturate combo, keyframe pulse)
- `:is()` / `:where()` / `*-of-type` selector demos

#### CSS Form Input Styling

- Form input styling demo section with CSS-styled TextInput and TextArea widgets
- `#demo-input` with `:hover`, `:focus`, `::placeholder` pseudo-class/element demos
- `#accent-input` with warm color scheme (yellow/amber) CSS styling
- `#disabled-input` with `opacity: 0.5` disabled state demo
- `#demo-textarea` with `:hover`, `:focus` CSS styling and `caret-color` demo
- Text input focus bridge to EventRouter for `:focus` CSS matching in windowed runner

### Fixed

- Backdrop-filter demo parent containers now include `rounded(12.0)` so glass corner radius is visible against page background
- CSS animation ticking moved to synchronous main-thread execution to eliminate phase jitter caused by background-thread timing misalignment
- Mid-frame transition redraw: transitions created during `apply_complex_selector_styles` now properly trigger frame requests (prevents stalled hover-leave animations)
- iOS runner cleanup for platform trait consistency
- Clippy warnings in windowed.rs and ios.rs

## [0.1.12] - 2025-01-19

### Added
- Momentum scrolling for touch devices with velocity tracking
- `dispatch_scroll_chain_with_time()` method for mobile scroll dispatch with time-based velocity
- Single-threaded animation scheduler for mobile efficiency

### Changed
- Android render loop now uses vsync for frame pacing instead of manual timing
- Non-blocking poll when animating, 100ms idle timeout for power saving
- Re-enabled scroll physics for bounce animations on Android

### Fixed
- Animation smoothness on Android by removing mutex contention between threads
- Double-waiting issue that was cutting frame rate in half
- Added expected cfg values for fuchsia and ohos targets to fix CI warnings

## [0.1.1] - Initial Release

- Initial public release with desktop, Android, and iOS support
