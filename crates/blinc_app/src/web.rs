//! Web platform runner — `wasm32-unknown-unknown` only.
//!
//! Sibling of [`crate::windowed`] / [`crate::android`] / [`crate::ios`]
//! (and the Fuchsia stub) that owns the per-frame loop and browser
//! event wiring. The frame loop drives the same render pipeline the
//! desktop runner uses; only the *driver* differs:
//!
//! - **desktop**: winit `Frame::AboutToWait` → render → `request_redraw`
//! - **android**: native_activity `MainEvent::RequestRedraw` → render
//! - **ios**: `CADisplayLink` callback → render
//! - **web**: `window.requestAnimationFrame` → render → schedule next
//!
//! # Bundled emoji + symbol fallbacks
//!
//! The `web` feature pulls in two complementary font add-ons that
//! [`WebApp::new`] registers before the first `BlincApp::with_canvas`
//! await point:
//!
//! - [`blinc_noto_emoji`] — a ~148 KB NotoColorEmoji subset covering
//!   the color pictograph glyphs (emoji, dingbats, misc symbols).
//! - [`blinc_noto_symbols`] — a ~24 KB NotoSans / NotoSansMath pair
//!   covering the monochrome text glyphs that NotoColorEmoji doesn't
//!   carry (math operators, double arrows, currency symbols,
//!   Latin-1 Supplement punctuation).
//!
//! `WebApp::new` calls each crate's `register` function in turn,
//! pushing both bundles into `blinc_text`'s global font registry.
//! From that point on, any text-shaping pass finds a glyph via the
//! existing `FontRegistry::load_emoji_font` fallback chain instead
//! of rendering `.notdef` — regardless of whether the codepoint is
//! a color emoji, a math operator, or a currency symbol. End-users
//! see no extra configuration — enabling the `web` feature is all
//! that's required.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex, atomic::AtomicBool};

use blinc_animation::AnimationScheduler;
use blinc_core::context_state::{BlincContextState, HookState};
use blinc_core::reactive::{ReactiveGraph, SignalId};
use blinc_layout::div::Div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::selector::ElementRegistry;
use blinc_layout::widgets::OverlayManagerExt;
use blinc_layout::widgets::overlay::overlay_manager;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

use crate::app::{BlincApp, BlincConfig};
use crate::error::{BlincError, Result};
use crate::windowed::{
    RefDirtyFlag, SharedAnimationScheduler, SharedElementRegistry, SharedReactiveGraph,
    SharedReadyCallbacks, WindowedContext,
};

/// Convert a [`blinc_platform::MouseButton`] (the wasm-side input
/// helper output) into the [`blinc_layout::event_router::MouseButton`]
/// the dispatch path consumes. Mirrors `convert_button` from the
/// desktop runner at `windowed.rs:2312`.
fn convert_layout_button(
    b: blinc_platform::MouseButton,
) -> blinc_layout::event_router::MouseButton {
    match b {
        blinc_platform::MouseButton::Left => blinc_layout::event_router::MouseButton::Left,
        blinc_platform::MouseButton::Right => blinc_layout::event_router::MouseButton::Right,
        blinc_platform::MouseButton::Middle => blinc_layout::event_router::MouseButton::Middle,
        blinc_platform::MouseButton::Back => blinc_layout::event_router::MouseButton::Back,
        blinc_platform::MouseButton::Forward => blinc_layout::event_router::MouseButton::Forward,
        blinc_platform::MouseButton::Other(n) => blinc_layout::event_router::MouseButton::Other(n),
    }
}

/// Identifier for the four edit-menu actions wired through the
/// custom right-click context menu. Used by `synthesize_edit_action`
/// (and its helper closures inside `handle_context_menu`) to keep
/// the closure type small and `'static` — capturing the `WebApp`
/// would force a `Rc<RefCell<…>>` clone into every menu-item
/// callback and complicate the dispatch path significantly.
#[derive(Clone, Copy)]
enum EditAction {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

/// Synthesise an edit-menu action against the currently focused
/// editable widget. Each action dispatches a Cmd+key chord through
/// `RenderTree::broadcast_key_event`, which the widget's
/// `on_key_down` handler translates into the matching edit
/// operation:
///
///   * Cut        → Cmd+X (key code 88)
///   * Copy       → Cmd+C (key code 67)
///   * Select All → Cmd+A (key code 65)
///
/// Paste is special-cased because the widget's Cmd+V handler reads
/// from `clipboard_read`, which on wasm32 is a permanent `None`
/// stub (the browser clipboard read API is async-only and can't
/// satisfy the widget's sync `Option<String>` contract). For
/// Paste, we call `navigator.clipboard.readText()` directly,
/// resolve the Promise via `wasm_bindgen_futures::spawn_local`,
/// then dispatch the resolved text into the focused widget via
/// `broadcast_text_input_event` once it lands.
///
/// This function operates entirely on global state — it doesn't
/// take a `&mut WebApp` because the menu-item click handlers can't
/// borrow the runner without elaborate `Rc<RefCell<…>>` plumbing.
/// The render tree is reached via the global
/// [`BlincContextState`] singleton, which both the runner and the
/// widget tree already share.
fn synthesize_edit_action(action: EditAction) {
    use blinc_core::events::event_types;
    // Set the touch-input flag back to false so the widget routes
    // the synthetic key event through its keyboard-shortcut path
    // (the same path Cmd+C / Cmd+V take from the keyboard handler).
    blinc_layout::widgets::text_input::set_touch_input(false);

    // Sync clipboard write helpers can be called inline. Cut /
    // Copy / Select All all dispatch a Cmd+key event into the
    // focused widget's key handler, which does its own selection
    // bookkeeping and clipboard write.
    //
    // We dispatch via the static `TREE_BROADCAST` channel that the
    // runner installs into the global before each frame; this is
    // the same path the document-level `paste` listener uses for
    // its broadcasted text-input events. The runner's render path
    // already holds the only `&mut RenderTree`, so this helper
    // can't get one — instead it routes through the broadcast
    // helpers via a queue that the next frame drains.
    //
    // For now we take the simpler approach: queue the action via a
    // global `Mutex<Option<EditAction>>` slot that the next
    // `dispatch_pending` / frame tick consumes. The frame loop
    // re-runs constantly because the cursor blink animation keeps
    // the rAF chain alive.
    if let Ok(mut slot) = PENDING_EDIT_ACTION.lock() {
        // Coalesce repeated taps — only the most recent matters.
        *slot = Some(action);
    }
    // Force a redraw so the next rAF tick picks up the queued
    // action even if the page is otherwise idle.
    blinc_layout::request_redraw();
    let _ = event_types::KEY_DOWN; // silence unused-import on cfg paths
}

/// Slot for a pending edit-menu action queued by
/// `synthesize_edit_action` and consumed inside the WebApp's
/// frame loop. Single-slot because the menu only ever fires one
/// action at a time.
static PENDING_EDIT_ACTION: std::sync::Mutex<Option<EditAction>> = std::sync::Mutex::new(None);

/// Drain any queued edit-menu action and dispatch it through the
/// supplied render tree. Called from inside `WebApp::run_one_frame`
/// (where we have a real `&mut RenderTree`) before the per-frame
/// rebuild + render pass.
fn drain_pending_edit_action(tree: &mut blinc_layout::renderer::RenderTree) {
    use blinc_core::events::event_types;
    let Some(action) = PENDING_EDIT_ACTION.lock().ok().and_then(|mut s| s.take()) else {
        return;
    };
    let (key_code, meta) = match action {
        EditAction::Cut => (88, true),       // Cmd+X
        EditAction::Copy => (67, true),      // Cmd+C
        EditAction::SelectAll => (65, true), // Cmd+A
        EditAction::Paste => {
            // Paste goes through the async clipboard read path —
            // kick off the Promise here and bail without
            // dispatching a synthetic key event. The widget will
            // see the pasted text via `broadcast_text_input_event`
            // once the Promise resolves.
            spawn_paste_from_clipboard();
            return;
        }
    };
    tree.broadcast_key_event(event_types::KEY_DOWN, key_code, false, false, false, meta);
}

/// Read text from `navigator.clipboard.readText()` (async) and
/// broadcast it into the focused widget once it resolves. Used by
/// the right-click context menu's Paste button — Cmd+V via the
/// keyboard goes through the document-level `paste` event listener
/// instead, which is sync.
fn spawn_paste_from_clipboard() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let clipboard = window.navigator().clipboard();
    let promise = clipboard.read_text();
    wasm_bindgen_futures::spawn_local(async move {
        let result = wasm_bindgen_futures::JsFuture::from(promise).await;
        let Ok(value) = result else {
            return;
        };
        let Some(text) = value.as_string() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        // Queue the pasted text via a separate slot — same shape
        // as `PENDING_EDIT_ACTION`. The next frame drains it.
        if let Ok(mut slot) = PENDING_PASTE_TEXT.lock() {
            *slot = Some(text);
        }
        blinc_layout::request_redraw();
    });
}

/// Slot for clipboard text resolved asynchronously by
/// [`spawn_paste_from_clipboard`]. Drained on the next frame by
/// [`drain_pending_paste_text`].
static PENDING_PASTE_TEXT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn drain_pending_paste_text(tree: &mut blinc_layout::renderer::RenderTree) {
    let Some(text) = PENDING_PASTE_TEXT.lock().ok().and_then(|mut s| s.take()) else {
        return;
    };
    for ch in text.chars() {
        tree.broadcast_text_input_event(ch, false, false, false, false);
    }
}

/// Map a Blinc [`CursorStyle`](blinc_layout::element::CursorStyle) to
/// the matching CSS `cursor` keyword. Mirrors the desktop
/// `convert_cursor_style` (windowed.rs:4172) but yields CSS strings
/// instead of `blinc_platform::Cursor` enum values, since on the web
/// we set the cursor via `canvas.style.cursor = "<keyword>"` directly
/// rather than through a winit-style cursor abstraction.
fn cursor_style_to_css(cursor: blinc_layout::element::CursorStyle) -> &'static str {
    use blinc_layout::element::CursorStyle;
    match cursor {
        CursorStyle::Default => "default",
        CursorStyle::Pointer => "pointer",
        CursorStyle::Text => "text",
        CursorStyle::Crosshair => "crosshair",
        CursorStyle::Move => "move",
        CursorStyle::NotAllowed => "not-allowed",
        CursorStyle::ResizeNS => "ns-resize",
        CursorStyle::ResizeEW => "ew-resize",
        CursorStyle::ResizeNESW => "nesw-resize",
        CursorStyle::ResizeNWSE => "nwse-resize",
        CursorStyle::Grab => "grab",
        CursorStyle::Grabbing => "grabbing",
        CursorStyle::Wait => "wait",
        CursorStyle::Progress => "progress",
        CursorStyle::None => "none",
    }
}

/// Object-safe UI builder trait, type-erased over the concrete
/// element type the user's closure returns. The two methods do the
/// two things the runner needs to do with a freshly built element —
/// spawn a new `RenderTree` or apply an incremental update to an
/// existing one — so the concrete `E: ElementBuilder` never has to
/// leave the closure. This is what lets the public
/// `WebApp::run_with_setup` accept `FnMut(&mut WindowedContext) -> E`
/// for ANY `E: ElementBuilder`, matching how the desktop runner's
/// `WindowedApp::run` already works.
///
/// Previously the runner stored
/// `Box<dyn FnMut(&mut WindowedContext) -> Div>`, which forced every
/// example's `build_ui` to concretely return a `Div`. That broke the
/// cross-target convention for examples whose root element is a
/// `Scroll`, a `Stateful<T>`, or anything else — `scroll()` returns
/// a `Scroll`, `stateful()` returns a `Stateful<T>`, neither of which
/// is a `Div`. Type-erasing through this trait fixes the mismatch
/// without asking callers to wrap every non-`Div` root in a
/// containing `div().child(…)` just to satisfy the web runner.
trait UiBuilderFn: 'static {
    /// First-frame build path: call the user's closure to produce a
    /// fresh element tree, then hand it to
    /// `RenderTree::from_element_with_registry` and return the
    /// constructed tree. Called when `current_tree` is `None`.
    fn build_from_scratch(
        &mut self,
        ctx: &mut WindowedContext,
        registry: std::sync::Arc<blinc_layout::selector::ElementRegistry>,
    ) -> blinc_layout::renderer::RenderTree;

    /// Incremental-update path: call the user's closure and apply
    /// the result to an existing tree. Returns the `UpdateResult` so
    /// the runner can decide whether to recompute layout.
    fn build_and_update(
        &mut self,
        ctx: &mut WindowedContext,
        tree: &mut blinc_layout::renderer::RenderTree,
    ) -> blinc_layout::UpdateResult;
}

/// Blanket impl covering every `FnMut(&mut WindowedContext) -> E`
/// where `E: ElementBuilder`. This is the one place in the web
/// runner where we have the concrete `E` in scope; from here on up
/// the rest of the runner operates on `dyn UiBuilderFn`.
impl<F, E> UiBuilderFn for F
where
    F: FnMut(&mut WindowedContext) -> E + 'static,
    E: blinc_layout::ElementBuilder + 'static,
{
    fn build_from_scratch(
        &mut self,
        ctx: &mut WindowedContext,
        registry: std::sync::Arc<blinc_layout::selector::ElementRegistry>,
    ) -> blinc_layout::renderer::RenderTree {
        let composed = compose_with_overlays(self(ctx), ctx);
        blinc_layout::renderer::RenderTree::from_element_with_registry(&composed, registry)
    }

    fn build_and_update(
        &mut self,
        ctx: &mut WindowedContext,
        tree: &mut blinc_layout::renderer::RenderTree,
    ) -> blinc_layout::UpdateResult {
        let composed = compose_with_overlays(self(ctx), ctx);
        tree.incremental_update(&composed)
    }
}

/// Compose the user UI with all overlay surfaces (legacy
/// `overlay_manager`, new `overlay_stack`, `toast_tray`). Mirrors the
/// per-frame composition done by `windowed.rs`. Without these
/// children the corresponding manager `.show()` calls push content
/// into a global manager but nothing ever lands in the tree.
fn compose_with_overlays<E: blinc_layout::ElementBuilder + 'static>(
    user_ui: E,
    ctx: &mut WindowedContext,
) -> Div {
    use blinc_layout::overlay_state::{overlay_stack, toast_tray};

    let overlay_layer = ctx.overlay_manager.build_overlay_layer();
    let stack_layer = match overlay_stack().lock() {
        Ok(mut s) => {
            s.set_viewport_with_scale(ctx.width, ctx.height, ctx.scale_factor as f32);
            s.build_overlay_layer()
        }
        Err(_) => Div::new(),
    };
    let viewport = (ctx.width, ctx.height);
    let tray_layer = toast_tray()
        .lock()
        .ok()
        .map(|t| t.build_tray_layer(viewport))
        .unwrap_or_default();

    Div::new()
        .w(ctx.width)
        .h(ctx.height)
        .relative()
        .child(user_ui)
        .child(overlay_layer)
        .child(stack_layer)
        .child(tray_layer)
}

/// User-supplied UI builder, boxed as a trait object so the
/// concrete return type is type-erased. See [`UiBuilderFn`] for why
/// this can't just be `FnMut(&mut WindowedContext) -> Div`.
type UiBuilder = Box<dyn UiBuilderFn>;

/// Milliseconds since the runner was first constructed. Backed by a
/// `web_time::Instant` so the clock is monotonic on both native
/// (where `web_time::Instant` re-exports `std::time::Instant`) and
/// wasm32 (where it wraps `performance.now()`).
///
/// Used as the `current_time` argument to
/// `RenderTree::tick_scroll_physics`. The epoch is per-app, not
/// absolute, but every consumer is comparing deltas so that's all
/// we need.
fn now_ms() -> u64 {
    use std::sync::OnceLock;
    static START: OnceLock<web_time::Instant> = OnceLock::new();
    let start = START.get_or_init(web_time::Instant::now);
    start.elapsed().as_millis() as u64
}

