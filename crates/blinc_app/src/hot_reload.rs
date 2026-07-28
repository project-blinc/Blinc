//! Subsecond hot-reload websocket client (debug builds, behind the
//! `hot-reload` feature).
//!
//! Connects to the `dx serve --hot-patch` dev-server, hands it the
//! running process's ASLR reference + build id + pid as query
//! parameters, and applies incoming jump-table patches via
//! [`subsecond::apply_patch`]. The patch points themselves are
//! installed in `windowed.rs` by wrapping the user's UI builder in
//! [`subsecond::call`]; this module is the missing client side that
//! lets the dev-server compute the ASLR delta and ship a patch.
//!
//! # Why not depend on `dioxus-devtools`?
//!
//! `dioxus-devtools` works fine, but transitively it pulls in
//! `dioxus-core` + `dioxus-signals` + the rest of the Dioxus reactive
//! runtime — none of which Blinc apps want in their dep tree. The
//! wire protocol we actually use is small (one externally-tagged
//! enum carrying a `JumpTable`), so we mirror the relevant subset
//! locally and stay decoupled. We do still depend on
//! [`dioxus_cli_config`] for the env-var conventions (`DIOXUS_DEVSERVER_IP`,
//! `DIOXUS_DEVSERVER_PORT`, `DIOXUS_BUILD_ID`) — that crate is tiny
//! and zero-dep on native, and reusing it means our client speaks
//! dx's protocol without us reverse-engineering the env layout.
//!
//! # Compatibility
//!
//! Tracks `dx` CLI 0.7+. The `DevserverMsg` mirror below is kept
//! deliberately permissive — fields we don't act on (`templates`,
//! `assets`, `for_build_id`, `ms_elapsed`) are accepted as
//! [`serde_json::Value`] / `default`-able primitives so a minor
//! schema bump from dx (e.g. adding a new field) won't break us. If
//! dx ever changes a field we do read (`jump_table`, `for_pid`),
//! we'll need to update this file.

use std::path::PathBuf;
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;
use subsecond::JumpTable;

/// Set to `true` by the websocket thread after `subsecond::apply_patch`
/// succeeds. The frame loop in `windowed.rs` reads + clears it on the
/// next event loop tick to force a tree rebuild — `subsecond::call`
/// only re-evaluates the closure body on a fresh invocation, so a
/// static UI keeps painting the pre-patch tree until something marks
/// it dirty.
static REBUILD_PENDING: AtomicBool = AtomicBool::new(false);

/// Wake callback the windowed runner registers in `connect()` so the
/// patch thread can nudge the event loop awake after flipping
/// `REBUILD_PENDING`. Without this nudge a window sitting in
/// `ControlFlow::Wait` (no input, no animations) would never look at
/// the flag — the patch silently lands but the user sees nothing
/// until they wiggle the mouse.
type WakeFn = Box<dyn Fn() + Send + Sync + 'static>;
static WAKE_FN: OnceLock<WakeFn> = OnceLock::new();

/// Drain the rebuild-pending flag. Returns `true` if a hot-patch
/// landed since the last call. Called once per event-loop tick by
/// the windowed runner; the cost is a single relaxed atomic read +
/// CAS so it's safe to do on every frame.
pub fn take_rebuild_pending() -> bool {
    REBUILD_PENDING.swap(false, Ordering::AcqRel)
}

/// Queue of absolute paths the dev-server has rebuilt since the last
/// frame. Pushed by the websocket thread on receipt of a
/// `HotReloadMsg.assets` list, drained by the frame loop via
/// [`take_invalidations`] so cached decoded copies (image LRU, SVG
/// atlas, etc.) can be dropped before the rebuild paints.
static ASSET_QUEUE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Drain the queued asset invalidations. Returns the list of
/// absolute paths the dev-server has rebuilt since this method was
/// last called; the queue is left empty.
///
/// Called once per frame by the windowed runner alongside
/// [`take_rebuild_pending`]. Returning `Vec` rather than a slice
/// keeps the lock window to a single `mem::take` — callers iterate
/// the result without holding the mutex.
pub fn take_invalidations() -> Vec<PathBuf> {
    ASSET_QUEUE
        .lock()
        .map(|mut g| std::mem::take(&mut *g))
        .unwrap_or_default()
}

