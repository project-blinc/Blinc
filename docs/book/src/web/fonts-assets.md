# Fonts & Assets

Browsers can't hand wgpu their system fonts — those live in the compositor's 2D pipeline, not in the WebGPU pipeline. Blinc rasterizes glyphs natively via swash, which needs the actual TTF/OTF bytes in wasm memory. The wasm32 init path **deliberately skips system font discovery** (no filesystem), so the font registry starts empty.

> **Without a registered font, every text element fails to shape glyphs and renders as nothing.** Loading at least one font is mandatory.

## Two patterns

### Pattern 1: bundled font (`include_bytes!`)

The simplest option. Font bytes ship inside the wasm artifact via `include_bytes!`. Adds ~750 KB to the bundle per typical TTF, but the font is "available" the moment `WebApp::new` returns — no extra network round-trip, no fallback flicker.

```rust
use blinc::app::web::WebApp;
use wasm_bindgen::prelude::*;

const ARIAL_TTF: &[u8] = include_bytes!("../fonts/Arial.ttf");

#[wasm_bindgen(start)]
pub fn _start() {
    console_error_panic_hook::set_once();

    wasm_bindgen_futures::spawn_local(async {
        WebApp::run_with_setup(
            "blinc-canvas",
            // Sync setup callback — runs once between init and the
            // first frame. Just hands the font bytes to the registry.
            |app| {
                let faces = app.load_font_data(ARIAL_TTF.to_vec());
                tracing::info!("registered {faces} font face(s)");
            },
            build_ui,
        )
        .await
        .unwrap();
    });
}
```

`load_font_data` returns the number of font faces registered. Most TTFs have a single face; TTC collections have several.

This is the pattern `web_hello`, `web_scroll`, and `web_drag` use. It's the right choice for prototypes, demos, and any app that ships a single small font.

### Pattern 2: fetched font (`run_with_async_setup`)

Recommended for real apps that ship more than one font, or for any font over a few hundred KB. The font lives next to `index.html` as a static asset; the browser caches it independently across reloads, and the wasm artifact stays small.

```rust
use blinc::app::web::WebApp;
use blinc::app::BlincError;
use blinc::platform_web::WebAssetLoader;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn _start() {
    console_error_panic_hook::set_once();

    wasm_bindgen_futures::spawn_local(async {
        WebApp::run_with_async_setup(
            "blinc-canvas",
            // The `Box::pin(async move { ... })` ceremony is the
            // stable-Rust workaround for the lack of `async FnOnce`.
            // Once async closures stabilize, this drops back to
            // `|app| async move { ... }`.
            |app| Box::pin(async move {
                let bytes = WebAssetLoader::fetch_bytes("fonts/Inter.ttf")
                    .await
                    .map_err(|e| BlincError::Platform(e.to_string()))?;
                app.load_font_data(bytes);
                Ok(())
            }),
            build_ui,
        )
        .await
        .unwrap();
    });
}
```

`run_with_async_setup` is the async sibling of `run_with_setup`. The setup closure runs *once* between init and the first frame. The runner awaits the returned future synchronously before installing the UI builder, so by the time the first rAF tick fires, the font is already in the registry.

`WebAssetLoader::fetch_bytes` is a one-shot helper that fetches a single URL and returns the raw bytes. It does **not** keep a copy in the loader cache — the bytes have a downstream owner (the font registry, which takes ownership in `load_font_data`), and caching them on the loader side too would just double the memory.

