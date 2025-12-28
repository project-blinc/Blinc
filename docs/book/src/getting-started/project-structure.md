# Project Structure

## Recommended Layout

For a typical Blinc application:

```
my-app/
├── Cargo.toml
├── src/
│   ├── main.rs           # Application entry point
│   ├── app.rs            # Main UI builder
│   ├── components/       # Reusable UI components
│   │   ├── mod.rs
│   │   ├── header.rs
│   │   ├── sidebar.rs
│   │   └── card.rs
│   ├── screens/          # Full-page views
│   │   ├── mod.rs
│   │   ├── home.rs
│   │   └── settings.rs
│   └── state/            # Application state
│       ├── mod.rs
│       └── app_state.rs
└── assets/               # Static assets
    ├── fonts/
    ├── images/
    └── icons/
```

## Entry Point Pattern

```rust
// src/main.rs
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};

mod app;
mod components;
mod screens;
mod state;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let config = WindowConfig {
        title: "My App".to_string(),
        width: 1200,
        height: 800,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| app::build(ctx))
}
```

## Component Organization

### Simple Component

```rust
// src/components/card.rs
use blinc_app::prelude::*;

pub fn card(title: &str) -> Div {
    div()
        .p(16.0)
        .rounded(12.0)
        .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
        .flex_col()
        .gap(8.0)
        .child(
            text(title)
                .size(18.0)
                .weight(FontWeight::SemiBold)
                .color(Color::WHITE)
        )
}
```

### Component with Children

```rust
// src/components/card.rs
pub fn card_with_content<E: ElementBuilder>(title: &str, content: E) -> Div {
    div()
        .p(16.0)
        .rounded(12.0)
        .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
        .flex_col()
        .gap(8.0)
        .child(
            text(title)
                .size(18.0)
                .weight(FontWeight::SemiBold)
                .color(Color::WHITE)
        )
        .child(content)
}
```

### Stateful Component with BlincComponent

```rust
// src/components/animated_card.rs
use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;
use blinc_animation::SpringConfig;
use std::sync::Arc;

#[derive(BlincComponent)]
pub struct AnimatedCard {
    #[animation]
    scale: f32,
    #[animation]
    opacity: f32,
}

pub fn animated_card(ctx: &WindowedContext, title: &str) -> Div {
    let scale = AnimatedCard::use_scale(ctx, 1.0, SpringConfig::snappy());
    let opacity = AnimatedCard::use_opacity(ctx, 1.0, SpringConfig::gentle());

    let hover_scale = Arc::clone(&scale);
    let leave_scale = Arc::clone(&scale);

    div()
        .p(16.0)
        .rounded(12.0)
        .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
        .transform(Transform::scale(scale.lock().unwrap().get()))
        .opacity(opacity.lock().unwrap().get())
        .on_hover_enter(move |_| {
            hover_scale.lock().unwrap().set_target(1.05);
        })
        .on_hover_leave(move |_| {
            leave_scale.lock().unwrap().set_target(1.0);
        })
        .child(text(title).size(18.0).color(Color::WHITE))
}
```

## Screen Organization

```rust
// src/screens/home.rs
use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;
use crate::components::{header, card};

pub fn home_screen(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
        .flex_col()
        .child(header::header(ctx))
        .child(
            div()
                .flex_1()
                .p(24.0)
                .flex_col()
                .gap(16.0)
                .child(card("Welcome"))
                .child(card("Getting Started"))
        )
}
```

## State Management Patterns

### Global App State

```rust
// src/state/app_state.rs
use blinc_core::reactive::Signal;
use blinc_app::windowed::WindowedContext;

pub struct AppState {
    pub user_name: Signal<String>,
    pub theme: Signal<Theme>,
    pub sidebar_open: Signal<bool>,
}

impl AppState {
    pub fn new(ctx: &WindowedContext) -> Self {
        Self {
            user_name: ctx.use_signal(String::new()),
            theme: ctx.use_signal(Theme::Dark),
            sidebar_open: ctx.use_signal(true),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Theme {
    Light,
    Dark,
}
```

### Using App State

```rust
// src/app.rs
use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;
use crate::state::AppState;
use crate::screens;

pub fn build(ctx: &WindowedContext) -> impl ElementBuilder {
    let state = AppState::new(ctx);

    div()
        .w(ctx.width)
        .h(ctx.height)
        .flex_row()
        .child(sidebar(ctx, &state))
        .child(main_content(ctx, &state))
}

fn sidebar(ctx: &WindowedContext, state: &AppState) -> Div {
    let is_open = ctx.get(state.sidebar_open).unwrap_or(true);

    if is_open {
        div().w(250.0).h_full().bg(Color::rgba(0.1, 0.1, 0.15, 1.0))
            // ... sidebar content
    } else {
        div().w(0.0).h(0.0)
    }
}
```

## Module Re-exports

```rust
// src/components/mod.rs
mod card;
mod header;
mod sidebar;
mod animated_card;

pub use card::*;
pub use header::*;
pub use sidebar::*;
pub use animated_card::*;
```

```rust
// src/screens/mod.rs
mod home;
mod settings;

pub use home::*;
pub use settings::*;
```

## Asset Loading

For images and other assets, use relative paths from your project root:

```rust
// Load an image
image("assets/images/logo.png")
    .w(100.0)
    .h(100.0)
    .contain()

// Load an SVG icon
svg("assets/icons/menu.svg")
    .w(24.0)
    .h(24.0)
    .tint(Color::WHITE)
```

## Tips

1. **Keep components small** - Each component should do one thing well
2. **Use BlincComponent** - For any component with animations or complex state
3. **Separate concerns** - UI building, state management, and business logic
4. **Use the prelude** - `use blinc_app::prelude::*` imports common items
5. **Consistent naming** - Use `_screen` suffix for full-page views, no suffix for components
