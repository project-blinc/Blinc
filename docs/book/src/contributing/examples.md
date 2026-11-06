# Writing a Cross-Target Example

Every example under [`examples/blinc_app_examples/examples/`](https://github.com/project-blinc/Blinc/tree/main/examples/blinc_app_examples/examples)
runs on **every** platform Blinc supports — desktop via
`WindowedApp::run`, web via `WebApp::run_with_setup`, and (where
the widgets allow) iOS and Android via the mobile runners — with
no per-target forks. A single source file is the source of truth
for all targets.

The [Example Gallery](../web/example-gallery.md) is assembled from
this same set, auto-discovered by `tools/build-web-examples` and
published to the site via CI. **Adding a new example requires
writing one file that follows the convention below. Nothing else.**
No manifest entry. No wrapper crate. No CI change.

## The convention

Every cross-target example must define exactly one function with
this signature, as a top-level `pub fn`:

```rust
pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder {
    // The actual demo UI.
}
```

> **Edition 2024 note**: when calling `WindowedApp::run(config, build_ui)`
> directly, edition 2024 RPIT capture rules cause `build_ui` to only
> implement `FnMut` for one specific lifetime — the higher-ranked
> bound on `run` fails. The fix is to pass the builder via a closure
> wrapper: `WindowedApp::run(config, |ctx| build_ui(ctx))`. The
> alternative is to annotate the named fn with `+ use<>` to opt the
> return type out of lifetime capture, but the closure form is more
> portable across editions and is what every example uses.

And its `fn main` must be cfg-gated to non-wasm targets:

```rust
#[cfg(not(target_arch = "wasm32"))]
fn main() -> blinc_app::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "My Example".to_string(),
        width: 800,
        height: 600,
        ..Default::default()
    };

    // Closure wrapper: on edition 2024 a free fn `-> impl Element`
    // captures its input lifetime in the return type, which breaks the
    // higher-ranked `FnMut` bound. The closure form is portable across
    // editions. See `WindowedApp::run` docs for the `+ use<>` alternative.
    blinc_app::windowed::WindowedApp::run(config, |ctx| build_ui(ctx))
}
```

That's it. Run the codegen tool:

```bash
cargo run -p blinc-build-web-examples
```

Your example is now auto-discovered, wrapped as a wasm32 crate under
`examples/_generated/<name>/`, built by CI, and appears in the
[Example Gallery](../web/example-gallery.md) with the title and
description pulled from your `//!` doc comment.

## The full template

A complete minimal example looks like this:

```rust
//! My New Example
//!
//! One-paragraph description of what the demo shows. This text
//! becomes the gallery page description verbatim — keep it short.
//! Bullet points render fine:
//! - First thing the example demonstrates
//! - Second thing
//!
//! Run with: cargo run -p blinc_app_examples --example my_new --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "My New Example".to_string(),
        width: 800,
        height: 600,
        ..Default::default()
    };

    // Closure wrapper: on edition 2024 a free fn `-> impl Element`
    // captures its input lifetime in the return type, which breaks the
    // higher-ranked `FnMut` bound. The closure form is portable across
    // editions. See `WindowedApp::run` docs for the `+ use<>` alternative.
    blinc_app::windowed::WindowedApp::run(config, |ctx| build_ui(ctx))
}

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder {
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
        .items_center()
        .justify_center()
        .child(
            text("Hello, Blinc!")
                .size(32.0)
                .color(Color::WHITE),
        )
}
```

Save that as `examples/blinc_app_examples/examples/my_new.rs` and run
`cargo run -p blinc-build-web-examples`. The gallery picks it up
on the next book build.

## What the codegen tool extracts from your file

- **Title** — the first non-empty line of the `//!` doc block.
  The " Example" / " Demo" suffix is stripped for display, so
  `//! Scroll Container Example` becomes **Scroll Container**.
- **Description** — everything from the second `//!` paragraph
  up to (but not including) the first `Run with:` line. Rendered
  verbatim as markdown on the gallery page.
- **Dependencies** — the tool greps your source for
  `blinc_cn::` / `blinc_icons::` / `blinc_tabler_icons::` /
  `blinc_canvas_kit::` / `blinc_theme::` / etc. and adds matching
  `path = "..."` dependencies to the generated wrapper's
  `Cargo.toml`. If you use a workspace crate the tool doesn't know
  about yet, add it to the `INFERABLE_DEPS` table in
  `tools/build-web-examples/src/main.rs`.

## Constraints

### The return type must be `impl ElementBuilder`, not `Div`

`impl ElementBuilder` lets you return anything Blinc considers a
valid root element: `Div`, `Scroll`, `Stateful<T>`, `Canvas`,
`MotionContainer`, etc. Returning `Div` specifically would force
you to wrap non-`Div` roots like `scroll().child(...)` in an
extra `div().child(...)` just to satisfy the type system, which
adds a pointless layout node.

The web runner (`WebApp::run_with_setup`) accepts any
`ElementBuilder` via the internal `UiBuilderFn` trait — see
[`crates/blinc_app/src/web.rs`](https://github.com/project-blinc/Blinc/blob/main/crates/blinc_app/src/web.rs)
for the type-erasure machinery. You should never need to think
about it; just return whatever feels natural.

### `ctx` must be `&mut WindowedContext`, not `&WindowedContext`

The web runner's frame loop holds a mutable borrow of the context
for reactive state bookkeeping. Taking `&mut` makes your `build_ui`
compatible with both `WindowedApp::run` (desktop) and
`WebApp::run_with_setup` (web); taking `&` only works on desktop.

### `fn main` must be `#[cfg(not(target_arch = "wasm32"))]`-gated

Without the cfg gate, `cargo check --target wasm32-unknown-unknown`
would compile your `WindowedApp::run` call into a wasm binary, and
that method isn't available on the web target. The gate also means
the auto-generated wrapper crate can `include!` your example source
without colliding with its own `#[wasm_bindgen(start)]` entry
point.

### State initialization goes inside `build_ui`, not before it

Historically a lot of the framework's examples initialized an
`Arc<Mutex<...>>` or a timeline in `fn main` and captured it into
the closure passed to `WindowedApp::run`. That pattern doesn't
translate to the web target, because the wasm wrapper only has
access to `build_ui` — it never sees whatever state `fn main`
set up. Put the state setup inside `build_ui` and use
`ctx.use_state_keyed` / `ctx.use_animated_timeline` to persist it
across rebuilds:

```rust
pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder {
    // Persistent state keyed by string — survives rebuilds.
    let count = ctx.use_state_keyed("counter", || 0i32);

    // Persistent animation timeline — also survives rebuilds.
    let timeline = ctx.use_animated_timeline();

    div().child(/* ... use count and timeline ... */)
}
```

## Opting out

Some examples can't run on the web target — multi-window demos,
CLI diagnostics, OS-specific runners. For these, add a `//! no-web:`
line with a short reason to the top of the doc block:

```rust
//! Multi-Window Demo
//!
//! no-web: the web target has no multi-window concept — a browser
//! tab is a single `<canvas>`. `open_window_with()` doesn't
//! translate to the browser. Kept desktop-only on purpose.
//!
//! Demonstrates: ...
```

The codegen tool skips any file with `no-web:` in its doc block
(no wrapper crate, no gallery entry) without erroring out. The
desktop build is untouched, and the example continues to work as
`cargo run -p blinc_app_examples --example <name> --features windowed`.

Currently opted out:

- `css_parser_demo` — CLI diagnostic, no event loop
- `fuchsia_hello` — Fuchsia OS target
- `multi_window_demo` — multi-window not supported on web

## Running locally

**Desktop**:

```bash
cargo run -p blinc_app_examples --example my_new --features windowed
```

Unchanged from before the cross-target convention.

**Web**:

```bash
# 1. Generate (or regenerate) the wasm wrapper crate
cargo run -p blinc-build-web-examples

# 2. Build it with wasm-pack
cd examples/_generated/my_new
wasm-pack build --target web --release

# 3. Serve it
./serve.sh 8000
# Open http://localhost:8000 in Chrome 113+
```

For iterating on a single example, once the wrapper exists you can
skip step 1 on subsequent runs — cargo's `rerun-if-changed` in the
wrapper's `build.rs` catches edits to your upstream example
automatically. Only add / remove / rename operations require a
fresh codegen pass.

## What the tool generates

Running `cargo run -p blinc-build-web-examples` (no flags) writes:

- **`examples/_generated/<name>/`** — one wrapper crate per
  discovered example. Contents: `Cargo.toml`, `build.rs`,
  `src/lib.rs`, `index.html`, `serve.sh`, `.gitignore`.
- **`docs/book/src/web/example-gallery.md`** — the gallery index.
- **`docs/book/src/web/example-gallery/<name>.md`** — one page per
  example with an iframe of the wasm build.
- **`docs/book/src/SUMMARY.md`** — patched between
  `<!-- begin:web-examples -->` / `<!-- end:web-examples -->`
  markers to include the new gallery pages in the book's TOC.

Everything under `examples/_generated/` is gitignored (except
`.gitignore` + `.gitkeep` markers) so the generated tree is
rebuilt on every CI run and never ends up in a commit.

Flags:

- `--build` — after codegen, run `wasm-pack build --target web
  --release` in each wrapper. Used by CI.
- `--stage-to <dir>` — after `--build`, copy each wrapper's
  `index.html` + `pkg/` into `<dir>/<name>/`. Used by CI to drop
  artifacts into `target/book/examples/` for mdBook iframe
  resolution. Implies `--build`.
- `--no-gallery` — skip the markdown + SUMMARY patch. Useful for
  lint-only CI steps that don't need to touch the book source.

## Why this design

The earlier version of the repo had hand-written wrapper crates
for every web example (`examples/web_hello`, `web_drag`,
`web_assets`, `web_mobile_demo`). That worked for a handful but
didn't scale: every new example meant a new directory, new
`Cargo.toml`, new `index.html`, new `serve.sh`, plus duplicated
code between the desktop and web entry points.

The convention-driven approach collapses all that to one `.rs`
file that compiles for both targets. The wrapper crate generation
is purely mechanical: the codegen tool parses your example with
`syn`, checks for the convention, and emits the wrapper from a
template. There's no magic, no AST rewriting — just file I/O.
