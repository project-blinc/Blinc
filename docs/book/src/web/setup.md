# Setup & Build

This page walks through the minimum scaffolding to get a Blinc app running in the browser. The four runnable examples in the repo (`examples/web_hello`, `web_scroll`, `web_drag`, `web_assets`) all follow this exact layout — copy any of them as a starting point.

## Toolchain

You need three things on top of a normal Rust toolchain:

1. **The `wasm32-unknown-unknown` target**

   ```bash
   rustup target add wasm32-unknown-unknown
   ```

2. **`wasm-pack`** — drives the wasm build, runs `wasm-bindgen`, and post-processes the output via `wasm-opt`

   ```bash
   cargo install wasm-pack
   ```

3. **A static file server** — `python3 -m http.server` is fine. Browsers refuse to import wasm modules from `file://` URLs, so even a "static" example has to be served over HTTP.

The repo's example `serve.sh` scripts auto-pick the first available server (`python3` → `python` → `ruby` → `npx http-server`).

## `Cargo.toml`

```toml
[package]
name = "my_web_app"
version = "0.1.0"
edition = "2024"

[lib]
# `cdylib` is what wasm-pack needs to emit a `.wasm` artifact + JS shim.
# `rlib` keeps `cargo check` happy on non-wasm targets.
crate-type = ["cdylib", "rlib"]

# wasm-pack invokes Binaryen's `wasm-opt` as a post-processing step.
# Nightly rustc emits bulk-memory and reference-type ops by default,
# but the wasm-opt bundled with wasm-pack 0.13 needs the corresponding
# feature flags or it errors with "Bulk memory operations require bulk
# memory [--enable-bulk-memory]". Pass them through explicitly.
[package.metadata.wasm-pack.profile.release]
wasm-opt = ['-O', '--all-features']

[package.metadata.wasm-pack.profile.dev]
wasm-opt = false

# Strictly a wasm32 example. Native builds of this crate are
# meaningless — there's no entry point that does anything outside
# the browser. The target gate keeps `cargo build --workspace` from
# trying to link an empty cdylib on macOS / Linux.
[target.'cfg(target_arch = "wasm32")'.dependencies]
blinc_app = { version = "0.6", default-features = false, features = ["web"] }
blinc_layout = { version = "0.6" }
blinc_core = { version = "0.6" }
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
web-sys = { version = "0.3", features = ["console"] }
console_error_panic_hook = "0.1"
tracing = "0.1"
tracing-wasm = "0.2"
```

The critical bits:

- **`crate-type = ["cdylib", "rlib"]`** — wasm-pack needs `cdylib` to emit the `.wasm` artifact. The `rlib` is optional but lets `cargo check` work without `--target wasm32-unknown-unknown`.
- **`features = ["web"]` on `blinc_app`** — gates in `WebApp`, `WebApp::run`, the `requestAnimationFrame` driver, and the wasm32-only event listeners. Without `default-features = false`, you'd accidentally pull in `winit` and the desktop platform crates.
- **`[target.'cfg(target_arch = "wasm32")'.dependencies]`** — every Blinc dep is target-gated so a desktop `cargo build --workspace` doesn't try to compile the web example.

## `src/lib.rs`

```rust
#![cfg(target_arch = "wasm32")]

use blinc_app::web::WebApp;
use blinc_app::windowed::WindowedContext;
use blinc_core::Color;
use blinc_layout::div::{div, Div};
use blinc_layout::text::text;
use wasm_bindgen::prelude::*;

const FONT: &[u8] = include_bytes!("../fonts/Inter.ttf");

/// wasm-bindgen entry point. The `start` attribute makes this run
/// automatically when the browser loads the generated `.js` shim.
#[wasm_bindgen(start)]
pub fn _start() {
    // Install the panic hook so any Rust panic shows up in the
    // browser console with a stack trace instead of a useless
    // `RuntimeError: unreachable executed`.
    console_error_panic_hook::set_once();

    // Bridge `tracing::*` macros into the browser DevTools console.
    // INFO level keeps the per-frame DEBUG lines from the renderer
    // out of the console — at 60fps those drown the JS thread.
    tracing_wasm::set_as_global_default_with_config(
        tracing_wasm::WASMLayerConfigBuilder::new()
            .set_max_level(tracing::Level::INFO)
            .build(),
    );

    // `WebApp::run` is `async`, but `#[wasm_bindgen(start)]` can't
    // return a future. Spawn it on the wasm-bindgen-futures executor
    // instead.
    wasm_bindgen_futures::spawn_local(async {
        let result = WebApp::run_with_setup(
            "blinc-canvas",
            // Setup callback runs once between init and the first
            // frame. Use it to register fonts (required — the wasm32
            // init path skips system font discovery, so the registry
            // starts empty), CSS, and any one-shot config.
            |app| {
                app.load_font_data(FONT.to_vec());
            },
            build_ui,
        )
        .await;

        if let Err(e) = result {
            web_sys::console::error_1(
                &format!("WebApp::run failed: {e}").into(),
            );
        }
    });
}