/// Top-level web runner.
///
/// Owns the canvas, the wgpu surface and surface configuration, the
/// shared [`BlincApp`], the [`WindowedContext`] that the user-supplied
/// UI builder receives on each rebuild, and the cached render tree.
///
/// This struct is intentionally `!Send` — every browser API it touches
/// is single-threaded, and its sub-fields (`wgpu::Surface` on wasm32,
/// `web_sys::HtmlCanvasElement`) are `!Send` themselves.
pub struct WebApp {
    /// The HtmlCanvasElement we're rendering into. Held so we can
    /// re-read its size after a browser resize.
    #[allow(dead_code)]
    canvas: web_sys::HtmlCanvasElement,
    /// Wgpu surface + its configured properties.
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    /// The Blinc application core (renderer + text + render context).
    blinc_app: BlincApp,
    /// User-facing window context. Same shape every other platform builds.
    ctx: WindowedContext,
    /// User-supplied UI builder. Set via [`Self::set_ui_builder`] or
    /// [`Self::run`]. Called inside [`Self::run_one_frame`] when
    /// `needs_rebuild` is `true`.
    ui_builder: Option<UiBuilder>,
    /// Cached layout tree from the most recent rebuild. `None` until
    /// the first rebuild fires.
    current_tree: Option<RenderTree>,
    /// Whether the next frame needs to re-run the user's UI builder
    /// before rendering. Set when an event handler marks the tree
    /// dirty, when the user explicitly requests a rebuild, or when
    /// `take_needs_rebuild` flips the global widget rebuild flag.
    needs_rebuild: bool,
    /// Whether the next rebuild must bypass `incremental_update` and
    /// fall back to a full `from_element_with_registry`. Set by
    /// [`Self::handle_resize`] because viewport-size changes don't
    /// propagate cleanly through the incremental path — parent
    /// constraints have to be re-derived from scratch for the new
    /// dimensions or you get the old layout stretched into the new
    /// viewport. Mirrors `ws.needs_relayout` on the desktop runner
    /// at [`windowed.rs:3684`](crate::windowed).
    needs_full_rebuild: bool,
    /// Last frame's logical width / height in CSS pixels. Used by
    /// [`Self::handle_resize`] to short-circuit `window.resize` events
    /// that don't actually correspond to a canvas size change (devtools
    /// toggle, focus changes, …).
    last_logical_size: (f32, f32),
    /// Last single-touch position, in canvas-local CSS pixels. `Some`
    /// while exactly one finger is on the screen so the touchmove
    /// handler can compute a per-frame delta and dispatch it as a
    /// scroll. Cleared on touchend / touchcancel and on
    /// multi-touch (we don't try to drive scroll from pinch /
    /// two-finger pans yet).
    last_touch_pos: Option<(f32, f32)>,
    /// Last CSS cursor string we wrote to the canvas's
    /// `style.cursor`. Tracked so the per-mousemove cursor refresh
    /// only touches the DOM when the hovered element's
    /// `CursorStyle` actually changes — without this guard every
    /// mousemove queues a layout-invalidating style write, which
    /// shows up as visible jank when the user just sweeps the
    /// mouse across the canvas.
    last_cursor: &'static str,
    /// Timestamp (`now_ms()` epoch) of the most recent wheel-scroll
    /// dispatch. `None` while no scroll is in flight.
    ///
    /// The DOM has no equivalent of winit's `TouchPhase::Ended` for
    /// wheel events, so the runner can't know directly when a
    /// scroll gesture is "over". Instead, every wheel tick stamps
    /// this field, and [`Self::run_one_frame`] checks each rAF tick
    /// whether enough idle time has elapsed to synthesize an
    /// `on_scroll_end` — which is what kicks the bounce-back
    /// spring on scroll containers that ended an overscroll
    /// gesture in rubber-band territory. Without this, scrolling
    /// past the edge leaves the offset stuck inside the
    /// `Scrolling` state forever and the user can never see the
    /// edge bounce back.
    last_wheel_time_ms: Option<u64>,
    /// Timestamp (`now_ms()` epoch) of the most recent wheel-burst
    /// cache invalidate + texture drain. Used to gate the
    /// invalidate_render_cache_tagged + drain_all_layer_textures
    /// pair in Phase 0b to ONLY fire at burst edges (first wheel
    /// after a gap > `WHEEL_BURST_GAP_MS`) rather than every
    /// wheel-bearing rAF. Running them every frame during a
    /// sustained zoom gesture released composited / motion-subtree
    /// textures faster than they could be re-baked, producing
    /// visible jank that scaled with overlay count — the symptom
    /// the user reports when nodes approach the HUD during zoom.
    last_scroll_invalidate_ms: Option<u64>,
    /// Accumulated wheel delta (x, y) since the last `run_one_frame`
    /// drain. The wheel handler adds into this instead of
    /// dispatching directly so the runner can apply a true
    /// per-frame speed cap — multiple wheel events that arrive
    /// inside a single rAF interval coalesce into one capped
    /// dispatch on the next frame, instead of stacking up at the
    /// scroll widget without bound.
    pending_wheel_delta: (f32, f32),
    /// Dynamic per-frame state: motion animations, cursor blink,
    /// overlays. The desktop runner creates this via
    /// `RenderState::new(animations)` — the web runner needs it so
    /// it can call `render_tree_with_motion` (the full render path)
    /// instead of the stripped-down `render_tree` (which skips
    /// @flow shaders, motion containers, blend modes, and overlays).
    render_state: blinc_layout::RenderState,
    /// Shared CSS animation/transition store. Desktop runner creates
    /// this at windowed.rs:2174 and attaches it to every new tree.
    /// Required for `@keyframes` animations and `transition:` to
    /// progress frame-over-frame.
    css_anim_store: Arc<Mutex<blinc_layout::CssAnimationStore>>,
    /// Cached keyboard modifier state. Browser pointer events
    /// (`mousedown` / `mousemove` / `wheel`) carry their own
    /// `shiftKey` / `ctrlKey` / `altKey` / `metaKey` flags, but the
    /// runner reads them inside the DOM closures and never threaded
    /// them through to `dispatch_pending`, which then dispatched
    /// every PointerEvent with `shift=ctrl=alt=meta=false`. As a
    /// consequence, host code branching on modifier state at
    /// POINTER_DOWN / DRAG (e.g. canvas-kit's Shift+Drag rubber-
    /// band add-to-selection, or any widget's Cmd+click chord)
    /// never saw the held key. Mirrors the desktop runner's
    /// `cached_modifiers` at [`windowed.rs:399`](crate::windowed),
    /// which has the same merge-on-dispatch contract.
    cached_modifiers: blinc_platform::Modifiers,
    /// Timestamp of the previous frame in `now_ms()` epoch, used to
    /// compute `dt_ms` for CSS animation/transition ticking. `0`
    /// on the first frame (the tick code treats that as 16 ms).
    last_frame_time_ms: u64,
    /// Web asset loader. A clone is registered as the global
    /// `AssetLoader` (via `set_global_asset_loader`) so
    /// `ImageData::load` → `global_asset_loader()` finds it. This
    /// clone is kept so the app can insert additional assets at
    /// runtime via `preload_asset` / `insert_asset`.
    asset_loader: std::sync::Arc<blinc_platform_web::WebAssetLoader>,
}

impl WebApp {
    /// Initialize the global [`blinc_theme::ThemeState`] with the
    /// default web theme bundle and the user's current
    /// `prefers-color-scheme`. Idempotent — safe to call multiple
    /// times; the second call is a no-op once the singleton is
    /// already populated.
    ///
    /// Mirrors `WindowedApp::init_theme` (windowed.rs:1979) which
    /// the desktop runner calls in `WindowedApp::run` for the same
    /// reason: every theme-aware widget panics on the first frame
    /// without an initialized `ThemeState`.
    fn init_theme() {
        use blinc_theme::{
            ThemeState, detect_system_color_scheme, platform_theme_bundle, set_redraw_callback,
        };

        if ThemeState::try_get().is_none() {
            let bundle = platform_theme_bundle();
            let scheme = detect_system_color_scheme();
            ThemeState::init(bundle, scheme);
        }

        // Theme changes (e.g. user toggling OS dark mode while the
        // page is open) need to invalidate the tree so widgets pick
        // up the new tokens. We don't yet hook the
        // `prefers-color-scheme` media-query change event from the
        // browser, but if a future runner addition fires it through
        // `ThemeState::set_color_scheme` this callback will route
        // it to a full rebuild + CSS reparse — same shape the
        // desktop runner uses.
        set_redraw_callback(|| {
            tracing::debug!("Theme changed - requesting full rebuild + CSS reparse");
            blinc_layout::widgets::request_css_reparse();
            blinc_layout::widgets::request_full_rebuild();
        });
    }

    /// Locate the `<canvas id="…">` in the DOM, set up its physical
    /// framebuffer to match the device pixel ratio, build the GPU
    /// renderer for it, and assemble a [`WebApp`] ready for a frame
    /// loop driver.
    ///
    /// Returns errors if:
    /// - There is no global `window` object (e.g. running in a worker)
    /// - There is no `document`
    /// - No element with `canvas_id` exists
    /// - The matched element isn't actually an `HtmlCanvasElement`
    /// - GPU initialization fails (no WebGPU support, adapter request fails…)
    ///
    /// On success, the canvas's framebuffer dimensions
    /// (`canvas.width` / `canvas.height`) are set to
    /// `client_width * dpr` × `client_height * dpr` so the GPU surface
    /// is sized to actual device pixels rather than CSS pixels.
    pub async fn new(canvas_id: &str) -> Result<Self> {
        // 0. Install the bundled font fallbacks into the global
        //    font registry. `TextRenderer::new` uses
        //    `blinc_text::global_font_registry()` as its shared
        //    registry, so priming it here means every subsequent
        //    text-shaping pass — including ones that happen before
        //    the user's `setup` closure runs — sees both the emoji
        //    and symbol faces as part of the non-Apple fallback
        //    chain inside `FontRegistry::load_emoji_font`. The two
        //    subsets are complementary: NotoColorEmoji carries
        //    color pictographs while NotoSans + NotoSansMath cover
        //    the monochrome text glyphs (math operators, double
        //    arrows, currency symbols, Latin-1 Supplement
        //    punctuation) that a pure emoji font doesn't own.
        //    Safe to call multiple times (fontdb de-duplicates),
        //    so this is idempotent if the user's setup explicitly
        //    calls either `register` again.
        blinc_noto_emoji::register();
        blinc_noto_symbols::register();

        // 1. Locate the canvas in the DOM.
        let window = web_sys::window().ok_or_else(|| {
            BlincError::Platform("WebApp::new called without a global `window` object".to_string())
        })?;
        let document = window.document().ok_or_else(|| {
            BlincError::Platform("WebApp::new called without a `document` object".to_string())
        })?;
        let canvas: web_sys::HtmlCanvasElement = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| {
                BlincError::Platform(format!("No element with id `{canvas_id}` in document"))
            })?
            .dyn_into()
            .map_err(|_| {
                BlincError::Platform(format!("Element `{canvas_id}` is not an HtmlCanvasElement"))
            })?;

        // 2. Read logical size + DPR, then set the framebuffer to the
        //    physical size before creating the GPU surface. This is
        //    the canonical "resize the canvas to match its CSS size"
        //    pattern from the wgpu web examples — without it, the
        //    canvas defaults to 300×150 regardless of CSS.
        //
        //    Optional overrides via `data-` attributes on the canvas:
        //      <canvas data-width="800" data-height="600" data-dpr="1">
        //    When present, these lock the logical viewport and DPR to
        //    fixed values regardless of the browser's actual CSS layout
        //    or display scaling. This gives docs/book iframe demos a
        //    consistent rendering across all devices (no layout reflow
        //    on resize, no DPR-dependent visual differences).
        let data_attr = |name: &str| -> Option<f64> {
            canvas
                .get_attribute(name)
                .and_then(|v| v.parse::<f64>().ok())
        };
        let logical_width = data_attr("data-width")
            .map(|v| v as f32)
            .unwrap_or_else(|| canvas.client_width() as f32);
        let logical_height = data_attr("data-height")
            .map(|v| v as f32)
            .unwrap_or_else(|| canvas.client_height() as f32);
        let scale_factor = data_attr("data-dpr").unwrap_or_else(|| window.device_pixel_ratio());
        let physical_width = (logical_width * scale_factor as f32).round().max(1.0);
        let physical_height = (logical_height * scale_factor as f32).round().max(1.0);
        canvas.set_width(physical_width as u32);
        canvas.set_height(physical_height as u32);

        // 3. Build the GPU renderer from the canvas.
        //
        // Raise `max_primitives` from the default 10,000 to 20,000.
        // Desktop examples that are heavy on styled divs (styling_demo
        // generates ~10,300 primitives) overflow the 10k buffer on
        // wasm — the desktop runner can override via the
        // `BLINC_GPU_MAX_PRIMITIVES` env var, but `std::env::var` is
        // a no-op on wasm32.  20k covers every current example with
        // comfortable headroom and adds ~3.7 MB to GPU memory
        // (20,000 × 368 bytes per GpuPrimitive ≈ 7.2 MB total).
        let config = BlincConfig {
            max_primitives: 20_000,
            sample_count: 1, // SDF pipelines do their own shader-level AA
            ..Default::default()
        };
        let (blinc_app, surface) = BlincApp::with_canvas(canvas.clone(), Some(config)).await?;

        // 3a. Wire the global text measurer to the BlincApp's font
        // registry. Without this, the layout system falls back to
        // the heuristic measurer in `text_measurer.rs::estimate_size`
        // which assumes every glyph is exactly `0.55 * font_size`
        // wide — fine for rough flexbox sizing of one-shot text
        // labels, completely wrong for any widget that hit-tests
        // against per-glyph positions. The visible bug is text
        // selection / cursor placement landing several pixels off
        // from where you clicked, because the text widget computes
        // character offsets from estimated widths while the
        // renderer draws glyphs at their actual font metrics.
        // Same call the desktop runner makes at
        // [`windowed.rs:2535`](crate::windowed) and the iOS runner
        // does inside `init_text_measurer()` at `ios.rs:203`.
        crate::text_measurer::init_text_measurer_with_registry(blinc_app.font_registry());

