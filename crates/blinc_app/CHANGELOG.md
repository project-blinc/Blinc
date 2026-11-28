# Changelog

All notable changes to `blinc_app` will be documented in this file.

## [Unreleased]

- The bold and italic runs of a rich text draw on the baseline of the text beside them. An underline on text positioned by its baseline follows the glyphs.

- Desktop accessibility delivers semantic snapshots, focus and actions through AccessKit. Collection starts on native activation, updates retain stable IDs, unchanged nodes are omitted and deactivation frees the cache. The iOS bridge accepts the expanded role names.

- Programmatic focus wakes idle desktop windows. Mobile frames synchronize overlay and requested focus before applying focus styles.

### Accessibility
- iOS accessibility snapshot/action FFI connects GPU-rendered controls to native UIKit accessibility elements and text-field editing. The host scales content and input coordinates with Dynamic Type.
- The registered iOS Rust builder uses the complete UI build path, including queued core styles, overlays and build-scope cleanup.

### Added
- The desktop runner has a measurement hook, off unless set: `BLINC_BENCH_WHEEL=<.class|#id>` wheels the page until the first match is in view, waits for it to settle, then feeds horizontal swipes over its centre through the normal input path at 60 steps a second, and exits after `BLINC_BENCH_WHEEL_TURNS` swipes (40 by default). Two builds run under `/usr/bin/time -l` with the same script can be compared by instruction count.