/// User UI builder. Re-invoked by the runner whenever a rebuild is
/// requested.
fn build_ui(_ctx: &mut WindowedContext) -> Div {
    div()
        .w_full()
        .h_full()
        .bg(Color::rgba(0.07, 0.07, 0.10, 1.0))
        .items_center()
        .justify_center()
        .child(
            text("Hello, WebGPU!")
                .size(32.0)
                .color(Color::rgba(0.92, 0.92, 0.95, 1.0)),
        )
}
```

The two non-obvious bits:

- **`#![cfg(target_arch = "wasm32")]` at the top** — the rest of the file uses `wasm-bindgen` and `web-sys`, which only exist on wasm32. The cfg attribute makes the whole module a no-op when someone runs `cargo check` from a desktop checkout.
- **`load_font_data(FONT.to_vec())` inside the setup closure** — required. The wasm32 init path deliberately skips system font discovery (no filesystem), so without at least one explicitly registered font every text element renders as nothing. See [Fonts & Assets](./fonts-assets.md) for the alternative fetch-based pattern.

## `index.html`

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>My Blinc Web App</title>
    <style>
      html, body { margin: 0; padding: 0; height: 100%; background: #0d0d12; }
      body { display: flex; flex-direction: column; }
      #blinc-canvas { display: block; width: 100vw; height: 100vh; }
      #unsupported {
        display: none; position: absolute; inset: 0;
        align-items: center; justify-content: center;
        flex-direction: column; gap: 12px;
        font-family: system-ui, sans-serif; color: #ededf0;
        text-align: center; padding: 24px;
      }
      .no-webgpu #blinc-canvas { display: none; }
      .no-webgpu #unsupported { display: flex; }
    </style>
  </head>
  <body>
    <canvas id="blinc-canvas"></canvas>

    <div id="unsupported">
      <strong>WebGPU not available</strong>
      <span>This app requires Chrome / Edge 113+ or a browser with WebGPU enabled.</span>
    </div>

    <script type="module">
      // CRITICAL: probe via a *throwaway* canvas, never the canvas
      // we hand to wgpu. Calling `getContext("webgl2")` on the live
      // canvas locks its context type forever and breaks wgpu's
      // surface creation with "canvas already in use".
      const hasWebGPU = "gpu" in navigator;
      const probeCanvas = document.createElement("canvas");
      const hasWebGL2 = !!probeCanvas.getContext("webgl2");

      if (!hasWebGPU && !hasWebGL2) {
        document.body.classList.add("no-webgpu");
      } else {
        // wasm-pack `--target web` emits an ES module loader at
        // `pkg/<crate>.js` that exports the wasm `init` function.
        // Importing it kicks off the loader; the `start` function
        // fires automatically.
        const { default: init } = await import("./pkg/my_web_app.js");
        await init();
      }
    </script>
  </body>
</html>
```

The throwaway canvas probe is mandatory. The W3C [HTML canvas-context spec](https://html.spec.whatwg.org/multipage/canvas.html#dom-canvas-getcontext) says calling `getContext("webgl2")` on a canvas locks its context type to webgl2 forever — every subsequent `getContext("webgpu")` on the same element returns `null`, and wgpu's surface creation fails with "canvas already in use".

## Build commands

```bash
# Development build (no wasm-opt, fast iteration)
wasm-pack build --target web --dev

# Release build (wasm-opt -O, smaller and faster)
wasm-pack build --target web --release

# Then serve `pkg/` and `index.html` together
python3 -m http.server 8000
# open http://localhost:8000/
```

`wasm-pack build` produces a `pkg/` directory containing:

- `<crate>.js` — JS shim (~100 KB) that calls `wasm-bindgen`-generated bindings
- `<crate>_bg.wasm` — the actual wasm artifact (typically 6-8 MB pre-strip, smaller after `wasm-opt`)
- `<crate>.d.ts` — TypeScript bindings (optional)
- `package.json` — npm metadata (optional)

`pkg/` is regenerated on every build, so it should be in `.gitignore`:

```gitignore
# wasm-pack output — regenerated by `wasm-pack build --target web --release`.
pkg/
```

## Bundle size

A minimal Blinc app is ~6-8 MB pre-strip. Where the bytes go:

- **Renderer + WGSL shaders + wgpu** — ~3 MB
- **rustybuzz / unicode-bidi / unicode-linebreak** (text shaping) — ~600 KB
- **resvg / tiny-skia** (SVG rasterization) — ~700 KB
- **Layout (taffy + flexbox)** — ~200 KB
- **Reactive graph + state hooks + Stateful machinery** — ~400 KB
- **Bundled font (if any)** — ~750 KB per typical TTF

For tighter bundles, see the [Fonts & Assets](./fonts-assets.md) chapter on fetching fonts at runtime instead of bundling them — the `web_assets` example is **612 KB smaller** than `web_hello` purely because Arial is fetched on first load instead of baked into the wasm.

## Next

- [Examples](./examples.md) — walkthrough of the four runnable web examples
- [Fonts & Assets](./fonts-assets.md) — bundled vs fetched fonts, `WebAssetLoader` API