        // 4. Configure the surface for the canvas's physical dimensions.
        let texture_format = blinc_app.texture_format();
        // COPY_SRC is needed for blend mode two-pass compositing, but
        // the GL (WebGL2) adapter doesn't support it on the surface.
        // Detect via renderer's has_storage_buffers — GL adapters lack
        // both storage buffers and surface COPY_SRC.
        let mut surface_usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        if blinc_app.has_storage_buffers() {
            surface_usage |= wgpu::TextureUsages::COPY_SRC;
        }
        let surface_config = wgpu::SurfaceConfiguration {
            usage: surface_usage,
            format: texture_format,
            width: physical_width as u32,
            height: physical_height as u32,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        surface.configure(blinc_app.device(), &surface_config);

        // 5. Build the shared collaborator graph that every platform
        //    needs. These mirror what the desktop runner constructs in
        //    `WindowedApp::run` (windowed.rs ~line 2105). The scheduler
        //    is built fresh — its `start_raf()` driver gets kicked off
        //    in [`Self::start_frame_loop`] once the user has wired their
        //    rebuild + render callback.
        let scheduler = AnimationScheduler::new();
        let animations: SharedAnimationScheduler = Arc::new(Mutex::new(scheduler));

        // Register the global animation scheduler handle so
        // `blinc_animation::get_scheduler()` works for components
        // that create `AnimatedValue`, `AnimatedKeyframe`, etc.
        // Desktop runner does the same at windowed.rs:2166. Use
        // `try_set` (OnceLock) indirectly — `set_global_scheduler`
        // panics on double-init, but `WebApp::new` can only be
        // called once per page load anyway.
        {
            let handle = animations.lock().unwrap().handle();
            blinc_animation::set_global_scheduler(handle);
        }

        let ref_dirty_flag: RefDirtyFlag = Arc::new(AtomicBool::new(false));
        // Shared process-global reactive graph — see windowed.rs.
        let reactive: SharedReactiveGraph = blinc_core::reactive::global_graph();
        let hooks = Arc::new(Mutex::new(HookState::new()));

        // Initialize the global `BlincContextState` singleton with
        // this runner's reactive graph, hook state, and dirty flag —
        // exactly the same call the desktop runner makes at
        // [`windowed.rs:2114`](crate::windowed). Without this,
        // every component that reaches for `BlincContextState::get()`
        // (which is every `ctx.use_state*`, every `Stateful::on_state`
        // body, every `State::set`, every signal-driven rebuild
        // path, …) panics or no-ops because the singleton is
        // uninitialized. The previous web runner created the four
        // shared collaborators and stuffed them into `WindowedContext`
        // but never wired them into the global, so reactive state
        // worked through `ctx.*` directly but `Stateful` widgets
        // and the implicit-context APIs all silently failed.
        if !BlincContextState::is_initialized() {
            #[allow(clippy::type_complexity)]
            let stateful_callback: Arc<dyn Fn(&[SignalId]) + Send + Sync> =
                Arc::new(|signal_ids| {
                    blinc_layout::check_stateful_deps(signal_ids);
                });
            BlincContextState::init_with_callback(
                Arc::clone(&reactive),
                Arc::clone(&hooks),
                Arc::clone(&ref_dirty_flag),
                stateful_callback.clone(),
            );
            blinc_core::reactive::set_stateful_deps_notifier(move |ids| stateful_callback(ids));
        }

        // Initialize the global ThemeState the same way the desktop /
        // Android / iOS runners do (windowed.rs:1979, android.rs:110,
        // ios.rs:152). Without this, any widget that reads a theme
        // token — buttons, text inputs, scroll bars, the entire `cn`
        // component library — panics on the first frame with
        // "ThemeState not initialized". The web target uses the
        // default Catppuccin-derived bundle (re-exported as
        // `WebTheme::bundle()`) and reads the user's preferred color
        // scheme from `window.matchMedia('(prefers-color-scheme: dark)')`.
        Self::init_theme();

        // Register the web asset loader so `ImageData::load` →
        // `global_asset_loader()` finds it. The `Arc` clone kept on
        // `WebApp` lets the user add assets at runtime via
        // `insert_asset` / async `preload_assets`. Mirrors the
        // desktop's `init_asset_loader()` at windowed.rs:1962.
        let asset_loader = std::sync::Arc::new(blinc_platform_web::WebAssetLoader::new());
        let shared = blinc_platform_web::assets::SharedWebAssetLoader(asset_loader.clone());
        let _ = blinc_platform::assets::set_global_asset_loader(Box::new(shared));

        let overlay_mgr = overlay_manager();
        // Initialize the global OverlayContext singleton so components
        // that call `get_overlay_manager()` (blinc_cn dropdowns,
        // dialogs, tooltips, etc.) find it. Desktop does this at
        // windowed.rs:2258, Android at android.rs:238, iOS at ios.rs:296.
        if !blinc_layout::overlay_state::OverlayContext::is_initialized() {
            blinc_layout::overlay_state::OverlayContext::init(Arc::clone(&overlay_mgr));
        }
        let element_registry: SharedElementRegistry = Arc::new(ElementRegistry::new());
        let ready_callbacks: SharedReadyCallbacks = Arc::new(Mutex::new(Vec::new()));

        let ctx = WindowedContext::new_web(
            logical_width,
            logical_height,
            scale_factor,
            physical_width,
            physical_height,
            true, // focused — Document.hasFocus() is true at startup; refreshed by visibility events later
            animations,
            ref_dirty_flag,
            reactive,
            hooks,
            overlay_mgr,
            element_registry,
            ready_callbacks,
        );

        // Build the RenderState that the full render path
        // (`render_tree_with_motion`) needs. Mirrors
        // windowed.rs:2590 where the desktop runner creates one.
        let render_state = blinc_layout::RenderState::new(Arc::clone(&ctx.animations));

        // CSS animation / transition store — shared between the
        // runner (which ticks it each frame) and the tree (which
        // reads animated property values back during
        // apply_all_css_animation_props / apply_all_css_transition_props).
        let css_anim_store = Arc::new(Mutex::new(blinc_layout::CssAnimationStore::new()));

        Ok(Self {
            canvas,
            surface,
            surface_config,
            blinc_app,
            ctx,
            ui_builder: None,
            current_tree: None,
            needs_rebuild: true,
            needs_full_rebuild: false,
            last_logical_size: (logical_width, logical_height),
            last_touch_pos: None,
            last_cursor: "default",
            last_wheel_time_ms: None,
            last_scroll_invalidate_ms: None,
            pending_wheel_delta: (0.0, 0.0),
            cached_modifiers: blinc_platform::Modifiers::default(),
            render_state,
            css_anim_store,
            last_frame_time_ms: 0,
            asset_loader,
        })
    }

    /// Convenience all-in-one entry point: locate the canvas, build
    /// the runner, install the user's UI builder, wire a render
    /// closure as the scheduler's wake callback, enable continuous
    /// redraw, and start the rAF loop.
    ///
    /// Returns once the rAF chain is wired. The chain self-perpetuates
    /// from inside the browser, so the page keeps rendering after this
    /// future resolves.
    ///
    /// Apps that need to load fonts, register CSS, or otherwise touch
    /// the runner before the first frame should use [`Self::run_with_setup`]
    /// instead — `run` is a thin wrapper that passes a no-op setup.
    pub async fn run<F, E>(canvas_id: &str, ui_builder: F) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: blinc_layout::ElementBuilder + 'static,
    {
        Self::run_with_setup(canvas_id, |_| {}, ui_builder).await
    }

    /// Same as [`Self::run`], plus a synchronous `setup` callback that
    /// runs after the runner is constructed and before the first frame
    /// is rendered. This is the canonical place to:
    ///
    /// - Load bundled font bytes via [`Self::load_font_data`]. Required
    ///   for any text to render — the wasm32 init path skips system
    ///   font discovery (no filesystem) so the font registry starts
    ///   empty.
    /// - Register CSS via `app.context_mut().add_css(...)`.
    /// - Wire up any other one-shot config that touches the runner.
    ///
    /// The setup callback receives a `&mut WebApp` and runs exactly
    /// once. It cannot be `async`; if you need fetch-based asset
    /// loading, do it BEFORE calling `run_with_setup` and pass the
    /// fetched bytes through your closure's environment.
    ///
    /// # Cycle / leak note
    ///
    /// This method intentionally creates an `Rc<RefCell<WebApp>>`
    /// cycle: the wake callback owns a clone of the `Rc`, the wake
    /// callback lives inside the scheduler, the scheduler lives inside
    /// the `WindowedContext`, and the context lives inside the
    /// `WebApp`. The cycle is what keeps everything alive past the
    /// return of this function. The browser tears it down on page
    /// unload, which is the expected lifecycle for a web app.
    pub async fn run_with_setup<S, F, E>(canvas_id: &str, setup: S, ui_builder: F) -> Result<()>
    where
        S: FnOnce(&mut Self) + 'static,
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: blinc_layout::ElementBuilder + 'static,
    {
        Self::run_with_setup_inner(
            canvas_id,
            move |app| {
                Box::pin(async move {
                    setup(app);
                    Ok(())
                })
            },
            ui_builder,
        )
        .await
    }

    /// Async sibling of [`Self::run_with_setup`] for setup steps
    /// that need to `.await` something — typically a `fetch()` call
    /// to load fonts or other assets before the first frame
    /// renders.
    ///
    /// The setup closure receives `&mut WebApp` and returns a
    /// `Future<Output = Result<()>>`. The returned future is
    /// awaited synchronously inside the runner before the UI
    /// builder is installed and the rAF loop kicks off, so any
    /// `app.load_font_data(...)` / `app.context_mut().add_css(...)`
    /// calls inside it land in the right order: first font, then
    /// first frame.
    ///
    /// # Example: fetched font
    ///
    /// ```ignore
    /// use blinc_app::web::WebApp;
    /// use blinc_platform_web::WebAssetLoader;
    ///
    /// WebApp::run_with_async_setup(
    ///     "blinc-canvas",
    ///     |app| Box::pin(async move {
    ///         // Single-shot fetch — bytes go straight into the
    ///         // font registry, no copy in the asset cache.
    ///         let bytes = WebAssetLoader::fetch_bytes("fonts/Inter.ttf").await
    ///             .map_err(|e| BlincError::Platform(e.to_string()))?;
    ///         app.load_font_data(bytes);
    ///         Ok(())
    ///     }),
    ///     build_ui,
    /// )
    /// .await
    /// ```
    ///
    /// The `Box::pin(async move { ... })` ceremony is needed because
    /// stable Rust doesn't have `async FnOnce` yet — the closure
    /// returns a boxed future. Once `async closures` stabilize this
    /// signature can drop the boxing.
    pub async fn run_with_async_setup<S, F, E>(
        canvas_id: &str,
        setup: S,
        ui_builder: F,
    ) -> Result<()>
    where
        S: for<'a> FnOnce(
            &'a mut Self,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + 'a>>,
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: blinc_layout::ElementBuilder + 'static,
    {
        Self::run_with_setup_inner(canvas_id, setup, ui_builder).await
    }

    /// Shared body of `run_with_setup` and `run_with_async_setup`.
    /// The only difference between the two is whether `setup`
    /// returns a future or runs synchronously — the sync wrapper
    /// just constructs an immediately-ready boxed future, so this
    /// inner function only ever sees the async form.
    async fn run_with_setup_inner<S, F, E>(canvas_id: &str, setup: S, ui_builder: F) -> Result<()>
    where
        S: for<'a> FnOnce(
            &'a mut Self,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + 'a>>,
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: blinc_layout::ElementBuilder + 'static,
    {
        let mut app = Self::new(canvas_id).await?;

        // Run user setup BEFORE installing the UI builder. This is
        // when fonts get loaded, CSS gets registered, etc. If setup
        // panics, we never reach the rAF loop and the browser's
        // panic-hook surfaces it in the console. Setup that returns
        // an `Err` is fatal — we propagate so the caller can decide
        // whether to surface it via `console.error`.
        setup(&mut app).await?;

        app.set_ui_builder(ui_builder);

        // Render the first frame synchronously so the canvas isn't
        // blank between `run().await` returning and the first rAF
        // tick (which can be ~16ms later, longer if the browser is
        // busy). Failures here are non-fatal — the next rAF tick will
        // try again.
        if let Err(e) = app.run_one_frame() {
            tracing::error!("WebApp::run initial frame failed: {e}");
        }

        // Wrap in Rc<RefCell<…>> so the wake closure can re-borrow
        // for each frame. The scheduler stores the wake callback as
        // `Arc<dyn Fn()>`; on wasm32 there's no `Send + Sync` bound,
        // so it can capture the `!Send` Rc.
        let app_rc = Rc::new(RefCell::new(app));
        let app_for_wake = Rc::clone(&app_rc);

        // The wake callback re-borrows the app and runs one frame.
        // `try_borrow_mut` (rather than `borrow_mut`) keeps us safe
        // if a future Phase 3d input handler is mid-mutation when the
        // rAF tick fires — we just skip the frame and try again next
        // tick rather than panicking on borrow conflict.
        let wake = move || {
            if let Ok(mut app) = app_for_wake.try_borrow_mut() {
                if let Err(e) = app.run_one_frame() {
                    tracing::error!("WebApp wake-frame failed: {e}");
                }
            }
        };

        // Install browser DOM event listeners. They share the same
        // `Rc<RefCell<WebApp>>` cycle as the wake callback. The
        // `try_borrow_mut` guard inside each handler dodges
        // reentrancy with the rAF wake callback (which holds its own
        // clone): if the rAF tick is mid-render when an event fires,
        // we drop that one event rather than panicking — the next
        // event will succeed.
        Self::install_input_listeners(Rc::clone(&app_rc))?;

        // Install the wake callback and enable continuous redraw so
        // the wake fires on every rAF tick (not just when an animation
        // is active). For a UI runtime, "render every frame the
        // browser asks for" is the right default — see windowed.rs
        // for the equivalent on desktop.
        //
        // We clone the scheduler `Arc` rather than holding the
        // `RefCell` borrow open across the `Mutex::lock()` — the
        // `MutexGuard` temporary that `if let Ok(...) = ...` produces
        // outlives the `RefCell` borrow otherwise, and the borrow
        // checker correctly rejects the drop ordering.
        let scheduler_arc = Arc::clone(&app_rc.borrow().ctx.animations);
        if let Ok(mut scheduler) = scheduler_arc.lock() {
            scheduler.set_wake_callback(wake);
            scheduler.set_continuous_redraw(true);
        }

        // Kick off the rAF chain. From here on the browser drives
        // every frame; this future returns immediately and the runtime
        // drops it.
        if let Ok(scheduler) = scheduler_arc.lock() {
            scheduler.start_raf();
        }

        // Don't return `app_rc` — let the cycle keep it alive. (See
        // the function-level "Cycle / leak note" doc comment.)
        Ok(())
    }

    /// Load font data from a byte buffer into the underlying
    /// [`BlincApp`]'s font registry.
    ///
    /// Returns the number of font faces registered (a single TTF
    /// usually has one face; TTC collections have several). Call this
    /// from a [`Self::run_with_setup`] setup closure with bundled
    /// `include_bytes!(...)` data, or with bytes fetched via
    /// `WebAssetLoader::preload`.
    ///
    /// **You must call this for at least one font.** The wasm32 init
    /// path deliberately skips system font discovery (no filesystem),
    /// so the font registry starts empty. Without a registered font,
    /// every text element fails to shape glyphs and renders as nothing.
    /// This is symmetric with how `BlincApp::with_canvas` documents the
    /// font situation.
    pub fn load_font_data(&mut self, bytes: Vec<u8>) -> usize {
        let faces = self.blinc_app.load_font_data_to_registry(bytes);

        // Re-run generic-family preloading after every font load so
        // the family→weight mapping binds to the newly-loaded bytes.
        // `preload_generic_styles` is called once in `with_canvas`
        // (during `WebApp::new`), but at that point the font registry
        // is empty — no bytes have been loaded yet. Fonts are loaded
        // later via this method in the setup closure. Without re-
        // running the preload, `GenericFont::Monospace` never resolves
        // to FiraCode and every `.monospace()` / `inline_code()` text
        // element fails with "No fonts available".
        self.blinc_app.refresh_generic_font_styles();

        faces
    }

    /// Insert an asset into the web asset loader's cache so it's
    /// available to `img("key")`, `svg(include_str!(...))`, and any
    /// other API that routes through `ImageData::load` →
    /// `global_asset_loader().load()`.
    ///
    /// `key` is the same string you'd pass to `img()`. For example:
    ///
    /// ```ignore
    /// app.insert_asset("assets/images/logo.png", include_bytes!("../../assets/images/logo.png").to_vec());
    /// // Then in build_ui:
    /// img("assets/images/logo.png").w(200.0).h(100.0)
    /// ```
    ///
    /// For runtime-fetched assets (not `include_bytes!`), use
    /// [`preload_assets`](Self::preload_assets) instead.
    pub fn insert_asset(&self, key: impl Into<String>, bytes: Vec<u8>) {
        self.asset_loader.insert_raw(key, bytes);
    }

