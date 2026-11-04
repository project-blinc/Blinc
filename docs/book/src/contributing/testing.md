# Testing

Blinc tests a UI by building it, driving it, and reading back either the tree or the frame. There
is no window and no manual step. This page covers what to write, how to run it, and what keeps a
test from flaking.

## Kinds of test

| Kind | Where it lives | What it checks |
| --- | --- | --- |
| Unit | `#[cfg(test)]` in the crate | Parsing, arithmetic, state machines, reactive graph |
| Tree | `tests/` of a crate, with its helpers | Render props, layout, handlers, after building a real tree |
| Headless | `blinc_recorder` `Headless`, feature `headless` | The whole frame: stylesheet, state rules, animation, layout, paint, input |
| Image golden | `Headless::assert_snapshot` | A frame against a recorded picture |
| Text golden | `blinc_cn` tests | A dump of a widget's render props in each state |

Prefer the cheapest kind that can fail for the right reason. A colour a rule should give is an
assertion on a render prop. A transition's last frame is a headless run. A picture is for what
cannot be said as a value.

## Headless

`Headless` builds a real UI offscreen through the renderer a window uses. You control the pointer
and the clock.

```rust
use blinc_cn::prelude::*;
use blinc_layout::prelude::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

#[test]
fn a_button_shows_the_pointer_cursor() {
    if !Headless::available() {
        return; // no GPU adapter on this machine
    }
    let mut ui = Headless::new(400, 200).expect("headless");
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    ui.mount(|_| div().p(4.0).child(button("Save")));
    ui.settle();

    let (x, y) = ui.centre(".cn-button");
    ui.move_to(x, y);
    assert_eq!(ui.cursor_at(x, y), Some(blinc_layout::element::CursorStyle::Pointer));
}
```

- **Mount and settle.** `mount` builds the UI and `settle` runs frames until nothing animates or
  eases. `advance(ms)` steps the clock by hand, a frame at a time, to look inside a transition.
- **Drive.** `move_to`, `hover(selector)`, `click(selector)`, `mouse_down`, `mouse_up`, `scroll`,
  `leave`, `key(code)` and `key_with_shift(code, true)`. A key reaches every handler, as in a
  window. Tab is 9, Enter 13, Space 32, Left 37, Right 39.
- **Read.** `find(selector)` and `centre(selector)` take `#id` or `.class`. `tree()` gives the
  render tree for props and layout, `focused()` the focused node, `cursor_at(x, y)` and
  `cursor_over(selector)` the cursor the window would set, `snapshot()` the frame.
- **Inputs are real.** A click on a text input focuses it the way a press does, and Tab walks the
  same tab stops the runners do.

A test that names a missing selector panics: it is wrong, not skipped.

## Image goldens

`assert_snapshot(name)` compares the current frame with a recorded PNG.

```rust
ui.hover(".cn-button").settle();
ui.assert_snapshot("button_hover");
```

Goldens are per machine and platform, in `.blinc/snapshots/<crate>/<os>/` at the workspace root,
and are not tracked. Each machine records its own:

```sh
BLINC_SNAPSHOT=bless cargo nextest run -p blinc_recorder --features headless
```

A platform with no recorded goldens skips, so CI never fails for want of one. `BLINC_SNAPSHOT=off`
skips everywhere, and `BLINC_SNAPSHOT=report` logs every difference without failing, to review a
change across many frames. A mismatch writes the actual and diff images to `failed/` beside the
goldens.

For a frame with a little legitimate noise use `Golden::in_crate().tolerance(n).check(name, &frame)`:
`n` is the largest channel difference a pixel may have. Keep it small, and prefer removing the
noise (below) to widening it.

## Text goldens

A widget's look in each state can be recorded as text: its render props (fill, border, opacity,
radius, shadow, scale, size, cursor) for idle, hover, pressed and disabled. The file is checked in,
so it is the same on every platform and a change shows in review.

```sh
BLESS_GOLDEN=1 cargo nextest run -p blinc_cn
```