This is the pattern the [`web_assets`](https://github.com/project-blinc/Blinc/tree/main/examples/web_assets) example uses. The wasm artifact is **612 KB smaller** than `web_hello` purely because Arial is no longer baked into the bundle.

### Pattern 3: bundled fallback + fetched main (recommended for production)

The first-frame timing of pattern 2 has one downside: the canvas is blank until the fetch resolves. For apps that care about FOIT/FOUT, ship a tiny system-ish fallback font *bundled* and fetch the real font asynchronously. The fallback renders the first frame; the real font replaces it the moment it lands:

```rust
use blinc::app::web::WebApp;
use blinc::app::BlincError;
use blinc::platform_web::WebAssetLoader;

// Tiny system-ish fallback bundled inside the wasm — ~50-100 KB.
const FALLBACK_TTF: &[u8] = include_bytes!("../fonts/SystemFallback.ttf");

WebApp::run_with_async_setup(
    "blinc-canvas",
    |app| Box::pin(async move {
        // 1. Bundled fallback first — first frame renders text immediately.
        app.load_font_data(FALLBACK_TTF.to_vec());

        // 2. Fetch the real font in parallel — replaces the fallback
        //    once it lands. The font registry handles the override
        //    automatically by face name + weight.
        let inter = WebAssetLoader::fetch_bytes("fonts/Inter.ttf")
            .await
            .map_err(|e| BlincError::Platform(e.to_string()))?;
        app.load_font_data(inter);
        Ok(())
    }),
    build_ui,
)
.await
```

This is the production-grade pattern. The bundled fallback keeps the wasm artifact moderate-sized (a real 750 KB font is replaced by a 50-100 KB stripped subset), the first frame renders immediately, and the high-quality font replaces the fallback transparently.

## Multiple fonts

`load_font_data` is additive — call it once per font:

```rust
WebApp::run_with_async_setup(
    "blinc-canvas",
    |app| Box::pin(async move {
        let inter = WebAssetLoader::fetch_bytes("fonts/Inter-Regular.ttf").await?;
        app.load_font_data(inter);

        let inter_bold = WebAssetLoader::fetch_bytes("fonts/Inter-Bold.ttf").await?;
        app.load_font_data(inter_bold);

        let mono = WebAssetLoader::fetch_bytes("fonts/JetBrainsMono.ttf").await?;
        app.load_font_data(mono);
        Ok(())
    }),
    build_ui,
).await
```

For lots of fonts, parallelize via `futures::join!` or `wasm_bindgen_futures::spawn_local` so the network round-trips overlap:

```rust
let (inter_regular, inter_bold, mono) = futures::join!(
    WebAssetLoader::fetch_bytes("fonts/Inter-Regular.ttf"),
    WebAssetLoader::fetch_bytes("fonts/Inter-Bold.ttf"),
    WebAssetLoader::fetch_bytes("fonts/JetBrainsMono.ttf"),
);

app.load_font_data(inter_regular?);
app.load_font_data(inter_bold?);
app.load_font_data(mono?);
```

## Other assets

`WebAssetLoader::preload(urls)` is the API for general-purpose asset preloading. Unlike `fetch_bytes`, it stores fetched bytes in the loader's HashMap so later synchronous `AssetLoader::load(...)` calls can resolve them:

```rust
use blinc::platform_web::WebAssetLoader;
use blinc::platform::assets::AssetPath;

let loader = WebAssetLoader::new();

// Fetch + cache
loader.preload(&[
    "images/logo.png",
    "icons/menu.svg",
    "data/translations.json",
]).await?;

// Synchronous lookup later (e.g. from a render handler)
let logo_bytes = loader.load(&AssetPath::Relative("images/logo.png".into()))?;
```

This is the pattern Blinc's image loader, SVG loader, and any custom asset consumer expects: bytes are pre-loaded into a cache up front via `preload`, and downstream consumers do synchronous `load(...)` calls that resolve from the cache. **The synchronous `AssetLoader::load` call panics if the asset isn't in the cache** — the trait is sync because the rest of Blinc is sync, and the browser doesn't let you block on I/O from the main thread, so the only way to satisfy the contract is to pre-fetch everything you'll need.

For one-shot bytes that have a downstream owner (like fonts), use `fetch_bytes`. For bytes that need synchronous lookup later (images, SVG, JSON config), use `preload` + `load`.

## Why no system fonts?

Browser-provided fonts (system fonts, `@font-face` declarations, the `FontFace` API) are NOT accessible from wgpu. They live in the browser's compositor and 2D-canvas pipeline, not in the WebGPU pipeline. Blinc rasterizes glyphs in wasm via swash, which operates on TTF/OTF bytes — and those bytes have to come from somewhere the wasm runtime can read, which on the browser means either the wasm artifact itself or a `fetch()` response.

The Local Font Access API ([Working Draft](https://wicg.github.io/local-font-access/)) would allow Blinc to enumerate system fonts and request their bytes, but it's only shipped in Chrome (gated behind a permission prompt) and isn't widely supported. Until that changes, fetched-or-bundled is the only path.