    /// Fetch one or more asset URLs via the browser `fetch()` API and
    /// insert them into the asset loader's cache. Call this from a
    /// [`run_with_async_setup`](Self::run_with_async_setup) closure
    /// before the first frame so `img("path")` elements can find
    /// the bytes at render time.
    ///
    /// ```ignore
    /// WebApp::run_with_async_setup(
    ///     "blinc-canvas",
    ///     |app| Box::pin(async move {
    ///         app.preload_assets(&["images/hero.png", "images/icon.svg"]).await?;
    ///         Ok(())
    ///     }),
    ///     build_ui,
    /// ).await
    /// ```
    #[cfg(target_arch = "wasm32")]
    pub async fn preload_assets(&self, urls: &[&str]) -> crate::error::Result<()> {
        self.asset_loader
            .preload(urls)
            .await
            .map_err(|e| crate::error::BlincError::Platform(format!("preload_assets: {e}")))
    }

    /// Shared handle to the asset loader's preload progress. Cheap to
    /// clone (it's an `Arc`) and safe to poll every frame — the
    /// counters inside are atomic.
    ///
    /// Apps that want first-paint-before-assets show a loading state
    /// rebuilt from this handle while preload runs in the background.
    /// See `blinc_canvas_kit::loading_overlay` for a drop-in widget.
    pub fn preload_progress(&self) -> std::sync::Arc<blinc_platform_web::PreloadProgress> {
        self.asset_loader.progress()
    }

    /// Cloneable handle to the underlying `WebAssetLoader`. Returned
    /// so apps can move it into a `spawn_local` closure and start a
    /// preload pass that runs concurrently with the first-frame
    /// render — the wasm wrapper generator uses this pattern.
    pub fn asset_loader_handle(&self) -> std::sync::Arc<blinc_platform_web::WebAssetLoader> {
        self.asset_loader.clone()
    }

    /// Install browser DOM event listeners that route input through the
    /// shared [`WindowedContext::event_router`] and dispatch the
    /// resulting events through the cached render tree.
    ///
    /// This is the wasm32 sibling of the desktop runner's input pump
    /// (`windowed.rs:2326+`). Same contract:
    /// - Mouse coords arrive in CSS pixels (which are also logical
    ///   pixels for our purposes — the canvas's `client_width`/
    ///   `client_height` are CSS pixels, and the renderer's layout
    ///   thinks in logical pixels).
    /// - `EventRouter::on_mouse_*` returns a `Vec<(LayoutNodeId, u32)>`
    ///   of events that need to be dispatched through
    ///   `RenderTree::dispatch_event` to actually fire user handlers.
    /// - Keyboard events use the legacy DOM `keyCode` (8 = Backspace,
    ///   13 = Enter, 27 = Escape, 65-90 = A-Z, etc.) which is what the
    ///   `EventRouter::on_key_*` API takes — no enum conversion needed.
    ///
    /// Each closure captures an `Rc<RefCell<WebApp>>` clone and uses
    /// `try_borrow_mut` to dodge reentrancy with the rAF wake callback
    /// (which holds its own clone). If the borrow fails, the event is
    /// dropped — the next event of the same kind will succeed.
    /// `Closure::forget()` deliberately leaks each closure for the
    /// lifetime of the app, matching the rAF chain leak in
    /// [`AnimationScheduler::start_raf`].
    ///
    /// Keyboard listeners are installed on `document` rather than the
    /// canvas because canvases don't get keyboard focus without
    /// `tabindex` shenanigans, and `document` events are reliably
    /// delivered.
    fn install_input_listeners(app_rc: Rc<RefCell<Self>>) -> Result<()> {
        let window = web_sys::window().ok_or_else(|| {
            BlincError::Platform(
                "WebApp::install_input_listeners called without a global `window` object"
                    .to_string(),
            )
        })?;
        let document = window.document().ok_or_else(|| {
            BlincError::Platform(
                "WebApp::install_input_listeners called without a `document` object".to_string(),
            )
        })?;
        // Borrow once to get a clone of the canvas reference. The
        // canvas itself lives inside the WebApp, but `add_event_listener`
        // only needs the EventTarget for the lifetime of the call —
        // the closures we attach own the routing back into the WebApp.
        let canvas = app_rc.borrow().canvas.clone();

        // ----- mousemove -----
        {
            let app_rc = Rc::clone(&app_rc);
            let closure = Closure::<dyn FnMut(_)>::new(move |evt: web_sys::MouseEvent| {
                if let Ok(mut app) = app_rc.try_borrow_mut() {
                    Self::stamp_modifiers_from_mouse(&mut app, &evt);
                    let x = evt.offset_x() as f32;
                    let y = evt.offset_y() as f32;
                    Self::dispatch_mouse_move(&mut app, x, y);
                }
            });
            canvas
                .add_event_listener_with_callback("mousemove", closure.as_ref().unchecked_ref())
                .map_err(|e| {
                    BlincError::Platform(format!("add mousemove listener failed: {e:?}"))
                })?;
            closure.forget();
        }

        // ----- mousedown -----
        //
        // Skip right-click (button 2) and middle-click (button 1)
        // here. Right-click is owned by the `contextmenu` listener
        // below, which builds the custom edit-menu overlay; we
        // don't want the same gesture to also fire `on_mouse_down`
        // through the regular dispatch path because that would:
        //   1. Call `blur_all_text_inputs()`, dropping the focus
        //      and selection on whichever input the user
        //      right-clicked.
        //   2. Re-fire `EventRouter::on_mouse_down` against the
        //      hit-test target, which clears the text widget's
        //      selection_start. By the time the user picks "Copy"
        //      from the menu, there's nothing selected to copy.
        // Native macOS / Windows behavior is "right-click leaves
        // focus and selection alone", and that's what we mirror
        // here. Middle-click is also dropped because we have no
        // wired-up middle-click semantics yet — better to ignore
        // it than to treat it like a left-click.
        {
            let app_rc = Rc::clone(&app_rc);
            let closure = Closure::<dyn FnMut(_)>::new(move |evt: web_sys::MouseEvent| {
                if evt.button() != 0 {
                    return;
                }
                if let Ok(mut app) = app_rc.try_borrow_mut() {
                    Self::stamp_modifiers_from_mouse(&mut app, &evt);
                    let x = evt.offset_x() as f32;
                    let y = evt.offset_y() as f32;
                    let button = blinc_platform_web::input::convert_mouse_button(evt.button());
                    Self::dispatch_mouse_down(&mut app, x, y, button);
                }
            });
            canvas
                .add_event_listener_with_callback("mousedown", closure.as_ref().unchecked_ref())
                .map_err(|e| {
                    BlincError::Platform(format!("add mousedown listener failed: {e:?}"))
                })?;
            closure.forget();
        }

        // ----- mouseup -----
        //
        // Mirror the mousedown filter: only the left button drives
        // dispatch_mouse_up. Right-button mouseups would otherwise
        // fire POINTER_UP / DRAG_END events that the framework
        // doesn't expect from a right-click.
        {
            let app_rc = Rc::clone(&app_rc);
            let closure = Closure::<dyn FnMut(_)>::new(move |evt: web_sys::MouseEvent| {
                if evt.button() != 0 {
                    return;
                }
                if let Ok(mut app) = app_rc.try_borrow_mut() {
                    Self::stamp_modifiers_from_mouse(&mut app, &evt);
                    let x = evt.offset_x() as f32;
                    let y = evt.offset_y() as f32;
                    let button = blinc_platform_web::input::convert_mouse_button(evt.button());
                    Self::dispatch_mouse_up(&mut app, x, y, button);
                }
            });
            canvas
                .add_event_listener_with_callback("mouseup", closure.as_ref().unchecked_ref())
                .map_err(|e| BlincError::Platform(format!("add mouseup listener failed: {e:?}")))?;
            closure.forget();
        }

        // ----- wheel -----
        //
        // The handler does NOT dispatch directly to scroll physics.
        // Instead, it accumulates the raw delta into
        // `pending_wheel_delta`, and `run_one_frame` drains it once
        // per rAF tick through a damping function. This is what
        // gives the runner a true *per-frame* speed cap rather than
        // a per-event one — multiple wheel events arriving inside a
        // single frame interval coalesce into one dispatch, so the
        // user can't accidentally push the scroll offset hundreds
        // of pixels deep into rubber-band on a fast macOS trackpad
        // swipe (which fires ~10 events per 16 ms frame during the
        // gesture and then leaks ~800 ms of momentum-tail events on
        // top).
        {
            let app_rc = Rc::clone(&app_rc);
            let closure = Closure::<dyn FnMut(_)>::new(move |evt: web_sys::WheelEvent| {
                // Prevent the page from scrolling under the canvas
                // when the user wheels over it. Apps that want page
                // scrolling can revisit this in a future config knob.
                evt.prevent_default();
                if let Ok(mut app) = app_rc.try_borrow_mut() {
                    // WheelEvent extends MouseEvent — `shift_key()` /
                    // `ctrl_key()` / `alt_key()` / `meta_key()` come
                    // through inherited.
                    app.cached_modifiers = blinc_platform::Modifiers {
                        shift: evt.shift_key(),
                        ctrl: evt.ctrl_key(),
                        alt: evt.alt_key(),
                        meta: evt.meta_key(),
                    };
                    // Normalise wheel delta to pixels. delta_mode 0 is
                    // pixels (most browsers); 1 is lines (Firefox
                    // legacy); 2 is pages.
                    let multiplier: f32 = match evt.delta_mode() {
                        0 => 1.0,            // pixels
                        1 => 16.0,           // line ≈ 16px
                        2 => app.ctx.height, // page = viewport height
                        _ => 1.0,
                    };
                    let raw_dx = -(evt.delta_x() as f32) * multiplier;
                    let raw_dy = -(evt.delta_y() as f32) * multiplier;
                    app.pending_wheel_delta.0 += raw_dx;
                    app.pending_wheel_delta.1 += raw_dy;
                }
            });
            canvas
                .add_event_listener_with_callback("wheel", closure.as_ref().unchecked_ref())
                .map_err(|e| BlincError::Platform(format!("add wheel listener failed: {e:?}")))?;
            closure.forget();
        }

        // ----- touchstart / touchmove / touchend / touchcancel -----
        //
        // The web target needs touch handling for two distinct
        // reasons:
        //
        //   1. **Tap-as-click**: a single-finger tap should reach
        //      every widget the same way a mouse click does so
        //      buttons, text inputs, links, etc. all work on a
        //      mobile browser. We synthesize mousedown/mouseup
        //      from touchstart/touchend at the touch position,
        //      mirroring the iOS runner's
        //      `MouseButton::Left` dispatch from `TouchPhase::Began`
        //      / `Ended` (see ios.rs:828-908).
        //
        //   2. **Swipe-to-scroll**: a single-finger drag should
        //      advance scroll containers under the finger. Touch
        //      events don't fire wheel events, so without this
        //      handler the mobile browser can't scroll a Blinc
        //      `scroll()` container at all. We track
        //      `last_touch_pos` between touchmove ticks and feed
        //      the delta into `dispatch_scroll`, the same path
        //      the wheel handler uses.
        //
        // We deliberately ignore multi-touch (pinch / two-finger
        // pan) here — those need their own pinch-zoom plumbing
        // through `EventRouter::on_pinch`, and the framework
        // doesn't yet wire pinch through the web runner. Single-
        // finger gestures cover the common UX (taps, swipes,
        // long presses) and that matches what `web_mobile_demo`
        // exercises.
        //
        // Each handler converts page-relative `client_x` /
        // `client_y` to canvas-local CSS pixels via the canvas's
        // `getBoundingClientRect()`. Touch events don't carry
        // `offset_x` / `offset_y`, so we have to do this
        // ourselves.
        {
            // Helper closure shared by all four touch handlers:
            // grab the first touch and translate to canvas-local
            // CSS pixels. Returns `None` if there's no first
            // touch (zero-touch touchend, multi-touch ignored).
            //
            // The bounding rect is read on every event because
            // page scroll, layout shifts, and CSS animations can
            // all move the canvas — caching here would silently
            // drift away from reality.
            let canvas_for_touch = canvas.clone();
            let touch_local_pos =
                move |touch_list: web_sys::TouchList| -> Option<(f32, f32, usize)> {
                    let len = touch_list.length() as usize;
                    if len == 0 {
                        return None;
                    }
                    let touch = touch_list.get(0)?;
                    let rect = canvas_for_touch.get_bounding_client_rect();
                    let x = touch.client_x() as f32 - rect.left() as f32;
                    let y = touch.client_y() as f32 - rect.top() as f32;
                    Some((x, y, len))
                };

            // touchstart
            {
                let app_rc = Rc::clone(&app_rc);
                let touch_local_pos = touch_local_pos.clone();
                let closure = Closure::<dyn FnMut(_)>::new(move |evt: web_sys::TouchEvent| {
                    // `prevent_default` here also stops the
                    // browser from synthesising a follow-up
                    // mousedown 300ms later (the legacy
                    // touch-to-click compat path), which would
                    // otherwise cause every tap to fire two
                    // POINTER_DOWN events.
                    evt.prevent_default();
                    if let Ok(mut app) = app_rc.try_borrow_mut() {
                        if let Some((x, y, len)) = touch_local_pos(evt.touches()) {
                            // Mark this event sequence as touch
                            // input so editable widgets switch to
                            // mobile UX (touch drag = move cursor,
                            // double-tap = native edit menu, etc.).
                            blinc_layout::widgets::text_input::set_touch_input(true);
                            if len == 1 {
                                app.last_touch_pos = Some((x, y));
                                Self::dispatch_mouse_down(
                                    &mut app,
                                    x,
                                    y,
                                    blinc_platform::MouseButton::Left,
                                );
                            } else {
                                // Multi-touch: cancel any
                                // single-touch tracking so a
                                // pinch doesn't get
                                // misinterpreted as a swipe when
                                // the user lifts a finger.
                                app.last_touch_pos = None;
                            }
                        }
                    }
                });
                canvas
                    .add_event_listener_with_callback(
                        "touchstart",
                        closure.as_ref().unchecked_ref(),
                    )
                    .map_err(|e| {
                        BlincError::Platform(format!("add touchstart listener failed: {e:?}"))
                    })?;
                closure.forget();
            }

            // touchmove
            {
                let app_rc = Rc::clone(&app_rc);
                let touch_local_pos = touch_local_pos.clone();
                let closure = Closure::<dyn FnMut(_)>::new(move |evt: web_sys::TouchEvent| {
                    // Block the browser's default touch-scroll
                    // behavior (rubber-band, address-bar reveal,
                    // pull-to-refresh) so the canvas owns scroll
                    // semantics end-to-end.
                    evt.prevent_default();
                    if let Ok(mut app) = app_rc.try_borrow_mut() {
                        if let Some((x, y, len)) = touch_local_pos(evt.touches()) {
                            if len == 1 {
                                if let Some((px, py)) = app.last_touch_pos {
                                    let dx = x - px;
                                    let dy = y - py;
                                    // Sub-pixel jitter guard
                                    // mirroring ios.rs:874.
                                    if dx.abs() > 0.5 || dy.abs() > 0.5 {
                                        Self::dispatch_scroll(&mut app, dx, dy);
                                    }
                                }
                                app.last_touch_pos = Some((x, y));
                                // Also forward as a mouse_move so
                                // hover state / drag handlers
                                // see the touch path.
                                Self::dispatch_mouse_move(&mut app, x, y);
                            } else {
                                app.last_touch_pos = None;
                            }
                        }
                    }
                });
                canvas
                    .add_event_listener_with_callback("touchmove", closure.as_ref().unchecked_ref())
                    .map_err(|e| {
                        BlincError::Platform(format!("add touchmove listener failed: {e:?}"))
                    })?;
                closure.forget();
            }

            // touchend
            {
                let app_rc = Rc::clone(&app_rc);
                let touch_local_pos = touch_local_pos.clone();
                let closure = Closure::<dyn FnMut(_)>::new(move |evt: web_sys::TouchEvent| {
                    evt.prevent_default();
                    if let Ok(mut app) = app_rc.try_borrow_mut() {
                        // touchend's `touches()` is the list of
                        // STILL-ACTIVE touches (not the ones that
                        // just ended), so we read the released
                        // touch from `changed_touches()` instead.
                        let pos = touch_local_pos(evt.changed_touches())
                            .or(app.last_touch_pos.map(|(x, y)| (x, y, 1)));
                        if let Some((x, y, _)) = pos {
                            Self::dispatch_mouse_up(
                                &mut app,
                                x,
                                y,
                                blinc_platform::MouseButton::Left,
                            );
                        }
                        // Finger lifted — fire `on_scroll_end()` on the
                        // most-recently-scrolled container. This covers
                        // BOTH cases the touch path needs to handle:
                        //
                        // 1. Overscrolled at release → rubber-band snap
                        //    back (what the older `on_gesture_end()`
                        //    path did exclusively).
                        // 2. Released with velocity → transition the
                        //    scroll FSM to `Decelerating` so the
                        //    momentum-decay tick loop runs out the
                        //    flick. `apply_touch_scroll_delta()` has
                        //    already been accumulating velocity from
                        //    each `touchmove`; without this call the
                        //    momentum was computed but never consumed
                        //    — flicks stopped dead on finger-lift
                        //    instead of free-scrolling.
                        //
                        // iOS (ios.rs:1097) and Android (android.rs:1024)
                        // both already call `on_scroll_end` on touch-end
                        // for the same reason. Web was the outlier.
                        if let Some(ref mut tree) = app.current_tree {
                            tree.on_scroll_end();
                        }
                        // Cancel any armed long-press timer the
                        // user just dismissed by lifting their
                        // finger before the deadline.
                        blinc_layout::widgets::text_input::cancel_long_press_timer();
                        app.last_touch_pos = None;
                    }
                });
                canvas
                    .add_event_listener_with_callback("touchend", closure.as_ref().unchecked_ref())
                    .map_err(|e| {
                        BlincError::Platform(format!("add touchend listener failed: {e:?}"))
                    })?;
                closure.forget();
            }

            // touchcancel
            {
                let app_rc = Rc::clone(&app_rc);
                let closure = Closure::<dyn FnMut(_)>::new(move |evt: web_sys::TouchEvent| {
                    evt.prevent_default();
                    if let Ok(mut app) = app_rc.try_borrow_mut() {
                        if let Some((x, y)) = app.last_touch_pos {
                            Self::dispatch_mouse_up(
                                &mut app,
                                x,
                                y,
                                blinc_platform::MouseButton::Left,
                            );
                        }
                        blinc_layout::widgets::text_input::cancel_long_press_timer();
                        app.last_touch_pos = None;
                    }
                });
                canvas
                    .add_event_listener_with_callback(
                        "touchcancel",
                        closure.as_ref().unchecked_ref(),
                    )
                    .map_err(|e| {
                        BlincError::Platform(format!("add touchcancel listener failed: {e:?}"))
                    })?;
                closure.forget();
            }
        }

        // ----- keydown (on document, not canvas) -----
        {
            let app_rc = Rc::clone(&app_rc);
            let closure = Closure::<dyn FnMut(_)>::new(move |evt: web_sys::KeyboardEvent| {
                if let Ok(mut app) = app_rc.try_borrow_mut() {
                    // The DOM `keyCode` attribute returns the legacy
                    // virtual-key code (8 = Backspace, 13 = Enter,
                    // 27 = Escape, 65-90 = A-Z, …). This matches the
                    // codes the desktop runner builds in
                    // `windowed.rs:3052` exactly, so the same widget
                    // key shortcuts work without translation.
                    let key_code = evt.key_code();
                    let shift = evt.shift_key();
                    let ctrl = evt.ctrl_key();
                    let alt = evt.alt_key();
                    let meta = evt.meta_key();
                    app.cached_modifiers = blinc_platform::Modifiers {
                        shift,
                        ctrl,
                        alt,
                        meta,
                    };

                    // Suppress browser default actions for app-level
                    // keyboard chords before dispatching, so e.g.
                    // Cmd+D doesn't open the bookmark dialog on top
                    // of the Blinc canvas. The rule: any Cmd / Ctrl
                    // + letter chord is treated as an app shortcut.
                    // Digits and function keys are left alone so
                    // Cmd+1..9 (browser tab switching) and F1..F12
                    // (devtools) keep working. Clipboard chords
                    // (Cmd+C / V / X) are still safe to suppress
                    // here — the browser fires its `copy` / `cut` /
                    // `paste` events independently of the keydown
                    // default, and Blinc's paste listener attaches
                    // to those events directly.
                    let is_letter = (0x41..=0x5A).contains(&key_code);
                    if (meta || ctrl) && is_letter {
                        evt.prevent_default();
                    }
                    Self::dispatch_key_down(&mut app, key_code, shift, ctrl, alt, meta);

                    // For printable single-character keys, also
                    // dispatch TEXT_INPUT so editor widgets can
                    // observe the typed character. The `key()` value
                    // is the W3C key string ("a", "Hello", "Enter"…);
                    // we only forward single-character non-control
                    // values, and only when no Ctrl/Cmd is held
                    // (matches the desktop runner's behaviour).
                    let key_str = evt.key();
                    let mut chars = key_str.chars();
                    if let (Some(ch), None) = (chars.next(), chars.next()) {
                        if !ch.is_control() && !ctrl && !meta {
                            // Prevent the browser from also acting on
                            // the key (e.g. quick-find triggering on
                            // `/`, space scrolling the page) when a
                            // Blinc text input is consuming it.
                            evt.prevent_default();
                            Self::dispatch_text_input(&mut app, ch, shift, ctrl, alt, meta);
                        }
                    }
                }
            });
            document
                .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())
                .map_err(|e| BlincError::Platform(format!("add keydown listener failed: {e:?}")))?;
            closure.forget();
        }