/// Push paths onto the invalidation queue and signal a rebuild. Used
/// by the dx websocket handler and the local file watcher; exposed
/// pub-crate so both can share the rebuild trigger logic.
fn enqueue_invalidations(paths: impl IntoIterator<Item = PathBuf>) {
    let paths: Vec<_> = paths.into_iter().collect();
    if paths.is_empty() {
        return;
    }
    if let Ok(mut q) = ASSET_QUEUE.lock() {
        q.extend(paths);
    }
    REBUILD_PENDING.store(true, Ordering::Release);
    if let Some(wake) = WAKE_FN.get() {
        wake();
    }
}

/// Ask the running app to rebuild its tree on the next tick.
///
/// The frame loop drops its cached tree and re-invokes the UI builder,
/// so whatever the builder now returns is what renders. Callers that
/// regenerate the UI from outside the builder -- a recompiled `.blinc`
/// program, say -- use this to make the result visible.
///
/// Also nudges a window parked in `ControlFlow::Wait` awake; without
/// that the rebuild lands but nothing shows until the next input event.
pub fn request_rebuild() {
    REBUILD_PENDING.store(true, Ordering::Release);
    if let Some(wake) = WAKE_FN.get() {
        wake();
    }
}

/// Raised once per settled batch of source edits by
/// [`watch_sources`], drained by [`take_sources_dirty`].
static SOURCES_DIRTY: AtomicBool = AtomicBool::new(false);
/// Wall-clock millis of the most recent source event.
static LAST_SOURCE_EVENT_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Whether a settle thread is already waiting for the writes to stop.
static SETTLING: AtomicBool = AtomicBool::new(false);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Whether source files have changed since this was last called.
///
/// Call it at the top of the UI builder, recompile if it returns true,
/// and build the view from the result. Recompiling has to happen on the
/// main thread -- a `BlincDsl` owns JIT function pointers and is not
/// `Send` -- which is why this is a flag rather than a callback.
pub fn take_sources_dirty() -> bool {
    SOURCES_DIRTY.swap(false, Ordering::AcqRel)
}

/// Watch `dir` for edits to files with any of `extensions`, and raise
/// the [`take_sources_dirty`] flag once the writes stop.
///
/// One save is several file events: editors write, truncate, rename and
/// touch metadata, and `notify` reports every step. Recompiling on each
/// one reads files mid-rewrite, and asking for a rebuild on each one
/// costs a full pass over the UI per event. So the settle happens here,
/// on the watcher's thread, and the frame loop hears about the save
/// exactly once.
///
/// `settle` is how long the events have to go quiet. 100-200ms covers
/// an editor save without being perceptible.
pub fn watch_sources(
    dir: impl AsRef<std::path::Path>,
    extensions: &'static [&'static str],
    settle: std::time::Duration,
) -> Option<WatcherHandle> {
    let settle_ms = settle.as_millis() as u64;
    watch_dir_quiet(dir, move |paths| {
        let touched = paths.iter().any(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| extensions.contains(&e))
        });
        if !touched {
            return;
        }
        LAST_SOURCE_EVENT_MS.store(now_ms(), Ordering::Release);
        // One waiter is enough; the rest of the burst just moves the
        // deadline it is watching.
        if SETTLING.swap(true, Ordering::AcqRel) {
            return;
        }
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(settle_ms));
                let quiet_for =
                    now_ms().saturating_sub(LAST_SOURCE_EVENT_MS.load(Ordering::Acquire));
                if quiet_for >= settle_ms {
                    break;
                }
            }
            SETTLING.store(false, Ordering::Release);
            SOURCES_DIRTY.store(true, Ordering::Release);
            request_rebuild();
        });
    })
}

/// Watch a directory and run `on_change` before the rebuild is queued.
///
/// [`watch_dir`] invalidates cached assets and asks for a rebuild,
/// which is all a changed PNG needs. A changed source file needs
/// something to happen first -- recompiling it -- and that has to
/// complete before the builder runs again, hence a callback rather
/// than a second queue the frame loop would have to drain in the right
/// order.
///
/// The callback runs on the watcher thread. Keep it short, and expect
/// it to be called several times for one save: editors write through
/// temp files, and `notify` reports every step.
pub fn watch_dir_with<F>(dir: impl AsRef<std::path::Path>, on_change: F) -> Option<WatcherHandle>
where
    F: Fn(&[PathBuf]) + Send + Sync + 'static,
{
    watch_dir_inner(dir, Some(Box::new(on_change)), true)
}