### Changed
- The library builds only as an rlib. The staticlib output put every dependency into one archive on each build. That archive was up to a gigabyte, also for a crate that depended on `blinc_app`. To build an iOS static library, run `cargo rustc -p blinc_app --features ios --target aarch64-apple-ios --crate-type staticlib`.
- `BLINC_DAMAGE_RECT` is read once per process instead of on each frame.
- Browser frames reuse retained paint for eligible motion and scroll rows instead of walking the tree on every frame. Property, subtree, focus, stylesheet and layout changes invalidate the cache; unsupported content keeps the full-paint path. Unchanged pointer and focus state skips the stylesheet state pass. HTML canvas and worker runners share this path without allocating a static-layer texture.
- Direct retained frames reuse prepared text and preserve motion glyph row ownership across successive updates. Canvas replay distinguishes actual ordered content from a resolver marker, and leaves unchanged groups intact when no canvas needs splicing.
- Background-only CSS colour transitions update their owning cached fills, including inside moving groups. Text and children retain their paint; unsupported sources repaint. The frame trace distinguishes cached geometry repaint from a full walk.
- The post-focus drain invalidates subtree paint only when it applies a rebuild. A queued region poll that changes no children no longer discards the cache after property classification.
- Native input keeps the paint cache while queued properties await classification. Explicit repaint requests still invalidate immediately, so signal-driven translations can reach the retained-row path during input.
- The desktop runner retains paint for eligible reactive translation writes. Retained rows move the geometry and text together on the CPU or GPU node table; unsupported content repaints. Text under these translations keeps its row between writes and snaps after the complete translation. Collected text keeps an ancestor’s inner viewport clip fixed while moving inside it.
- An SVG with a CSS transform (an open tree item's chevron) is no longer dropped while rows scroll it: it is culled by where it is drawn, not by the rect it was painted at, which rows leave in place when they fold the move into its transform.
- `RenderContext::render_tree_with_motion_opt` renders the frame as one GPU submission with one command encoder.
- A window paces capped frames on a fixed grid of `1 / cap` steps instead of waiting the cap's interval after each frame ends, so an animation holds the cap's rate (120 fps at a cap of 120) rather than drifting below it with the frame's own time; a frame later than a step takes the next one.
- Text under a reactive translation is kept unsnapped for rows only when the rows carry its content. Painted into the static batch, as a text input's text is after its first scroll write, it snaps to whole pixels as static text does.
- Node rows take text in a bound group whose clip rect holds the clip in force above every bound node, with the tighter clip in its shape: the day numbers of a calendar's sliding months inside a scrolled page. Every frame of a month turn repainted the page.
- An SVG that a translucent group draws as part of the group (a disabled pagination button's chevron) moves with the rows of its scroll container. It stayed where the last full paint left it until the scroll stopped.
- The collected clip of text, SVGs and images follows the painter's rule: only the outermost scroll container's viewport, and that of a container whose content rows move, is the scroll clip; a nested scroll container's viewport joins the primary clip, which moves with the content. Intersecting rounded clips keeps a corner's radius only where the corner is the clip's own. A clip rect that is the one in force when a node further up the chain began stays with that node.
- Images drawn in a batch's ordered stream (inside a group) are moved by rows instead of making the rows decline the batch: each takes the row of its node, and its clip inside the scroll container follows the rows while the scroll clip stays. Such an image is now clipped by its scroll container's viewport as well.
- Text and SVGs in content that rows move keep their scroll clip apart from the clip inside it: text draws the inner clip as a shape clip, an SVG as its primary clip with the scroll clip second, as images do. A primitive whose clip would have to move in part, and cannot be split, makes the rows decline rather than draw it cut. A canvas replayed on rows keeps the clip it was painted with; a path keeps a plain clip rect.
- A window loads images on a worker thread: decoding, block compression and upload no longer hold up the frame that first needs an image, which is drawn by the first paint after it arrives, its placeholder until then. `RenderContext::set_async_image_decode` turns this on; headless rendering keeps loading images inline, as does wasm. An image that fails to load is tried again as its `retry` says, inline and on the worker alike, and at once when hot reload sees its file change.
- An image keeps only scroll containers' clips as its scroll clip; any other clip, sharp or rounded, is intersected into its primary clip. A sharp clip inside a scroll container (a `stack()`) no longer becomes part of what stays with the viewport.
- Consecutive images are drawn in one pass and one submission rather than one each; a placeholder or shadow between them ends the run, so the order is kept. A page whose images are redrawn every frame (content moved by rows while a spinner turns) spends far less CPU submitting them.
- A path clip on a primitive that rows move follows the rows from the node that introduced it, turns included; one an ancestor introduced stays. The CPU rows give the moved clip its own points. A clip rect that is the unbounded default counts as no clip, so it does not stop a turn.
- A scroll container with elements that have a transform of their own, or that a motion binding moves, is moved by rows as it scrolls; a bound element's row chains to the container's. Only something fixed or sticky, a layout animation, a `clip-path`, glass, an overlay root or a CSS animation that writes its values each frame still keeps a container off rows. `RenderTree::scroll_row_blockers` lists those for a container. A moving element's label inside a scrolled container is placed a glyph edge away from a full paint.
- A canvas inside a scroll container is moved by rows as the container scrolls: it is drawn into the dynamic batch at the pose it was painted at, replayed there each frame with its rows kept, and the rows move it. A canvas no longer counts as an animation that keeps its container off rows. A replay that changes the number of primitives, or draws meshes, GPU passes or auxiliary rows, makes the frame a full paint.
- A scroll container with translucent elements or layer effects in it is moved by rows as it scrolls. The text in a translucent group keeps the container's row: it was baked at the pose of the frame because a scroll is not a binding, and stayed there while the rest scrolled.
- An SVG inside a node moved by rows, or inside a scroll container, moves with the rows as an image does: its rect and transform come from the chain of rows, so it is not painted again on each frame. A moving card with an icon in it, and a scrolled list whose rows have icons, were painted in full on every frame. The patched path used when motion is baked to textures still leaves an SVG where it was painted.
- A window draws at 30 frames a second while the only thing moving is a CSS opacity or colour cycle that repeats, an indeterminate progress bar or a skeleton, where it drew at the display's rate. A rebuild, a relayout, a scroll, a vsync-class animation (transform, layout, clip) and a binding that moves anything but opacity draw at once; a frame an input event asks for can wait up to 33 ms while the cap applies. The motion trace (`BLINC_MOTION_TRACE`) records the gate of the cache frame (`fast_gate`), the offsets of the scrolled containers (`scrolls`), the animations a binding carries (`css_carried`, `css_gate`) and why the ambient rate did or did not apply (`ambient_terms`, `ambient_only`).
- A grid or list of equal cards that each fade by a motion binding is moved by rows: a group is found again by its id, not by its position, size and opacity, which were the same for every card inside its own wrapper. Such a grid was painted in full on every frame. With the groups in one atlas, 140 faded cards cost a frame about what 140 moving cards do.
- The iOS and Android runners ask `RenderTree::animation_needs_full_paint` for the full paint a CSS animation, transition or FLIP slide needs, instead of raising it on every frame any of them runs. A CSS animation or FLIP slide a binding carries, and a CSS animation baked into a layer, now take the cached-paint path there as they do in a desktop window.
- The compositor and the cache check treat a CSS animation that a binding carries as a bound node: its frames are rows, not a CSS patch or a baked layer, so no texture is kept for it.
- On macOS the desktop runner no longer renders a window the OS reports as occluded (covered, minimized or on another Space). Its frames are dropped, so a covered window stops using the GPU and CPU and no longer blocks the main thread in the drawable acquire; the first frame after it is visible again paints what changed.
- `BLINC_MOTION_TRACE` records `p4_done_us` for each drawn frame: when the style and animation pass and the paint finished, as offsets from the start of the render phase, so a long render phase says whether it was spent styling, painting or presenting.
- A FLIP slide no longer turns off the fast paint path while it plays when bound nodes are moved by rows: the window, the compositor and the CSS composite check ask `has_flip_animations_off_rows`. The iOS and Android runners still paint in full for any FLIP slide.
- Collecting text, SVGs and images in a scroll container with very many children finds the children in view through the layout index, not by testing each.
- Scrolling a list is a frame of one row. A scroll container whose content is plain is painted once, over a window as far past its viewport as the viewport is long, and its scroll offset moves the content by a row, on the GPU with the node table and on the CPU without it; a frame in which only such offsets changed does not walk or paint the tree. The text and images under it are collected over the same window, and are never baked as a node's text is when it settles. Past the window the content is painted again around the new offset. The runner no longer drops the render cache for a scroll the rows move, nor forces a full paint while one glides, as it still does for a scroll container that is not on rows (anything fixed, sticky, moving, faded, an SVG or a canvas under it, or nested in another on rows). The scrollbar is drawn each frame from the container's `ScrollbarConfig` and physics (visibility, fade, colours, width, thumb), as the walker draws it for the others, so `Always`, `Hover`, `Auto` and `Never` behave as they did.
- The desktop runner draws at most 20 frames a second while the only motion left is the last small wobble of a spring (within half a pixel, half a degree, or a hundredth of a scale or opacity of its target), where it drew at the animation cap with nothing changing between frames. The springs still tick at every wake and settle when they did; scrolling, CSS, motion, stateful springs, flows, pointer queries and image fades keep their rate, and a rebuild or relayout draws at once.
- Bound nodes are painted as vectors once and moved by rows, which are applied on the CPU unless the GPU node table is on (`BlincConfig::node_table`, default from `BLINC_NODE_TABLE`). A frame in which only bound values change resolves the primitives and glyphs of the dynamic batch through the rows before it is drawn, and it matches a full paint of the same pose: translate, scale, turn and fade, nested, clipped, groups with text, text and images. They used to be baked to textures and blitted, which left a label behind, placed a nested node wrongly and clipped a moving card wrongly. `BlincConfig::motion_promotion` (default from `BLINC_MOTION_PROMOTION`, off when it is unset, `1` or `true` turns it on) bakes them as before; CSS animations are not affected either way.
- Text under a motion container or a CSS transform that only moves and scales it is placed on the display grid as static text is: its quads are folded to the size of their bitmaps and their corners rounded to whole device pixels. Motion text used to sit at fractional positions at any scale, which softened it, most on a 1x display. A node at rest is rasterized at its exact scale (a pressed button's included), one in flight at a size rounded up to a quarter, and text moves smoothly with its card until the node stops. With the node table, text is baked at the pose it settles at when its node stops and returns to its painted pose when it moves again, so a settled label is as sharp as a full paint of that pose.
- Text that a scale binding or a CSS transform shows larger than it is laid out is rasterized at the size it is shown, laid out at the base size and with the quads shrunk back, so a card that grows keeps sharp text where it used to magnify a small bitmap. With the node table the size follows the spring's target, and the glyphs are prepared again in place, keeping their slots, when it changes.
- `BlincConfig::node_table` (default from `BLINC_NODE_TABLE`, off when unset) moves motion-bound nodes with rows of the GPU node table instead of baking them to a texture. Their subtrees are painted as vectors into the dynamic batch once, each primitive naming its innermost binding's row, and a frame in which only bound values change writes the rows and leaves the batch alone: translate, scale, turn and fade of a node and of nodes nested to four deep, with the text under them: its glyphs take a row of their own and the cached glyphs move with the card instead of staying where the last full paint put them. A binding settled where it was painted is left alone while others move, and a node that moves only text has a record of its own. In this mode the cache paint no longer draws motion text and SVG into the static layer, which left a copy behind at the painted pose. A clip an ancestor put on the primitives stays where it is; one the node or a bound ancestor introduced is scaled and moved by the rows. Opacity and effect groups move too, text included: a group's opacity is set on its layer command, its effects size it by where its primitives are drawn, and the text in it keeps a row of its own and is prepared again as the node's scale settles. What the rows cannot say (a clip that is not a rectangle and moves, or that a turn would have to rotate, a blend mode or a 3D layer, a stretch with a turn, ancestors that are turned, a chain of more than four) is patched on the CPU or repainted as before. So is a group that has to keep its place in the paint order: a glass group, one with an image or an SVG inside it, and one that content painted after it lies over, which stay in the static batch. An SVG inside a bound node is repainted while the node moves. An image is moved by the chain of rows through its transform, faded, scaled and turned with the card, its clip staying where an ancestor put it and following where the node did; one that is still showing a placeholder, or has a shadow of its own, is repainted instead. `BlincConfig::motion_promotion` turns the texture baking on or off without the table.
- A desktop window that has drawn nothing for two seconds frees its app's between-frame GPU memory: the window-sized static layer, the offscreen targets and the pooled layer textures (`BlincApp::release_idle_memory`). The next frame paints in full and allocates them again. Not under Wayland keep-alive, which never stops drawing.
- The desktop app loads its default system font by path, which maps it, instead of reading it into memory: on macOS, SF's `SFNS.ttf` is no longer copied into the heap.
- Each frame rendered with `render_tree_with_motion_opt`, fast paint or full, calls `GpuRenderer::next_frame`, so an overlay pass that drew only while the app started frees its window-sized texture a second later.
- The iOS and Android runners present through the compositor as the desktop runner does: a frame with no input, state change, scroll or CSS motion blits the cached static layer and re-records only canvas and overlay content, instead of repainting the tree. Input, text and key events, keyboard insets, rebuilds, stateful updates, scrolling, overlays and CSS state changes request a full paint, and a full paint runs at least once a second. The iOS surface is configured with `COPY_DST`; Android adds it when the surface supports it and otherwise keeps the direct path.

### Fixed
- On desktop, hover follows the pointer onto an element drawn over the one it was on, such as a close button over a tab's highlight, and a fast pointer that stops no longer leaves the hover on an element it left.
- Translucent images and text in an element with a 3D transform are no longer faded twice.
- An image or icon in an element with a 3D transform is drawn when the element holds no text.
- An image's brush placeholder is drawn while the image loads.
- A glass or blur image placeholder blurs what is behind the image while it loads, inside groups, moving containers and 3D layers too, and content painted after the image stays above it. In a moving container inside a 3D layer it shows its tint.
- A window with a glass element on the page draws its background images. The page background was painted over them, so they did not show. They are drawn over the background and under the glass, which blurs them.
- Image placeholders are drawn in frames with glass, inside the same clips, transforms and opacity as the image. A CSS translate on an image follows the display scale in those frames.
- A skeleton image placeholder moves in a window, where it stood still.
- An image's shadow is drawn once. It was drawn with the element and again with the image, so a translucent shadow came out darker.
- An SVG inside an overflow fade fades with it, as text and shapes do.
- A desktop or iOS window that has gone idle draws a frame when a shown scrollbar is due to fade, so the page scrollbar hides 1.5 s after a scroll instead of waiting for the next input, and its fade no longer lands on the frames of whatever the user does next.
- Text, SVGs and images inside bound or scrolled content no longer draw on top of open menus, selects and toasts. The dynamic batch is drawn by paint layer, each layer followed by the motion content on it. The static batch's foreground (a select list beside bound content) goes in at its own layer, above the moving content and text below it and below overlays, and the dynamic batch's foreground follows. Collected content inside an overlay root takes a layer above what it covers, as the paint walker gives it. A batch that cannot be drawn in parts is drawn whole, with the static foreground below it.
- Ctrl+Alt layout text uses platform-produced characters instead of a synthesized key fallback; AltGr typing no longer runs ordinary Ctrl editing shortcuts.
- Desktop function keys F1–F12 and Insert reach shortcut handlers with distinct key codes. Key handlers run before generated text and can cancel insertion with `EventContext::prevent_default()`; browser canvas and worker runners honor that cancellation too.
- Retained text follows the current raster scale when its node rotates and scales together, including inside opacity groups. Rotating rows no longer leave glyph bitmaps at the size of the last full paint.
- Rows decline content with filled paths (mesh triangles, a chart's pie or donut drawn once), whose corners they cannot move yet, instead of leaving the shapes where they were painted while the rest scrolls.
- After the caches of collected content (text, SVGs, images) are thrown away, the next frame collects them again in a full paint instead of drawing from the cached batches without them; motion images no longer vanish until a click or hover when rows decline.
- The Android runner no longer aborts at start with `Failed to wait for GPU to come idle before reconfiguring the Surface`. wgpu fails a swapchain reconfigure when a Vulkan driver reports a timeline semaphore's new value a moment after the wait on it returned, which the first resize after a frame could hit on a Pixel 10 Pro. The runner drains the queue, retrying, before each reconfigure.
- Text that a scale animation or a spring scales stays sharp while the scale changes. Its glyphs are rasterized at the scale the node has in each frame, which was rounded up to a quarter, and put on whole device pixels while it scales, as they are at rest; text that only moves keeps its smooth placement. A label under a CSS zoom was 0.81 to 0.91 as sharp as the same label laid out at that size mid-animation and is 0.95 or more now, on the vector, CPU rows and node table paths. While the scale changes the size is on the glyph atlas's half-pixel step, so a bitmap does not depend on which size in the step was asked for first.
- Text inside an opacity group is rasterized at the size its card shows it. A card scaled by a CSS animation and faded as well drew its label from a base-size bitmap and magnified it, 0.59 as sharp as the label laid out at that size; it is now as sharp as when the card is not faded. While the animation plays the size is rounded up to a quarter, and at rest the glyphs sit on whole device pixels.
- The iOS and Android runners apply `BlincConfig::motion_promotion` to the render context they build the app from; they did not, so it stayed on and a phone baked every bound node and CSS animation into a texture of its own, and moved them by blitting. With it off (the default) they are moved by rows, as in a desktop window.
- Hover follows content that moves under a pointer that does not. At the top of a frame, when a scroll offset or the layout changed, the window runs the pointer move again at the pointer, so the row that scrolls under a resting pointer is hovered (its `:hover` style, its enter and leave events, the cursor) instead of the row that was there. The desktop window and the web runner skip a layout after a state-style pass that changed nothing the layout reads.
- A window paints in full a frame in which the layout under a scroll container on rows changed, so a virtual list scrolled on rows shows the rows it has now, text and backgrounds together.
- An image inside a motion container is drawn after the dynamic batch the container's background is in, as its text and SVGs are. It was drawn with the static content, under the background, so a card with an image in it showed none while it moved. The image also grows with a bound scale of its container, with its corner radius and border, where it kept its size.
- Text under a node whose scale is within 0.001 of 1 but not 1 (the first frames of a spring) scaled about the origin instead of the node's centre and sat up to a fraction of a pixel off, because the pivot and the matrix used different bounds. Both use 1e-6 now.
- A motion container's bound rotation turns the text, SVGs and images inside it, as a CSS rotation does. They stayed level while the box turned. The scale pivot of a container's content follows its bound translate, so the label no longer drifts off the card's centre when translate, scale and rotation are bound together.
- A key press types the character of the active keyboard layout (for example `@` on AltGr+Q in a German layout) instead of one looked up in a US-layout table. The US table stays the fallback when the platform supplies no text.
- The desktop runner takes the swapchain image only for a frame that is drawn, so frames that are skipped no longer hold images and the Wayland keep-alive no longer starves later acquires. A failed acquire marks the UI for rebuild so the next frame does not reuse a stale cache, is logged again, and an unknown error asks for one retry instead of a redraw loop. Tested on a virtual KWin only; macOS and Windows share this code and are untested.
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
- A `:root` variable set by a later stylesheet reaches the stylesheets added before it, so an application can override the `--shadow-*` and other tokens the component rules read. Before, it did only after a colour scheme change.

### Added
- The iOS runner has measurement hooks, off unless set: `BLINC_BENCH_STATS` prints the build, drawable-acquire and render time of each window of frames (`BLINC_BENCH_WINDOW`, 120 by default) with the process footprint, and `BLINC_BENCH_SCROLL` feeds a scripted drag and fling as touches. `tools/blinc-bench/mobile` has benchmark scenes that use them.
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
