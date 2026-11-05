# Your First App

Build a counter using the `blinc` dependency from [Installation](installation.md).
The UI is composed in Rust, the appearance is defined in CSS, and a Computed
keeps the displayed count bound to its Signal.

## Application

Create `src/main.rs`:

```rust
use blinc::{
    app::windowed::{WindowedApp, WindowedContext},
    cn,
    layout::IntoReactive,
    prelude::*,
};

fn main() -> Result<()> {
    let theme = cn::cn_bundle().with_css(include_str!("app.css"));
    let config = WindowConfig {
        title: "Counter".into(),
        width: 420,
        height: 320,
        ..Default::default()
    };
    WindowedApp::run_with_theme(config, theme, ColorScheme::Dark, build_ui)
}

fn build_ui(ctx: &mut WindowedContext) -> Div {
    let count = ctx.use_signal(0i32);
    let displayed = computed(move |g| g.get(count).unwrap_or_default().to_string());

    div().class("counter")
        .child(cn::h2("Counter"))
        .child(Text::bound(displayed.into_reactive()).class("counter-value"))
        .child(div().class("counter-actions")
            .child(counter_button(count, "Decrease", -1))
            .child(counter_button(count, "Increase", 1)))
}

fn counter_button(count: Signal<i32>, label: &'static str, delta: i32) -> impl ElementBuilder {
    cn::button(label).on_click(move |_| {
        count.update(|value| value + delta);
    })
}
```

## Stylesheet

Create `src/app.css` beside `main.rs`:

```css
.counter {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-4);
  background: var(--background);
}

.counter-value {
  font-size: 48px;
  font-weight: 700;
  color: var(--primary);
}

.counter-actions {
  display: flex;
  gap: var(--space-2);
}
```

`cn_bundle()` installs the built-in theme and component rules. The extra
stylesheet owns the application's layout and count display. Buttons retain
their themed hover, press, disabled and keyboard-focus styles.

## How updates work

`ctx.use_signal(0i32)` retains the count across UI builds. `Signal` is `Copy`,
so both event closures can capture the same handle directly.

The Computed reads the count and formats it. `Text::bound` subscribes to that
value; changing the count replaces the displayed text in place. There is no
stateful subtree or dependency list to rebuild for a text update.

`counter_button` receives the Signal and updates it in its click handler.
The component library routes pointer and keyboard activation to that handler.
Use Effects for side effects such as persistence or application integrations;
keep derived display values in Computeds.

## Next steps

- [State Management](../core/state.md): Signals, Computeds, Effects and property bindings.
- [CSS Styling](../core/css-styling.md): selectors, theme variables and motion.
- [Reusable Components](../components/blinc-component.md): typed reactive props and hooks.
- [Lists & Conditional Content](../core/lists-branches.md): updates that change children.