/// [`watch_dir_with`] without the automatic rebuild request, for
/// callbacks that decide for themselves when the app should look.
fn watch_dir_quiet<F>(dir: impl AsRef<std::path::Path>, on_change: F) -> Option<WatcherHandle>
where
    F: Fn(&[PathBuf]) + Send + Sync + 'static,
{
    watch_dir_inner(dir, Some(Box::new(on_change)), false)
}

/// Watch a directory for asset changes. The watcher runs on its own
/// background thread, watches `dir` recursively, and pushes any file
/// that changes onto the same queue [`take_invalidations`] drains.
///
/// Multiple calls are additive — each call spawns a watcher for the
/// given directory; existing watchers stay running. Pass each
/// directory at most once: re-watching the same path doesn't
/// deduplicate events, just doubles them.
///
/// Typical use: call once at app startup, before
/// `WindowedApp::run`, with the project's asset directory.
///
/// ```ignore
/// fn main() -> blinc_app::Result<()> {
///     #[cfg(feature = "hot-reload")]
///     blinc_app::hot_reload::watch_dir("assets");
///     blinc_app::windowed::WindowedApp::run(WindowConfig::default(), build_ui)
/// }
/// ```
///
/// In release builds the call is harmless — `notify`'s thread spins
/// up but the cache invalidations it triggers are no-ops since the
/// renderer just re-reads the asset on next render either way.
/// Apps that want zero overhead in release should gate the call
/// behind `#[cfg(debug_assertions)]`.
///
/// Returns the watcher handle so the caller can keep it alive (or
/// let it drop, in which case the watcher thread exits). The handle
/// is `Send + Sync` so storing it in a static / `Arc` works fine.
pub fn watch_dir(dir: impl AsRef<std::path::Path>) -> Option<WatcherHandle> {
    watch_dir_inner(dir, None, true)
}

type ChangeFn = Box<dyn Fn(&[PathBuf]) + Send + Sync + 'static>;

fn watch_dir_inner(
    dir: impl AsRef<std::path::Path>,
    on_change: Option<ChangeFn>,
    enqueue: bool,
) -> Option<WatcherHandle> {
    use notify::{RecursiveMode, Watcher};

    let dir = dir.as_ref().to_path_buf();
    if !dir.exists() {
        tracing::warn!(dir = %dir.display(), "hot-reload: watch_dir target does not exist, skipping");
        return None;
    }

    let mut watcher =
        match notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
            Ok(event) => {
                // Only fire on data-change kinds — `Create`, `Modify`,
                // `Remove`. `Access` events are noise (open / close /
                // touch), `Other` is platform-specific filler. notify's
                // event-kind classifier hides this distinction behind
                // its `EventKind` variants.
                use notify::EventKind;
                if matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                ) {
                    // Before the rebuild is queued, not after: the
                    // callback is what makes the next build produce
                    // something different.
                    if let Some(cb) = &on_change {
                        cb(&event.paths);
                    }
                    if enqueue {
                        enqueue_invalidations(event.paths);
                    }
                }
            }
            Err(e) => tracing::warn!(error = ?e, "hot-reload: watcher error"),
        }) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(error = ?e, "hot-reload: failed to create file watcher");
                return None;
            }
        };

    if let Err(e) = watcher.watch(&dir, RecursiveMode::Recursive) {
        tracing::warn!(error = ?e, dir = %dir.display(), "hot-reload: failed to watch dir");
        return None;
    }

    tracing::info!(dir = %dir.display(), "hot-reload: watching directory");
    Some(WatcherHandle(Box::new(watcher)))
}

/// Opaque watcher handle returned by [`watch_dir`]. Drop it to stop
/// watching. `Box<dyn Watcher>` under the hood — exposing the
/// concrete type would force callers to depend on `notify`, which
/// the public API otherwise doesn't require.
pub struct WatcherHandle(Box<dyn notify::Watcher + Send + Sync>);

