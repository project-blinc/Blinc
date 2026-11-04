# Blinc Framework: Agent Skills Reference

> Concise, example-driven reference for AI agents generating Blinc UI code.
> Blinc is a Rust-native GPU-accelerated UI framework with flexbox layout, reactive signals, a CSS styling engine and a component library.
> This file describes Blinc 0.6.

> **Do NOT search the web for Blinc APIs.** This file holds verified API references. For anything not covered here, read the source in `crates/` and the book in `docs/book/src/`.

---

## Project Structure

```
crates/
  blinc_app/       # WindowedApp, WindowedContext, prelude, platform runners
  blinc_core/      # Color, Transform, Signal, Computed, Effect, Owner, FlowShader
  blinc_layout/    # Div, text, widgets, CSS application, regions (for_each, show), canvas
  blinc_css/       # CSS parser, ElementStyle, css_style!, style!, css!
  blinc_cn/        # Component library (shadcn/ui-inspired), styled by CSS rules
  blinc_theme/     # Themes, ThemeState, tokens as CSS variables
  blinc_animation/ # SpringConfig, AnimatedValue, keyframes
  blinc_flow/      # Flow shader parser and WGSL codegen
  blinc_gpu/       # GPU renderer (wgpu)
  blinc_recorder/  # Headless test harness (render and drive a UI without a window)
  blinc_text/ blinc_svg/ blinc_image/ blinc_paint/ blinc_platform/ blinc_router/ blinc_macros/
```

Rust 1.89 or newer. Common imports:

```rust
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::reactive::{computed, effect, signal, Computed, Signal};
use blinc_layout::IntoReactive;
```

---

## How to Build UI (read first)

State is `Signal<T>` (Copy), `Computed<T>` and `Effect`. Do not introduce `State<T>`, `use_state` or `use_state_keyed` in new code; widgets take signals.

A property that follows data is a **binding**, not a rebuild. Reach for these in order:

1. **A stylesheet** for looks: base styles, `:hover`, `:active`, `:focus`, `:disabled`, transitions, `@keyframes`.
2. **A bound property**: pass a `Signal` or `Computed` where a value goes (`.bg(sig)`, `.w(&computed)`, `text(..).color(c)`). Only that property is patched.
3. **A class or attribute that follows a signal**: `.class_when("open", sig)`, `.attr_when("data-state", "open", sig)`. Stylesheet rules for it switch on and off in place.
4. **A list or a branch**: `for_each`, `show`, `show_or`, `visible`, `when`, `collapsed_when`, when the children themselves change.
5. **`stateful::<S>()`** only for a finite state machine whose states build different structure. Every transition rebuilds its subtree.

Do not read a signal with `.get()` while building and branch on it: the build runs once and the UI will not update. Bind it instead.

---

## App Entry Point

```rust
use blinc_app::prelude::*;
use blinc_app::windowed::WindowedApp;

fn main() -> Result<()> {
    WindowedApp::run(WindowConfig::default(), |ctx| {
        div()
            .w(ctx.width)
            .h(ctx.height)
            .flex_col()
            .items_center()
            .justify_center()
            .child(text("Hello Blinc").size(32.0).color(Color::WHITE))
    })
}
```

The build closure is `FnMut(&mut WindowedContext) -> impl ElementBuilder`. It runs again when the window is resized or a rebuild is requested, so create state with `ctx.use_signal(..)`, which keeps one signal per call site across rebuilds.

With a theme bundle (and the cn stylesheet):

```rust
use blinc_app::prelude::*;
use blinc_app::windowed::WindowedApp;
use blinc_cn::cn_bundle;

fn main() -> Result<()> {
    let bundle = cn_bundle().with_css(".my-card { border-radius: 16px; }");
    WindowedApp::run_with_theme(WindowConfig::default(), bundle, ColorScheme::Dark, |_ctx| {
        div().class("my-card").p(4.0).child(text("Themed"))
    })
}
```

`HybridTheme::bundle()` (also `RestrainedTheme`, `ExpressiveTheme`) is the theme without cn. Application rules take precedence over the component layer.

---

## Reactive State

