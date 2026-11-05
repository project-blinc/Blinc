# Examples

The repo ships four runnable web examples under [`examples/`](https://github.com/project-blinc/Blinc/tree/main/examples). Each is a single-file `lib.rs` under 250 lines, with the same `index.html` + `serve.sh` scaffolding from the [Setup](./setup.md) chapter. They're the canonical reference for every pattern in this book — when in doubt, copy from one of these.

| Example | What it demonstrates | Lines |
|---------|---------------------|-------|
| [`web_hello`](https://github.com/project-blinc/Blinc/tree/main/examples/web_hello) | Minimum: canvas + bundled font + one Div | ~130 |
| [`web_scroll`](https://github.com/project-blinc/Blinc/tree/main/examples/web_scroll) | Wheel input → scroll widget → physics tick | ~200 |
| [`web_drag`](https://github.com/project-blinc/Blinc/tree/main/examples/web_drag) | Mouse drag → Stateful + State::set → visual update | ~245 |
| [`web_assets`](https://github.com/project-blinc/Blinc/tree/main/examples/web_assets) | Fetch fonts at runtime via `WebAssetLoader` | ~135 |

## `web_hello` — the smallest possible app

The "Hello, WebGPU!" canonical example. Centered text on a dark background, no input handlers, no animations, no state. If this draws on a real canvas in a real browser, the entire wgpu / wasm-bindgen / requestAnimationFrame / `WebApp::run` pipeline is alive end-to-end.

```rust
use blinc::app::web::WebApp;
use blinc::layout::div::{div, Div};
use blinc::layout::text::text;
use blinc::core::Color;
use wasm_bindgen::prelude::*;

const ARIAL: &[u8] = include_bytes!("../fonts/Arial.ttf");

#[wasm_bindgen(start)]
pub fn _start() {
    console_error_panic_hook::set_once();

    wasm_bindgen_futures::spawn_local(async {
        WebApp::run_with_setup(
            "blinc-canvas",
            |app| { app.load_font_data(ARIAL.to_vec()); },
            build_ui,
        ).await.unwrap();
    });
}

fn build_ui(_ctx: &mut blinc::app::windowed::WindowedContext) -> Div {
    div()
        .w_full().h_full()
        .bg(Color::rgba(0.07, 0.07, 0.10, 1.0))
        .items_center().justify_center()
        .child(
            text("Hello, WebGPU!")
                .size(32.0)
                .color(Color::WHITE),
        )
}
```

Read the full example: [`examples/web_hello/src/lib.rs`](https://github.com/project-blinc/Blinc/blob/main/examples/web_hello/src/lib.rs).

## `web_scroll` — wheel input + scroll physics

A vertical list of 24 cards inside a `scroll()` container. Demonstrates:

- **Wheel input** routes through `EventRouter::on_scroll_nested` and is dispatched via `RenderTree::dispatch_scroll_chain` (which walks the chain of scroll containers from leaf to root for nested-scroll consumption)
- **The scroll widget's per-frame physics tick** (`tree.tick_scroll_physics(now_ms)`) runs every rAF frame, advancing any active deceleration
- **Click events** also fire on cards via `on_click` handlers
- **The no-bounce default** for wasm32 — `scroll()` returns a `ScrollConfig::no_bounce()` config because DOM wheel events have no "gesture ended" phase to drive bounce-back from. See the [Overview](./overview.md#scroll-defaults-to-bounce-disabled) for the rationale.

```rust
const CARD_COUNT: usize = 24;

let mut content = div().w_full().flex_col().p_px(20.0).gap_px(12.0);
for idx in 0..CARD_COUNT {
    let label = format!("Card {}", idx + 1);
    let card_index = idx + 1;
    content = content.child(
        div()
            .w_full().h_fit()
            .bg(Color::rgba(0.16, 0.16, 0.21, 1.0))
            .rounded(12.0).p_px(16.0)
            .child(text(&label).size(20.0).color(Color::WHITE))
            .on_click(move |_| {
                web_sys::console::log_1(
                    &format!("clicked card #{card_index}").into(),
                );
            }),
    );
}

div()
    .w_full().h_full()
    .bg(Color::rgba(0.07, 0.07, 0.10, 1.0))
    .child(
        scroll()
            .w_full()
            .h(ctx.height - 96.0)
            .child(content)
            .on_scroll(|e| {
                tracing::info!(
                    "scroll delta=({:.1}, {:.1})",
                    e.scroll_delta_x, e.scroll_delta_y,
                );
            }),
    )
```

Full example: [`examples/web_scroll/src/lib.rs`](https://github.com/project-blinc/Blinc/blob/main/examples/web_scroll/src/lib.rs).

## `web_drag` — gesture interaction with Stateful + State

A single draggable card that lifts (opacity dip + raised z-index) and follows the cursor while held, then snaps back on release. **Structurally identical to the `sortable_list_section` in the desktop `sortable_demo.rs`** — same `DragFSM`, same `Stateful::on_state` recipe, same handler chain, same code that runs on Android and iOS:

```rust
use blinc::layout::stateful::{stateful_with_key, StateTransitions};
use blinc::core::reactive::State;
use blinc::core::context_state::BlincContextState;
use blinc::core::events::event_types;

#[derive(Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum DragFSM {
    #[default]
    Idle,
    Dragging,
}

impl StateTransitions for DragFSM {
    fn on_event(&self, event: u32) -> Option<Self> {
        match (self, event) {
            (DragFSM::Idle, event_types::DRAG) => Some(DragFSM::Dragging),
            (DragFSM::Dragging, event_types::DRAG_END) => Some(DragFSM::Idle),
            (DragFSM::Dragging, event_types::POINTER_UP) => Some(DragFSM::Idle),
            _ => None,
        }
    }
}

fn draggable_card() -> Stateful<DragFSM> {
    let blinc = BlincContextState::get();
    let offset: State<(f32, f32)> =
        blinc.use_state_keyed("card_offset", || (0.0, 0.0));

    let offset_for_drag = offset.clone();
    let offset_for_end = offset.clone();

    stateful_with_key::<DragFSM>("draggable-card")
        .deps([offset.signal_id()])
        .on_state(move |ctx| {
            let (ox, oy) = offset.get();
            let dragging = matches!(ctx.state(), DragFSM::Dragging);

            let mut card = div()
                .w(220.0).h(120.0)
                .bg(Color::rgba(0.32, 0.55, 0.92, 1.0))
                .rounded(16.0)
                .items_center().justify_center()
                .child(text("Drag me").size(20.0).color(Color::WHITE));

            if dragging {
                card = card
                    .transform(Transform::translate(ox, oy))
                    .opacity(0.85)
                    .z_index(100);
            }
            card
        })
        .on_drag(move |e| {
            offset_for_drag.set((e.drag_delta_x, e.drag_delta_y));
        })
        .on_drag_end(move |_e| {
            offset_for_end.set((0.0, 0.0));
        })
}
```

The framework's `DragFSM` Stateful state transitions automatically as DRAG / DRAG_END events fire — `ctx.state()` reads the current FSM state without you having to maintain a parallel `bool`. The visual offset lives in its own `State<(f32, f32)>` cell because it changes far more frequently than the FSM (every drag tick vs only at gesture boundaries).

Full example: [`examples/web_drag/src/lib.rs`](https://github.com/project-blinc/Blinc/blob/main/examples/web_drag/src/lib.rs).

## `web_assets` — fetched font instead of bundled

Demonstrates the `WebApp::run_with_async_setup` + `WebAssetLoader::fetch_bytes` pattern. The font isn't bundled inside the wasm artifact; it's fetched at runtime as a separate static asset that the browser caches independently. This example's wasm is **612 KB smaller** than `web_hello` purely because Arial is no longer baked into the bundle:

```rust
use blinc::app::web::WebApp;
use blinc::app::BlincError;
use blinc::platform_web::WebAssetLoader;

const FONT_URL: &str = "fonts/Arial.ttf";

#[wasm_bindgen(start)]
pub fn _start() {
    console_error_panic_hook::set_once();

    wasm_bindgen_futures::spawn_local(async {
        WebApp::run_with_async_setup(
            "blinc-canvas",
            // The `Box::pin(async move { ... })` ceremony is the
            // stable-Rust workaround for the lack of `async FnOnce`.
            // Once async closures stabilize this drops back to
            // `|app| async move { ... }`.
            |app| Box::pin(async move {
                let bytes = WebAssetLoader::fetch_bytes(FONT_URL)
                    .await
                    .map_err(|e| BlincError::Platform(e.to_string()))?;
                app.load_font_data(bytes);
                Ok(())
            }),
            build_ui,
        ).await.unwrap();
    });
}
```

See the [Fonts & Assets](./fonts-assets.md) chapter for the full picture, including the recommended bundled-fallback-then-fetched-real-font pattern for production apps.

Full example: [`examples/web_assets/src/lib.rs`](https://github.com/project-blinc/Blinc/blob/main/examples/web_assets/src/lib.rs).

## Running an example locally

```bash
git clone https://github.com/project-blinc/Blinc
cd Blinc/examples/web_hello
wasm-pack build --target web --release
./serve.sh
# open http://localhost:8000/
```

`./serve.sh` picks the first available static-file server on your system (`python3` → `python` → `ruby` → `npx http-server`) and runs it from the example directory. If `pkg/` doesn't exist yet (i.e. you forgot to `wasm-pack build` first), the script exits with a hint.
