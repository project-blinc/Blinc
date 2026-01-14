# {{project_name}} iOS Platform

A Blinc UI application for iOS with Metal rendering.

## Requirements

- macOS 13+ (Ventura or later)
- Xcode 15+
- Rust toolchain with iOS targets
- iOS 15+ deployment target

## Setup

### 1. Install iOS Rust targets

```bash
rustup target add aarch64-apple-ios
rustup target add aarch64-apple-ios-sim
```

### 2. Build the Rust library

From the project root directory:

```bash
# Debug build (faster compile, for development)
./build-ios.sh

# Release build (optimized, for testing performance)
./build-ios.sh release
```

This compiles the Blinc Rust library as a static library for both device and simulator.

### 3. Open in Xcode

```bash
open platforms/ios/BlincApp.xcodeproj
```

### 4. Build and Run

1. Select your target (simulator or device)
2. Press Cmd+R to build and run

## Project Structure

```text
platforms/ios/
├── BlincApp.xcodeproj/     # Xcode project
├── BlincApp/
│   ├── AppDelegate.swift        # App entry point
│   ├── BlincViewController.swift # Main view controller
│   ├── BlincMetalView.swift     # Metal layer view
│   ├── Blinc-Bridging-Header.h  # Rust FFI declarations
│   └── Info.plist               # App configuration
├── libs/
│   ├── device/                  # arm64 library (real devices)
│   │   └── lib{{project_name_snake}}.a
│   └── simulator/               # arm64 library (Apple Silicon simulators)
│       └── lib{{project_name_snake}}.a
└── README.md
```

## How It Works

### Swift - Rust Integration

The integration uses C FFI (Foreign Function Interface):

1. **Bridging Header** (`Blinc-Bridging-Header.h`) declares the C functions exported by Rust
2. **Static Library** (`lib{{project_name_snake}}.a`) contains the compiled Rust code
3. Swift calls these C functions to control the Blinc UI

### Rendering Pipeline

1. `CADisplayLink` fires at ~60fps
2. Swift checks `blinc_needs_render()` to see if UI changed
3. If needed, `blinc_build_frame()` rebuilds the UI tree
4. `blinc_render_frame()` renders to the Metal surface

### Touch Events

Touch events are forwarded from Swift to Rust:

```swift
override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
    for touch in touches {
        let point = touch.location(in: view)
        blinc_handle_touch(ctx, touchId, Float(point.x), Float(point.y), 0)
    }
}
```

## Troubleshooting

### "Library not found: -l{{project_name_snake}}"

Make sure you've run `./build-ios.sh` first to compile the Rust library.

### Simulator shows black screen

1. Check that you built for the simulator target (`aarch64-apple-ios-sim`)
2. Verify the library is in `libs/simulator/`
3. Check Xcode console for error messages

### Touch events not working

Ensure:

1. The view controller is receiving touch events (check `touchesBegan` is called)
2. The render context was created successfully
3. A UI builder is registered via `blinc_set_ui_builder()`

## Architecture

```text
+--------------------------------------------------+
|                  Swift/UIKit                      |
|  +----------------------------------------------+ |
|  |          BlincViewController                 | |
|  |  - CADisplayLink (60fps timer)               | |
|  |  - Touch event forwarding                    | |
|  |  - Metal view management                     | |
|  +----------------------------------------------+ |
|                       |                           |
|                 C FFI | (bridging header)         |
|                       v                           |
|  +----------------------------------------------+ |
|  |               Rust/Blinc                     | |
|  |  - IOSRenderContext (UI state)               | |
|  |  - IOSGpuRenderer (Metal/wgpu)               | |
|  |  - EventRouter (touch -> UI)                 | |
|  |  - AnimationScheduler                        | |
|  +----------------------------------------------+ |
+--------------------------------------------------+
```
