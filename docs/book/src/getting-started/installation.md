# Installation

Blinc requires Rust 1.89 or newer and a GPU supported by Vulkan, Metal or
DirectX 12. Browser applications use WebAssembly and WebGPU.

## Add Blinc

```toml
[dependencies]
blinc = { version = "0.6", features = ["cn"] }
```

The `blinc` package exposes the framework through `blinc::app`,
`blinc::layout`, `blinc::core`, `blinc::theme` and the other API modules.
`blinc::prelude::*` imports the common builders, core types and reactive
functions. The `cn` feature adds the component library.

## Verify the installation

Create `src/main.rs`:

```rust
use blinc::{app::windowed::WindowedApp, cn, prelude::*};

fn main() -> Result<()> {
    let theme = cn::cn_bundle().with_css(r#"
        .welcome {
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
        |_ctx| div().class("welcome").child(cn::h1("Your workspace")),
    )
}
```

Run `cargo run`. A window opens using Blinc's built-in theme and the
application's stylesheet. Continue with [Your First App](first-app.md) to add
Signals, Computeds and interactive components.

## Feature flags

Desktop windowing is enabled by default. Other platforms disable defaults
and select their runner:

```toml
# Browser application
blinc = { version = "0.6", default-features = false, features = ["web", "cn"] }

# Android application
blinc = { version = "0.6", default-features = false, features = ["android", "cn"] }

# iOS application
blinc = { version = "0.6", default-features = false, features = ["ios", "cn"] }
```

| Feature | API |
|---------|-----|
| `windowed` | Desktop runner; enabled by default |
| `web` | Browser runner and `blinc::platform_web` helpers |
| `android`, `ios` | Mobile runners |
| `cn` | Components through `blinc::cn` |
| `svg` | SVG loading and rasterization through `blinc::svg` |
| `icons` | Lucide icon data and SVG helpers through `blinc::icons` |
| `media` | `blinc::media` and the layout playback widgets |
| `dialogs`, `tray`, `notifications`, `hotkeys` | Desktop system integrations |
| `windowed-full` | Desktop runner and all four system integrations |
| `clipboard-image` | Image clipboard support |
| `svg-raster-images` | SVG module with embedded raster decoding |
| `icons-registry` | Icons module with name-based lookup |
| `theme-watcher` | System colour-scheme watcher |
| `hot-reload` | Application hot reload |

For headless rendering, use `default-features = false` without a runner
feature. Add `icons`, `svg` or `media` when using the corresponding examples
in this book. Module flags expose the public API; the application renderer
also uses SVG internally, and components depend on icon data.

## Platform setup

macOS uses Metal. Windows uses DirectX 12 or Vulkan with supported GPU
drivers. Linux needs its graphics and window-system development libraries;
for a Vulkan setup:

```bash
# Ubuntu / Debian
sudo apt install libvulkan-dev

# Fedora
sudo dnf install vulkan-devel

# Arch
sudo pacman -S vulkan-icd-loader
```

Platform-specific setup is covered in the [Web](../web/setup.md),
[Android](../mobile/android.md) and [iOS](../mobile/ios.md) guides.
