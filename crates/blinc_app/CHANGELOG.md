# Changelog

All notable changes to `blinc_app` will be documented in this file.

## [Unreleased]

### Accessibility
- iOS accessibility snapshot/action FFI connects GPU-rendered controls to native UIKit accessibility elements and text-field editing. The host scales content and input coordinates with Dynamic Type.
- The registered iOS Rust builder uses the complete UI build path, including queued core styles, overlays and build-scope cleanup.

### Changed
- A desktop window that has drawn nothing for two seconds frees its app's between-frame GPU memory: the window-sized static layer, the offscreen targets and the pooled layer textures (`BlincApp::release_idle_memory`). The next frame paints in full and allocates them again. Not under Wayland keep-alive, which never stops drawing.
- The desktop app loads its default system font by path, which maps it, instead of reading it into memory: on macOS, SF's `SFNS.ttf` is no longer copied into the heap.
- Each frame rendered with `render_tree_with_motion_opt`, fast paint or full, calls `GpuRenderer::next_frame`, so an overlay pass that drew only while the app started frees its window-sized texture a second later.
- The iOS and Android runners present through the compositor as the desktop runner does: a frame with no input, state change, scroll or CSS motion blits the cached static layer and re-records only canvas and overlay content, instead of repainting the tree. Input, text and key events, keyboard insets, rebuilds, stateful updates, scrolling, overlays and CSS state changes request a full paint, and a full paint runs at least once a second. The iOS surface is configured with `COPY_DST`; Android adds it when the surface supports it and otherwise keeps the direct path.