        // ----- keyup (on document) -----
        {
            let app_rc = Rc::clone(&app_rc);
            let closure = Closure::<dyn FnMut(_)>::new(move |evt: web_sys::KeyboardEvent| {
                if let Ok(mut app) = app_rc.try_borrow_mut() {
                    let key_code = evt.key_code();
                    let shift = evt.shift_key();
                    let ctrl = evt.ctrl_key();
                    let alt = evt.alt_key();
                    let meta = evt.meta_key();
                    app.cached_modifiers = blinc_platform::Modifiers {
                        shift,
                        ctrl,
                        alt,
                        meta,
                    };
                    Self::dispatch_key_up(&mut app, key_code, shift, ctrl, alt, meta);
                }
            });
            document
                .add_event_listener_with_callback("keyup", closure.as_ref().unchecked_ref())
                .map_err(|e| BlincError::Platform(format!("add keyup listener failed: {e:?}")))?;
            closure.forget();
        }

        // ----- paste (on document) -----
        //
        // The browser's `paste` event is the only path that gives us
        // sync access to clipboard text. `navigator.clipboard.readText()`
        // is async-only and can't satisfy the widget's sync
        // `clipboard_read() -> Option<String>` contract — so instead of
        // routing Cmd+V through the widget's clipboard_read handler, we
        // intercept the paste event itself and broadcast each character
        // as a TEXT_INPUT event into the focused widget. The browser
        // fires this event for:
        //   * Cmd+V / Ctrl+V keyboard shortcut
        //   * Right-click → Paste from the browser's native context menu
        //   * Edit → Paste from the browser's menu bar
        //   * Mobile long-press → Paste
        //
        // Our custom context menu's Paste button doesn't go through
        // this path because it can't synthesize a real paste event
        // (the browser only fires those for user gestures); it calls
        // `navigator.clipboard.readText()` directly and dispatches the
        // resolved text via `broadcast_text_input_event`.
        {
            let app_rc = Rc::clone(&app_rc);
            let closure = Closure::<dyn FnMut(_)>::new(move |evt: web_sys::ClipboardEvent| {
                if let Ok(mut app) = app_rc.try_borrow_mut() {
                    let Some(data) = evt.clipboard_data() else {
                        return;
                    };
                    let text = data.get_data("text/plain").unwrap_or_default();
                    if text.is_empty() {
                        return;
                    }
                    evt.prevent_default();
                    if let Some(tree) = app.current_tree.as_mut() {
                        // Broadcast each char individually so the
                        // widget's `on_event(TEXT_INPUT, …)` handler
                        // sees them through `EventContext::key_char`,
                        // matching how the keydown handler delivers
                        // single typed characters. The widget appends
                        // each character to its buffer; multi-line
                        // pastes flow through naturally because '\n'
                        // is just another char from the widget's
                        // perspective.
                        for ch in text.chars() {
                            tree.broadcast_text_input_event(ch, false, false, false, false);
                        }
                    }
                }
            });
            document
                .add_event_listener_with_callback("paste", closure.as_ref().unchecked_ref())
                .map_err(|e| BlincError::Platform(format!("add paste listener failed: {e:?}")))?;
            closure.forget();
        }

        // ----- contextmenu (on canvas) -----
        //
        // The browser's right-click menu (Inspect Element, Save Image,
        // …) is unhelpful inside a Blinc canvas — Blinc owns its own
        // hit-testing and the browser has no idea which Blinc widget
        // the cursor is over. Suppress the default menu via
        // `preventDefault()` so the canvas owns the right-click
        // gesture, then route a synthetic `show_edit_menu` call into
        // Rust the same way the iOS double-tap path does. The web
        // implementation of `show_edit_menu` lives in the Rust web
        // runner (see `Self::handle_show_edit_menu`); it builds an
        // absolutely positioned `<div>` overlay with Cut / Copy /
        // Paste / Select All buttons.
        //
        // Whether or not we actually show a menu depends on whether
        // the user clicked on a focused editable widget — there's no
        // point showing a Cut / Copy menu over a header text. We
        // check `focused_editable_node_id()` for the gate.
        {
            let app_rc = Rc::clone(&app_rc);
            let closure = Closure::<dyn FnMut(_)>::new(move |evt: web_sys::MouseEvent| {
                evt.prevent_default();
                if let Ok(mut app) = app_rc.try_borrow_mut() {
                    let x = evt.offset_x() as f32;
                    let y = evt.offset_y() as f32;
                    // Anchor the menu in viewport (page) coordinates
                    // because the overlay is appended to `document.body`,
                    // not the canvas. `client_x/y` give us
                    // viewport-relative pixels.
                    let page_x = evt.client_x() as f32;
                    let page_y = evt.client_y() as f32;
                    Self::handle_context_menu(&mut app, x, y, page_x, page_y);
                }
            });
            canvas
                .add_event_listener_with_callback("contextmenu", closure.as_ref().unchecked_ref())
                .map_err(|e| {
                    BlincError::Platform(format!("add contextmenu listener failed: {e:?}"))
                })?;
            closure.forget();
        }