```rust
use blinc_app::prelude::*;
use blinc_core::reactive::{computed, signal};
use blinc_layout::IntoReactive;

fn counter(ctx: &mut WindowedContext) -> impl ElementBuilder {
    let count = ctx.use_signal(0_i32);
    let label = computed(move |g| format!("Count: {}", g.get(count).unwrap_or(0)));
    let color = computed(move |g| {
        if g.get(count).unwrap_or(0) > 10 { Color::RED } else { Color::WHITE }
    });

    div()
        .flex_col()
        .gap(2.0)
        .child(Text::bound(label.into_reactive()).size(32.0).color(color))
        .child(
            div()
                .flex_row()
                .gap(2.0)
                .child(button("-").on_click(move |_| count.update(|n| n - 1)))
                .child(button("+").on_click(move |_| count.update(|n| n + 1))),
        )
}
```

- `signal(v)` makes a free signal; `ctx.use_signal(v)` keeps one per call site across rebuilds; `ctx.use_signal_keyed("key", || v)` is for loops and factories called from one line.
- `sig.get()`, `sig.set(v)`, `sig.update(|v| ..)`. Signals are `Copy`: capture them by value.
- `computed(|g| ..)` tracks every signal read through `g.get(sig)`. `effect(|g| ..)` runs a side effect when its reads change; keep the returned handle alive.
- Anything created while a list row, branch or full tree build runs belongs to that scope and is disposed with it. `Owner::run` scopes your own.
- Reactive setters take `impl IntoReactive<T>`: a value, a `Signal`, a `Computed`. The call site is the same for all three.

Bound setters on `Div`: `.bg`, `.opacity`, `.rounded`, `.border_color`, `.border_width`, `.shadow`, `.transform`, `.scale`, `.rotate`, `.w`, `.h`, `.p`, `.gap`, `.visible`. On text: `Text::bound(src)`, `.size`, `.color`, `.weight`. Visual-only updates skip layout.

---

## CSS-First Styling

Style with a stylesheet. Use builder methods for values computed at run time.

```rust
fn styled(ctx: &mut WindowedContext) -> impl ElementBuilder {
    ctx.add_css(
        r#"
        .card { background: var(--surface-elevated); border-radius: 12px; padding: 16px;
                transition: background 200ms ease; }
        .card:hover { background: var(--surface-overlay); }
        #title { color: var(--text-primary); }
        "#,
    );
    div().class("card").child(text("Title").id("title"))
}
```

`ctx.add_css` appends on every call, so call it once: in a closure that runs on rebuilds, guard it with a flag. A theme bundle's `.with_css(..)` is the cleaner place.

Target with `.id("x")` (`#x`) and `.class("x")` (`.x`). `text(..)` has `.id` but not `.class`: wrap it in a classed `div` for class rules.

### Theme tokens

A theme's tokens are `:root` variables. Use `var(--primary)`, `var(--text-primary)`, `var(--surface-elevated)`, `var(--radius-lg)`, `var(--shadow-md)` (a real `box-shadow` list). `theme(token)` no longer exists. Your own variables work too:

```css
:root { --brand: #3498db; }
.card { background: var(--brand); }
```

Colours follow CSS: `rgb()` and `rgba()` components are 0 to 255, and named colours take CSS values.

### Supported CSS

| Category | Properties |
|----------|-----------|
| **Layout** | `width`, `height`, `min-*`, `max-*`, `padding`, `margin`, `gap`, `display: flex`, `flex-direction`, `align-items`, `justify-content`, `flex-grow`, `flex-shrink`, `flex-wrap`, `overflow`, `aspect-ratio` |
| **Position** | `position: absolute/relative`, `top`, `right`, `bottom`, `left` |
| **Visual** | `background` (colours, gradients), `opacity`, `border`, `border-radius`, `box-shadow`, `outline`, `outline-offset`, `backdrop-filter: blur()`, `overflow-fade` |
| **Text** | `color`, `font-size`, `font-weight`, `font-family`, `text-align`, `line-height`, `letter-spacing`, `text-decoration`, `text-overflow: ellipsis`, `white-space: nowrap` |
| **Effects** | `mix-blend-mode`, `pointer-events`, `cursor`, `filter: brightness() saturate() contrast()`, `clip-path: inset/polygon/circle` |
| **Transition** | `transition: all 300ms ease`, `transition-property`, `transition-duration`, `transition-timing-function` |
| **Animation** | `@keyframes`, `animation: name duration timing iteration` |
| **SVG** | `fill`, `stroke`, `stroke-width`, `stroke-dasharray`, `stroke-dashoffset`, `d: path(...)` |
| **3D** | `perspective`, `rotate-x`, `rotate-y`, `translate-z`, `shape-3d`, `depth` |
| **Variables** | `:root { --name: value; }`, `var(--name)`, `color-mix()` |
| **Selectors** | `#id`, `.class`, tag (SVG), `:hover`, `:active`, `:focus`, `:disabled`, `:checked`, `::placeholder`, `:first-child`, `:last-child`, `:not()`, `:is()`, `:empty`, `[attr]`, `[attr="v"]`, `>`, `+`, `~`, `*` |

