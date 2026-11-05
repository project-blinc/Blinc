# Blinc

Blinc is a GPU-accelerated UI framework for Rust. Build desktop, mobile and
browser interfaces with declarative Rust components, CSS styling and reactive
data bindings.

Compose your interface in Rust, give it a visual language with stylesheets,
and bind its properties to Signals and Computeds. Blinc handles layout, input,
text shaping and GPU rendering across platforms.

[Website](https://blinc.rs) · [Components](https://blinc.rs/components/) ·
[Documentation](https://blinc.rs/docs/) ·
[GitHub](https://github.com/project-blinc/Blinc)

## What you can build with

- **Composable UI:** layout primitives and components for forms, navigation,
  dialogs, charts and data display.
- **CSS styling:** control layout, colours, typography, corner shapes,
  transitions and keyframe animations. Built-in themes support light and dark
  colour schemes.
- **Reactive properties:** Signals hold values, Computeds derive them, and
  Effects connect changes to application logic. Bound properties update in
  place; reactive lists and branches handle structural changes.
- **GPU drawing:** SDF shapes, text, images, SVG, glass effects, custom canvas
  drawing and flow shaders share the rendering pipeline.
- **Cross-platform applications:** desktop windows, Android and iOS apps,
  WebAssembly browser applications and headless rendering.

## Get started

Blinc requires Rust 1.89 or newer. Desktop windowing is enabled by default;
enable `cn` for the component library:

```toml
[dependencies]
blinc = { version = "0.6", features = ["cn"] }
```

```rust,no_run
# #[cfg(all(feature = "cn", feature = "windowed"))]
# mod example {
use blinc::{app::windowed::WindowedApp, cn, prelude::*};

fn main() -> Result<()> {
    let theme = cn::cn_bundle().with_css(r#"
        .workspace {
            width: 100%;
            height: 100%;
            display: flex;
            align-items: center;
            justify-content: center;
            background: var(--background);
        }
    "#);

    WindowedApp::run_with_theme(
        WindowConfig::default(), theme, ColorScheme::Dark,
        |ctx| {
            let clicks = ctx.use_signal(0u32);
            let label = computed(move |g| {
                format!("Clicks: {}", g.get(clicks).unwrap_or_default())
            });

            div().class("workspace")
                .child(cn::button(label).on_click(move |_| {
                    clicks.update(|n| n + 1);
                }))
        },
    )
}
# }
# fn main() {}
```

The button's label follows `clicks` through a Computed, while its appearance
comes from the component stylesheet and theme. Use application CSS to restyle
the documented component classes.

## Reusable components

`BlincComponent` generates reactive props and persistent Signal hooks for a
component's fields. Pass values, Signals or Computeds into those props and
forward them to the UI:

```rust
use blinc::{core::Signal, prelude::*};

#[derive(BlincComponent)]
struct ProjectTitle {
    label: String,
}

fn project_title(label: Signal<String>) -> Div {
    let props = ProjectTitle::props(label);
    div().class("project-title")
        .child(Text::bound(props.label).class("project-title-label"))
}
```

## Features

`windowed` is enabled by default. For a browser application:

```toml
blinc = { version = "0.6", default-features = false, features = ["web", "cn"] }
```

For headless use, set `default-features = false` without choosing a platform.

| Feature | Enables |
|---------|---------|
| `windowed` | Desktop runner |
| `web`, `android`, `ios` | Corresponding application platform feature |
| `cn` | Themed component library |
| `svg` | SVG loading and rasterization APIs |
| `icons` | Lucide icon data and SVG helpers |
| `media` | Media module and layout playback widgets |
| `clipboard-image` | Image clipboard support |
| `svg-raster-images` | SVG module and embedded raster-image decoding |
| `icons-registry` | Icons module and name-based icon lookup |
| `theme-watcher` | System colour-scheme watcher |
| `dialogs`, `tray`, `notifications`, `hotkeys` | Corresponding desktop integration |
| `windowed-full` | Desktop runner plus dialogs, tray, notifications and hotkeys |
| `hot-reload` | Application hot reload |

The `svg` flag exposes the SVG module; the application renderer also uses SVG
internally. Components depend on icon data, while `icons` exposes that library
for direct use.

## API modules

The `prelude` includes application builders, core types, `signal`, `computed`
and `effect`. Framework APIs are also available through these modules:

| Module | Crate |
|--------|-------|
| `app` | `blinc_app` |
| `layout` | `blinc_layout` |
| `core` | `blinc_core` |
| `animation` | `blinc_animation` |
| `css` | `blinc_css` |
| `flow` | `blinc_flow` |
| `gpu` | `blinc_gpu` |
| `image` | `blinc_image` |
| `macros` | `blinc_macros` |
| `paint` | `blinc_paint` |
| `platform` | `blinc_platform` |
| `platform_web` | Browser asset and input helpers, with `web` on WebAssembly |
| `router` | `blinc_router` |
| `text` | `blinc_text` |
| `theme` | `blinc_theme` |
| `cn` | `blinc_cn`, with `cn` |
| `svg` | `blinc_svg`, with `svg` |
| `icons` | `blinc_icons`, with `icons` |
| `media` | `blinc_media`, with `media` |

`BlincComponent` and `flow!` support a single `blinc` dependency, including one
renamed in Cargo.toml. The individual `blinc_*` crates remain available.