/// Subset of `dx` CLI's `DevserverMsg` we deserialise. The full
/// upstream enum carries Dioxus VDOM template patches, asset
/// reload notifications, and various lifecycle pings — we only
/// act on `HotReload`. Unrecognised variants land in `Unknown`
/// and are silently ignored, which lets the server add new
/// message kinds without breaking older clients.
///
/// Default serde representation (externally tagged: `{"HotReload":
/// {...}}`) — matches the upstream type which has no
/// `#[serde(tag = ...)]` attribute.
#[derive(Debug, Deserialize)]
enum DevserverMsg {
    HotReload(HotReloadMsg),
    #[serde(other)]
    Unknown,
}

/// Mirror of `dioxus_devtools_types::HotReloadMsg`. Fields we don't
/// consume are kept permissive so dx schema additions don't break
/// deserialisation.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct HotReloadMsg {
    /// Subsecond binary patch — function-body swap.
    jump_table: Option<JumpTable>,
    /// PID guard the dev-server stamps so apps in a multi-window /
    /// multi-process build don't apply each other's patches.
    for_pid: Option<u32>,
    /// VDOM template patches (Dioxus-only). Captured as opaque JSON
    /// so we don't pull `dioxus-core` into the deserialiser.
    #[allow(dead_code)]
    templates: serde_json::Value,
    /// Absolute paths of assets dx has rebuilt out-of-band (e.g. an
    /// image the user edited that dx detected via the file watcher).
    /// Drives the runtime cache invalidation pass — see
    /// [`take_invalidations`] for how the frame loop consumes this.
    /// dx 0.7+ ships them as `Vec<PathBuf>`; older / variant servers
    /// that omit the field land here as the empty default.
    assets: Vec<PathBuf>,
    /// Build id stamp for de-duping patches across rebuilds.
    #[allow(dead_code)]
    for_build_id: Option<u64>,
    /// Wall-clock build duration the server reports. Informational.
    #[allow(dead_code)]
    ms_elapsed: Option<u64>,
}

/// Spawn the hot-reload client thread.
///
/// `wake` is invoked after every successful patch — the windowed
/// runner passes its `WakeProxy::wake` (from
/// `blinc_platform_desktop`, not linkable here) so the event loop
/// comes out of `ControlFlow::Wait` and looks at
/// [`take_rebuild_pending`]. If
/// the app is a non-windowed runner (or for whatever reason has no
/// wake mechanism) pass `|| {}` and the patch will still apply but
/// won't visibly update the UI until the next natural redraw.
///
/// If `DIOXUS_DEVSERVER_PORT` (and friends) aren't set — the normal
/// `cargo run` case — this returns immediately without spawning a
/// thread. When the env vars are present (the app is a child of
/// `dx serve --hot-patch`), a single background thread connects to
/// the dev-server's websocket, sends the running process's ASLR
/// offset, and processes incoming hot-patch messages until the
/// socket closes.
///
/// Safe and cheap to call unconditionally at app startup.
pub fn connect<F>(wake: F)
where
    F: Fn() + Send + Sync + 'static,
{
    // Register the wake callback regardless of whether dx is running —
    // a future reconnect attempt should reuse the same hook.
    let _ = WAKE_FN.set(Box::new(wake));

    let Some(endpoint) = dioxus_cli_config::devserver_ws_endpoint() else {
        tracing::debug!(
            "hot-reload: DIOXUS_DEVSERVER_PORT not set, not connecting (run under `dx serve` to enable)"
        );
        return;
    };

    let _ = std::thread::Builder::new()
        .name("blinc-hot-reload".into())
        .spawn(move || run(endpoint));
}

fn run(endpoint: String) {
    let uri = format!(
        "{endpoint}?aslr_reference={}&build_id={}&pid={}",
        subsecond::aslr_reference(),
        dioxus_cli_config::build_id(),
        process::id(),
    );

    let (mut ws, _resp) = match tungstenite::connect(&uri) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = ?e, endpoint = %uri, "hot-reload: failed to connect");
            return;
        }
    };

    tracing::info!(endpoint = %uri, "hot-reload: connected");

    while let Ok(msg) = ws.read() {
        if let tungstenite::Message::Text(text) = msg {
            handle_text(text.as_str());
        }
    }

    tracing::debug!("hot-reload: websocket closed");
}