An attribute is a `name=value` class: `.attr("data-state", "open")` matches `[data-state="open"]`. A `disabled` or `checked` attribute matches `:disabled` and `:checked`.

### Cascade

Rules inside `@layer name { .. }` are a component's own. An unlayered application rule beats a layered one whatever the specificity, as in browsers. cn's stylesheet is layered, so your rules override it.

### Transitions and keyframes

```css
.card { transition: all 300ms ease; }
.card:hover { transform: translateY(-4px) scale(1.02); box-shadow: 0 8px 20px rgba(0, 0, 0, 0.3); }

@keyframes pulse { 0% { opacity: 0.5; } 50% { opacity: 1; } 100% { opacity: 0.5; } }
.pulsing { animation: pulse 2000ms ease-in-out infinite; }
```

A `transform` transition does not reflow layout; width, height and padding transitions do, every frame.

### Inline styles

`css_style!` (CSS-like, semicolons) and `style!` (Rust-like, commas) build an `ElementStyle`; apply it with `.style(&s)`. `css!` builds a whole `Stylesheet`.

```rust
use blinc_layout::css_style;

let s = css_style! { background: Color::BLUE; border-radius: 12.0; opacity: 0.9; };
div().style(&s)
```

---

## Elements and Layout

```rust
let card = div().w(200.0).h(100.0).rounded(8.0).id("card");
let title = text("Hello").size(24.0).weight(FontWeight::Bold).color(Color::WHITE);
let headings = (h1("Title"), h2("Subtitle"), p("Body"), caption("Small"), label("Label"));
let layers = stack().child(div()).child(div());
let photo = image("photo.png").w(200.0).cover();
let icon = svg("icon.svg").size(24.0, 24.0);
let drawing = canvas(|ctx, bounds| { /* draw commands */ }).w(200.0).h(100.0);
```

**Spacing units.** `.p()`, `.px()`, `.py()`, `.m()`, `.mx()`, `.my()` and `.gap()` take 4px units (`.p(4.0)` is 16px). `.w()`, `.h()`, `.left()`, `.top()`, `.rounded()` and `.size()` take pixels. CSS `padding: 16px` is always pixels, so prefer CSS for spacing.

```rust
div()
    .flex_col().flex_row()   // direction
    .gap(2.0)                // 8px
    .p(4.0)                  // 16px
    .justify_center().items_center()
    .flex_wrap().flex_1().flex_grow().flex_shrink_0()
    .w_full().h_full().w_fit().h_fit()
    .absolute().left(10.0).top(10.0)
```

`w_fit()` and `h_fit()` override the parent's `align-items`; use `flex_shrink_0()` when you only want the child not to shrink.

---

## Lists and Branches

```rust
use blinc_core::reactive::signal;

#[derive(Clone)]
struct Todo { id: u32, title: String }

fn todo_list() -> impl ElementBuilder {
    let todos = signal(vec![Todo { id: 1, title: "Write docs".into() }]);
    let signed_in = signal(false);

    div()
        .flex_col()
        .gap(2.0)
        .for_each(todos, |t: &Todo| t.id, |t: Todo| text(t.title).size(14.0))
        .show_or(
            signed_in,
            || text("Welcome back"),
            move || button("Sign in").on_click(move |_| signed_in.set(true)),
        )
}
```