### Fixed
- The desktop and web runners apply the stylesheet's class rules again to the nodes an incremental update replaced the props of, after an update that changed only looks (`VisualOnly`) or layout (`LayoutChanged`). The update replaces node props with the new tree's props, which carry no class styles, so every element whose background or text colour came from a class lost it and the window turned black (#70). Nodes the update left alone keep their styles and are not visited.
- Letter spacing is drawn in device pixels, scaled like the font size. It was applied in points to text drawn at device size, so at 2x and 3x a letter-spaced run drew narrower or wider than it measured, and a text field's caret drifted into the last characters as the text grew.
- The iOS and Android runners apply `:hover`, `:active` and `:focus` styles only when the event router's hover, press or focus state changes, or after a rebuild or prop update, as the desktop runner does. The pass walks every styled element and ran on every frame, which was most of an idle frame's CPU on iOS.
- The iOS accessibility snapshot is reused until something raises a paint request, and for at most 500 ms. It walked and serialised the whole semantic tree on every frame.
- `RenderContext::render_tree_with_motion_opt` with `try_fast_paint = false` always repaints the tree. The compositor's inner call could still reuse the cached layer for a caller that asked for a full paint.
- `apply_binding_deltas` accepts a frame with no composite bindings when the cached dynamic batch holds no foreground or ordered content. A batch that had only been through cross-stream resolution was rejected, which invalidated the static layer on every frame of an app with a canvas.
- The iOS runner ticks scroll physics on every frame and keeps drawing while a fling runs. It ticked only on frames that rebuilt the tree, so a flick advanced in occasional steps and then stopped, instead of gliding.
- The iOS runner ticks CSS transitions and animations and applies `:hover`, `:active` and `:focus` styles on every frame, not only on a rebuild, so press and focus states change as they do on Android and desktop. A touch requests a redraw.
- The iOS and Android runners reparse CSS and rebuild when the theme changes, as the desktop runner does.
- The web runner marks the motions still in the tree and closes the motion frame on every frame, as the desktop runner does. A motion unmounted without a rebuild exits and is dropped, and a Motion shown again after a theme switch plays its entrance instead of staying in its finished state.
- Text no longer wraps one character per line at 3x display density when layout rounds its box a fraction of a pixel narrower than the text. The wrap allowance scales with the density, so Calendar day numbers stay on one line on phones.
- A one-finger drag scrolls containers in worker-hosted web demos. `WebInput` gains `TouchStart`, `TouchScroll` and `TouchEnd`, which the worker runner handles as the main-thread runner does: the drag advances the scroll container under the finger, and lifting it lets a flick carry on.
- `use_state_keyed` and `use_signal_keyed` run their initial-value closure with no lock held. A closure that creates a signal, such as a text input state, or calls another hook no longer deadlocks the UI thread on first build.
- iOS text measurement shares the renderer’s font registry, keeping carets aligned with fallback glyphs.
- Browser wheel input is dispatched before pointer movement changes its target, preserving swipe cancellation and preventing queued input from navigating another Calendar.
- Browser wheel input recognizes gradually accelerating strokes after a momentum tail, so Calendar swipes continue after a completed or cancelled gesture without an outside click.
- Nested CSS transforms keep collected content in its parent's coordinate system, including rotated Tree chevrons inside sliding drawers.
- Web wheel input preserves source timestamps and successive finger-stroke boundaries, including Magic Mouse momentum tails and reversals, instead of combining them into one latched gesture.
- Direct rendering paints foreground popup surfaces after ordinary text and SVGs, keeping background captions behind open controls.
- Desktop and web runners reconcile overlay focus before styling and share navigation handling. Web consumes navigation keys without browser defaults or duplicate text input; desktop maps Shift+F10 for context menus.

### Added
- `blinc_app::web::WorkerRuntime` drives a worker-owned `WebApp` from a host (frames, JSON input, resize, status), and `set_asset_base_url` resolves relative asset paths against the page instead of the worker script.
- Mobile IME connections retain UIKit marked text and Android composing spans, synchronize candidate replacements and selections, and reject edits from stale focus sessions. Generated mobile hosts include the native editing bridge.
- Opt-in `css-hot-reload` feature with `WindowedContext::watch_css` and `unwatch_css`. Debug desktop builds reload named CSS sources on save, wake idle windows, preserve component state and retain the last valid styles on errors. Watchers and file reads are excluded from release, web and mobile builds.
- Named CSS overrides through `WindowedContext::set_css` replace rules without resetting component state. Theme and root variables are shared across sources; invalid edits preserve the previous styles.
- `WebApp::new_offscreen`, normalized `WebInput` dispatch and host-driven resize/frame APIs share the DOM runner’s rendering pipeline for dedicated workers.
- `WindowedContext::headless_with_scale` separates logical layout from capture pixel density.

### Changed
- `BlincComponent` generates companion reactive props with `IntoReactive` setters, and ordinary field hooks return `Signal`. Props preserve Signal and Computed bindings.

## [0.6.0] - 2026-11-04

### Changed
- Flow shaders are drawn by canvases (`Canvas::flow`, `DrawContext::draw_shader`) as canvas GPU passes, and no longer collected from elements. The element path, its caches and `FlowElement` are removed.

### Fixed
- Collected text inherits overflow fade from clipping ancestors in main-tree and overlay rendering, including styled runs and promoted text layers.
- A rebuild that makes a fresh tree disposes what the builds before it created. The windowed (every window), web, iOS and Android runners built each widget under no scope, so the computeds and effects a widget makes while building lived as long as the process: a window resized twenty times left twenty effects for each bound number input, all running on every change of its value. A build merged into the existing tree is kept until the next fresh tree. `cn::bind_bool_targets` registers its once-per-signal effect outside any build's scope.
- A CSS-animated element under an overlay is drawn below it. Promoted layers (a spinner's rotating arc, a pulsing bar) were composited after the dynamic batch that holds a toast or popover, so they drew over it; layers of the main tree now composite before the batch and those inside an overlay subtree after it. Their text and SVG still draw after it.
- A CSS-animated element scrolled wholly out of its ancestors' clips is hidden. Its composited layer was blitted with no clip when the clips left nothing visible, so an indeterminate progress bar scrolled under a fixed header drew over the header. Ancestor clips that do not overlap now give an empty clip, and the layer is skipped.
- Styled text (`RichText`) without a colour of its own takes the CSS colour it inherits. It took only a colour set on its own node, so text under a coloured container drew in the element's default.
- A bound value a scroll or drag handler writes is drawn at once. The window drew after input only when something called `request_redraw()` or queued a rebuild, so a signal written from a scroll handler waited for an unrelated frame, such as the pointer leaving and re-entering.
- A trackpad scroll gesture stays with the scroll handler it started on, its momentum included, wherever the pointer goes, so a swipe on one element no longer drives the one the pointer moves onto. The window reports the platform's scroll phases to the render tree, which tells momentum from a new touch even where the platform reports momentum starting as a touch.

### Performance
- A window showing an unfocused text field goes idle. The compositor's end-of-frame redraw signal counts only canvases that ask for frames (`RenderTree::canvas_wants_frames`), not every visible one.
- A spinning `cn::spinner` or other promoted motion layer keeps the compositor fast path while a cross-stream opacity group is cached elsewhere on the page. Every such frame used to repaint the whole tree, because the binding patcher rejected any cached group while any motion binding existed. The fast path now skips the patcher when every painted binding is a promoted layer, which it blits at its current transform; a frame that asks for a full paint still walks the tree, so scrolling the spinner away lets the window go idle.
- **Compositor fast path engages on composite-promotable CSS frames** — CSS-driven opacity / transform / single-shadow / single-filter animations now reuse the previous frame's static cache and only re-composite the overlay batch, instead of re-rasterising the whole subtree from SDF primitives. Drops per-frame CPU by 40–60% on css_features_demo's pulse / spin / drift animations.
- **Per-hit cache invalidation on pointer moves** — hover-driven CSS state changes now invalidate the static cache only for the actual hit node and its hovered ancestors, instead of flushing the whole cache. Combined with dropping DRAG / DRAG_END from the per-hit gate (drag pipelines route through their own invalidation), pointer-heavy demos (sortable, drag) keep the fast path alive between moves.
- **Render-cache invalidation hooks** wired into the spots where the previous-frame snapshot would actually go stale: stateful prop updates (`apply_state_callback`), structural subtree rebuilds (`process_pending_subtree_rebuilds`), and motion settle (when an `animate_bounds` / motion-binding FSM transitions from `Animating` → `Settled`). Pre-fix these silently kept blitting a stale snapshot for a frame after the underlying scene changed.
- **FPS-cap bypass for in-flight motion / vsync-class CSS** — when a motion FSM is mid-flight or any visible CSS animation touches a vsync-class property (transform, layout sizing, clip-path), the frame ships at native vsync regardless of `WindowConfig::animation_fps_cap`. Pairs with the `needs_vsync_for_smoothness` predicate so the cap only paces frames that genuinely tolerate sub-vsync rates.
- **Fast-path bail while overlays / bounds animations are live** — compositor fast path explicitly stands down while `OverlayStack` / `ToastTray` are dirty or any `animate_bounds` entry is mid-flight; the static cache isn't reliable across these subtree topologies.
- **`pre_warm_pipelines` on Vulkan** — desktop runners pre-compile path + SDF pipelines before the first surface present so the first user input doesn't pay a 100–300 ms shader compile spike on PowerVR / Mesa drivers.

### Added
- Page Up and Page Down reach key handlers as key codes 33 and 34, as in the DOM; they were dropped.
- **Edition 2024 `+ use<>` and closure-wrapper documentation** on `WindowedApp::run` and `WindowedApp::run_with_theme`. The named-`fn`-as-builder pattern (`WindowedApp::run(config, build_ui)`) trips the higher-ranked `FnMut` bound on edition 2024 because RPIT now captures input lifetimes. The doc block lays out both fixes — closure wrapper `|ctx| build_ui(ctx)` (portable across editions) and `+ use<>` on the named fn (Rust 1.82+).
- **Asset hot-reload** under the `hot-reload` feature. New `blinc_app::hot_reload::watch_dir(path)` spawns a `notify`-backed file watcher that pushes changed paths onto the same invalidation queue dx's wire format would have populated. Image cache (`RenderContext::image_cache`, keyed by URI string) drops entries by path-suffix match against the absolute path the watcher reports — relative paths the user wrote in `image("assets/logo.png")` cleanly match against `/abs/.../assets/logo.png`. SVG document cache and SVG atlas are blanket-cleared on any invalidation since they're keyed by hash of source bytes with no path reverse-lookup. Why a Blinc-side watcher: dx's `assets: Vec<PathBuf>` field on `HotReloadMsg` is gated on Dioxus's `asset!()` macro (see `dioxus-cli/src/build/builder.rs::hotreload_bundled_assets`) and never fires for Blinc apps that don't pull in `dioxus-core`. The wire format is still parsed for forward-compat — when a future `blinc dev` driver ships its own dev server, it can populate the same channel without needing the watcher dance. Limitations: `include_bytes!` / `include_str!` assets live in rodata that subsecond can't touch, so swap to runtime `std::fs::read` paths if you want them reloadable; glyph caches and font face mappings aren't path-keyed and aren't invalidated yet (a font edit would invalidate the LRU but not the underlying fontdb entry).
- **CSS hot-reload** under the `hot-reload` feature. Inline `ctx.add_css(r#"..."#)` strings refresh on every subsecond patch — edit colors, padding, border-radius, etc. and the running window updates without resize or restart. Three pieces wire it together: (1) `WindowedContext::reset_for_hot_reload` clears the merged stylesheet, drops `css_sources`, resets `rebuild_count` to zero so the common `if ctx.rebuild_count == 0 { ctx.add_css(...) }` guard re-fires, and clears the global `ACTIVE_STYLESHEET` so widget state callbacks don't see a stale `Arc` mid-rebuild; (2) the hot-reload trigger sets `ws.render_tree = None` to route the rebuild through the no-existing-tree branch that calls `apply_all_stylesheet_styles` — `incremental_update` only diffs visual props and would skip CSS rule re-application, which is why pre-fix you had to resize the window to see the change; (3) the wake closure passed to `hot_reload::connect` now flips `frame_dirty` before calling `wake()`, mirroring the animation-scheduler wake — without it `Event::Frame` early-returned on the gate and the rebuild check never ran. Note: subsecond patches function bodies, not rodata, so a `const STYLESHEET: &str = "..."` keeps pointing at the pre-patch string after a patch — inline the literal in the `add_css` call instead. Same caveat for `static` strings and `&'static [u8]` font assets. See `docs/book/src/advanced/hot-reload.md` for the full caveat list.
- **Experimental hot-reload** via the `hot-reload` cargo feature on `blinc_app`. Wraps the user UI closure in `subsecond::call` so changes to the binary crate's UI code can be hot-patched at runtime. Currently driven by Dioxus's `dx serve --hotpatch` CLI; a native `blinc dev` driver is on the roadmap. Debug-only — release builds carry zero overhead even with the feature enabled. See `docs/book/src/advanced/hot-reload.md` for setup. Resolves issue #30 (Level 1 — minimum viable).

### Changed
- Context regression tests and their shared helpers live under `context/tests/`, leaving production modules with test declarations only.
- Decompose inherited element collection, node content, subtree bake updates, text preparation and frame dispatch into bounded context modules. The context shell and every production module stay below 1,000 lines; rendering order, public signatures and cache ownership are preserved.
- Split canvas replay, cache lifecycle, animation updates, image/SVG rendering, overlays and compositor helpers into private context modules. Public API paths and rendering behavior are preserved. Comments describe current contracts without line references or historical issue narratives.
- **Animation scheduler ticks on the main thread by default** (`AnimationThreadMode::Main`). Springs, keyframe animations, timelines, and `tick_callback`s now advance synchronously inside Phase 3 of each rendered frame, in lockstep with paint. Eliminates the half-frame jitter the bg-thread tick can introduce — animation values read at paint time are exactly in phase with the frame being drawn — and removes one thread from the runtime. Idle cost drops to zero (no thread to park). Apps that need fixed-rate ticking independent of rendering (game physics, audio sequencers, telemetry) opt into `AnimationThreadMode::Background` via `WindowConfig::animation_thread_mode(AnimationThreadMode::Background)` to get the prior bg-thread behaviour back.
- `AnimationScheduler::tick()` is now safe to call from the main thread under any mode — when a bg thread is running, it returns the latest activity state without advancing animations (preventing the double-tick race that existed before, where the bg thread and `RenderState::tick`'s call to `scheduler.tick()` both re-stepped springs every frame).
- `AnimationScheduler::tick()` now also invokes registered `tick_callback`s. Previously they only fired from the bg thread, which meant `Main`-mode apps never saw their callbacks run. Callbacks receive `dt` in seconds and run after the spring/keyframe/timeline pass.
- `add_spring` / `add_keyframe` / `add_timeline` / `add_tick_callback` / `set_spring_target` / `start_keyframe` / `start_timeline` now also fire `wake_callback` directly. In `Main` mode this is the only path that wakes the windowed runner when an animation registers from a non-event-handler context (custom timer thread, async task). In `Background` mode it duplicates the bg-thread's edge-trigger fire — harmless, the wake-proxy and `frame_dirty` flip on the receiving side are idempotent.

### Added
- `AnimationThreadMode` enum + `WindowConfig::animation_thread_mode(mode)` builder. Re-exported from `blinc_app::prelude`. See the docs on the enum for the trade-off.

### Fixed
- Glass background brushes retain the same collected children, grouped opacity and nested backdrop scope as glass materials.
- Nested glass panels preserve joint opacity, effects, clips, live updates and cached replay, including collected text/images and GPU output. Sampling uses owned textures without requiring a sampleable destination or replaying GPU callbacks; sampling-scope changes invalidate cached composite bakes.
- Background glass ancestors retain joint opacity/effects across direct, cached and dedicated overlay frames. Collected images and GPU callbacks join the ordered backdrop, with callbacks executed once and background sibling order preserved through opacity updates.
- Painted foreground glass ancestors composite native and collected siblings together through opacity/effects, live updates and cached replay. Removing a glass material preserves opaque foreground image order instead of painting the container background over it.
- Supported glass panels composite their material and collected/native children together through opacity, effects and animation. Direct, cached and dedicated overlay frames retain the prepared backdrop; grouped GPU callbacks execute once and removed backgrounds clear stale glass samples.
- Nested native overlay panels retain collected content and cover enclosing foreground content while preserving outer isolation, inherited effects/clips and live animation ownership across app frame modes.
- Image builder opacity is applied once in native groups and independent image dispatch. CSS overrides replace the builder value while parent opacity, authored pixel alpha, tint and load fades retain their own factors.
- Dedicated overlay trees share ordered frame composition while loading existing pixels. Dynamic and collected content, groups and GPU output retain their owners; main replay data and promoted textures survive independent paints. Overlay bakes are released after each call, glass uses a separate reusable backdrop, and atlas reclamation or growth invalidates stale main glyph replay.
- Inline overlay content retains supported native ancestor motion, opacity, effects, transforms and clips above ordinary foreground, including live zero recovery and snapshot interleaving.
- Inline overlay text, styled text, SVGs, images, canvases and GPU scenes share root opacity/effects and paint order above static foreground content. Snapshot, live binding and CSS updates retain that ownership.
- Grouped canvas meshes preserve opacity/effects, clip coverage and paint order in snapshot, animated and compositor frames. Mesh scenes share the GPU dispatcher with independent output, and glass backdrops reuse the scene result instead of executing its hooks twice.
- Grouped canvas GPU passes execute within their visual owners during snapshot, animated and compositor frames. Replay preserves display scale, and callback-containing promoted scratch batches refresh when painted.
- Stateless tree rendering shares ordered group composition with animated frames, keeping collected text, SVGs, images and foreground content inside supported opacity/effect groups. Snapshot frames release their replay cache, and switching frame modes invalidates cached ownership.
- Cached subtree fingerprints include named-sample destination state, so clip, transform and opacity changes refresh their bakes.
- Window runners drain property writes per tree, including secondary windows. Shared renderer caches and promoted textures refresh when their owning tree changes, and canvas replay preserves its update namespace.
- Cached direct frames retain canvas meshes and custom GPU passes, preserving their viewports and dispatching them once per frame.
- Collected 3D content uses the allocated texture extent so pooled texture reuse preserves its raster scale and source mapping.
- Collected text, SVGs and images stay inside eligible native effect and blend groups. Image-mask resources are prepared for both command streams before dispatch.
- Motion-frame image elements and Div image backgrounds share native group opacity, order, clips and transforms. Captures retain loaded textures with shared fit/filter/mask preparation, refresh load fades and placeholders, and survive image-cache eviction. Asset invalidation also clears captured frame content.
- Motion-frame SVGs share native group opacity and paint order, including raster overrides, authored alpha, transforms and clips. Captures copy atlas shadow pixels so cached draws survive atlas changes; independent SVG rendering shares the raster preparation path.
- The panel-fade regression measures glyph intensity under shared opacity, preserving the text-presence and mid-animation checks used by hosted compatibility tests.
- StyledText runs now share their native opacity group, retaining every span and wrapped fragment in collection order through cache replay, nested styles and clips.
- Motion-frame plain text, shadows and decorations render inside their native opacity group with owned glyph clips and transforms. Independent text keeps its collected pass; CSS content that cannot patch the cache requests a fresh walk.
- Repaint retained native cross-stream groups when motion bindings change, including settled writes and opacity endpoints, so joined ranges and later siblings retain their paint order.
- Replacing or explicitly restarting a CSS animation/transition refreshes cached output even when node membership and promotion stay unchanged. Ordinary ticks, loops and snapshot clones preserve cache ownership.
- CSS cache ownership refreshes on animation/transition start, completion, removal and promotion changes. Visible output omitted by an earlier transparent paint requests a fresh walk; unchanged transparent, offscreen and settled ownership keeps its cache.
- CSS opacity cache updates require a valid group command and preserve child colors through zero. Rejected patches force repaint before cache reuse; direct frames with active CSS use the full walker.
- Native leaves, canvas callbacks and single-child wrappers preserve group opacity across overlapping output, nested groups and cached frame replay.
- Motion and overlay groups keep canvas images, shapes and nested opacity together across cached frames, empty output and zero-opacity recovery. Existing retained-callback guards request a full repaint.
- Static groups retain dynamic canvas output with their siblings through changing images, shapes and empty frames. Retained callbacks force a fresh walk/cache repaint and keep requesting redraw; unrelated canvases retain overlay collection.
- Retained foreground motion and CSS groups request repaint before cache replay or background-index mutation. Settled writes and opacity recovery preserve isolation with static foreground siblings.
- Static-owned motion binding ranges request repaint before dynamic-cache mutation. Their nested groups use ordered single-sample replay, preserving overlap isolation with static siblings.
- 3D text bakes retain polygon/path clip rows in layer coordinates. Clips owned inside the layer tilt with its text; outside ancestor contours stay in screen space. Transformed text keeps its affine when moved into the layer viewport.
- Motion-subtree text retains collected polygon/path clips and matching auxiliary rows through converted glyph preparation and cached overlay replay. Existing binding repaint and CSS text-promotion guards remain in place.
- Motion-renderer static and foreground text caches retain polygon/path rows through cloning, damage filtering, transformed glyphs and replay. Fast paint reuses cached CSS text without appending duplicate primitives or rows.
- Main `render_tree` text retains collected polygon/path clips in direct, unified, glass and foreground-effect branches. Prepared glyphs and converted text carry their owning auxiliary rows through batch merges.
- Overlay-tree text retains inherited CSS polygon/path clips in each clip owner's physical frame, including styled spans and transformed glyphs. Direct glyph layers and converted primitives own and restore their matching auxiliary rows.
- Prepared glyph path-clip controls cover direct/unified rendering, transformed fill rules, active owners, merged auxiliary growth and replacement atlases.
- Standalone glass path-clip regressions cover direct/frame rendering, active paint owners, transformed fill rules and changing backdrops.
- Image/glass/blur brushes used by canvas path fills and strokes retain path clips through cached canvas collection and foreground dispatch.
- Canvas primitive path clips survive concatenation across callbacks, including shared contour rows and text rendered through the primitive stream. Auxiliary rebasing preserves ordinary per-side border widths.
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
