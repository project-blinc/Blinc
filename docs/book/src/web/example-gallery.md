# Example Gallery

Every example in [`examples/blinc_app_examples/examples/`](https://github.com/project-blinc/Blinc/tree/main/examples/blinc_app_examples/examples)
that follows the cross-target convention is auto-built for the web
target by `tools/build-web-examples` and embedded below. The same
`build_ui` function that runs on desktop, iOS, and Android runs
here — no per-target forks. See the
[Contributing → Examples](../contributing/examples.md) page for the
convention that makes this work.

Click any card to open the example in a focused view with a
lazily started demo. Each demo spawns its own WebGPU context, so
loading more than ~8 at once will start hitting Chrome's
per-tab GPU context limit — the per-example pages keep that
manageable.

## Examples

- [**Canvas Element**](./example-gallery/canvas_demo.md) — This example demonstrates the canvas element for custom GPU drawing
- [**Canvas Kit Interactive**](./example-gallery/canvas_kit_demo.md) — Demonstrates `blinc_canvas_kit` features:
- [**Carousel Demo - Selector API Showcase**](./example-gallery/carousel_demo.md) — Demonstrates the new selector API features:
- [**Chrome-Style Tabs**](./example-gallery/chrome_tabs_demo.md) — Demonstrates the notch element in "reverse" mode: instead of a dropdown
- [**blinc_cn Components**](./example-gallery/cn_demo.md) — Showcases all available blinc_cn components in a scrollable grid layout.
- [**Run with `cargo run -p blinc_app_examples --features cn --example cn_pickers_demo`.**](./example-gallery/cn_pickers_demo.md) — Auto-built from the cross-target source.
- [**Code Element**](./example-gallery/code_demo.md) — Demonstrates both read-only code display and editable code editor.
- [**Complex SVG**](./example-gallery/complex_svg_demo.md) — Displays an SVG at various sizes to test rasterization quality,
- [**CSS Debug**](./example-gallery/css_debug.md) — Tests three known CSS issues:
- [**CSS Visual Features**](./example-gallery/css_features_demo.md) — Showcases newly added CSS visual features:
- [**Layer Effects**](./example-gallery/effects_demo.md) — Showcases GPU-accelerated layer effects including:
- [**Emoji and HTML Entities**](./example-gallery/emoji_demo.md) — This example demonstrates:
- [**@flow Shader**](./example-gallery/flow_demo.md) — Demonstrates the @flow DAG-based shader system.
- [**Fluid Surface**](./example-gallery/fluid_demo.md) — Combines a canvas flow shader with `pointer-query` CSS-driven interaction.
- [**Skeleton animation with glTF + `blinc_canvas_kit`.**](./example-gallery/gltf_animation_demo.md) — Loads Sketchfab's buster_drone (39 meshes, 92 nodes, one 25-second
- [**Gpu Pass Demo**](./example-gallery/gpu_pass_demo.md) — Auto-built from the cross-target source.
- [**Image CSS Styling**](./example-gallery/image_css_demo.md) — Demonstrates CSS properties that work on images via stylesheets:
- [**Image Layer Test**](./example-gallery/image_layer_test.md) — Tests the rendering order of images vs primitives (paths, backgrounds).
- [**Keyframe Animation Canvas**](./example-gallery/keyframe_canvas.md) — Demonstrates keyframe animations with the canvas element for:
- [**Markdown Editor**](./example-gallery/markdown_demo.md) — A split-view markdown editor with:
- [**3D Mesh Demo — renders the Khronos glTF `DamagedHelmet` sample model**](./example-gallery/mesh_3d_demo.md) — Demonstrates:
- [**Motion Demo**](./example-gallery/motion_demo.md) — Auto-built from the cross-target source.
- [**Music Player Glass Card**](./example-gallery/music_player.md) — Recreates an iOS-style "Now Playing" music player card with liquid glass
- [**Node-editor demo — pre-wired graph with three node types, typed**](./example-gallery/node_editor_demo.md) — What it shows:
- [**Notch Menu Bar**](./example-gallery/notch_demo.md) — Demonstrates a macOS-style menu bar with a notched dropdown that
- [**Overflow Fade**](./example-gallery/overflow_fade_demo.md) — Demonstrates the `overflow-fade` CSS property which applies smooth alpha
- [**Overlay System**](./example-gallery/overlay_demo.md) — This example demonstrates the overlay infrastructure for modals, dialogs,
- [**Pointer Query**](./example-gallery/pointer_query_demo.md) — Demonstrates the CSS-driven continuous pointer query system.
- [**Rich Text Element**](./example-gallery/rich_text_demo.md) — This example demonstrates the rich_text element for inline text formatting
- [**Rich Text Editor**](./example-gallery/rich_text_editor_demo.md) — Full editable rich text editor with cursor, selection, and inline
- [**Scroll Container**](./example-gallery/scroll.md) — This example demonstrates the scroll widget with opt-in webkit-style
- [**Semantic @flow**](./example-gallery/semantic_flow_demo.md) — Demonstrates the semantic step/chain/use system for @flow shaders.
- [**Sortable**](./example-gallery/sortable_demo.md) — Demonstrates drag-based interactions using FSM-driven stateful containers:
- [**Stateful API + Signal-bound modifiers demo.**](./example-gallery/stateful_demo.md) — Two complementary examples:
- [**End-to-end 3D demo wiring Blinc's SceneKit3D renderer up to**](./example-gallery/strangler_demo.md) — dispatch front-end used by any Blinc app that wants to drop
- [**Unified Styling API**](./example-gallery/styling_demo.md) — Demonstrates all styling approaches in Blinc:
- [**SVG Animation**](./example-gallery/svg_animation_demo.md) — Demonstrates SVG animation capabilities:
- [**Table Builder**](./example-gallery/table_demo.md) — This example demonstrates the TableBuilder API for declarative table creation.
- [**Tabler Icons**](./example-gallery/tabler_icons_demo.md) — Showcases outline and filled icons from the blinc_tabler_icons crate.
- [**Minimal text positioning test**](./example-gallery/text_position_test.md) — Tests that text is correctly centered within parent containers.
- [**Text Input Widgets**](./example-gallery/text_widgets.md) — Demonstrates ready-to-use text input and text area elements using the layout API.
- [**KHR_texture_transform**](./example-gallery/texture_transform_demo.md) — Loads Poly Haven's `marble_cliff_02` asset (CC0) — a displaced
- [**Theme System**](./example-gallery/theme_demo.md) — This example demonstrates the Blinc theming system capabilities:
- [**Timeline Animation**](./example-gallery/timeline_demo.md) — This example demonstrates timeline-based animations using the stateful API:
- [**Typography**](./example-gallery/typography_demo.md) — This example demonstrates typography helpers:
- [**Wet Glass**](./example-gallery/wet_glass_demo.md) — Procedural wet-window effect with real light refraction through water drops.
- [**Windowed Application**](./example-gallery/windowed.md) — This example demonstrates how to create a windowed Blinc application
