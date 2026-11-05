# Hot reloading

## CSS files

Enable `css-hot-reload` to update named stylesheets while a **debug desktop**
app is running. No Rust recompilation or dev server is needed. File watching
is off by default and does nothing in release, web or mobile builds.

Expose an application feature so development builds can opt in:

```toml
[dependencies]
blinc = "0.6"

[features]
dev-css = ["blinc/css-hot-reload"]
```

Register an embedded stylesheet for every build, then watch its source file
when developing. Given `src/main.rs` and `assets/app.css`:

```rust
use blinc::prelude::*;
use blinc::app::windowed::WindowedApp;

fn main() -> Result<()> {
    WindowedApp::run(WindowConfig::default(), |ctx| {
        if ctx.rebuild_count == 0 {
            ctx.set_css("app", include_str!("../assets/app.css"))
                .expect("valid application CSS");

            #[cfg(all(debug_assertions, feature = "dev-css"))]
            ctx.watch_css("app", concat!(env!("CARGO_MANIFEST_DIR"), "/assets/app.css"))
                .expect("watch application CSS");
        }
        div().class("card").child(text("Project settings"))
    })
}
```

```bash
cargo run --features dev-css
```

Each save replaces the named source through `set_css`; rules removed from the
file disappear, and theme and `:root` variables remain available. Signals and
component state survive. Invalid CSS or unreadable files produce a warning and
keep the last valid stylesheet; a later valid save recovers automatically. An
empty file removes that source's rules.

The watcher debounces writes and follows file replacement on save. It wakes an
idle window without requiring input. Repeated registration of the same name
and path reuses its watcher. Different names cascade in registration order;
registering an existing name with a different path replaces that watcher only
after the new file loads successfully.

Watchers belong to their window's context. `ctx.unwatch_css("app")` stops one
without removing its styles; closing the window stops all of them. Release
builds contain no CSS watcher, worker thread or file reads, even if the feature
remains enabled. Keep the embedded `set_css` call so those builds retain styles.
Browser hosts can send CSS strings directly to `set_css` without a file watcher.

## Rust hot-patching (experimental)

