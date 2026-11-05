# Project Structure

Keep application data, reusable components and the root view in separate
modules. A small application can start with this layout:

```text
my-app/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── app.rs
│   ├── app.css
│   ├── data.rs
│   └── components/
│       ├── mod.rs
│       └── project_card.rs
└── assets/
    ├── fonts/
    └── images/
```

Use the `blinc` dependency with `cn` from [Installation](installation.md).

## Entry point

The entry point installs the theme and application stylesheet, then hands
control to the desktop runner:

```rust
// src/main.rs
use blinc::{app::windowed::WindowedApp, cn, prelude::*};

mod app;
mod components;
mod data;

fn main() -> Result<()> {
    let theme = cn::cn_bundle().with_css(include_str!("app.css"));
    WindowedApp::run_with_theme(
        WindowConfig::default(), theme, ColorScheme::Dark, app::build,
    )
}
```

## Application data

Keep reactive handles together when several components share the same data.
Keyed Signal hooks retain their values across UI builds:

```rust
// src/data.rs
use blinc::{
    app::windowed::WindowedContext,
    core::Signal,
    layout::widgets::text_input::{SharedTextInputData, text_input_data},
};

#[derive(Clone)]
pub struct AppSignals {
    pub project_name: Signal<String>,
    pub name_input: SharedTextInputData,
}

impl AppSignals {
    pub fn new(ctx: &WindowedContext) -> Self {
        let project_name = ctx.use_signal_keyed("project.name", || "Website".to_string());
        let name_input = ctx.use_signal_keyed("project.editor", move || {
            let input = text_input_data();
            input.lock().unwrap().value = project_name.get();
            input
        }).get();
        Self { project_name, name_input }
    }
}
```

`AppSignals` contains handles, not copies of their current values. Passing a
Signal to several components connects them to the same source.

## Reusable components

A component accepts a value, Signal or Computed through `IntoReactive` and
forwards it to a bound property. Its CSS classes are the restyling surface:

```rust
// src/components/project_card.rs
use blinc::{cn, layout::IntoReactive, prelude::*};

pub fn project_card(title: impl IntoReactive<String>) -> Div {
    div().class("project-card")
        .child(cn::heading(2, title))
}
```

Re-export the component from its module:

```rust
// src/components/mod.rs
mod project_card;
pub use project_card::project_card;
```

## Root view

The input retains its editor model for content, cursor and selection. Its
change callback updates the project-name Signal, which the card already binds:

```rust
// src/app.rs
use blinc::{app::windowed::WindowedContext, cn, prelude::*};
use crate::{components::project_card, data::AppSignals};

pub fn build(ctx: &mut WindowedContext) -> Div {
    let data = AppSignals::new(ctx);
    let name = data.project_name;
    div().class("workspace")
        .child(cn::input(&data.name_input).label("Project name")
            .on_change(move |value| name.set(value.to_string())))
        .child(project_card(name))
}
```

## Application stylesheet

```css
/* src/app.css */
.workspace {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  padding: var(--space-6);
  background: var(--background);
}

.project-card {
  padding: var(--space-4);
  border: 1px solid var(--border);
  border-radius: var(--radius-default);
  background: var(--surface);
}
```

Use a separate stylesheet per screen or component when the application grows,
and load those sheets after the theme. Keep component classes documented so
callers can restyle them. For larger typed component APIs, use
[`BlincComponent`](../components/blinc-component.md) to generate reactive props
and named Signal hooks.