- `for_each(each, key, item)`: `each` is a `Signal<Vec<T>>`, `Computed<Vec<T>>` or plain `Vec<T>`. A row is built once per key, so bind the parts of a row that change to signals inside it.
- `show(cond, || ..)` builds the branch while the condition holds and tears it down, with what it created, when it stops. `show_or(cond, then, otherwise)` adds the other case. An element holds one list or branch.
- Keeping content but hiding it: `.visible(cond)` (`display: none`), `.when(cond, |d| d.child(..))`, `.collapsed_when(cond)` (no height, content stays laid out).

---

## Events

```rust
div()
    .on_click(|ctx| { /* ctx.local_x, ctx.local_y */ })
    .on_hover_enter(|_| {})
    .on_hover_leave(|_| {})
    .on_key_down(|ctx| { if (ctx.ctrl || ctx.meta) && ctx.key_code == 83 { /* save */ } })
    .on_drag(|ctx| { /* ctx.drag_delta_x, ctx.drag_delta_y */ })
    .on_scroll(|ctx| { /* ctx.scroll_delta_x, ctx.scroll_delta_y */ })
    .on_focus(|_| {})
    .on_blur(|_| {})
    .on_mount(|_| {})
    .on_unmount(|_| {})
```

Handlers get an `EventContext`: `mouse_x`, `mouse_y`, `local_x`, `local_y`, `key_code`, `key_char`, `shift`, `ctrl`, `alt`, `meta`, `drag_delta_x`, `drag_delta_y`, `scroll_delta_x`, `scroll_delta_y`, `node_id`. Capture signals by value in `move` closures and call `.set`/`.update` on them. Key events reach every handler that has one; each decides whether it is the target.

For hover, press and focus as signals when a stylesheet cannot say it, use `Interaction`: `let p = Interaction::keyed("k"); div().track(&p)`, then `p.hovered().signal()`.

### Keyboard navigation

An element with the `tabindex=0` attribute is a tab stop; `tabindex=text` marks a text input. Tab and Shift+Tab walk the stops in tree order, wrap, and scroll the stop into view. Enter and Space click the focused stop. Tab stays inside an open dialog or sheet. `:focus` matches the focused stop, so a rule draws the ring. A `disabled` attribute removes a stop.

```rust
div().attr("tabindex", "0").on_click(|_| {})
```

```css
.card:focus { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
```

A handler that wants arrow keys on a Tab-focused element tests `blinc_layout::event_router::keyboard_focused_node() == Some(ctx.node_id)`. The cn buttons, checkboxes, switches, toggles, radios, tabs, selects, menu triggers, accordion and collapsible triggers, links, sliders and text fields are stops already.

---

## Widgets

Widgets in `blinc_layout::widgets` bind to signals and shared data handles, not raw values.

```rust
use blinc_layout::widgets::*;

fn form(ctx: &mut WindowedContext) -> impl ElementBuilder {
    let agreed = ctx.use_signal(false);
    let plan = ctx.use_signal("free".to_string());
    let name = text_input_state_with_placeholder("Your name");
    let bio = text_area_state_with_placeholder("About you");

    div()
        .flex_col()
        .gap(2.0)
        .child(text_input(&name).id("name").on_change(|v| println!("{v}")))
        .child(text_area(&bio).id("bio").rows(5))
        .child(checkbox(agreed).label("I agree"))
        .child(radio_group(plan).option("free", "Free").option("pro", "Pro"))
        .child(scroll().h(120.0).child(div().h(400.0)))
}
```

`checkbox` and `toggle` take a `Signal<bool>`; `radio_group` and `input_otp` a `Signal<String>`; `number_input` a `Signal<f64>`. `.disabled(..)`, `.label(..)` and placeholders take a value, a `Signal` or a `Computed` and follow it. `text_input` and `text_area` take the data handles from `text_input_state*()` and `text_area_state*()`.

---

## Component Library (blinc_cn)

`blinc_cn` is a prebuilt, themeable component set, not a class-name utility. Its look is CSS: components carry `.cn-*` classes and `data-state` attributes, and `CN_STYLES` (inside `cn_bundle()`) holds the rules. To change a look, write a rule for the class; there are no colour or animation builder methods.

