# Mobile Development

Blinc supports building native mobile applications for both Android and iOS platforms. The same Rust UI code runs on mobile with platform-specific rendering backends (Vulkan for Android, Metal for iOS).

## Cross-Platform Architecture

```text
┌─────────────────────────────────────────────────────────────┐
│                      Your Blinc App                          │
│         (Shared Rust UI code, state, animations)             │
└─────────────────────────────┬───────────────────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         │                    │                    │
    ┌────▼────┐         ┌─────▼─────┐        ┌────▼────┐
    │ Desktop │         │  Android  │        │   iOS   │
    │ (wgpu)  │         │ (Vulkan)  │        │ (Metal) │
    └─────────┘         └───────────┘        └─────────┘
```

## Key Features

- **Shared UI Code**: Write your UI once in Rust, deploy everywhere
- **Native Performance**: GPU-accelerated rendering via Vulkan/Metal
- **Touch Support**: Full multi-touch gesture handling
- **Reactive State**: Same reactive state system as desktop
- **Animations**: Spring physics and keyframe animations work seamlessly

## Supported Platforms

| Platform | Backend | Min Version | Status |
|----------|---------|-------------|--------|
| Android  | Vulkan  | API 24 (7.0) | Stable |
| iOS      | Metal   | iOS 15+     | Stable |

## Project Structure

A typical Blinc mobile project looks like this:

```text
my-app/
├── Cargo.toml           # Rust dependencies
├── blinc.toml           # Blinc project config
├── src/
│   └── main.rs          # Shared UI code
├── platforms/
│   ├── android/         # Android-specific files
│   │   ├── app/
│   │   │   └── src/main/
│   │   │       ├── AndroidManifest.xml
│   │   │       └── kotlin/.../MainActivity.kt
│   │   └── build.gradle.kts
│   └── ios/             # iOS-specific files
│       ├── BlincApp/
│       │   ├── AppDelegate.swift
│       │   ├── BlincViewController.swift
│       │   └── Info.plist
│       └── BlincApp.xcodeproj/
└── build-android.sh     # Build scripts
```

## Quick Start

### 1. Create a new mobile project

```bash
blinc new my-app --template rust
cd my-app
```

### 2. Write your UI

```rust
use blinc_app::prelude::*;

fn app(ctx: &mut WindowedContext) -> impl ElementBuilder {
    let count = ctx.use_signal(|| 0);

    div()
        .width(ctx.width)
        .height(ctx.height)
        .bg(0x1a1a2e)
        .justify_center()
        .align_center()
        .child(
            button("Tap me!")
                .on_click(move |_| count.set_rebuild(count.get() + 1))
        )
        .child(
            text(format!("Count: {}", count.get()))
                .color(0xffffff)
        )
}
```

### 3. Build and run

```bash
# Android
blinc run android

# iOS
blinc run ios
```

## Next Steps

- [Android Development](./android.md) - Set up Android toolchain and build
- [iOS Development](./ios.md) - Set up iOS toolchain and build
- [CLI Reference](./cli.md) - Full CLI command reference