> **Status:** Experimental, debug-only. Currently driven by Dioxus's
> [`dx`](https://github.com/DioxusLabs/dioxus) CLI through its
> `--hot-patch` mode. The patch points and the websocket client
> that talks to dx's dev-server both live inside `blinc_app` —
> Blinc apps don't depend on the Dioxus reactive runtime.

Blinc can hot-patch the body of your UI builder closure while the
app is running, so iterating on a layout or styling tweak doesn't
need a full rebuild + relaunch. State held by `Stateful` widgets
and Signal hooks is preserved across
patches because Blinc keys it by `InstanceKey` rather than by
closure identity — your reactive graph survives the swap.

The integration uses the [`subsecond`](https://crates.io/crates/subsecond)
crate. `subsecond` is itself gated on `debug_assertions`, so even if
you ship a release binary with the `hot-reload` feature enabled, the
patch points compile down to direct calls and there is zero runtime
overhead.

When you enable the `hot-reload` feature, Blinc also pulls in
[`tungstenite`](https://crates.io/crates/tungstenite) (websocket
client, no TLS — dx serves over `ws://localhost`) and
[`dioxus-cli-config`](https://crates.io/crates/dioxus-cli-config)
(tiny env-var convention crate, zero deps on native). Both stay
out of default builds; they only show up when the feature is on.

### Setup

1. Enable the feature on `blinc`:

   ```toml
   [dependencies]
   blinc = { version = "0.6", features = ["windowed", "hot-reload"] }
   ```

2. Install the Dioxus CLI (one-time):

   ```bash
   cargo install dioxus-cli
   ```

3. Run your binary crate under `dx`:

   ```bash
   dx serve --hot-patch \
       --package my_app \
       --features blinc/hot-reload \
       --platform macos
   ```

   `dx` builds your binary, launches it, and watches the source
   tree. The Blinc runtime opens a websocket back to dx as soon as
   the window is ready (announcing the running process's ASLR
   offset, build id, and pid as query parameters), so dx can
   compute jump-table offsets when it ships a patch. When dx
   detects a change in the binary crate it compiles a patch, ships
   it over the websocket, and `subsecond::apply_patch` applies it
   in place. The next frame Blinc renders picks up the new closure
   body.

> The websocket protocol matches `dx` CLI 0.7+. Blinc keeps a
> small mirror of the relevant `DevserverMsg` subset locally; if
> a future dx release changes the schema for the fields we read
> (`jump_table`, `for_pid`), the `hot-reload` feature will need a
> compatibility bump. Other fields (template patches, asset
> reloads, etc.) deserialise opaquely so dx can add new fields
> without breaking us.

### What gets hot-reloaded

- **Code in the binary crate** — i.e. the crate where `main.rs`
  lives. `subsecond` only patches the "tip" crate; this is a
  hard limitation of the dynamic-linking approach it uses.
- **CSS strings inlined in your UI builder.** The patched closure
  sees the new string literal and re-registers the stylesheet on
  the rebuild that follows the patch. Blinc's hot-reload runtime
  clears the accumulated stylesheet, drops `css_sources`, and
  resets `rebuild_count` to zero before re-invoking the closure,
  so the common
  ```rust
  if ctx.rebuild_count == 0 { ctx.add_css(MY_CSS); }
  ```
  guard fires again with the new content and the result is a
  fresh sheet — no stale rules from the pre-patch run.
- **Runtime-loaded image and SVG assets**, when you opt the
  app's asset directory into the file watcher:
  ```rust
  fn main() -> blinc::app::Result<()> {
      #[cfg(feature = "hot-reload")]
      blinc::app::hot_reload::watch_dir("assets");
      WindowedApp::run(WindowConfig::default(), |ctx| build_ui(ctx))
  }
  ```
  The watcher tails the directory recursively. When a file under
  it changes, Blinc drops the matching entry from its image cache
  (matched by path-suffix against the URI you passed to
  `image("...")`), wipes the SVG document cache and atlas, and
  forces the next frame to re-read the bytes off disk. Works for
  PNG, JPEG, WebP, runtime-loaded fonts, and SVG sources loaded
  from a path. Does *not* fire for `include_bytes!`-embedded
  assets — those live in rodata that subsecond can't touch (see
  *What doesn't* below).
- **State held by `Stateful` widgets and hooks**. Blinc's
  `InstanceKey` is derived from `#[track_caller]` + a per-frame
  call counter, so the same logical widget gets the same key
  before and after a patch. Signal hooks and FSM state survive.

### What doesn't

- **Code in workspace dependencies** (`blinc_layout`, `blinc_app`,
  `blinc_gpu`, etc.). Editing the framework itself still requires
  a full rebuild.
- **CSS in `const` strings.** `subsecond` patches function bodies,
  not the binary's read-only data segment. A
  `const STYLESHEET: &str = "..."` referenced from your UI builder
  will keep pointing at the *original* string after a patch, so
  edits to the const won't be picked up. Inline the CSS directly
  in the `ctx.add_css(r#"..."#)` call — that string literal lives
  in the patched function body, so subsecond rewrites it
  alongside the rest of the closure. Same reasoning applies to
  `static` strings.
- **`include_bytes!` / `include_str!` assets.** Files baked into
  the binary at compile time live in the same rodata segment that
  CSS const strings do — subsecond can't update them. A
  `register_font(include_bytes!("Inter.ttf").to_vec())` keeps
  serving the original font bytes after a patch. To get
  hot-reloadable font / image assets, load them via runtime path
  instead (`image("assets/logo.png")`,
  `register_font(std::fs::read("Inter.ttf")?)`) and put the
  containing dir under [`watch_dir`](#what-gets-hot-reloaded).
- **Static initialisers in the patched crate.** `subsecond`
  tracks statics across patches but does not re-run their
  destructors, and thread-locals get reset. If your binary crate
  initialises a heavy `OnceLock` at startup, that initialiser
  won't re-run after a patch.
- **Struct field-layout changes.** `subsecond` cannot safely
  patch a closure if a struct it captures has had fields added,
  removed, or reordered — the in-memory layout has shifted out
  from under the patched code. In practice this means: change
  *behaviour* freely, but if you change a field on a struct that
  participates in your reactive graph (e.g. a custom
  `#[derive(BlincComponent)]` struct), restart.
- **Release builds.** `subsecond::call` is a no-op when
  `debug_assertions` is off. Release builds compile the
  `hot-reload` feature in but receive no patches at runtime.

### Multi-window apps

The integration patches both the primary-window UI builder and any
secondary windows opened via `WindowedApp::create_window(...)`.
Each window's builder closure goes through its own `subsecond::call`
patch point, so a code change is picked up on the next frame of
every open window.

### Troubleshooting

- **"Patch had no effect."** Double-check that the change is in
  your binary crate, not in a workspace dependency. `dx serve
  --hot-patch` prints which crate it patched; if the line doesn't
  mention your binary, the edit was outside the patchable scope.
- **dx logs `Ignoring hotpatch since there is no ASLR reference`.**
  The app didn't open the websocket back to dx, so dx couldn't
  compute jump-table offsets. Check that you compiled the app
  with the `hot-reload` feature on (`--features
  blinc/hot-reload`) and that you're running the resulting
  binary as a child of `dx serve --hot-patch` (which sets the
  `DIOXUS_DEVSERVER_*` env vars the client reads). On startup
  the client logs `hot-reload: connected` at info level when the
  websocket opens.
- **CSS edit doesn't show up.** For external CSS files, use
  [`watch_css`](#css-files). For Rust hot-patching, if your stylesheet lives in a
  `const STYLESHEET: &str = ...` (or other binary-resident static),
  the patched closure references the pre-patch address and never
  sees the new string — see *What doesn't* above. Inline the CSS
  in the `ctx.add_css(r#"..."#)` call instead. Once the closure
  body owns the literal, edits round-trip through the patch
  without restart.
- **State got cleared after a patch.** This means `subsecond`
  detected a structural change (a captured struct's fields moved)
  and forced a full re-instance. The next frame rebuilds from
  scratch. This is expected — restart and rebuild if you need the
  old state back.
- **Crash after a patch.** Most likely a struct-layout edit that
  squeaked past `subsecond`'s safety checks. Restart the app to
  recover, then file an issue against Blinc with the diff that
  triggered it. Even if the root cause is in `subsecond`, the
  Blinc team wants to know which patterns are unsafe in practice
  so we can document them here.

### What is shipped

The in-process patch points sit behind a feature flag, alongside the
websocket client that talks to `dx serve --hot-patch`. Editing a UI
body in your binary crate patches in place without a restart. Blinc
apps link no part of the Dioxus reactive runtime — only `subsecond`,
`tungstenite`, and the env-var convention crate.