```rust
use blinc_app::windowed::WindowedContext;
use blinc_cn::prelude::*;
use blinc_layout::prelude::{div, text, ElementBuilder};

fn settings(ctx: &mut WindowedContext) -> impl ElementBuilder {
    let notify = ctx.use_signal(true);
    let volume = ctx.use_signal(40.0_f32);
    let choice = ctx.use_signal("a".to_string());

    card()
        .child(card_header().title("Settings").description("Preferences"))
        .child(
            card_content()
                .child(switch(notify))
                .child(slider(volume).min(0.0).max(100.0).step(5.0))
                .child(select(choice).option("a", "Option A").option("b", "Option B")),
        )
        .child(card_footer().child(button("Save").variant(ButtonVariant::Primary)))
}
```

- State: `checkbox`, `switch`, `toggle` take `Signal<bool>`; `radio_group`, `toggle_group`, `select`, `combobox`, `input_otp` take `Signal<String>`; `number_input` a `Signal<f64>`; `slider` a `Signal<f32>` or `Signal<f64>`.
- Overlays are shown from a handler and return an `OverlayHandle`: `dialog().title(..).description(..).confirm_text(..).on_confirm(|| ..).show()`, `alert_dialog()`, `sheet()`, `drawer()`. There is no open signal.
- `popover(trigger_fn).content(|| ..)`, `collapsible(Signal<bool>)`, `sidebar(Signal<bool>)`, `tabs`, `accordion`, `tree_view`, `calendar`, `date_picker`, `time_picker`, `color_picker`, `table`, charts. Read the component's source in `crates/blinc_cn/src/components/` for its builder.
- Disabled, labels, descriptions and errors follow a signal in place.

---

## Stateful Elements

`stateful::<S>()` runs a state machine and rebuilds its subtree from `on_state` on each transition. Use it when states change what is built; use a stylesheet or a binding for colours and sizes.

```rust
fn card_button(label: &'static str) -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(move |ctx| {
        let bg = match ctx.state() {
            ButtonState::Idle => Color::BLUE,
            ButtonState::Hovered => Color::CYAN,
            ButtonState::Pressed => Color::GRAY,
            ButtonState::Disabled => Color::GRAY,
        };
        div().bg(bg).child(text(label))
    })
}
```

- Built-in states: `ButtonState` (`Idle`, `Hovered`, `Pressed`, `Disabled`), `ToggleState`, and `NoState` for `.deps(..)` only. A custom machine implements `StateTransitions` (plural).
- `on_state` returns a `Div`; it does not mutate. `ctx.state()` returns the state by copy.
- Reactive rebuild: `stateful::<NoState>().deps([sig.signal_id()]).on_state(..)`. Prefer a binding.
- `ctx.use_fsm(initial)` and `use_fsm_keyed(key, initial)` return shared state for a machine.

---

## Animation

Order of preference: CSS `transition` and `@keyframes`; then `animate_bounds` for layout-driven changes; then springs.

```rust
use blinc_layout::visual_animation::VisualAnimationConfig;

fn faq(open: Signal<bool>) -> impl ElementBuilder {
    let closed = computed(move |g| !g.get(open).unwrap_or(false));
    // the body animates its height as the section opens
    div()
        .overflow_clip()
        .animate_bounds(VisualAnimationConfig::height().with_key("faq-1").clip_to_animated())
        .collapsed_when(&closed)
        .child(text("Answer"))
}
```

`VisualAnimationConfig::{height, width, size, position, all}`; add `.with_key("stable-id")` for persistence across rebuilds.

Springs: `SpringConfig::{stiff, snappy, gentle, wobbly}` or `SpringConfig::new(stiffness, damping, mass)`. `AnimatedValue::new(get_scheduler(), initial, config)` with `.set_target(v)` and `.get()`; share one as `SharedAnimatedValue` (`Arc<Mutex<AnimatedValue>>`) and hand it to `motion().opacity(value)`. Inside `stateful::on_state`, `ctx.use_spring("name", target, config)` returns the current `f32`.

`motion()` wraps content in enter and exit animations (`fade_in(ms)`, `slide_in(dir, ms)`, `stagger(..)`). A list row can animate out with `Row::new(el).on_leave(start, done)` from `blinc_layout::region`.

| Need | Use |
|------|-----|
| Hover or focus colour change | CSS `transition` with `:hover` |
| Lift, scale, slide | CSS `transition: transform` |
| Looping animation | CSS `@keyframes` |
| Accordion or collapse | `animate_bounds` with `collapsed_when` |
| Enter, exit, stagger | `motion()` |
| Row leaving a list | `Row::on_leave` |

