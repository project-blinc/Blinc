# Installation

## Prerequisites

Blinc requires:
- **Rust 1.70+** (for stable async and other features)
- A GPU with Vulkan, Metal, or DX12 support

## Adding Blinc to Your Project

Add `blinc_app` to your `Cargo.toml`:

```toml
[dependencies]
blinc_app = { version = "0.1", features = ["windowed"] }
```

The `windowed` feature enables desktop windowing support. For headless rendering (e.g., server-side), omit this feature.

## Feature Flags

| Feature | Description |
|---------|-------------|
| `windowed` | Desktop window support via winit (default) |
| `android` | Android platform support |

## Verifying Installation

Create a simple test application:

```rust
// src/main.rs
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};

fn main() -> Result<()> {
    WindowedApp::run(WindowConfig::default(), |ctx| {
        div()
            .w(ctx.width)
            .h(ctx.height)
            .bg(Color::rgba(0.1, 0.1, 0.15, 1.0))
            .flex_center()
            .child(text("Blinc is working!").size(32.0).color(Color::WHITE))
    })
}
```

Run with:

```bash
cargo run
```

You should see a window with "Blinc is working!" displayed in the center.

## Recommended Dev Dependencies

For a better development experience, add these to your `Cargo.toml`:

```toml
[dev-dependencies]
tracing-subscriber = "0.3"
```

Then initialize logging in your app:

```rust
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    WindowedApp::run(/* ... */)
}
```

## Platform-Specific Notes

### macOS
No additional setup required. Blinc uses Metal for GPU rendering.

### Windows
Ensure you have up-to-date GPU drivers. Blinc uses DX12 by default, falling back to Vulkan.

### Linux
Install Vulkan development libraries:

```bash
# Ubuntu/Debian
sudo apt install libvulkan-dev

# Fedora
sudo dnf install vulkan-devel

# Arch
sudo pacman -S vulkan-icd-loader
```

### Android
See the Android platform guide for cross-compilation setup.