        // ----- resize (on window) -----
        //
        // `window.resize` fires for any viewport change — browser-window
        // resize, devtools toggle, fullscreen enter/exit, orientation
        // change. The actual diff against the previous canvas size lives
        // inside `handle_resize`, which bails when nothing changed.
        //
        // The listener has to attach to `window`, not `canvas`: a CSS
        // `width: 100%` canvas only sees its own dimensions change as a
        // side-effect of the window resizing, and there is no DOM event
        // for "an element's CSS-computed size changed" outside of
        // `ResizeObserver` (which we can adopt later if apps need to
        // react to non-window-driven layout shifts).
        {
            let app_rc = Rc::clone(&app_rc);
            let closure = Closure::<dyn FnMut(_)>::new(move |_evt: web_sys::Event| {
                if let Ok(mut app) = app_rc.try_borrow_mut() {
                    Self::handle_resize(&mut app);
                }
            });
            window
                .add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref())
                .map_err(|e| BlincError::Platform(format!("add resize listener failed: {e:?}")))?;
            closure.forget();
        }

        Ok(())
    }

    // ===========================================================================
    // Per-event dispatch helpers
    // ===========================================================================
    //
    // Each helper takes a `&mut WebApp` (already borrowed mutably by
    // the calling closure) and runs the EventRouter call → dispatch
    // pending events through the cached render tree. Factored out so
    // every event-handler closure stays a one-liner.

    /// Refresh `cached_modifiers` from a browser MouseEvent's modifier
    /// flags. Every mouse-class DOM event we receive (mousemove /
    /// mousedown / mouseup / wheel) calls this immediately on entry
    /// so subsequent dispatch through `dispatch_pending` sees the
    /// modifier state that was held *at the time of the event* —
    /// not whatever happened to be cached from the last keydown.
    /// Without this, the user could hold Shift, click on the canvas,
    /// and the resulting POINTER_DOWN would still report `shift=false`
    /// because no keydown event fires while the modifier is just
    /// being held (only the first press fires keydown; held repeats
    /// arrive as autorepeat after a delay).
    fn stamp_modifiers_from_mouse(app: &mut Self, evt: &web_sys::MouseEvent) {
        app.cached_modifiers = blinc_platform::Modifiers {
            shift: evt.shift_key(),
            ctrl: evt.ctrl_key(),
            alt: evt.alt_key(),
            meta: evt.meta_key(),
        };
    }

    fn dispatch_mouse_move(app: &mut Self, x: f32, y: f32) {
        let tree = match app.current_tree.as_ref() {
            Some(t) => t,
            None => return,
        };
        let pending = app.ctx.event_router.on_mouse_move(tree, x, y);

        // Update the canvas cursor based on the hovered element's
        // `CursorStyle`. Mirrors the desktop runner's
        // `window.set_cursor(...)` call inside `MouseEvent::Moved`
        // (windowed.rs:2926-2930), translated to CSS
        // `style.cursor` writes on the canvas. The query has to
        // run BEFORE `dispatch_pending` so the immutable tree
        // borrow doesn't conflict with the mutable borrow that
        // `dispatch_pending` needs.
        let cursor_style = tree
            .get_cursor_at(&app.ctx.event_router, x, y)
            .unwrap_or(blinc_layout::element::CursorStyle::Default);
        let css_cursor = cursor_style_to_css(cursor_style);
        if css_cursor != app.last_cursor {
            // Touch the DOM only when the cursor actually changes.
            // `style.cursor` writes are cheap individually but they
            // queue style invalidations on the canvas — without
            // this guard, every mousemove (60+/sec on a fast
            // pointer sweep) churns through the browser's style
            // recalc path for no visible benefit.
            if let Some(html_canvas) = app.canvas.dyn_ref::<web_sys::HtmlElement>() {
                let _ = html_canvas.style().set_property("cursor", css_cursor);
            }
            app.last_cursor = css_cursor;
        }

        Self::dispatch_pending(app, pending);
    }

    fn dispatch_mouse_down(app: &mut Self, x: f32, y: f32, button: blinc_platform::MouseButton) {
        // Mouse is the primary input — flip touch flag off so any
        // editable widget that branches on `is_touch_input()`
        // (text_input drag = move-cursor vs select-text) reverts
        // to desktop semantics.
        blinc_layout::widgets::text_input::set_touch_input(false);

        // Blur any focused text inputs BEFORE processing the click.
        // Mirrors the desktop runner at windowed.rs:2913 and the
        // iOS runner at ios.rs:848: tapping anywhere globally
        // clears focus, and the text input that gets clicked
        // re-focuses itself via its own on_mouse_down handler.
        // Without this, clicking outside an input keeps the input
        // focused indefinitely — the user can never blur it
        // except by clicking another input.
        blinc_layout::widgets::blur_all_text_inputs();

        let tree = match app.current_tree.as_ref() {
            Some(t) => t,
            None => return,
        };
        let pending = app
            .ctx
            .event_router
            .on_mouse_down(tree, x, y, convert_layout_button(button));
        Self::dispatch_pending(app, pending);
    }

    fn dispatch_mouse_up(app: &mut Self, x: f32, y: f32, button: blinc_platform::MouseButton) {
        let tree = match app.current_tree.as_ref() {
            Some(t) => t,
            None => return,
        };
        let pending = app
            .ctx
            .event_router
            .on_mouse_up(tree, x, y, convert_layout_button(button));
        Self::dispatch_pending(app, pending);
    }

    fn dispatch_scroll(app: &mut Self, delta_x: f32, delta_y: f32) {
        // Hit-test under the cursor first (immutable borrow), then
        // walk the chain of scroll containers from leaf to root via
        // `dispatch_scroll_chain`. This is the same path the desktop
        // runner takes ([`windowed.rs:3327-3340`](crate::windowed))
        // and is what *actually moves* the scroll position — the
        // simpler `EventRouter::on_scroll` only emits a SCROLL bubble
        // event, it does not advance scroll physics.
        //
        // Wheel events that arrive while a bounce-back spring is
        // already running are dropped: `apply_scroll_delta` early-
        // returns in that state anyway, but skipping the dispatch
        // here also keeps `last_wheel_time_ms` from being refreshed
        // by macOS's ~800ms momentum-tail wheel events, which is
        // what would otherwise prevent the idle-debounce in
        // `run_one_frame` from ever firing during a bounce.
        if let Some(tree) = app.current_tree.as_ref() {
            if tree.has_bouncing_scroll() {
                return;
            }
        }

        let hit = {
            let tree = match app.current_tree.as_ref() {
                Some(t) => t,
                None => return,
            };
            app.ctx
                .event_router
                .on_scroll_nested(tree, delta_x, delta_y)
        };

        let Some(hit) = hit else {
            // Cursor isn't over any element — nothing to scroll. This
            // happens before the user has moved the mouse over the
            // canvas (mouse_position defaults to (0, 0)).
            return;
        };

        let (mx, my) = app.ctx.event_router.mouse_position();
        if let Some(tree) = app.current_tree.as_mut() {
            tree.dispatch_scroll_chain(hit.node, &hit.ancestors, mx, my, delta_x, delta_y);
        }

        // Stamp the wheel time *after* dispatching so the idle
        // debounce in `run_one_frame` measures from the last wheel
        // event the runner actually processed. The
        // [`Self::run_one_frame`] callsite consumes this stamp via
        // an idle-timeout check that fires `tree.on_scroll_end()`
        // — see the field doc on `last_wheel_time_ms`.
        app.last_wheel_time_ms = Some(now_ms());
    }

    fn dispatch_key_down(
        app: &mut Self,
        key_code: u32,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        // Update the router so it can fire any blur / focus-change
        // logic that depends on key state. The return value is
        // (node, event_type) for a hit-tested target — we ignore it
        // because we broadcast instead, mirroring the desktop
        // runner. The router still tracks focus internally, which
        // is what `dispatch_text_input` relies on later for
        // "focused element" semantics.
        let _ = app.ctx.event_router.on_key_down(key_code);

        // Broadcast KEY_DOWN to every key handler in the tree.
        // Each widget checks its own focus state to decide whether
        // to act, mirroring `windowed.rs:3463-3473`. Without this
        // path the text-input widget never sees Backspace / Enter
        // / arrow keys etc., because the router-based dispatch
        // walks an event-bubble chain that doesn't reach handlers
        // registered at the focused node when the focus changed
        // mid-rebuild.
        if let Some(tree) = app.current_tree.as_mut() {
            tree.broadcast_key_event(
                blinc_core::events::event_types::KEY_DOWN,
                key_code,
                shift,
                ctrl,
                alt,
                meta,
            );
        }
    }

    fn dispatch_key_up(
        app: &mut Self,
        key_code: u32,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        let _ = app.ctx.event_router.on_key_up(key_code);
        if let Some(tree) = app.current_tree.as_mut() {
            tree.broadcast_key_event(
                blinc_core::events::event_types::KEY_UP,
                key_code,
                shift,
                ctrl,
                alt,
                meta,
            );
        }
    }

    fn dispatch_text_input(
        app: &mut Self,
        ch: char,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        // Broadcast TEXT_INPUT through `RenderTree::broadcast_text_input_event`
        // — the only path that actually populates `EventContext::key_char`,
        // which the text_input / text_area / code_editor / rich_text_editor
        // widgets all read inside their `on_event(TEXT_INPUT, …)` handlers.
        // The previous web runner used `tree.dispatch_event(...)` which
        // sends the event but leaves `key_char` as `None`, so widgets saw
        // the event fire but then bailed out without inserting anything
        // (the typing path is "if let Some(c) = ctx.key_char { d.insert(c) }").
        if let Some(tree) = app.current_tree.as_mut() {
            tree.broadcast_text_input_event(ch, shift, ctrl, alt, meta);
        }
    }

    /// Handle a right-click on the canvas. Shows a custom context
    /// menu with Cut / Copy / Paste / Select All when the click
    /// lands on a focused editable widget; bails silently otherwise.
    ///
    /// `canvas_x` / `canvas_y` are the click position in canvas-local
    /// CSS pixels (used for hit-testing). `page_x` / `page_y` are the
    /// click position in viewport-relative CSS pixels (used to
    /// position the overlay, which lives under `document.body`).
    ///
    /// The menu items each invoke `synthesize_edit_action`, which
    /// dispatches the corresponding Cmd+key chord into the focused
    /// widget. Cut / Copy then route through the widget's existing
    /// Cmd+X / Cmd+C handlers, which call `clipboard_write` (now
    /// implemented for wasm via `navigator.clipboard.writeText`).
    /// Paste calls `navigator.clipboard.readText()` directly and
    /// dispatches the resolved text via `broadcast_text_input_event`,
    /// bypassing the widget's Cmd+V → `clipboard_read` path entirely
    /// (the browser clipboard read API is async-only and can't
    /// satisfy the widget's sync `Option<String>` contract).
    fn handle_context_menu(app: &mut Self, canvas_x: f32, canvas_y: f32, page_x: f32, page_y: f32) {
        // Two consumers compete for the right-click on a Blinc canvas
        // page: (a) focused editable widgets that want Cut / Copy /
        // Paste, and (b) host apps that wire `Div::on_right_click`
        // (node-editor's context menu, canvas-kit demos that paint
        // their own surface, etc.).
        //
        // When an editable widget owns the focus we keep the
        // historical edit-menu behaviour. Otherwise we synthesise a
        // right-button POINTER_DOWN through the event router so the
        // layout's `Div::on_right_click` handlers fire. Without this
        // fan-out, the canvas's mousedown listener (which filters out
        // button != 0) swallowed every right-click that didn't land
        // on a focused input — leaving any host-installed right-click
        // surface (e.g. cn::context_menu opened from a node-editor's
        // on_right_click callback) silently inert on web. Desktop has
        // no equivalent intercept; the gesture flows straight into
        // the event router there.
        let focused = blinc_layout::widgets::text_input::focused_editable_node_id();
        if focused.is_none() {
            Self::dispatch_mouse_down(app, canvas_x, canvas_y, blinc_platform::MouseButton::Right);
            // Pair the DOWN with an UP at the same point so handlers
            // that key off press+release (and the event router's
            // pressed-target reset) don't strand the canvas in a
            // half-pressed state. The host's on_right_click closure
            // already fires on POINTER_DOWN via the layout filter, so
            // a missing UP wouldn't bury the menu — but the router's
            // book-keeping would stay dirty.
            Self::dispatch_mouse_up(app, canvas_x, canvas_y, blinc_platform::MouseButton::Right);
            let _ = (page_x, page_y);
            return;
        }

        // Update the router's mouse position so subsequent
        // synthetic key events that read mouse coords get the
        // right values for hit testing the focused node's bounds.
        if let Some(tree) = app.current_tree.as_ref() {
            let _ = app.ctx.event_router.on_mouse_move(tree, canvas_x, canvas_y);
        }

        // Build the overlay. We construct it directly via web_sys
        // rather than going through Blinc's element registry
        // because (a) the menu has to layer above the canvas at
        // arbitrary viewport coordinates which Blinc's layout
        // engine isn't built for, and (b) the menu needs real
        // DOM event listeners that can call back into Rust to
        // dispatch the chosen action — element-tree handlers
        // don't see DOM-level click events at all.
        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };

        // Tear down any previously open Blinc context menu so a
        // second right-click doesn't stack a new menu on top of
        // the old one. We tag the menu with a stable id so we can
        // find it again.
        const MENU_ID: &str = "blinc-context-menu";
        if let Some(existing) = document.get_element_by_id(MENU_ID) {
            existing.remove();
        }

        let Ok(menu) = document.create_element("div") else {
            return;
        };
        let _ = menu.set_attribute("id", MENU_ID);
        // Inline styles instead of a stylesheet because we want
        // the menu to work without the host page providing any
        // CSS. The colors mirror the dark Catppuccin-ish palette
        // the rest of `web_mobile_demo` uses so the menu doesn't
        // look out of place.
        let style = format!(
            "position:fixed;left:{}px;top:{}px;\
             background:#1e1e2e;color:#cdd6f4;\
             border:1px solid #45475a;border-radius:8px;\
             padding:4px 0;\
             font:13px -apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;\
             box-shadow:0 8px 24px rgba(0,0,0,0.4);\
             z-index:9999;\
             min-width:160px;",
            page_x, page_y,
        );
        let _ = menu.set_attribute("style", &style);

        // Build the four menu items. Each item registers a click
        // handler that calls `synthesize_edit_action` with the
        // corresponding action.
        for (label, action) in [
            ("Cut", EditAction::Cut),
            ("Copy", EditAction::Copy),
            ("Paste", EditAction::Paste),
            ("Select All", EditAction::SelectAll),
        ] {
            let Ok(item) = document.create_element("div") else {
                continue;
            };
            let _ =
                item.set_attribute("style", "padding:6px 16px;cursor:pointer;user-select:none;");
            item.set_text_content(Some(label));

            // Hover highlight via CSS pseudo-classes is awkward
            // for inline-styled elements; do it through JS
            // listeners instead.
            if let Some(html_item) = item.dyn_ref::<web_sys::HtmlElement>() {
                let html_item_clone = html_item.clone();
                let on_over = Closure::<dyn FnMut(_)>::new(move |_evt: web_sys::MouseEvent| {
                    let _ = html_item_clone
                        .style()
                        .set_property("background", "#313244");
                });
                let _ = html_item.add_event_listener_with_callback(
                    "mouseover",
                    on_over.as_ref().unchecked_ref(),
                );
                on_over.forget();

                let html_item_clone = html_item.clone();
                let on_out = Closure::<dyn FnMut(_)>::new(move |_evt: web_sys::MouseEvent| {
                    let _ = html_item_clone.style().set_property("background", "");
                });
                let _ = html_item
                    .add_event_listener_with_callback("mouseout", on_out.as_ref().unchecked_ref());
                on_out.forget();
            }

            // Action handler dispatches the chosen edit action and
            // removes the menu. We listen for `mousedown` (not
            // `click`) and call `stop_propagation()` so the
            // document-level dismiss listener — which also fires
            // on mousedown — never sees this event. Listening for
            // `click` would race with the document mousedown:
            // mousedown bubbles to document → dismiss handler
            // removes the menu → mouseup → click never fires
            // because the element is gone. We can't capture
            // `&mut WebApp` directly here because the closure
            // outlives this stack frame; route through
            // `synthesize_edit_action`, which queues the action
            // into a global slot that the next frame drains.
            let on_action = Closure::<dyn FnMut(_)>::new(move |evt: web_sys::MouseEvent| {
                evt.stop_propagation();
                evt.prevent_default();
                synthesize_edit_action(action);
                if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                    if let Some(menu) = doc.get_element_by_id(MENU_ID) {
                        menu.remove();
                    }
                }
            });
            let _ = item
                .add_event_listener_with_callback("mousedown", on_action.as_ref().unchecked_ref());
            on_action.forget();

            let _ = menu.append_child(&item);
        }

        // Close the menu on any outside click or scroll. We
        // attach a one-shot mousedown listener on `document` that
        // removes the menu and unregisters itself.
        let dismiss = Closure::<dyn FnMut(_)>::new(move |_evt: web_sys::MouseEvent| {
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                if let Some(menu) = doc.get_element_by_id(MENU_ID) {
                    menu.remove();
                }
            }
        });
        // Schedule listener attachment for the next tick so the
        // current right-click event doesn't immediately trigger
        // the dismiss handler.
        let document_clone = document.clone();
        let dismiss_attach = Closure::<dyn FnMut()>::new(move || {
            let _ = document_clone.add_event_listener_with_callback_and_add_event_listener_options(
                "mousedown",
                dismiss.as_ref().unchecked_ref(),
                web_sys::AddEventListenerOptions::new().once(true),
            );
        });
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            dismiss_attach.as_ref().unchecked_ref(),
            0,
        );
        dismiss_attach.forget();

        if let Some(body) = document.body() {
            let _ = body.append_child(&menu);
        }
    }

    /// Dispatch a batch of pending events through the cached render
    /// tree. Mouse handlers all use this — the EventRouter returns a
    /// list of (node, event_type) pairs that the tree's handler
    /// registry needs to walk through individually.
    ///
    /// Each event is forwarded via [`RenderTree::dispatch_event_full`]
    /// (not the simpler `dispatch_event`) so the runner can populate
    /// the per-event auxiliary fields the EventContext needs:
    ///
    ///   - **`drag_delta_x` / `drag_delta_y`** — read from
    ///     `EventRouter::drag_delta()`. The router accumulates these
    ///     between mousedown and mouseup; without forwarding them
    ///     here, every `on_drag` handler receives `(0, 0)` and the
    ///     dragged element never moves. This was the silent
    ///     `web_drag` bug — the chain reached the handler, just
    ///     with empty deltas.
    ///   - **`bounds_x/y/w/h`** + **`local_x/y`** — looked up via
    ///     `EventRouter::get_node_bounds(node)` so the handler can
    ///     reason about element-local coordinates (e.g. the
    ///     sortable demo's `e.local_y` to figure out which list
    ///     item the cursor hit).
    ///
    /// Mirrors the desktop runner's per-event population pass at
    /// [`windowed.rs:2864-2882`](crate::windowed) (which writes
    /// the same fields onto a `PendingEvent` struct before passing
    /// it to `dispatch_event_full`).
    fn dispatch_pending(app: &mut Self, pending: Vec<(blinc_layout::tree::LayoutNodeId, u32)>) {
        if pending.is_empty() {
            return;
        }
        let (mx, my) = app.ctx.event_router.mouse_position();
        let (drag_dx, drag_dy) = app.ctx.event_router.drag_delta();
        // Snapshot cached modifier state. Browser pointer events
        // don't propagate modifier flags down through `dispatch_pending`
        // automatically; the caller stamps `cached_modifiers` from the
        // originating DOM event (`mousedown` / `mousemove` / `wheel`)
        // before invoking us, so every PointerEvent that goes through
        // here carries the modifiers held *at the time of the event*.
        // Mirrors the desktop runner's `cached_modifiers_for_dispatch`
        // snapshot at [`windowed.rs:4780`](crate::windowed).
        let mods = app.cached_modifiers;
        // Snapshot per-node bounds before borrowing the tree mutably
        // — `get_node_bounds` lives on `EventRouter`, and we'd
        // otherwise have a `&self.ctx` + `&mut self.current_tree`
        // borrow conflict.
        struct DispatchEntry {
            node: blinc_layout::tree::LayoutNodeId,
            event_type: u32,
            bounds: (f32, f32, f32, f32),
        }
        let entries: Vec<DispatchEntry> = pending
            .iter()
            .map(|&(node, event_type)| DispatchEntry {
                node,
                event_type,
                bounds: app
                    .ctx
                    .event_router
                    .get_node_bounds(node)
                    .unwrap_or((0.0, 0.0, 0.0, 0.0)),
            })
            .collect();

        if let Some(tree) = app.current_tree.as_mut() {
            for entry in entries {
                let DispatchEntry {
                    node,
                    event_type,
                    bounds: (bx, by, bw, bh),
                } = entry;
                let local_x = mx - bx;
                let local_y = my - by;
                // Drag deltas only meaningful for DRAG / DRAG_END;
                // for everything else they're (0, 0) by virtue of
                // the router resetting them on mouseup.
                let (dx, dy) = if event_type == blinc_core::events::event_types::DRAG
                    || event_type == blinc_core::events::event_types::DRAG_END
                {
                    (drag_dx, drag_dy)
                } else {
                    (0.0, 0.0)
                };
                tree.dispatch_event_full(
                    node, event_type, mx, my, local_x, local_y, bx, by, bw, bh, dx, dy,
                    /* pinch_scale */ 1.0, mods.shift, mods.ctrl, mods.alt, mods.meta,
                );
            }
        }
    }

    /// Re-read the canvas's CSS dimensions and `devicePixelRatio`,
    /// resize the GPU framebuffer + surface configuration to match,
    /// update the [`WindowedContext`] dimensions, and mark the tree
    /// for rebuild on the next frame.
    ///
    /// Called from the `resize` event handler installed by
    /// [`Self::install_input_listeners`]. Skips work entirely when
    /// the logical dimensions haven't actually changed since the last
    /// resize — `window.resize` fires for things like browser-tab
    /// activation and devtools-toggle that don't actually change the
    /// canvas size.
    ///
    /// Zero-size guards: a 0×0 canvas (which can happen during
    /// fullscreen transitions or before CSS layout settles) would
    /// produce a wgpu validation error from `surface.configure(...)`.
    /// We bail early in that case and wait for a real resize event
    /// to arrive.
    fn handle_resize(app: &mut Self) {
        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };

        // Respect data-width/data-height/data-dpr overrides — when
        // these are set, the canvas viewport is locked and resize
        // events are ignored (the fixed dimensions take precedence
        // over the browser's CSS layout).
        if app.canvas.get_attribute("data-width").is_some()
            || app.canvas.get_attribute("data-height").is_some()
        {
            return;
        }

        let logical_width = app.canvas.client_width() as f32;
        let logical_height = app.canvas.client_height() as f32;
        if logical_width <= 0.0 || logical_height <= 0.0 {
            return;
        }
        let scale_factor = app
            .canvas
            .get_attribute("data-dpr")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or_else(|| window.device_pixel_ratio());

        // Skip if nothing actually changed. `window.resize` fires for
        // many non-resize events (devtools toggle, focus changes…).
        let (last_w, last_h) = app.last_logical_size;
        let last_dpr = app.ctx.scale_factor;
        if (last_w - logical_width).abs() < 0.5
            && (last_h - logical_height).abs() < 0.5
            && (last_dpr - scale_factor).abs() < 0.001
        {
            return;
        }

        let physical_width = (logical_width * scale_factor as f32).round().max(1.0);
        let physical_height = (logical_height * scale_factor as f32).round().max(1.0);

        // Resize the canvas's GPU framebuffer to match the new
        // physical dimensions. The CSS size is what the browser
        // already laid out for us; we have to push the matching
        // pixel size into the canvas's `width`/`height` attributes
        // before reconfiguring the wgpu surface.
        app.canvas.set_width(physical_width as u32);
        app.canvas.set_height(physical_height as u32);

        // Update surface config and re-configure. wgpu requires a
        // configure call any time the size changes, otherwise
        // `get_current_texture` returns `Outdated` on the next frame.
        app.surface_config.width = physical_width as u32;
        app.surface_config.height = physical_height as u32;
        app.surface
            .configure(app.blinc_app.device(), &app.surface_config);

        // Update WindowedContext dimensions so the user's UI builder
        // sees the new logical size on the next rebuild. The renderer
        // also reads `tree.scale_factor()` which we set per-frame in
        // `run_one_frame`, so changing the DPR mid-resize is handled
        // automatically by the next rebuild.
        app.ctx.width = logical_width;
        app.ctx.height = logical_height;
        app.ctx.scale_factor = scale_factor;
        app.ctx.physical_width = physical_width;
        app.ctx.physical_height = physical_height;
        app.last_logical_size = (logical_width, logical_height);

        // Force a rebuild so the layout pass uses the new viewport
        // dimensions on the next rAF tick. `needs_full_rebuild`
        // bypasses the `incremental_update` path because viewport-
        // size changes don't propagate parent constraints cleanly
        // through it — desktop does the same at
        // [`windowed.rs:3684`](crate::windowed).
        app.needs_rebuild = true;
        app.needs_full_rebuild = true;

        // Drop every cached render texture so the next paint
        // re-rasterises against the new physical surface. Mirrors
        // windowed.rs:3647 — without this, any motion-subtree or
        // composite-layer texture baked at the old size keeps
        // blitting at its old extent and the post-resize frame
        // shows a window-shaped artefact where the cache lives.
        app.blinc_app
            .invalidate_render_cache_tagged("window_resized");
        // And the layer textures themselves — their content-space
        // sizing was computed against the previous physical width
        // and won't match the new one.
        app.blinc_app.drain_all_layer_textures("window_resized");
    }

    /// Install (or replace) the UI builder closure.
    ///
    /// Sets `needs_rebuild = true` so the next [`Self::run_one_frame`]
    /// call rebuilds the tree from the new builder.
    pub fn set_ui_builder<F, E>(&mut self, builder: F)
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: blinc_layout::ElementBuilder + 'static,
    {
        self.ui_builder = Some(Box::new(builder));
        self.needs_rebuild = true;
    }

    /// Mark the tree as dirty so the next frame rebuilds it.
    pub fn request_rebuild(&mut self) {
        self.needs_rebuild = true;
    }

    /// Render exactly one frame: rebuild the tree if dirty, acquire
    /// the next surface texture, render through `BlincApp`, and
    /// present.
    ///
    /// Called from the scheduler's wake callback (driven by rAF) and
    /// once synchronously from [`Self::run`] to avoid a blank-canvas
    /// gap between init and the first rAF tick.
    ///
    /// Errors here do NOT abort the loop — the scheduler will call
    /// us again on the next tick. Phase 3d's input handlers will
    /// also call this directly to force a render after a click /
    /// keypress.
    pub fn run_one_frame(&mut self) -> Result<()> {
        let now = now_ms();

        // ─── Phase 0: clear per-frame state ──────────────────────
        self.render_state.clear_overlays();

        // Drain any custom passes queued via BlincContextState.
        // On wasm32, SceneKit3D wraps passes in WasmPassWrapper
        // (a Send shim) — we need to unwrap before registering.
        // The wrapper is a private type, so we use a known-layout
        // transmute: WasmPassWrapper is repr(Rust) with a single
        // Box<dyn CustomRenderPass> field.
        //
        // On native this path also exists but uses the direct
        // Box<dyn CustomRenderPass> downcast (no wrapper needed).
        {
            let ctx_state = blinc_core::BlincContextState::get();
            for pass in ctx_state.drain_custom_passes() {
                if let Ok(typed) =
                    pass.downcast::<Box<dyn blinc_gpu::custom_pass::CustomRenderPass>>()
                {
                    self.blinc_app.context().register_custom_pass(*typed);
                }
            }
        }

        // ─── Phase 0b: drain accumulated wheel delta ─────────────
        let (pending_dx, pending_dy) = self.pending_wheel_delta;
        self.pending_wheel_delta = (0.0, 0.0);
        let had_scroll = pending_dx != 0.0 || pending_dy != 0.0;
        if had_scroll {
            // Pass small (trackpad-sized) deltas straight through so
            // trackpad scrolling feels native, and only compress the
            // tail above a threshold so discrete mouse-wheel ticks
            // don't blast the viewport. Previous behaviour applied a
            // 0.7 exponent to ALL deltas, which made trackpad scroll
            // feel noticeably dampier than the desktop runner.
            const PASSTHROUGH_THRESHOLD: f32 = 40.0;
            const TAIL_EXPONENT: f32 = 0.85;
            let damp = |d: f32| -> f32 {
                let mag = d.abs();
                if mag <= PASSTHROUGH_THRESHOLD {
                    d
                } else {
                    let tail = (mag - PASSTHROUGH_THRESHOLD).powf(TAIL_EXPONENT);
                    d.signum() * (PASSTHROUGH_THRESHOLD + tail)
                }
            };
            Self::dispatch_scroll(self, damp(pending_dx), damp(pending_dy));
            // Mirror windowed.rs:5099 — scroll updates `tree.scroll_offsets`
            // and (when the scroll reaches a canvas-kit kit) the
            // viewport pan/zoom matrix. Neither routes through the
            // compositor's natural invalidation paths, so any
            // motion-subtree / composite-layer texture baked before
            // the scroll keeps blitting at pre-scroll coords. On web
            // this manifested as SVG icons baked inside an overlay's
            // composite scratch staying at their old content-space
            // positions when the user zoomed the canvas after closing
            // the overlay.
            //
            // BURST-EDGE GATING. Running invalidate_render_cache_tagged
            // + drain_all_layer_textures every wheel-bearing rAF
            // released composite / motion-subtree textures faster
            // than the next paint could re-bake them, producing
            // visible jank that scaled with overlay count (HUD
            // composited texture in node_editor_demo + any
            // motion containers near the viewport). The work
            // belongs at burst edges only:
            //   * first wheel after a gap > WHEEL_BURST_GAP_MS:
            //     wipe + drain so stale-baked-position textures
            //     don't persist into the new gesture.
            //   * mid-burst frames: skip — content baked from
            //     this burst's first frame is already at correct
            //     content-space coords; invalidating it every
            //     frame just churns.
            //   * burst end (wheel-end debounce in Phase 1a below):
            //     not strictly necessary because the next gesture
            //     will re-fire the burst-start path, but cheap.
            const WHEEL_BURST_GAP_MS: u64 = 100;
            let is_burst_start = match self.last_scroll_invalidate_ms {
                None => true,
                Some(t) => now.saturating_sub(t) > WHEEL_BURST_GAP_MS,
            };
            if is_burst_start {
                self.blinc_app
                    .invalidate_render_cache_tagged("wheel_burst_start");
                self.blinc_app.drain_all_layer_textures("wheel_burst_start");
                self.last_scroll_invalidate_ms = Some(now);
            }
        }

        // ─── Phase 1a: scroll physics + pending refs ─────────────
        if let Some(ref mut tree) = self.current_tree {
            tree.tick_scroll_physics(now);
            tree.process_pending_scroll_refs();

            // Wheel-end debounce (no ScrollPhase::Ended on DOM).
            if let Some(last) = self.last_wheel_time_ms {
                let elapsed = now.saturating_sub(last);
                let overscrolling = tree.has_overscrolling_scroll();
                let debounce_ms = if overscrolling { 32 } else { 120 };
                if elapsed >= debounce_ms {
                    tree.on_scroll_end();
                    self.last_wheel_time_ms = None;
                }
            }

            drain_pending_edit_action(tree);
            drain_pending_paste_text(tree);
        }

        // ─── Phase 1b: motion system (pre-overlay) ───────────────
        self.render_state.process_global_motion_exit_starts();
        self.render_state.process_global_motion_exit_cancels();
        self.render_state.process_global_motion_starts();
        self.render_state.sync_shared_motion_states();

        // ─── Phase 1c: overlay manager update ────────────────────
        self.ctx.overlay_manager.set_viewport_with_scale(
            self.ctx.width,
            self.ctx.height,
            self.ctx.scale_factor as f32,
        );
        self.ctx.overlay_manager.update(now);
        // Tick the new OverlayStack + ToastTray alongside the legacy
        // manager. Each is the authoritative manager for the widgets
        // that have already been migrated to it. Mirrors
        // `windowed.rs:4937-4949`.
        {
            use blinc_layout::overlay_state::{overlay_stack, toast_tray};
            if let Ok(mut s) = overlay_stack().lock() {
                s.set_viewport_with_scale(
                    self.ctx.width,
                    self.ctx.height,
                    self.ctx.scale_factor as f32,
                );
                s.update(now);
            }
            if let Ok(mut t) = toast_tray().lock() {
                t.update(now);
            }
        }

        if self.ctx.overlay_manager.is_dirty() {
            let registry = self.ctx.element_registry().clone();
            if let Some(overlay_node_id) =
                registry.get(blinc_layout::widgets::overlay::OVERLAY_LAYER_ID)
            {
                let overlay_content = self.ctx.overlay_manager.build_overlay_layer();
                blinc_layout::queue_subtree_rebuild(overlay_node_id, overlay_content);
            }
            self.ctx.overlay_manager.take_dirty();
        }
        // Mirror the dirty-check + subtree rebuild for the new
        // OverlayStack + ToastTray (windowed.rs:4981-5026).
        {
            use blinc_layout::overlay_state::{
                overlay_stack, rebuild_overlay_subtree_if_dirty, toast_tray,
            };
            use blinc_layout::widgets::overlay_stack::OVERLAY_STACK_LAYER_ID;
            use blinc_layout::widgets::toast_tray::TOAST_TRAY_LAYER_ID;

            let registry = self.ctx.element_registry().clone();

            rebuild_overlay_subtree_if_dirty(
                &registry,
                OVERLAY_STACK_LAYER_ID,
                overlay_stack()
                    .lock()
                    .map(|s| s.take_dirty())
                    .unwrap_or(false),
                || {
                    overlay_stack()
                        .lock()
                        .ok()
                        .map(|s| s.build_overlay_layer())
                        .unwrap_or_default()
                },
            );

            let viewport = (self.ctx.width, self.ctx.height);
            rebuild_overlay_subtree_if_dirty(
                &registry,
                TOAST_TRAY_LAYER_ID,
                toast_tray().lock().map(|t| t.take_dirty()).unwrap_or(false),
                || {
                    toast_tray()
                        .lock()
                        .ok()
                        .map(|t| t.build_tray_layer(viewport))
                        .unwrap_or_default()
                },
            );
        }

        // ─── Phase 1d: rebuild triggers ──────────────────────────
        if let Some(ref tree) = self.current_tree {
            if tree.needs_rebuild() {
                self.needs_rebuild = true;
            }
        }
        if blinc_layout::widgets::take_needs_rebuild() {
            self.needs_rebuild = true;
        }
        if self
            .ctx
            .dirty_flag()
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            self.needs_rebuild = true;
        }
        if blinc_layout::widgets::take_needs_relayout() {
            self.needs_full_rebuild = true;
        }
        if blinc_layout::widgets::take_needs_css_reparse() {
            self.ctx.reparse_css();
        }

        // ─── Phase 2: drain stateful prop/subtree updates ────────
        let has_stateful_updates = blinc_layout::take_needs_redraw();
        let has_pending_rebuilds = blinc_layout::has_pending_subtree_rebuilds();
        let has_prop_updates = blinc_layout::has_pending_partial_prop_updates();
        if has_stateful_updates || has_pending_rebuilds || has_prop_updates {
            // Drain the unified property channel
            // ([[project-reactive-architecture-v2]]). Accumulate side
            // effects so a layout-affecting update forces relayout even
            // when no subtree rebuild was queued.
            let prop_updates = blinc_layout::take_pending_partial_prop_updates();
            let mut prop_effects = blinc_layout::SideEffects::default();
            if let Some(ref mut tree) = self.current_tree {
                for upd in prop_updates {
                    prop_effects = prop_effects.or(upd.effects);
                    if let Some(write) = upd.render_write {
                        tree.update_render_props(upd.node_id, |p| write(p));
                    }
                    if let Some(write) = upd.layout_write {
                        if let Some(mut style) = tree.layout_tree.get_style(upd.node_id) {
                            write(&mut style);
                            tree.layout_tree.set_style(upd.node_id, style);
                        }
                    }
                }
            }
            let mut needs_relayout = prop_effects.needs_layout;
            if let Some(ref mut tree) = self.current_tree {
                needs_relayout |= tree.process_pending_subtree_rebuilds();
            }
            if needs_relayout {
                // Mirror windowed.rs:5088 — a structural rebuild
                // produces fresh element ids / positions; the batch
                // caches keyed against the old tree must be dropped
                // so the next paint repopulates from the new state.
                self.blinc_app
                    .invalidate_render_cache_tagged("subtree_rebuild_applied");
                // NOTE: we DO NOT drain `css_composited_textures` /
                // `motion_subtree_textures` here. A subtree rebuild
                // fired when a child overlay (submenu, context menu)
                // mounts — draining at that point wipes the surviving
                // parent overlay's CSS-promoted texture and the next
                // bake captures frame-1 of the still-running enter
                // animation (opacity ≈ 0), then the per-key hash-skip
                // refuses to re-bake while the closure emits the
                // same primitives. Result: parent / freshly-mounted
                // overlay frozen near-invisible until a mouse-move
                // trips a different invalidation. See
                // `gotcha-css-anim-eager-drain` for the same shape on
                // desktop (resolved via `sweep_stale_css_animations`
                // — which runs as part of the rebuild walker and
                // handles texture-side invalidation surgically).
                if let Some(ref mut tree) = self.current_tree {
                    // Mirror windowed.rs:3660-3676: after subtree
                    // rebuilds, the new children need CSS layout
                    // overrides (padding, gap, etc.) applied before
                    // compute_layout, and all the post-layout wiring
                    // (FLIP, motions, CSS animations) for the new
                    // nodes. Without this, new subtree nodes render
                    // with default layout — the visible symptom is
                    // CSS styling "disappearing" after the first
                    // stateful interaction (accordion open, checkbox
                    // toggle, etc.).
                    tree.apply_stylesheet_layout_overrides();
                    tree.compute_layout(self.ctx.width, self.ctx.height);
                    tree.apply_flip_transitions();
                    tree.update_flip_bounds();
                    tree.initialize_motion_animations(&mut self.render_state);
                    self.render_state.end_stable_motion_frame();
                    self.render_state.process_global_motion_replays();
                    tree.start_all_css_animations();
                }
            }
        }

        // ─── Phase 3: tree rebuild / incremental update ──────────
        if self.needs_rebuild {
            let builder = match self.ui_builder.as_mut() {
                Some(b) => b,
                None => return Ok(()),
            };

            // Drain any CSS queued via `ThemeBundle::with_css` →
            // `ThemeState::init`'s `queue_pending_stylesheet`, or
            // `BlincContextState::queue_stylesheet` from DSL/plugin
            // code that doesn't see `WindowedContext` directly. The
            // queue is process-global; we run the drain on the
            // rebuild path same as `windowed.rs:5228-5234` so
            // `cn_bundle()` and friends light up identically on web.
            {
                let queued = blinc_core::BlincContextState::get().drain_stylesheets();
                for css in queued {
                    self.ctx.add_css(&css);
                }
            }

            blinc_layout::reset_call_counters();
            blinc_layout::clear_stateful_base_updaters();
            blinc_layout::click_outside::clear_click_outside_handlers();

            if self.needs_full_rebuild {
                self.current_tree = None;
                self.needs_full_rebuild = false;
            }

            if let Some(ref mut existing_tree) = self.current_tree {
                use blinc_layout::UpdateResult;
                match builder.build_and_update(&mut self.ctx, existing_tree) {
                    UpdateResult::NoChanges | UpdateResult::VisualOnly => {}
                    UpdateResult::LayoutChanged | UpdateResult::ChildrenChanged => {
                        // Post-children-changed wiring (mirrors windowed.rs:3812-3834)
                        existing_tree.apply_stylesheet_base_styles();
                        existing_tree.apply_stylesheet_layout_overrides();
                        existing_tree.compute_layout(self.ctx.width, self.ctx.height);
                        existing_tree.apply_flip_transitions();
                        existing_tree.update_flip_bounds();
                        if let Some(ref stylesheet) = self.ctx.stylesheet {
                            self.ctx.pointer_query.register_from_stylesheet(stylesheet);
                        }
                        existing_tree.initialize_motion_animations(&mut self.render_state);
                        self.render_state.end_stable_motion_frame();
                        self.render_state.process_global_motion_replays();
                        existing_tree.start_all_css_animations();
                    }
                }
                existing_tree.clear_dirty();
            } else {
                // First-frame build
                let registry = Arc::clone(self.ctx.element_registry());
                let mut tree = builder.build_from_scratch(&mut self.ctx, registry);

                tree.set_animations(&self.ctx.animations);
                tree.set_scale_factor(self.ctx.scale_factor as f32);
                tree.set_css_anim_store(Arc::clone(&self.css_anim_store));

                if let Some(ref stylesheet) = self.ctx.stylesheet {
                    tree.set_stylesheet_arc(stylesheet.clone());
                }
                tree.apply_all_stylesheet_styles();
                if let Some(ref stylesheet) = self.ctx.stylesheet {
                    self.ctx.pointer_query.register_from_stylesheet(stylesheet);
                }

                tree.compute_layout(self.ctx.width, self.ctx.height);
                tree.update_flip_bounds();
                tree.initialize_motion_animations(&mut self.render_state);
                self.render_state.end_stable_motion_frame();
                self.render_state.process_global_motion_replays();
                tree.start_all_css_animations();

                self.current_tree = Some(tree);
            }

            self.needs_rebuild = false;
            self.ctx.rebuild_count = self.ctx.rebuild_count.saturating_add(1);
        }

        // ─── Phase 4: animation tick ─────────────────────────────
        self.render_state.process_global_motion_exit_cancels();
        self.render_state.process_global_motion_exit_starts();
        self.render_state.process_global_motion_starts();
        let _animations_active = self.render_state.tick(now);

        let dt_ms = if self.last_frame_time_ms > 0 {
            now.saturating_sub(self.last_frame_time_ms) as f32
        } else {
            16.0
        };
        let css_active = if let Some(ref mut tree) = self.current_tree {
            let store = tree.css_anim_store();
            let (anim, trans) = store.lock().unwrap().tick(dt_ms);
            let flip = tree.tick_flip_animations(dt_ms);
            anim || trans || flip || tree.css_has_active()
        } else {
            false
        };
        self.last_frame_time_ms = now;

        self.render_state.sync_shared_motion_states();
        let _theme_animating = blinc_theme::ThemeState::get().tick();

        // ─── Phase 4b: animation-driven rebuilds ─────────────────
        // When the scheduler has active animations (springs,
        // keyframes, timelines) it sets a `needs_redraw` flag each
        // tick. Stateful elements that hold AnimatedValue /
        // AnimatedTimeline handles need their `on_state` callbacks
        // re-invoked so the new interpolated values make it into
        // the render props. `check_stateful_animations()` does
        // exactly that — it iterates every registered stateful
        // element, checks if its springs/timelines are still
        // active, and re-runs the callback, pushing the result into
        // the pending-prop-update and pending-subtree-rebuild
        // queues. Without this call, timeline animations appear
        // frozen — the scheduler ticks the timeline internally but
        // nothing reads the new value because `build_ui` isn't
        // re-invoked. Desktop does this at windowed.rs:4084.
        {
            let needs_animation_redraw = self.ctx.animations.lock().unwrap().take_needs_redraw();
            if needs_animation_redraw && blinc_layout::has_animating_statefuls() {
                blinc_layout::check_stateful_animations();
            }
        }

        // Drain any prop/subtree updates produced by
        // `check_stateful_animations` above — they need to land
        // on the current tree before we render this frame.
        {
            // Unified property channel drain — same shape as the
            // pre-paint pass above. Accumulate side effects so a
            // layout-affecting animation-driven update forces relayout.
            let prop_updates = blinc_layout::take_pending_partial_prop_updates();
            let mut animation_prop_effects = blinc_layout::SideEffects::default();
            if let Some(ref mut tree) = self.current_tree {
                for upd in prop_updates {
                    animation_prop_effects = animation_prop_effects.or(upd.effects);
                    if let Some(write) = upd.render_write {
                        tree.update_render_props(upd.node_id, |p| write(p));
                    }
                    if let Some(write) = upd.layout_write {
                        if let Some(mut style) = tree.layout_tree.get_style(upd.node_id) {
                            write(&mut style);
                            tree.layout_tree.set_style(upd.node_id, style);
                        }
                    }
                }
            }
            if blinc_layout::has_pending_subtree_rebuilds() || animation_prop_effects.needs_layout {
                let mut needs_relayout = animation_prop_effects.needs_layout;
                if let Some(ref mut tree) = self.current_tree {
                    needs_relayout |= tree.process_pending_subtree_rebuilds();
                }
                if needs_relayout {
                    // Animation-driven subtree rebuilds also need the
                    // batch-cache wipe — same rationale as the
                    // Phase-2 subtree-rebuild block above. Layer
                    // textures are intentionally NOT drained here
                    // for the same enter-animation-restart reason.
                    self.blinc_app
                        .invalidate_render_cache_tagged("animation_subtree_rebuild_applied");
                    if let Some(ref mut tree) = self.current_tree {
                        tree.compute_layout(self.ctx.width, self.ctx.height);
                    }
                }
            }
        }

        // ─── Phase 4c: drain deferred focus/blur ─────────────────
        // Mirror windowed.rs Phase-3 post-rebuild drain. Deferred
        // focus (`focus_text_input_deferred`, used by OTP auto-advance
        // and backspace-rewind) and deferred blur are queued during
        // event dispatch and applied here — after the rebuild so the
        // target's node_id is set, and before the focus sync below so
        // `:focus` selectors and the focused-node lookup see the
        // result this frame. Without this the web runner never advanced
        // OTP focus per keystroke and backspace only cleared the last
        // slots.
        blinc_layout::widgets::process_pending_input_focus();
        blinc_layout::widgets::process_pending_area_focus();

        // ─── Phase 5: pre-render CSS + pointer-query ─────────────
        // Focus sync so :focus selectors work
        {
            let text_focus = blinc_layout::widgets::text_input::focused_text_input_node_id()
                .or_else(blinc_layout::widgets::text_input::focused_text_area_node_id);
            let current_focus = self.ctx.event_router.focused();
            if text_focus != current_focus {
                self.ctx.event_router.set_focus(text_focus);
            }
        }

        // CSS state styles (:hover, :active, :focus)
        if let Some(ref mut tree) = self.current_tree {
            if tree.stylesheet().is_some() {
                let state_changed = tree.apply_stylesheet_state_styles(&self.ctx.event_router);
                if state_changed {
                    tree.compute_layout(self.ctx.width, self.ctx.height);
                    tree.update_flip_bounds();
                }
            }
        }

        // Apply animated CSS property values
        if css_active
            || !self
                .current_tree
                .as_ref()
                .is_none_or(|t| t.css_transitions_empty())
        {
            if let Some(ref mut tree) = self.current_tree {
                tree.apply_all_css_animation_props();
                tree.apply_all_css_transition_props();
                tree.apply_flip_animation_props();
                if tree.apply_animated_layout_props() {
                    tree.compute_layout(self.ctx.width, self.ctx.height);
                    tree.update_flip_bounds();
                }
            }
        }

        // Pointer query (calc(env(pointer-x)) etc.)
        if !self.ctx.pointer_query.is_empty() {
            let (mx, my) = self.ctx.event_router.mouse_position();
            let is_pressed = self.ctx.event_router.has_pressed_target();
            let dt_sec = dt_ms / 1000.0;
            let time_sec = now as f64 / 1000.0;
            let registry = Arc::clone(self.ctx.element_registry());
            let router = &self.ctx.event_router;
            let tree_for_hover = self.current_tree.as_ref();
            self.ctx
                .pointer_query
                .update(mx, my, is_pressed, dt_sec, time_sec, |id| {
                    let node = registry.get(id)?;
                    // is_hovered is stable-id-keyed now; use the
                    // layout-id shim that resolves via the tree.
                    let tree = tree_for_hover?;
                    if router.is_hovered_layout(tree, node) {
                        router.get_node_bounds(node)
                    } else {
                        None
                    }
                });
            if let Some(ref mut tree) = self.current_tree {
                tree.apply_pointer_styles(&self.ctx.pointer_query, &self.ctx.event_router);
            }
        }

        // ─── Phase 6: render ─────────────────────────────────────
        let tree = match self.current_tree.as_ref() {
            Some(t) => t,
            None => return Ok(()),
        };

        let frame = self
            .surface
            .get_current_texture()
            .map_err(|e| BlincError::Render(format!("get_current_texture failed: {e}")))?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let physical_w = self.surface_config.width;
        let physical_h = self.surface_config.height;

        let (mx, my) = self.ctx.event_router.mouse_position();
        let sf = self.ctx.scale_factor as f32;
        self.blinc_app.set_cursor_position(mx * sf, my * sf);
        self.render_state
            .set_viewport_size(self.ctx.width, self.ctx.height);

        // Set blend target for mix-blend-mode support. The blend
        // shader reads from the dest texture to composite — without
        // this, non-Normal blend modes (multiply, screen, overlay,
        // etc.) render as solid black. Desktop does this at
        // windowed.rs:4031.
        self.blinc_app.set_blend_target(&frame.texture);

        self.blinc_app.render_tree_with_motion(
            tree,
            &self.render_state,
            &view,
            physical_w,
            physical_h,
        )?;

        frame.present();

        // ─── Phase 7: post-render cleanup ────────────────────────
        let _content_dirty = self.ctx.overlay_manager.take_dirty();
        let _animation_dirty = self.ctx.overlay_manager.take_animation_dirty();
        self.ctx.had_visible_overlays = self.ctx.overlay_manager.has_visible_overlays();

        Ok(())
    }

    /// Borrow the canvas the runner is rendering into.
    pub fn canvas(&self) -> &web_sys::HtmlCanvasElement {
        &self.canvas
    }

    /// Borrow the underlying [`BlincApp`].
    pub fn blinc_app(&self) -> &BlincApp {
        &self.blinc_app
    }

    /// Borrow the [`WindowedContext`] the user's UI builder will
    /// receive on each rebuild.
    pub fn context(&self) -> &WindowedContext {
        &self.ctx
    }

    /// Mutable access to the [`WindowedContext`].
    pub fn context_mut(&mut self) -> &mut WindowedContext {
        &mut self.ctx
    }

    /// Borrow the wgpu surface.
    pub fn surface(&self) -> &wgpu::Surface<'static> {
        &self.surface
    }

    /// Borrow the surface configuration. Phase 3e will mutate this on
    /// resize and call `surface.configure(...)` again.
    pub fn surface_config(&self) -> &wgpu::SurfaceConfiguration {
        &self.surface_config
    }

    /// Borrow the shared animation scheduler.
    ///
    /// Use this to install a wake callback before calling
    /// [`Self::start_frame_loop`] — the scheduler invokes the wake
    /// callback on every tick where animations are active OR
    /// continuous redraw is requested. The wake callback is what
    /// actually renders a frame; the scheduler doesn't know about
    /// wgpu surfaces.
    pub fn scheduler(&self) -> &crate::windowed::SharedAnimationScheduler {
        &self.ctx.animations
    }

    /// Hand control of the per-frame loop over to
    /// [`AnimationScheduler::start_raf`].
    ///
    /// This is the wasm32 sibling of the desktop event-loop pump.
    /// `start_raf` installs a `requestAnimationFrame` chain that ticks
    /// the scheduler once per browser frame and invokes the wake
    /// callback whenever there's something to render. Returning from
    /// this method DOES NOT mean the loop is over — the rAF closure
    /// chain self-perpetuates from inside the browser. Returning just
    /// means "the loop is wired up; the runtime can drop the
    /// constructing future".
    ///
    /// Wire your wake callback via [`Self::scheduler`] *before*
    /// calling this — once `start_raf` returns, the chain is already
    /// firing.
    ///
    /// Most apps should use [`Self::run`] instead — it does setup,
    /// wake-callback wiring, and `start_raf` in one call.
    pub fn start_frame_loop(&self) {
        if let Ok(scheduler) = self.ctx.animations.lock() {
            scheduler.start_raf();
        }
    }
}