---

## Canvas and Flow Shaders

`canvas(|ctx, bounds| ..)` draws with GPU commands. A flow shader is drawn by a canvas, not by CSS:

```rust
use blinc_layout::flow;

fn ripple_view() -> impl ElementBuilder {
    let ripple = flow!(ripple, fragment, {
        input uv: builtin(uv);
        input time: builtin(time);
        node d = distance(uv, vec2(0.5, 0.5));
        node wave = sin(d * 20.0 - time * 4.0) * 0.5 + 0.5;
        output color = vec4(wave, wave, wave, 1.0);
    });
    canvas(|_, _| {}).flow(ripple).w(400.0).h(400.0)
}
```

`flow!` compiles to WGSL at build time and yields a `FlowShader` constant. In its body, step types use underscore names and colours are hex integers (`0x1a3a5a`). A shader that reads `time` animates every frame; `Canvas::flow_loop` and `FlowClock` control that. The `@flow` stylesheet block and `div().flow(..)` are removed.

---

## Glass Material

```rust
use blinc_layout::element::{GlassMaterial, Material};

let frosted = div().glass();
let thick = div().material(Material::Glass(GlassMaterial::thick()));
```

Presets: `ultra_thin`, `thin`, `regular`, `thick`, `frosted`, `card`. In CSS: `backdrop-filter: blur(12px) saturate(180%)`.

---

## Component Patterns

```rust
fn card_with(title: &str, content: impl ElementBuilder + 'static) -> Div {
    div()
        .class("card")
        .flex_col()
        .child(text(title).size(18.0).weight(FontWeight::SemiBold))
        .child(content)
}
```

A function component takes signals as parameters and binds them inside; it never reads a signal at build time to decide structure. Style it with a class and a stylesheet rule.

---

## Testing

`blinc_recorder::Headless` renders a UI offscreen and drives it like a window (needs a GPU adapter; tests skip without one):

```rust,ignore
let mut ui = Headless::new(400, 200)?;
ui.theme(HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES), ColorScheme::Dark);
ui.mount(|_| div().child(button("Save")));
ui.settle();
ui.hover(".cn-button");     // also move_to, mouse_down, click, key(code), key_with_shift
ui.key(9);                  // Tab
ui.cursor_over(".cn-button");
```

Run tests with `cargo nextest run`: each test gets its own process, which UI code with process-wide state needs. Plain `cargo test` shares one process among threads. Golden tests compare against checked-in files; regenerate with `BLESS_GOLDEN=1`.

---

## Common Pitfalls

1. **No `State<T>` in new code.** Use `Signal`, `Computed`, `Effect`. Widgets take signals.
2. **Do not branch on `signal.get()` while building.** Bind the property, class or list instead.
3. **Signals are `Copy`.** Capture by value in `move` closures; no `.clone()`.
4. **`on_state` returns a `Div`**; it is a builder, not a mutation.
5. **A `stateful` element rebuilds on every transition.** Use it for structure, not for colours.
6. **`ctx.add_css` appends each call.** Add a stylesheet once, or use a bundle's `.with_css`.
7. **`theme(token)` is gone.** Write `var(--token)`.
8. **`rgb()` components are 0 to 255**, as in CSS.
9. **Spacing units differ.** `.p()`, `.m()`, `.gap()` are 4px units; `.w()`, `.h()` are pixels.
10. **`text()` has no `.class()`.** Use `.id("x")`, or wrap in a classed `div`.
11. **A `for_each` row is built once per key.** Bind what changes inside the row.
12. **Widget constructors take handles.** `text_input(&data)`, `text_area(&state)`; create them first with `text_input_state()` and `text_area_state()`.
13. **`blinc_app::prelude::*` and `blinc_cn::prelude::*` both export `button` (and more).** In a cn app import `blinc_cn::prelude::*` and name the layout items you need: `use blinc_layout::prelude::{div, text};`.
14. **cn components are restyled with CSS rules**, not builder colours. There are no `track_color`, `enter_animation` or similar methods.
15. **Dialogs are shown from a handler** with `.show()`; there is no open signal.
16. **Use `tracing::`** for logging, not `log::` or `eprintln!`.
17. **Web builds use `web_time::Instant`**, not `std::time::Instant`.
