# Changelog

All notable changes to `blinc_layout` will be documented in this file.

## [Unreleased]

### Changed
- Dynamic background opacity bindings retain a patchable group at opacity one, so cached fades update group opacity instead of scaling each overlapping child.
- CSS paint records retain whether their ranges belong to the dynamic motion batch, allowing static-cache patchers to request a full repaint instead of using indices from another batch.

### Added
- **`TextInput::on_submit(callback)`** — called when Enter is pressed in the field, with whether Shift was held. Without one Enter does nothing in a single-line field.
- **`widgets::CODE_EDITOR_STYLES`** — the code editor's find and replace bar is built from classed elements (`.blinc-find`, `.blinc-find-input`, `.blinc-find-toggle--on`, …), and this stylesheet is its designed look. Add it with `ctx.add_css(..)`; the cn theme bundle includes it. The bar's search now runs as the query is typed (Enter and Shift+Enter step through the matches) instead of when the editor next redrew.
- **`widgets::table::TableCell::class(name)`** + **`::id(id)`** — table cells can now carry CSS classes / ids for selector matching. Required by `blinc_cn::table` to layer `.cn-table-head` / `.cn-table-cell` over the layout widget's theme-token defaults.
- **`TextInput::text_align(TextAlign)`** — horizontal alignment of the visible text inside the field. `Left` (default) keeps the previous behaviour. `Center` swaps the text wrapper from absolute-positioned (with `scroll_offset`) to a flex-centred row, suppresses the absolute-positioned cursor (whose math doesn't account for the dynamic centre offset), and triggers a single-click-selects-all gesture so numeric / OTP / code fields behave like `<input type=number>` in browsers.
- **`TextInput::padding_x(px)`** — exposes the previously hard-coded 12 px internal padding so tight cells (number / OTP / code) can drop it to 4–6 px.
- **`TextInput::on_step(callback)`** — stepper hook fired on ↑ / ↓ / `+` / `−` / `=` keys while the field is focused. When set, the keys are consumed (no character insert for `+` / `−`); the callback receives `+1` or `-1`. Off by default so plain text inputs still accept a typed `-` as the leading sign of a negative number.
- **`refresh_text_input(state)`** + promoted-to-public **`stateful::refresh_stateful`** — external mutation of `SharedTextInputData` (value / cursor / selection) can now push the change through the Stateful so the visible field updates without waiting for an unrelated frame trigger.
- **`blinc_layout::widgets::number_input`** — typed numeric field wrapping `text_input` with `InputType::Number`. Bound to a `State<f64>`; builder methods set `min` / `max` / `step` / `precision`. On every keystroke the visible text parses → clamps → writes back to `state` (empty / mid-edit invalid strings like `""` / `"-"` / `"."` leave state untouched so the user can keep typing). `step_up` / `step_down` helpers operate on `NumberInputConfig` for caller-supplied stepper buttons (notably `cn::number_input`'s `+` / `−`).
- **`blinc_layout::widgets::toggle`** — first-class binary on/off control. Renders a button-shaped Stateful with theme-token defaults (colours from `ColorToken::Secondary` / `TextSecondary` / `TextPrimary` / `BorderSecondary`, radius from `RadiusToken::Default`, padding / typography from spacing + typography tokens). Pairs an external `State<bool>` with an internal `Stateful<ButtonState>` for hover / pressed detection; `:checked` pseudo-state covers the on-state for CSS overrides. Builder methods cover height, padding, icon, label, font size, custom on/off bg/fg, border-width, bordered-off (outline mode), and `on_change`. `blinc_cn::toggle` wraps this with shadcn-style `.cn-toggle` classes + variant / size ladders.

### Performance
- **Stateful layout-prop fast path** — `refresh_props_internal` discriminates three rebuild kinds via paired structural / topology hashes: `Visual` (no children change, no taffy `Style` change → visual-only prop update on existing nodes); `LayoutProps` (taffy `Style` changed, no topology change → patch styles in place + re-`compute_layout`); `Structural` (children added / removed / reordered → full subtree rebuild + mint + collect). The middle path is what keeps spring-driven `.w()` / `.h()` / `.left()` / `.top()` cheap — pre-fix every spring tick on a stateful went through the full Structural rebuild path, tearing down + re-collecting handlers / physics / motion bindings each frame.
- **`LayoutTree` node maps hash with FxHash**: the Taffy-to-`LayoutNodeId` reverse map and the baseline and incidental-`align_self` tables were std maps, and `children()` hashed every child through the reverse map, so flushing a wide tree was quadratic in SipHash. The keys are integers the tree makes itself.

### Added
- **Per-corner `CornerShape` on `ClipShape::RoundedRect`** carried end-to-end through the GPU SDF pipeline. `overflow: clip` on a squircle parent now follows the parent's squircle curve at each corner instead of the previous fixed circular arc — fixes the bg-leak between the parent's squircle outline and the clip's circular outline visible on cn::accordion hover.
- **`router.focused()` lookup** in `apply_complex_selector_styles::focused_node` — replaces the previous `element_registry.all_ids()` walk that only saw elements with a string `id`. Class-only widgets (e.g. `cn::input` attaches by class but has no id) now have `.cn-input:focus` rules apply correctly.
- **Outline propagation** through `Div::merge` and `RenderProps::merge_from`. The Stateful state-callback's `queue_prop_update` path uses `merge_from`; without carrying outline, every state transition wiped outline back to the defaults and the focus ring would flash off-then-on.
- `rich_text_editor` return type explicitly opts out of input lifetime capture (`+ use<>`) so callers on edition 2024 can stash it inside a `'static`-bounded `Div::child` without HRTB inference failures.
- `RenderTree::painted_node_ids()` — set of node ids the paint walker actually rendered this frame (clipped against the window viewport even on scrolls without `viewport_cull` opt-in). Exposes the source of truth for visibility-gated redraw decisions.
- `RenderTree::has_active_visible_visual_animations(painted)` and `has_active_visible_flip_animations(painted)` — visibility-gated counterparts of the existing `has_active_*` predicates. Used by the windowed redraw chain so off-screen `animate_bounds` / FLIP entries don't pin the chain alive.
- `RenderTree::css_has_visible_transitions(painted)` — companion to `css_transitions_empty`, returns true when at least one CSS transition has a painted target.
- `CssAnimationStore::has_visible_active(painted)` and `has_visible_vsync_class(painted)` — visibility-gated keyframe/transition checks. The `vsync_class` variant additionally filters on `KeyframeProperties::needs_vsync_for_smoothness` so the windowed app's animation FPS cap can bypass to native vsync when a visible transform / layout / clip-path keyframe is mid-cycle.
- `RenderTree::has_any_cursor_style()` and `HandlerRegistry::has_any_pointer_handler()` — short-circuit gates for the mouse-move pipeline; UIs with no pointer handlers, no `:hover`/`:active` rules, and no `cursor:` styles skip the per-move hit_test entirely.
- `Stylesheet::has_pointer_state_rules()` — detects `:hover` / `:active` selectors so the mouse-move skip path knows when CSS state styling is at risk.
- `stateful::has_visible_animating_statefuls(painted)` — registry of animating Statefuls now carries a `node_id_fn` so the windowed app can intersect with painted nodes. An off-screen spinner whose Stateful is mid-animation no longer pins the redraw chain.

### Changed
- Mouse-move hit-test pipeline short-circuits when nothing in the tree could react. Pure-static views (e.g. `hello_blinc`) no longer hit_test on every cursor move; cursor styling still works via a single one-shot resolve.
- Scroll FSM settles `Scrolling → Idle` after 200ms idle even when not overscrolling. Wheel-scroll inputs without `ScrollPhase::Ended` (mouse wheel, many Linux trackpad drivers) no longer leave `scroll_animating=true` permanently.

## [0.5.1] - 2026-04-13

### Added
- `video_player()` widget with signal-driven media controls (play/pause, seek, time, volume)
- `VideoControlsBuilder` lazy-built Stateful driven by player signals
- `AudioPlayerWidget` with optional waveform visualization

### Fixed
- Video seek bar wrapped in own Stateful with position/duration signal deps (was invisible until volume clicked)
- Video volume slider bar colors swapped (active/inactive were reversed)

## [0.5.0] - 2026-04-10

### Added

- `RenderTree::has_overscrolling_scroll()` for web scroll bounce-back debounce
- `FontRegistry::invalidate_generic_cache()` clears negative lookups after font loads

### Fixed

- Code editor `std::time::SystemTime` → `web_time::SystemTime` for wasm32 compatibility
- SVG color tint applied via post-rasterization `apply_tint()` for non-`currentColor` sources

## [Unreleased]

### Added

#### Lazy Image Loading (Complete)

- Image placeholder rendering for type 2 (image source) — uses cached preloaded thumbnail/blur-hash
- Skeleton shimmer placeholder (type 3) — animated gradient sweep with auto-redraw
- Fade-in animation when image loads (configurable `fade_duration_ms`)
- CSS `loading: lazy | eager` property
- CSS `image-placeholder-color`, `image-placeholder-image`, `image-placeholder-type`, `fade-duration` properties
- ElementStyle/RenderProps fields for full CSS-driven lazy loading
- `placeholder_image` and `fade_duration_ms` now flow through `ImageData` to renderer
- Placeholder images preloaded eagerly so they're ready when type 2 placeholder renders

#### Performance / memory

- `Scroll::viewport_cull(true)` opts the scroll into per-frame paint culling. The renderer skips painting any descendant whose post-scroll absolute bounds (plus a 200 px overscan) don't intersect the visible viewport. Layout still runs for every child — this only affects the paint walker. Saves GPU primitive memory and draw cost on long lists where most children are off-screen.
- `RustHighlighter` and `JsonHighlighter` share their compiled regex tables across all instances via `OnceLock<Arc<[TokenRule]>>`. An app with multiple `code()` blocks no longer pays the regex DFA build cost (and resident size) N times.
- `Text::new` skips `decode_html_entities` entirely when the input contains no `&` — drops one `String` allocation per text element per build for the common no-entity case.
- Motion-binding lookups in `render_layer_with_motion` are batched: one `motion_bindings.get(&node)` per node-pass instead of four. Non-bound nodes (the ~95% case) short-circuit before reaching the mutex-locked field accessors.
- Class-name storage migrated from `Vec<String>` to `Vec<Arc<str>>` end-to-end, interned through `blinc_core::intern`. Affects `Div`, `Text`, `Svg`, `Stateful`, link / list / blockquote / button widgets, and `ElementRegistry::classes`. The `element_classes()` trait method now returns `&[Arc<str>]`. Repeated class names share one allocation across all nodes.

### Changed
- `ElementRegistry::register_element_type` takes `&'static str` (was `String`). Element types come from `ElementBuilder::semantic_type_name`'s compile-time literals, so the per-build `String` allocation per element was unnecessary.
- `Div::class` / `Text::class` / `.class()` builders take `impl AsRef<str>` instead of `impl Into<String>`. Slightly more permissive and avoids the unconditional `String` allocation.

### Fixed
- Several intra-doc links in `selector::registry`, `widgets::scroll`, `widgets::rich_text_editor::render`, and `css_parser` that were rejected by `cargo doc -D rustdoc::broken-intra-doc-links`.

## [0.4.0] - 2026-04-05

### Added

#### Flow Shader 3D Targets
- CSS parser: `target: vertex` and `target: material` in `@flow` blocks
- Output target parsing for 3D: `position`, `world_normal`, `world_position`, `albedo`, `metallic`, `roughness`, `emissive`, `surface_normal`, `alpha_out`
- `parse_output_target()` helper for unified output name resolution

#### Code Editor Enhancements

- VS Code-style search bar via overlay (Cmd+F)
  - Find with case sensitive, whole word, regex toggles
  - Replace with replace current / replace all
  - Match highlighting with active match indicator
  - Signal-driven Stateful overlay (no deadlocks, no infinite rebuilds)
- Code folding with SVG chevron icons in gutter
- Bracket matching highlight (cursor adjacent to brace)
- Indentation guides (vertical lines at indent levels)
- Current line highlight
- Minimap (scaled-down code overview sidebar)
- Gutter: separate fold column, line numbers, separator with proper alignment

### Fixed

- Gutter line numbers vertically centered instead of top-aligned
- Fold toggle click not reaching correct line when lines folded
- Cursor X offset due to gutter width not subtracted from click coordinates
- Editor expanding vertically instead of scrolling on new lines
- Hover artifacts on sidebar, nav, and menubar components
- Checkbox background not updating on check/uncheck state change

## [0.1.15] - 2026-03-22

### Added

#### Code Editor Widget (`code_editor`)

- `code_editor(&state)` — full-featured editable code editor using Stateful incremental updates
- `code_editor_state("content")` — shared state constructor that persists across rebuilds
- Editing: type, Enter (auto-indent), Backspace, Delete, Tab/Shift+Tab (indent/dedent selected lines)
- Cmd+Backspace/Delete: delete word backward/forward
- Cmd+Z / Cmd+Shift+Z: undo/redo (200-entry history stack)
- Cmd+C/X/V: clipboard copy/cut/paste via platform commands
- Cmd+A: select all
- Cmd+Left/Right: word jump navigation
- Smart Home: toggle between first non-whitespace and column 0
- Page Up/Page Down navigation
- Mouse click cursor positioning, mouse drag selection, double-click word select
- Visual selection highlighting with absolutely-positioned divs
- Current line highlight (subtle overlay when focused)
- Cursor with blink animation
- Vertical scrolling via `overflow_y_scroll` on Stateful
- Monospace text measurement cache (`measure_mono()`)
- Per-line syntax highlight cache with dirty tracking
- Escape to blur/unfocus

#### Shared Text Editing Utilities (`text_edit` module)

- `word_boundary_left/right()` for word-jump navigation
- `word_at_position()` for double-click word selection
- `clipboard_read/write()` for OS clipboard integration (macOS pbpaste/pbcopy, Linux xclip)

#### Text Area Improvements

- Visual selection rendering (absolutely-positioned highlight divs)
- Word navigation: Cmd/Ctrl+Left/Right via shared `text_edit` utilities
- Clipboard: Cmd+C/X/V (copy/cut/paste)
- Select all: Cmd+A
- Control character filtering in TEXT_INPUT handler

#### Stateful Enhancements

- `overflow_y_scroll()` builder method on Stateful
- `inner_scroll_physics()` accessor for programmatic scroll control

#### Platform

- Letter key codes (A-Z) mapped to ASCII in windowed.rs KEY_DOWN events for Cmd+key shortcuts

### Other

- `class_name_with_state()` on `ComplexSelector` for extracting class names from state-bearing selectors (e.g. `.cn-sidebar-item:hover`)
- `get_classes()` on `ElementRegistry` for retrieving registered CSS classes by node ID

### Fixed

- Hover transition artifacts on sidebar, nav-link, and menubar components caused by incorrect base_styles caching and transition restart loops on sustained hover
- Checkbox background not updating on check/uncheck: parent node CSS class registrations now updated during full structural rebuilds so `.cn-checkbox--checked` is matched correctly
- Per-side borders with different colors now use SDF-based rendering with proper corner radius instead of rectangular fill strips
- Combobox dropdown scroll by using registered physics directly
- Smooth corner radius artifact on thin borders
- Stale element data (SVG tint, text content, images) during visual-only rebuilds: `element_type` is now updated in `update_subtree_props_from_builder`
- Base styles cache invalidated when CSS classes change during visual-only rebuilds
- Transition detection skipped for nodes already sustaining a hover state to prevent restart loops
- Toast stack container uses absolute positioning so it covers the full viewport independently of other overlay siblings

## [0.1.14] - 2026-02-24

### Added

- `set_auto_id()` and `children_builders_mut()` methods on `ElementBuilder` trait for stable child ID assignment within stateful containers
- `assign_inner_ids_recursive()` in stateful callback wrapper: auto-assigns deterministic inner IDs to children via `derive_child_key()` (e.g. `"button_0:div:0"`) so event handlers and CSS selectors persist across rebuilds

### Fixed

- CSS `color` property now inherits into text elements inside stateful container rebuilds (added text_color propagation to `apply_stylesheet_base_styles_for_subtree()` post-pass)
- Event handlers on inner children are now re-registered during visual-only subtree rebuilds (`update_subtree_props_from_builder`), ensuring closures capturing new state are properly propagated
- CSS text property inheritance (text_color, text_decoration, white_space, text_overflow) added to the full-tree post-pass at initial render time

## [0.1.13] - 2026-02-18

### Added

#### Pointer Query Pressure & Touch Count

- `env(pointer-pressure)` — normalized touch/click pressure (0.0-1.0), smoothed via `pointer-smoothing`
- `env(pointer-touch-count)` — number of active touch points (0 for mouse input)
- `set_pressure()` and `set_touch_count()` on `PointerQueryState` for per-event platform input
- Pressure smoothing using same exponential decay as position smoothing
- Registered `pointer-pressure` and `pointer-touch-count` env var names in renderer

#### Flow Shader Macro & Builders

- `flow!` macro for defining `@flow` shaders using Rust identifiers and primitives (no raw strings)
- `parse_flow_string()` public API for parsing `@flow` block strings into `FlowGraph`
- `FlowRef` enum (`Name`/`Graph`) with `From` impls for `&str`, `String`, and `FlowGraph`
- `.flow()` builder on `Div` accepting both `FlowRef::Name` (CSS reference) and `FlowRef::Graph` (direct)
- `.flow()` builder on `ElementStyle` for stylesheet-based flow references
- `flow:` property in `css!` and `style!` macros
- `flow_graph: Option<Arc<FlowGraph>>` field on `RenderProps` for direct graph attachment
- Flow parser: swizzle tolerance for `stringify!()` spaces (`uv . x` → `uv.x`)
- Flow parser: hex color tolerance for space after `#` (`# ff0000` → `#ff0000`)
- Flow parser: newline normalization in `parse_flow_string()` for `stringify!()` multi-line output

#### SVG Animation Properties

- `fill`, `stroke`, `stroke-width` as animatable CSS properties for SVG elements
- `stroke-dasharray`, `stroke-dashoffset` CSS properties for SVG line-drawing effects
- `d: path("...")` CSS property for SVG path morphing animation
- `svg_path_data` field on ElementStyle and RenderProps for path data propagation
- Decomposed transform fields (`rotate`, `scale_x`, `scale_y`) on ElementStyle to avoid lossy atan2 decomposition
- `parse_scale_values()` helper for extracting original scale factors from CSS

### Fixed

- Border morph/distortion on rounded clipping containers: overflow clip now deferred to after border rendering, preventing double-AA between border SDF and clip SDF at the same boundary
- Borders on elements with `overflow: clip` now render in the foreground layer (after images), matching CSS painting order (content → border → outline)
- `style_to_keyframe_properties()` preserves original rotation angle from CSS (avoids 359deg → -1deg via atan2)
- Clippy: use `Ok(v)` instead of `Some(v) = .ok()` pattern in glass parser

#### CSS Parser & Stylesheet Engine

- Full CSS parser with `#id` selector support and `:hover`, `:focus`, `:active`, `:disabled` pseudo-states
- Global stylesheet injection via `ctx.add_css()` with runtime theme variable support
- Stylesheet runtime integration: base styles, state overrides, and animation overrides (layered application)
- `css!` and `style!` macros for scoped inline styling that returns `ElementStyle`

#### Layout Properties

- `width`, `height` (px, %, auto)
- `min-width`, `max-width`, `min-height`, `max-height`
- `padding`, `margin` (shorthand and per-side)
- `gap` between flex children
- `display: flex | block | none`
- `flex-direction: row | column | row-reverse | column-reverse`
- `flex-wrap: wrap | nowrap`
- `flex-grow`, `flex-shrink`
- `align-items`, `align-self`, `justify-content` (start, center, end, stretch, space-between, space-around, space-evenly)
- `overflow: visible | clip | scroll`
- `border-width`, `border-color`
- `visibility: visible | hidden | collapse | normal` — hides rendering and collapses layout (Display::None in taffy)

#### Visual Properties

- `background` / `background-color` with solid colors, `linear-gradient()`, `radial-gradient()`, `conic-gradient()`
- Color formats: `#hex`, `rgb()`, `rgba()`, named colors, `theme()` tokens
- `border-radius` (uniform and per-corner)
- `opacity`
- `box-shadow` with offset, blur, spread, and color
- `transform: scale() rotate() translate()` (2D transforms)
- `backdrop-filter: glass | blur(Npx) | chrome | gold | metallic | wood`
- `backdrop-filter: liquid-glass(blur() saturate() brightness() border() tint())` variant with configurable border thickness and tint

#### SVG CSS Transform Inheritance

- SVGs now inherit CSS transforms from ancestor elements via `css_affine` propagation
- Affine decomposition into uniform scale (applied to bounds) + rotation angle (sent to shader)

#### Visibility

- `StyleVisibility` enum (`Visible`, `Hidden`) on `ElementStyle`
- CSS parser recognizes `visibility: hidden | visible | collapse | normal`
- `visibility: hidden` both skips rendering and collapses layout (sets `Display::None` in taffy)
- `visibility: visible` restores `Display::Flex` when reversing hidden state
- Visibility applied across all render paths: `render_layer_with_motion`, `render_text_recursive`, `collect_elements_recursive`
- Complex selector state changes (hover/leave) properly reset taffy styles via `base_taffy_styles`
- Layout recomputed after state style changes that affect layout properties

#### 3D CSS Transforms

- `perspective: <px>` for 3D perspective distance
- `rotate-x: <deg>`, `rotate-y: <deg>` for 3D axis rotation
- `translate-z: <px>` for Z-axis translation
- Correct inverse homography unprojection for flat elements with perspective

#### 3D SDF Shapes (Raymarched)

- `shape-3d: box | sphere | cylinder | torus | capsule | group`
- `depth: <px>` for 3D extrusion
- 32-step raymarching with analytical ray-AABB intersection for accurate hit detection
- Edge anti-aliasing via closest-approach distance tracking
- Blinn-Phong lighting with configurable `ambient`, `specular`, `light-direction`, `light-intensity`
- UV mapping: screen-space gradient evaluation for smooth gradients across all 3D faces

#### 3D Boolean Operations

- `3d-op: union | subtract | intersect | smooth-union | smooth-subtract | smooth-intersect`
- `3d-blend: <px>` for smooth blend radius
- `shape-3d: group` for collecting children into compound SDF

#### CSS Animations

- `@keyframes` with named animation definitions
- `animation` shorthand: name, duration, timing-function, delay, iteration-count, direction, fill-mode
- Timing functions: `linear`, `ease`, `ease-in`, `ease-out`, `ease-in-out`
- `animation-direction: normal | reverse | alternate | alternate-reverse`
- `animation-fill-mode: none | forwards | backwards | both`
- `animation-iteration-count: <number> | infinite`
- Animatable properties: `opacity`, `scale`, `scale-x`, `scale-y`, `translate-x`, `translate-y`, `rotate`, `rotate-x`, `rotate-y`, `perspective`, `depth`, `translate-z`, `blend-3d`, `background`, `border-color`, `border-radius`, `border-width`, `box-shadow`, `clip-path`, `filter` (including `blur`), `width`, `height`, `padding`, `margin`, `gap`

#### CSS Transitions

- `transition` shorthand: property, duration, timing-function, delay
- `transition: all 300ms ease` wildcard for all animatable properties
- Comma-separated multi-property transitions
- Smooth reverse transitions on hover-leave with mid-flight reversal support
- Filter identity defaults for proper interpolation (brightness/contrast/saturate default to 1.0)

#### CSS Filters

- `filter` property: `grayscale()`, `invert()`, `sepia()`, `hue-rotate()`, `brightness()`, `contrast()`, `saturate()`
- `filter: blur(Npx)` with GPU Kawase multi-pass blur via LayerEffect pipeline
- `filter: drop-shadow(x y blur color)` with GPU drop-shadow via LayerEffect pipeline
- Space-separated multi-function syntax: `filter: blur(4px) grayscale(1) brightness(1.5)`
- Supports `N`, `N%`, `Ndeg`, `Npx` argument formats
- Nested parenthesis handling in filter parser (e.g. `drop-shadow(4px 4px 8px rgba(0,0,0,0.5))`)

#### Backdrop-Filter Animation

- `backdrop-filter: blur(Npx)` now extracts actual blur radius (was ignoring value)
- `backdrop-filter: blur(Npx) saturate(N) brightness(N)` multi-function parsing
- Animatable `backdrop_blur`, `backdrop_saturation`, `backdrop_brightness` in `KeyframeProperties`
- Transition support: `transition: backdrop-filter 400ms ease` with smooth interpolation
- `@keyframes` support for backdrop-filter properties

#### Selector Hierarchy

- `.class` selectors via `Div::class("name")`
- Descendant combinator (space): `#parent .child`
- Child combinator (`>`): `#parent > .child`
- Compound selectors: `#id.class:hover`
- Structural pseudo-classes: `:first-child`, `:last-child`, `:nth-child(N)`, `:only-child`
- Complex selector matching engine with ancestor chain walking

#### Advanced Selectors

- Adjacent sibling combinator (`+`): `.a + .b`
- General sibling combinator (`~`): `.a ~ .b`
- `:not()` negation pseudo-class
- `:is()` / `:where()` functional pseudo-classes (matches any of listed selectors)
- `:first-of-type`, `:last-of-type`, `:nth-of-type(N)`, `:nth-last-of-type(N)`, `:only-of-type`
- `:empty`, `:root` pseudo-classes
- `*` universal selector

#### Layout Property Animation

- Animatable layout properties: `width`, `height`, `padding`, `margin`, `gap`
- Per-frame taffy style override with automatic `compute_layout()` when layout properties change
- `base_taffy_styles` snapshot for state reset

#### Theme System

- `theme()` function for accessing design tokens in CSS values
- Color tokens: `primary`, `secondary`, `background`, `surface`, `success`, `warning`, `error`, `info`, `text-primary`, `text-secondary`, etc.
- Radius tokens: `radius-none`, `radius-sm`, `radius-default`, `radius-md`, `radius-lg`, `radius-xl`, `radius-full`
- Shadow tokens: `shadow-none`, `shadow-sm`, `shadow-md`, `shadow-lg`, `shadow-xl`

#### Events

- `PINCH` event support in `EventContext` (center and scale fields)

#### CSS Form Widget Styling

- `caret-color` CSS property for text input cursor color
- `selection-color` CSS property for text selection highlight
- `::placeholder` pseudo-element for placeholder text styling (`color` property)
- `Stateful<S>` now forwards `element_id()` and `element_classes()` to ElementBuilder, enabling CSS matching for all Stateful-based widgets
- `.id()` and `.class()` builder methods on TextInput and TextArea
- CSS-aware `state_callback` in TextInput and TextArea: queries active stylesheet for base, `:hover`, `:focus`, `:disabled`, and `::placeholder` overrides
- `set_active_stylesheet()` / `active_stylesheet()` global for widget access to the current stylesheet
- `get_placeholder_style()` on Stylesheet for `::placeholder` pseudo-element lookup
- `Stateful::inner_layout_style()` method for capturing final taffy Style after all builder methods

### Fixed

- Stateful `base_style` capture timing: `on_state()` captured layout style before `.w()`/`.h()` were applied, causing widgets to revert to constructor defaults (e.g., `w_full()`) on state transitions. Now updated in `build()` with the final layout style
- CSS-parsed `backdrop-filter: blur()` glass now uses subtle white tint (`rgba(1,1,1,0.1)`) and zero border-thickness for clean frosted glass appearance (was fully transparent tint, making glass indistinguishable from backdrop)

- CSS-parsed `backdrop-filter: blur()` glass now uses subtle white tint (`rgba(1,1,1,0.1)`) and zero border-thickness for clean frosted glass appearance (was fully transparent tint, making glass indistinguishable from backdrop)
- CSS timing functions now map to spec-correct cubic-bezier values (`ease` was incorrectly using `ease-in-out` polynomial, causing 6.5x slower initial progress than CSS spec)
- Transform-origin mid-flight reversal jitter: `snapshot_before_keyframe_properties` now overlays `transform_origin` from active transition, preventing snap-back on hover-leave
- Cubic-bezier solver rewritten with f64 internal precision and binary-search fallback for jitter-free interpolation at 120fps
- Hover-leave reverse transitions now properly detected and animated (previously snapped to base state instantly)
- Transition repeat regression: pre-reset snapshots prevent spurious re-transitions when hover state persists after transition completion

## [0.1.12] - 2025-01-19

### Added

- `apply_touch_scroll_delta()` method for touch velocity tracking
- `scroll_time` field in `EventContext` for momentum scrolling
- `dispatch_scroll_chain_with_time()` in RenderTree for mobile scroll dispatch
- Momentum deceleration in scroll physics tick for touch devices

### Changed

- Scroll physics now supports velocity-based momentum scrolling
- `on_scroll_end()` starts momentum if velocity exceeds threshold

## [0.1.1] - Initial Release

- Initial public release with layout engine and scroll widgets
