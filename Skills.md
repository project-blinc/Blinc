# Blinc Framework — Agent Skills Reference

> Concise, example-driven reference for AI agents generating Blinc UI code.
> Blinc is a Rust-native GPU-accelerated UI framework with flexbox layout, reactive signals, spring animations, and a CSS styling engine.

> **Do NOT search the web for Blinc APIs.** This file contains verified, up-to-date API references. For anything not covered here, read the source in `crates/` directly.

---

## Project Structure

```
crates/
  blinc_app/       # Application framework, WindowedApp, prelude
  blinc_core/      # Color, Transform, Signal, BlincContextState
  blinc_layout/    # Div, text, stateful, motion, CSS parser, widgets
  blinc_animation/ # SpringConfig, AnimatedValue, keyframes
  blinc_paint/     # Low-level rendering primitives
  blinc_gpu/       # GPU renderer (wgpu)
  blinc_theme/     # Theming tokens, ThemeState (auto-injects CSS variables)
  blinc_cn/        # Component library (shadcn/ui-inspired prebuilt widgets)
  blinc_text/      # Text shaping and rendering
  blinc_svg/       # SVG loading and rendering
  blinc_image/     # Image loading
  blinc_platform/  # Platform abstraction
  blinc_macros/    # Derive macros (BlincComponent)
```

Common imports:
```rust
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_layout::stateful::stateful;
use blinc_layout::motion::motion;
use blinc_core::{Color, Transform, BlincContextState};
```

---

## CSS-First Styling (Preferred Approach)

**Always prefer CSS for all styling and layout.** CSS stylesheets provide consistency, separation of concerns, theme integration via CSS variables, and automatic hover/focus/disabled states without manual state management. Only fall back to builder methods for truly dynamic runtime values (e.g., values computed from signals). Avoid direct `.bg()` calls when a CSS class or ID selector would work.

### Loading a Stylesheet