fn handle_text(text: &str) {
    let parsed: DevserverMsg = match serde_json::from_str(text) {
        Ok(p) => p,
        Err(e) => {
            tracing::trace!(error = ?e, text, "hot-reload: ignored unparseable message");
            return;
        }
    };

    let DevserverMsg::HotReload(hot) = parsed else {
        return;
    };

    // `for_pid` filter applies uniformly: a patch for another process
    // would be unsafe, an asset list for another process would
    // pointlessly thrash our caches.
    let our_pid = process::id();
    if let Some(target) = hot.for_pid {
        if target != our_pid {
            tracing::trace!(
                for_pid = target,
                our_pid,
                "hot-reload: message was for a different process, skipping"
            );
            return;
        }
    }

    // Asset rebuilds (image / font / SVG / glTF / ...). Pushed to a
    // queue so the frame loop can drop matching cache entries before
    // the rebuild paints — the next render reads fresh bytes off disk.
    //
    // Note: dx 0.7's wire format only populates this for files
    // registered through Dioxus's `asset!()` macro (see
    // `dioxus-cli/src/build/builder.rs::hotreload_bundled_assets`).
    // Blinc apps don't use that macro, so this branch is effectively
    // dead in the dx-driven path today. The Blinc-side
    // [`watch_dir`] watcher fills the same queue, which is how
    // image / SVG hot-reload actually fires for our users.
    if !hot.assets.is_empty() {
        tracing::info!(
            count = hot.assets.len(),
            "hot-reload: dx ws delivered asset invalidations"
        );
        enqueue_invalidations(hot.assets);
    }

    // Code patch. Done after assets so the rebuild that follows runs
    // with both the new code and the invalidated caches in lockstep.
    //
    // SAFETY: `subsecond::apply_patch` is unsafe because the patcher
    // and the running process must agree on layout / linkage. The dx
    // CLI on the other end is what guarantees that contract — it
    // links the new code against this binary's symbol table and
    // ASLR offset (which we sent as a query parameter on connect).
    // Outside that contract the call would be undefined behaviour;
    // we trust the `for_pid` guard above + the build-id stamp the
    // server already filtered on to keep us in scope.
    if let Some(jump_table) = hot.jump_table {
        unsafe {
            match subsecond::apply_patch(jump_table) {
                Ok(()) => {
                    tracing::info!("hot-reload: patch applied");
                    REBUILD_PENDING.store(true, Ordering::Release);
                    if let Some(wake) = WAKE_FN.get() {
                        wake();
                    } else {
                        tracing::debug!(
                            "hot-reload: no wake callback registered, UI will refresh on next natural redraw"
                        );
                    }
                }
                Err(e) => tracing::warn!(error = ?e, "hot-reload: patch failed"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Verifies the watcher → queue path: drop a file into a watched
    /// dir and the path lands in ASSET_QUEUE within a reasonable
    /// timeout. Polls because notify dispatches on its own thread
    /// with platform-dependent latency (FSEvents on macOS coalesces
    /// over ~250ms).
    #[test]
    fn watch_dir_pushes_changes_onto_queue() {
        // Drain any leftover state from prior tests in the same process.
        let _ = take_invalidations();
        REBUILD_PENDING.store(false, Ordering::Release);

        let tmp = std::env::temp_dir().join(format!(
            "blinc-watch-test-{}-{}",
            process::id(),
            // small unique suffix so concurrent test runs don't collide
            Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(&tmp).expect("create test dir");

        let _handle = watch_dir(&tmp).expect("watcher should start");
        // Give notify a moment to register the watch on macOS / Linux
        // — notify doesn't have a "ready" signal, so we just wait
        // briefly. 200ms is enough for FSEvents / inotify in practice.
        std::thread::sleep(Duration::from_millis(200));

        let test_file = tmp.join("hello.txt");
        std::fs::write(&test_file, b"first").expect("write");

        // Poll the queue for up to 2s.
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut got = Vec::new();
        while Instant::now() < deadline {
            got.extend(take_invalidations());
            if got.iter().any(|p| p.ends_with("hello.txt")) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        std::fs::remove_dir_all(&tmp).ok();

        assert!(
            got.iter().any(|p| p.ends_with("hello.txt")),
            "expected hello.txt in queue, got: {got:?}"
        );
        assert!(REBUILD_PENDING.load(Ordering::Acquire));
    }
}
