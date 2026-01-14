# blinc_platform_ios

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

iOS platform implementation for Blinc UI.

## Overview

`blinc_platform_ios` provides UIKit integration, Metal rendering, and touch input handling for iOS and iPadOS applications.

## Supported Platforms

- iOS 14.0+
- iPadOS 14.0+

## Features

- **UIKit Integration**: Native iOS view hierarchy
- **Metal Rendering**: Hardware-accelerated graphics
- **Touch Input**: Full multi-touch support
- **iOS Lifecycle**: Proper app state handling
- **Safe Area**: Automatic safe area inset handling

## Quick Start

```rust
use blinc_platform_ios::ios_main;

#[no_mangle]
pub extern "C" fn main() {
    ios_main(|ctx| {
        // Build your UI
        div()
            .w_full()
            .h_full()
            .child(text("Hello iOS!"))
    });
}
```

## Project Setup

### Cargo.toml

```toml
[lib]
crate-type = ["staticlib"]

[dependencies]
blinc_platform_ios = "0.1"
```

### Xcode Project

1. Create a new iOS project in Xcode
2. Add your Rust library as a dependency
3. Configure the bridging header
4. Set up the Metal view

### Info.plist

```xml
<key>UILaunchStoryboardName</key>
<string>LaunchScreen</string>
<key>UISupportedInterfaceOrientations</key>
<array>
    <string>UIInterfaceOrientationPortrait</string>
    <string>UIInterfaceOrientationLandscapeLeft</string>
    <string>UIInterfaceOrientationLandscapeRight</string>
</array>
```

## Touch Handling

```rust
fn handle_touch(event: TouchEvent) {
    match event.phase {
        TouchPhase::Began => {
            // Touch started
        }
        TouchPhase::Moved => {
            // Touch moved
        }
        TouchPhase::Ended => {
            // Touch ended
        }
        TouchPhase::Cancelled => {
            // Touch cancelled
        }
    }
}
```

## Safe Area

```rust
// Get safe area insets
let insets = ctx.safe_area_insets();

// Build UI respecting safe area
div()
    .pt(insets.top)
    .pb(insets.bottom)
    .pl(insets.left)
    .pr(insets.right)
    .child(/* content */)
```

## Lifecycle

```rust
ios_main(|ctx| {
    // App became active
    ctx.on_did_become_active(|| {
        // Resume animations, etc.
    });

    // App will resign active
    ctx.on_will_resign_active(|| {
        // Pause animations, save state
    });

    // App entered background
    ctx.on_did_enter_background(|| {
        // Save data
    });

    build_ui()
});
```

## Building

```bash
# Build for iOS Simulator
cargo build --target aarch64-apple-ios-sim

# Build for iOS Device
cargo build --target aarch64-apple-ios --release

# Build universal binary
cargo lipo --release
```

## Requirements

- Xcode 14+
- iOS SDK 14.0+
- Rust with iOS targets:
  ```bash
  rustup target add aarch64-apple-ios
  rustup target add aarch64-apple-ios-sim
  ```

## License

MIT OR Apache-2.0