Call `ctx.add_css()` once (guard with a bool so it doesn't re-add on rebuilds):

```rust
let mut css_loaded = false;

WindowedApp::run(config, move |ctx| {
    if !css_loaded {
        ctx.add_css(r#"
            #my-card {
                background: #1e293b;
                border-radius: 12px;
                padding: 16px;
                box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
            }
            #my-card:hover {
                background: #334155;
                box-shadow: 0 8px 16px rgba(59, 130, 246, 0.3);
            }

            .btn-primary {
                background: #3b82f6;
                border-radius: 8px;
                cursor: pointer;
            }
            .btn-primary:hover {
                background: #2563eb;
            }
        "#);
        css_loaded = true;
    }
    build_ui(ctx)
});
```

### Targeting Elements

Use `.id()` for unique elements (matched by `#id`) and `.class()` for reusable styles (matched by `.class`):

```rust
div().id("my-card")                     // matches #my-card
    .child(div().class("btn-primary"))  // matches .btn-primary
```

### Theme CSS Variables

`ThemeState` automatically injects theme tokens as CSS variables. Two ways to access them:

```css
/* var(--token) — standard CSS variables from ThemeState */
h1 { color: var(--text-primary); }
p  { color: var(--text-secondary); }
.card {
    background: var(--surface-elevated);
    border-color: var(--primary);
}
svg { fill: var(--text-tertiary); }

/* theme(token) — shorthand function for theme tokens */
#button-primary {
    background: theme(primary);
    border-radius: theme(radius-default);
    box-shadow: theme(shadow-md);
}
```

Custom CSS variables are also supported:
```css
:root {
    --brand-color: #3498db;
    --card-radius: 12px;
}
#card {
    background: var(--brand-color);
    border-radius: var(--card-radius);
}
```

This is the preferred way to apply colors — via theme variables in CSS — rather than hardcoding `Color::rgba(...)` in builder methods.

### Supported CSS Properties

| Category | Properties |
|----------|-----------|
| **Layout** | `width`, `height`, `min-width`, `max-width`, `min-height`, `max-height`, `padding`, `margin`, `gap`, `display: flex`, `flex-direction`, `align-items`, `justify-content`, `flex-grow`, `flex-shrink`, `flex-wrap`, `overflow`, `aspect-ratio` |
| **Position** | `position: absolute/relative`, `top`, `right`, `bottom`, `left` |
| **Visual** | `background`, `opacity`, `border-radius`, `box-shadow`, `backdrop-filter: blur()`, `overflow-fade` |
| **Text** | `color`, `font-size`, `font-weight`, `font-family`, `text-align`, `line-height`, `letter-spacing`, `text-decoration`, `text-decoration-color`, `text-decoration-thickness`, `text-overflow: ellipsis`, `white-space: nowrap` |
| **Blend/FX** | `mix-blend-mode`, `pointer-events`, `cursor`, `filter: brightness() saturate() contrast()` |
| **Backdrop** | `backdrop-filter: blur()`, `backdrop-filter: liquid-glass(...)` |
| **Transition** | `transition: all 300ms ease`, `transition-property`, `transition-duration`, `transition-timing-function` |
| **Animation** | `@keyframes`, `animation: name duration timing-fn iteration`, `animation-name`, `animation-duration`, `animation-timing-function` |
| **Clip** | `clip-path: inset(...)`, `clip-path: polygon(...)`, `clip-path: circle(...)` |
| **SVG** | `fill`, `stroke`, `stroke-width`, `stroke-dasharray`, `stroke-dashoffset`, `d: path(...)` (morphing) |
| **3D** | `perspective`, `rotate-x`, `rotate-y`, `translate-z`, `shape-3d: box/sphere/cylinder/group`, `depth`, `3d-op: subtract/smooth-union`, `3d-blend` |
| **Variables** | `:root { --name: value; }`, `var(--name)`, `theme(token)` |
| **Selectors** | `#id`, `.class`, `tag` (SVG), `:hover`, `:active`, `:focus`, `:disabled`, `::placeholder`, `:first-child`, `:last-child`, `:not()`, `:is()`, `:empty`, `>`, `+`, `~`, `*` |

### CSS Transitions (Preferred for Smooth State Changes)

CSS transitions automatically animate property changes on hover, focus, etc.:

```css
.card {
    background: #1e3a5f;
    border-radius: 12px;
    transition: all 300ms ease;  /* animates ALL property changes */
}
.card:hover {
    background: #3b82f6;
    border-radius: 24px;
    box-shadow: 0 8px 20px rgba(0, 0, 0, 0.3);
}

/* Filter transitions */
.filter-card {
    transition: filter 400ms ease;
}
.filter-card:hover {
    filter: brightness(1.8) saturate(2.0) contrast(1.3);
}

/* Backdrop-filter transitions (glass morphism) */
.icon-wrapper {
    backdrop-filter: blur(12px) saturate(180%) brightness(80%);
    transition: backdrop-filter 400ms ease;
}
.icon-wrapper:hover {
    backdrop-filter: blur(16px) saturate(200%) brightness(110%);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.4);
}

/* Transform transitions (FLIP-style — smooth, no layout reflow) */
.card {
    transform: translateY(0);
    transition: transform 300ms ease;
}
.card:hover {
    transform: translateY(-4px) scale(1.02);
}

/* Layout transitions — width, height, padding (supported but triggers reflow per frame) */
.expandable {
    width: 100px;
    transition: width 400ms ease;
}
.expandable:hover {
    width: 200px;
}

/* Overflow fade transitions */
#fade-container {
    overflow: clip;
    overflow-fade: 0px;
    transition: overflow-fade 500ms ease;
}
#fade-container:hover {
    overflow-fade: 32px;
}

/* Button states (all in CSS, no Rust code needed!) */
#btn {
    background: theme(primary);
    transition: all 200ms ease;
}
#btn:hover { opacity: 0.9; transform: scale(1.02); }
#btn:active { transform: scale(0.98); }
#btn:disabled { opacity: 0.5; }
```

### CSS Keyframe Animations

Define reusable `@keyframes` for complex, multi-step animations:

```css
/* Looping pulse */
@keyframes pulse {
    0% { opacity: 0.5; }
    50% { opacity: 1.0; }
    100% { opacity: 0.5; }
}
#pulse-element {
    animation: pulse 2000ms ease-in-out infinite;
}

/* Multi-stop gradient animation */
@keyframes track-glow {
    0%   { background: linear-gradient(90deg, #d0d0d0, #e0e0e0, #ffffff); }
    33%  { background: linear-gradient(90deg, #d0d0d0, #ffffff, #d0d0d0); }
    66%  { background: linear-gradient(90deg, #ffffff, #e0e0e0, #d0d0d0); }
    100% { background: linear-gradient(90deg, #d0d0d0, #e0e0e0, #ffffff); }
}

/* Clip-path reveal on hover */
@keyframes clip-reveal {
    from { clip-path: inset(50% 50% 50% 50%); }
    to { clip-path: inset(0% 0% 0% 0%); }
}
#reveal:hover {
    animation: clip-reveal 400ms ease-out forwards;
}

/* SVG stroke-dash line drawing */
@keyframes draw-circle {
    0%   { stroke-dashoffset: 251; }
    100% { stroke-dashoffset: 0; }
}
#draw-svg {
    stroke-dasharray: 251;
    animation: draw-circle 3s ease-in-out infinite alternate;
}

/* SVG fill/stroke color cycling */
@keyframes color-cycle {
    0%   { fill: #ef4444; stroke: #dc2626; }
    33%  { fill: #3b82f6; stroke: #2563eb; }
    66%  { fill: #10b981; stroke: #059669; }
    100% { fill: #ef4444; stroke: #dc2626; }
}

/* SVG path morphing */
@keyframes morph-shape {
    0%   { d: path("M20,20 L80,20 L80,80 L50,80 L20,80 Z"); }
    50%  { d: path("M50,10 L90,40 L75,85 L25,85 L10,40 Z"); }
    100% { d: path("M20,20 L80,20 L80,80 L50,80 L20,80 Z"); }
}

/* 3D rotation */
@keyframes spin-y {
    0% { rotate-y: 0deg; }
    100% { rotate-y: 360deg; }
}
#spinning {
    perspective: 800px;
    animation: spin-y 4000ms linear infinite;
}
```

### Common CSS Patterns

```css
/* Text truncation with ellipsis */
.truncated {
    width: 250px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
}

/* Non-interactive overlay */
.overlay {
    pointer-events: none;
    opacity: 0.5;
}

/* SVG sub-element styling (tag-name selectors) */
#my-svg path { stroke: #8b5cf6; stroke-width: 5; }
#my-svg circle { fill: #f3e8ff; stroke: #a78bfa; }
#my-svg rect { fill: #fef3c7; stroke: #f59e0b; }

/* Liquid glass (advanced glass morphism) */
#glass-card {
    backdrop-filter: liquid-glass(blur(18px) saturate(180%) brightness(120%)
                                  border(4.0) tint(rgba(255, 255, 255, 1.0)));
    border-radius: 24px;
    box-shadow: 0 8px 80px rgba(0, 0, 0, 0.60);
}
```

### Advanced CSS Selectors

```css
/* Child combinator */
#parent > .child { background: #374151; }
#parent:hover > .child { background: #6366f1; }

/* Adjacent sibling: hover one, style the next */
.trigger:hover + .target { background: #f59e0b; }

/* General sibling: hover one, style all following siblings */
.trigger:hover ~ .item { background: #a78bfa; }

/* :not() */
.item:not(:first-child) { background: #0891b2; }

/* Universal inside parent */
#container > * { background: #059669; }
```

### Specificity (Low to High)

1. **CSS stylesheet** (`ctx.add_css()`) — lowest priority
2. **`css_style!` / `style!` macros** — inline element styles
3. **Builder methods** (`.bg()`, `.w()`, etc.) — highest priority, overrides CSS

### Inline Style Macros

Both macros return `ElementStyle`. Apply to elements with `.style()`:

```rust
use blinc_layout::css_style;
use blinc_layout::style;

// css_style! macro — CSS-like syntax, semicolons, hyphenated properties, Rust values
let card_style = css_style! {
    background: Color::rgba(0.2, 0.3, 0.9, 1.0);
    border-radius: 12.0;
    box-shadow: Shadow::new(0.0, 4.0, 8.0, Color::BLACK.with_alpha(0.2));
    opacity: 0.9;
};
div().style(&card_style)

// style! macro — Rust-friendly syntax, commas, underscored properties
let btn_style = style! {
    bg: Color::BLUE,
    rounded: 8.0,
    shadow_md
};
div().style(&btn_style)
```

### Why CSS First?

- **Hover/focus/disabled states for free** — no `stateful::<ButtonState>()` needed for pure visual changes
- **Transitions and animations built-in** — `transition: all 300ms ease` and `@keyframes` handle most animation needs without Rust code
- **Consistent theming** — one stylesheet + `var(--token)` styles the whole app
- **Rich selector system** — child `>`, sibling `+`/`~`, `:not()`, `:is()`, `:first-child`, etc.
- **Less code** — no per-element builder chains for every style property
- **CSS variables** — theme tokens auto-inject as CSS variables from `ThemeState`
- **Spacing consistency** — CSS uses standard `px` units, while builder spacing methods (`.p()`, `.gap()`, `.m()`) use a 4px unit system (see below)
- **Clip-path & 3D** — `clip-path`, `perspective`, `rotate-x/y`, `shape-3d` are all CSS-driven

---

## Element Builders

All elements use a fluent builder API. Core elements:

```rust
// Container
div().w(200.0).h(100.0).rounded(8.0).id("my-card")  // prefer CSS for bg/colors

// Text
text("Hello").size(24.0).weight(FontWeight::Bold).color(Color::WHITE)

// Typography helpers
h1("Title")  h2("Subtitle")  h3("Section")  p("Body")  caption("Small")
b("Bold")  small("Small")  muted("Muted")  label("Label")

// Stack (overlapping layers)
stack().child(background).child(foreground)

// Image & SVG
image("photo.png").w(200.0).cover()
svg("icon.svg").w(24.0).h(24.0).tint(Color::WHITE)

// Canvas (custom GPU drawing)
canvas(|ctx, bounds| { /* draw commands */ }).w(200.0).h(100.0)
```

### Layout (Flexbox)

**IMPORTANT — Builder spacing units:**
- `.w()`, `.h()`, `.left()`, `.top()`, `.rounded()`, `.size()` use **raw pixels**
- `.p()`, `.px()`, `.py()`, `.m()`, `.mx()`, `.my()`, `.gap()` use **4px units** (e.g., `.p(4.0)` = 16px, `.gap(2.0)` = 8px)

**To avoid confusion, prefer CSS for layout sizing** where values are always in standard `px`:
```css
.card { padding: 16px; margin: 8px; gap: 12px; }
```

Builder methods (when CSS is not practical):
```rust
div()
    .flex_col()          // Vertical layout
    .flex_row()          // Horizontal layout
    .gap(4.0)            // 4.0 * 4 = 16px gap between children
    .justify_center()    // Main axis center
    .items_center()      // Cross axis center
    .flex_center()       // Both axes centered
    .flex_wrap()         // Wrap children
    .p(4.0)              // 4.0 * 4 = 16px padding all sides
    .px(4.0).py(2.0)     // Horizontal 16px / vertical 8px padding
    .m(2.0)              // 2.0 * 4 = 8px margin all sides
    .flex_1()            // flex: 1 1 0%
    .flex_grow()         // flex-grow: 1
    .w(200.0)            // 200px width (raw pixels)
    .h(100.0)            // 100px height (raw pixels)
    .w_full().h_full()   // 100% width/height
    .absolute()          // position: absolute
    .left(10.0)          // 10px left (raw pixels)
    .top(10.0)           // 10px top (raw pixels)
```

---

## App Entry Point

```rust
fn main() -> Result<()> {
    let config = WindowConfig {
        title: "My App".to_string(),
        width: 800,
        height: 600,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .w(ctx.width)
        .h(ctx.height)
        .id("root")  // prefer CSS for styling
        .flex_col()
        .items_center()
        .justify_center()
        .child(text("Hello Blinc").size(32.0).color(Color::WHITE))
}
```

---

## Reactive State (Signals)

### Creating State

```rust
// Keyed state — persists across UI rebuilds
let count = ctx.use_state_keyed("counter", || 0i32);

// Read
let value = count.get();

// Write
count.set(5);
count.update(|v| v + 1);

// Get signal ID for dependency tracking
let id = count.signal_id();
```

`State<T>` is `Copy` — capture directly in closures, no need to clone.

### Using State in Closures

```rust
use blinc_core::BlincContextState;

div().on_click(move |_| {
    // BlincContextState provides thread-safe access from event handlers
    BlincContextState::get().update(count, |v| v + 1);
})
```

### Dependency Tracking with `stateful`

Elements that need to re-render when signals change use `stateful::<S>()` with `.deps()`:

```rust
fn count_display(count: State<i32>) -> impl ElementBuilder {
    stateful::<NoState>()
        .deps([count.signal_id()])
        .on_state(move |_ctx| {
            let current = count.get();
            div().child(text(&format!("{}", current)).size(64.0).color(Color::WHITE))
        })
}
```

---

## Stateful Containers

`stateful::<S>()` creates elements with automatic state transitions. `S` must implement `StateTransitions`.

### Built-in States

| State | Variants | Auto-transitions |
|-------|----------|------------------|
| `ButtonState` | `Idle`, `Hovered`, `Pressed`, `Disabled` | Hover enter/leave, press/release |
| `ToggleState` | `Off`, `On` | Click toggles |
| `NoState` | (unit struct) | None — used only for `.deps()` reactive tracking |

### Counter Example

A practical example combining signals, stateful dependency tracking, and events:

```rust
fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    let count = ctx.use_state_keyed("counter", || 0i32);

    div()
        .w(ctx.width).h(ctx.height).id("root")
        .flex_col().items_center().justify_center()
        // Display updates reactively via .deps()
        .child(count_display(count))
        // Buttons use CSS for hover styling (no manual ButtonState needed)
        .child(
            div().flex_row().gap(4.0)
                .child(counter_btn(count, "-", -1))
                .child(counter_btn(count, "+", 1))
        )
}

fn count_display(count: State<i32>) -> impl ElementBuilder {
    stateful::<NoState>()
        .deps([count.signal_id()])  // re-run on_state when count changes
        .on_state(move |_ctx| {
            let current = count.get();
            div().child(
                text(&format!("{}", current))
                    .size(64.0).weight(FontWeight::Bold).color(Color::WHITE)
            )
        })
}

fn counter_btn(count: State<i32>, label: &'static str, delta: i32) -> impl ElementBuilder {
    // Prefer CSS for hover/press styling:
    // #counter-btn { background: #333; } #counter-btn:hover { background: #555; }
    div()
        .class("counter-btn")
        .on_click(move |_| { count.update(|v| v + delta); })
        .child(text(label).size(28.0).weight(FontWeight::Bold).color(Color::WHITE))
}
```

**Key principle:** Use CSS for hover/press visual effects. Use `stateful::<NoState>()` with `.deps()` for signal-driven reactive updates. Only reach for `stateful::<ButtonState>()` with `on_state` when you need runtime logic that CSS can't express.

**Rules for `on_state` callback:**
- Callback signature: `Fn(&StateContext<S>) -> Div`
- Read state via `ctx.state()` (returns `S` by copy)
- **Return** a `Div` (builder pattern) — do NOT mutate
- Outer builder methods (`.id()`, `.class()`) apply to the container
- Inner `div()` returned from `on_state` is the visual content

### Conditional Rendering with `.when()`

`.when(condition, |div| ...)` conditionally modifies a builder. This only works **inside `on_state` callbacks** where the structure rebuilds on state change — outside of `on_state`, conditions are evaluated once at build time and won't react to changes.

```rust
fn expandable_section(expanded: State<bool>) -> impl ElementBuilder {
    stateful::<NoState>()
        .deps([expanded.signal_id()])
        .on_state(move |_ctx| {
            let is_open = expanded.get();
            div()
                .child(text("Section Title"))
                .when(is_open, |d| {
                    d.child(div().child(text("Expanded content here...")))
                })
        })
}
```
```

---

## Spring Animations

### Spring Presets

```rust
use blinc_animation::SpringConfig;

SpringConfig::stiff()   // Fast, no overshoot (buttons, toggles)
SpringConfig::snappy()  // Quick with slight bounce
SpringConfig::gentle()  // Soft, slow settle (modals, overlays)
SpringConfig::wobbly()  // Bouncy, playful
SpringConfig::new(stiffness, damping, mass)  // Custom (f32, f32, f32)
```

### Using Springs in StateContext

`ctx.use_spring()` is a convenience method that sets the target and returns the current animated value as `f32`:

```rust
fn animated_button() -> impl ElementBuilder {
    stateful::<ButtonState>()
        .on_state(|ctx| {
            // use_spring(name, target, config) -> f32
            // Set target based on current state; returns current interpolated value
            let target = match ctx.state() {
                ButtonState::Hovered => 1.05,
                ButtonState::Pressed => 0.95,
                _ => 1.0,
            };
            let s = ctx.use_spring("scale", target, SpringConfig::snappy());
            div().bg(Color::rgba(0.3, 0.5, 0.9, 1.0)).scale(s)
        })
}
```

For more control, use `ctx.use_animated_value_with_config()` which returns `SharedAnimatedValue` (`Arc<Mutex<AnimatedValue>>`):

```rust
let scale = ctx.use_animated_value_with_config("scale", 1.0, SpringConfig::snappy());
scale.lock().unwrap().set_target(1.05);
let current = scale.lock().unwrap().get();
```

### Motion Containers

`motion()` wraps content with enter/exit/stagger animations:

```rust
use blinc_layout::motion::motion;

// Fade in on enter
motion().fade_in(300).child(content)

// Slide in from direction
motion().slide_in(SlideDirection::Right, 200).child(content)

// Stagger children (delay_ms between each, animation preset)
motion()
    .stagger(StaggerConfig::new(50, AnimationPreset::fade_in(300)))
    .child(list_items)

// Combined enter animations
motion()
    .fade_in(300)
    .slide_in(SlideDirection::Up, 200)
    .child(modal_content)
```

### FLIP-Style Layout Animations (`animate_bounds`)

CSS handles most FLIP-style animations via `transform` transitions (translate, scale, rotate — no layout reflow). For **layout-driven** changes that CSS can't express — such as element reordering, accordion content expanding, or container sizes changing programmatically — use `animate_bounds()` with `VisualAnimationConfig`:

```rust
use blinc_layout::visual_animation::VisualAnimationConfig;

// Accordion: animate height changes
div()
    .animate_bounds(VisualAnimationConfig::height().with_key("accordion-1"))
    .overflow_clip()
    .child(expandable_content)

// Reorder list: animate position changes
div()
    .animate_bounds(VisualAnimationConfig::position())
    .child(sortable_item)

// Animate all bounds with custom spring
div()
    .animate_bounds(
        VisualAnimationConfig::all()
            .with_spring(SpringConfig::wobbly())
            .with_key("card")
    )
    .child(content)
```

**VisualAnimationConfig presets:**
- `::height()` — accordion/collapsible (most common)
- `::width()` — sidebars
- `::size()` — both width and height
- `::position()` — reordering (no clipping)
- `::all()` — position + size
- Chain `.gentle()`, `.wobbly()`, `.stiff()`, `.snappy()` for spring presets
- Use `.with_key("stable-id")` inside `stateful` components for persistence across rebuilds

**When to use what:**
| Need | Use |
|------|-----|
| Hover/focus color change | CSS `transition: all 300ms ease` + `:hover` |
| FLIP-style lift/scale/slide | CSS `transition: transform 300ms ease` + `:hover` |
| Continuous looping animation | CSS `@keyframes` + `animation:` |
| Reveal/clip animation | CSS `@keyframes` with `clip-path` |
| Accordion expand/collapse | `animate_bounds(VisualAnimationConfig::height())` |
| Sortable list reorder | `animate_bounds(VisualAnimationConfig::position())` |
| Enter/exit/stagger | `motion().fade_in(300)` |
| Spring-physics per state | `ctx.use_spring()` inside `on_state` |

---

## Events

```rust
div()
    .on_click(|ctx| { /* ctx.local_x, ctx.local_y */ })
    .on_hover_enter(|ctx| { /* mouse entered */ })
    .on_hover_leave(|ctx| { /* mouse left */ })
    .on_key_down(|ctx| {
        if (ctx.ctrl || ctx.meta) && ctx.key_code == 83 { /* Ctrl/Cmd+S */ }
    })
    .on_drag(|ctx| { /* ctx.drag_delta_x, ctx.drag_delta_y */ })
    .on_scroll(|ctx| { /* ctx.scroll_delta_x, ctx.scroll_delta_y */ })
    .on_focus(|ctx| { /* element focused */ })
    .on_blur(|ctx| { /* element blurred */ })
    .on_mount(|ctx| { /* added to tree */ })
    .on_unmount(|ctx| { /* removed from tree */ })
```

All handlers receive `EventContext` with fields: `mouse_x`, `mouse_y`, `local_x`, `local_y`, `key_code`, `key_char`, `shift`, `ctrl`, `alt`, `meta`, `drag_delta_x`, `drag_delta_y`, `scroll_delta_x`, `scroll_delta_y`.

---

## Widgets

Built-in widgets in `blinc_layout::widgets`. Note: `text_input` and `text_area` take shared state handles, not raw strings.

```rust
use blinc_layout::widgets::*;

// Text input — create shared state first, then pass reference
let input_data = text_input_data_with_placeholder("Enter name...");
text_input(&input_data).id("my-input").on_change(|val| { /* val: &str */ })

// Text area — same pattern
let area_state = text_area_state_with_placeholder("Enter description...");
text_area(&area_state).id("my-area").rows(5)

// Scroll container
scroll().w(400.0).h(300.0).child(long_content)

// Radio group — takes &State<String>
let selected = ctx.use_state_keyed("radio", || "opt1".to_string());
radio_group(&selected)
    .option("opt1", "Option 1")
    .option("opt2", "Option 2")

// Checkbox — takes &State<bool>
let checked = ctx.use_state_keyed("check", || false);
checkbox(&checked).label("Enable feature")
```

---

## Component Patterns

### Function Components

```rust
fn card(title: &str) -> Div {
    div()
        .id("card")  // style via CSS: #card { padding: 16px; border-radius: 12px; ... }
        .child(text(title).size(18.0).weight(FontWeight::SemiBold).color(Color::WHITE))
}
```

### Components with Children

```rust
fn card_with_content<E: ElementBuilder>(title: &str, content: E) -> Div {
    div()
        .class("card")  // style via CSS: .card { padding: 16px; ... }
        .flex_col()
        .child(text(title).size(18.0).weight(FontWeight::SemiBold).color(Color::WHITE))
        .child(content)
}
```

### BlincComponent Derive

For type-safe hooks (state + animation) that prevent key collisions:

```rust
use blinc_macros::BlincComponent;

#[derive(BlincComponent)]
struct Counter {
    count: i32,        // unmarked fields → State<T> (generates Counter::use_count)
    #[animation]
    scale: f32,        // #[animation] fields → SharedAnimatedValue (generates Counter::use_scale)
}

fn counter_demo(ctx: &WindowedContext) -> impl ElementBuilder {
    let count = Counter::use_count(ctx, 0);
    let scale = Counter::use_scale(ctx, 1.0, SpringConfig::snappy());
    // ...
}
```

Note: There is no `#[state]` attribute. Fields without any attribute are automatically state fields.

---

## Glass Material

```rust
div().glass()  // Default frosted glass effect

// Presets
use blinc_layout::element::GlassMaterial;

div().material(Material::Glass(GlassMaterial::ultra_thin()))
div().material(Material::Glass(GlassMaterial::thin()))
div().material(Material::Glass(GlassMaterial::regular()))
div().material(Material::Glass(GlassMaterial::thick()))
div().material(Material::Glass(GlassMaterial::frosted()))
div().material(Material::Glass(GlassMaterial::card()))

// Custom
div().material(Material::Glass(
    GlassMaterial::new()
        .blur(20.0)
        .tint(Color::rgba(1.0, 1.0, 1.0, 0.1))
        .saturation(1.2)
        .noise(0.03)
))
```

---

## Common Pitfalls

1. **Don't use `stateful(handle)`** — this is the deprecated API. Use `stateful::<S>()`.
2. **`on_state` returns `Div`** — it's a builder, not mutation. Return `div().bg(bg)`, don't call `div.set_bg()`.
3. **Signal is `Copy`** — capture directly in closures, no need to clone.
4. **`BlincContextState::get()`** — use this for signal updates inside event closures, not `ctx`.
5. **CSS loaded once** — guard `ctx.add_css()` with a boolean flag to avoid re-adding on each rebuild.
6. **`blinc_cn` is a component library** — NOT a className utility. It provides prebuilt shadcn/ui-style widgets.
7. **Prefer CSS for all styling** — use `#id:hover { ... }` and `var(--token)` in a stylesheet instead of hardcoding colors in builder methods. Use `stateful::<ButtonState>()` only when you need runtime logic (not just visual changes).
8. **Spacing units differ from pixels** — `.p()`, `.px()`, `.py()`, `.m()`, `.mx()`, `.my()`, `.gap()` use **4px units** (`.p(4.0)` = 16px). `.w()`, `.h()`, `.left()`, `.top()` use raw pixels. **Prefer CSS for spacing** (`padding: 16px; gap: 12px;`) to avoid this confusion.
9. **Widget constructors take shared state** — `text_input(&data)` and `text_area(&state)` take `&SharedTextInputData` / `&SharedTextAreaState`, not strings. Create state first with `text_input_data_with_placeholder("...")`.
10. **`use_spring` returns `f32`** — it's a convenience that sets target and returns current value. Don't try to `.lock()` on it. For `SharedAnimatedValue`, use `use_animated_value_with_config()` instead.
11. **`#[state]` attribute doesn't exist** — in `#[derive(BlincComponent)]`, unmarked fields are state. Only `#[animation]` is an explicit attribute.
12. **The trait is `StateTransitions`** (plural) — not `StateTransition`.
13. **`text()` has no `.class()` method** — use `.id("name")` with `#name { ... }` CSS selectors to style text. If you need class-based styling, wrap in `div().class("name").child(text("..."))`.
