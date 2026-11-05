# Component Catalog and Worker Demos

The [component catalog](https://blinc.rs/components/) gives each component its own
page. The initial CN pages cover Button, Input, Select, Dialog, Progress and Charts. Visible grid cards play their GIF previews; the index does not start live runtimes.
Blocks will join the same catalog as they are implemented.

Each page puts **Preview | Live | Code** below its description. Preview is a
high-resolution native recording. Live renders the same Rust fixture in a worker.
Code contains that fixture's compiled source and actual CSS. Props, keyboard
interaction, styling selectors, state attributes and theme variables accompany
the example. The usage documentation comes from the existing CN book sources.
When that content is below the viewport, a floating **Documentation** button
scrolls to the usage section. It disappears as the content comes into view and
uses an immediate jump when reduced motion is enabled.

## One fixture, two runners

`examples/blinc_component_demos` owns the shared fixtures and stylesheet. Native
headless recording and WASM both use the default `HybridTheme` with `CN_STYLES`
in dark mode and bundled Noto Sans. Live also offers the built-in Restrained and
Expressive themes, and a light/dark switch. Each installs the complete theme bundle,
including focus rings, corner shapes and motion tokens. Runtime values use Signal and Computed bindings. A new
session owns its bindings; changing a prop updates the existing component.

The recorder draws at 1120 × 640 physical pixels for a 560 × 320 logical viewport.
Its rounded, coloured cursor has a glow and click pulse. Scripted movement and
press/release events reach the real control; the script checks its resulting
value. Frames stream to GIF while the full PNG sequence remains available locally.
The cursor annotation renders above dialogs and is absent from hit testing.

## Worker ownership

`WebApp::new_offscreen` accepts a transferred `web_sys::OffscreenCanvas`, logical
width/height and display density. It shares the DOM runner's surface setup,
reactive context, input dispatch and rendering pipeline. The existing
`WebApp::new` and `WebApp::run` entry points continue to use a DOM canvas.

The browser host owns the DOM canvas, focus/text input bridge, composition and
clipboard events, ResizeObserver and DPR changes. It sends normalized `WebInput`
events in order. The worker owns WASM, component state, theme, layout, GPU surface
and frame scheduling. WebAssetLoader can fetch from either a Window or a worker.

For supported Live demos, the catalog starts one selected runtime when the detail page is visited. It
reports readiness after the first submitted GPU frame completes, preserves the
selected tab and renders no continuous frames while Preview/Code or the document
is hidden. Live resumes the worker. Reset creates a new worker and canvas. Applying custom
CSS replaces a named source in the existing WASM runtime, preserving values and
removing deleted rules.

The side CSS editor applies overrides with **Apply CSS** or Cmd/Ctrl+Enter.
**Reset CSS** removes them. Editing a draft leaves the running demo alone. Applying
updates the existing UI and reports parse errors beside the editor; invalid edits
retain the last valid styles. Overrides stay applied when switching themes. CSS
enters Blinc through `WindowedContext::set_css("preview", css)`.
DOM CSS cannot restyle
a component drawn on the GPU. Overlay parts live outside the fixture's root,
so their documented `.cn-*` selectors must be targeted directly.

## Capabilities and fallbacks

The worker path is an unreleased website integration alongside the published
0.6.0 component API. Actual adapter, device and surface creation determine
availability; `navigator.gpu` alone does not. The selected worker backend remains available in the canvas host
metadata for diagnostics. Failure keeps the recording, documentation and code available, with a
retry that creates a fresh transferable canvas.

A full DOM accessibility mirror is not provided by these canvas demos. The static
documentation and code remain readable without JavaScript or a GPU. Browser
verification must test the actual backend, input/IME/clipboard, theme, viewport,
visibility, retry/reset and disposal; native captures and protocol tests establish
different properties and do not prove browser GPU support.

## Authoring and staging

The manifest, templates and lifecycle scripts live under `web/components/`.
The existing `blinc-build-web-examples` tool has a separate `--catalog-to` mode;
it does not regenerate or rebuild the full example gallery for catalog work.

```sh
CARGO_BUILD_JOBS=2 cargo run -p blinc-build-web-examples -- \
  --catalog-to target/component-site --build
```

Use `--record` to refresh GIFs and posters. `--only <slug>` selects one recording.
Shared fixture changes invalidate affected recordings, and the build rejects stale
previews. GitHub Pages stages the generated catalog beside the landing page and
mdBook. No WASM runtime starts on the catalog index.

See `web/components/README.md` in the repository for scoped checks, preview
commands and the artifact manifest contract.

The catalog has its own [installation guide](https://blinc.rs/components/installation/),
with a compiled starter example and highlighted source. Live viewports follow their
containers through ResizeObserver rather than offering fixed size presets.

The catalog header switches the page colour scheme and remembers it across pages;
the live demo follows that scheme. The logo retains its original colours. The CSS
editor and source panels share the landing page syntax palette with line numbers.
Apply and Reset live in the editor toolbar; the focus treatment stays inside it.

Built-in theme-family changes update the existing runtime and reparse its styles
against the new tokens. They keep the worker, canvas, input values and component
state; selecting a theme during warmup applies that choice once the demo is ready.

## Charts family

`/components/cn/charts/` contains separate sections for line, area, stacked area,
grouped bars, stacked bars, horizontal bars, pie, donut, sparkline, threshold,
histogram and comparison charts. Each has its own Preview, Live and compiled Code.
Chart sections load their live host on demand; switching sections releases the
previous additional host. Inactive tabs and offscreen hosts pause their frames.
Only the primary warmed demo and one additional chart host can exist together.
The header colour scheme reaches an embedded chart without reloading it.
The old line-chart URL redirects to the Charts page.

Cursor movement follows the interaction. Button and Input have one approach,
a stationary click or typing pause, and one departure. Extra travel is reserved
for selecting an option, confirming a dialog or exploring chart values.
Noninteractive examples use a still preview without a cursor or playback control;
Sparkline is the first static example.

The editor starts with a component-specific recipe for colours, spacing, shape,
shadows, interactive states and motion. Recipes use the active theme’s root
variables, including `--primary`, `--surface-elevated`, `--radius-lg`,
`--shadow-md`, `--duration-fast` and `--ease-state`. Chart recipes show palette
and grid variables as well as tooltip styling. Later `:root` declarations apply
to earlier component rules; removing an override restores the selected theme.

## Widget coverage

The catalog includes all public component families and the built-in widgets that
add distinct capabilities: editors, Markdown, Canvas, virtual lists, gestures,
motion, content primitives, images, SVG, notched surfaces and media. Overlapping
controls use the polished component-library version. Coverage mappings in
`web/components/catalog.json` are checked against both public module inventories.

Audio and Video use native recordings with ashui’s original media samples. Their
Live playback uses the existing browser decoder through a host bridge, while Blinc renders the controls and video in its worker canvas. Waveform also renders live. Capture viewport dimensions come from each fixture’s
metadata instead of a single fixed size.