Read the diff before keeping it. A one-line rule change can legitimately rewrite many lines (a new
cursor appears in every state), and the diff should show only that.

## Running tests

```sh
cargo nextest run --workspace --all-features \
  --exclude blinc_dsl_core --exclude blinc_cn_dsl --exclude blinc_dsl_macro
cargo test --doc --workspace --all-features \
  --exclude blinc_dsl_core --exclude blinc_cn_dsl --exclude blinc_dsl_macro
```

Use **nextest**. It runs each test in its own process, and Blinc keeps process-wide state: the
reactive graph, the theme, which text input has focus, the overlay stack. Plain `cargo test` runs a
binary's tests as threads in one process, so they share all of it and fail in ways CI never sees.
Where a test has to share, a lock serialises it; such a test says so.

Because each test has a fresh process, a test initialises what it reads. A test that depends on
the theme sets it (`ui.theme(..)`), and does not rely on the operating system's scheme.

nextest stops at the first failure unless you pass `--no-fail-fast`. CI does, so run the same way
to see every failure at once.

## Keeping a test deterministic

- **Use the virtual clock.** `settle` and `advance` move time; never sleep or read the wall clock.
  A wall-clock value in a frame makes the golden depend on how long the run took. `Headless` holds
  the text caret at its reset-moment opacity for this reason.
- **Settle before you assert.** Colours and sizes ease. Assert after `settle`, or step `advance`
  to the instant you mean.
- **Skip without a GPU.** `Headless::available()` is false on a machine with no adapter. Return
  early with a message, as above.
- **Say what you wait for.** Wait for a state (`settle`), not a duration.

A suspected flake is checked by running it many times under load:

```sh
cargo nextest run -p blinc_recorder --features headless --test cn_textarea_snapshots \
  --no-fail-fast --stress-count 300
```

Run busy loops on every core while it does. A fault that appears once in a hundred runs is a fault,
and the failing run's output names it.

## Before you push

Hosted CI runs these, and any of them can fail for a change that looks unrelated, because goldens
and process-wide state live in crates other than the one you edited:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-features --all-targets -- -W clippy::all -A clippy::type_complexity -A dead_code
cargo check -p blinc_app --target wasm32-unknown-unknown --features web --no-default-features
cargo nextest run --no-fail-fast --workspace --all-features --exclude blinc_dsl_core --exclude blinc_cn_dsl --exclude blinc_dsl_macro
cargo test --doc --workspace --all-features --exclude blinc_dsl_core --exclude blinc_cn_dsl --exclude blinc_dsl_macro
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
cargo check -p blinc_gpu --target x86_64-pc-windows-msvc
cargo check -p blinc_app --no-default-features --target aarch64-apple-ios --features ios
```

Three more checks catch what the list above cannot:

- **A Windows target.** The tracked lockfile can pin two crates to versions that agree on one platform and disagree on another (`gpu-allocator` and `wgpu-hal` on D3D12). `cargo check` for `x86_64-pc-windows-msvc` finds it without a Windows machine.
- **The web wrappers.** An example that is new or changed must still build for wasm. Run `cargo run -p blinc-build-web-examples`, then `cargo check --target wasm32-unknown-unknown` on the wrapper of each example you touched, and revert the gallery pages it rewrites. An example's `fn main` is gated `#[cfg(not(target_arch = "wasm32"))]`.
- **A machine with no GPU.** The coverage runner has no adapter. Every test that builds a `Headless` starts with `if !Headless::available() { return; }`, including tests that go through a helper.

Run them on a clean checkout of your commit, not on a tree with uncommitted work: the workspace
feature set compiles code a single crate's check does not, and an untracked file can hide a
missing one. When a hosted job fails, read that job's log for the failing test before guessing;
operating systems differ only in what the log names.

## Recording a session

A headless run records like a live one, in the format the debugger opens, and it replays to the same
frame:

```rust
ui.start_recording();
ui.click(".cn-button").settle();
let export = ui.stop_recording().unwrap();

other.replay(&export);
```
