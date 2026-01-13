# Blinc iOS Swift Integration

This directory contains Swift files for integrating Blinc into iOS applications.

## Files

- **Blinc-Bridging-Header.h** - C header declaring the Rust FFI functions
- **BlincViewController.swift** - Example UIViewController with CADisplayLink integration

## Setup

### 1. Build the Rust library

```bash
# Build for iOS simulator (arm64)
cargo build --release --target aarch64-apple-ios-sim -p blinc_app --features ios

# Build for iOS device (arm64)
cargo build --release --target aarch64-apple-ios -p blinc_app --features ios
```

### 2. Add to Xcode project

1. Add the static library (`libblinc_app.a`) to your Xcode project
2. Add the bridging header path to your build settings:
   - `Objective-C Bridging Header: path/to/Blinc-Bridging-Header.h`
3. Link required frameworks:
   - `Metal.framework`
   - `MetalKit.framework`
   - `QuartzCore.framework`

### 3. Use in your app

```swift
import UIKit

class MyBlincViewController: BlincViewController {

    override func renderFrame() {
        // This is called by CADisplayLink when rendering is needed
        // Build your UI and render with Metal here

        // Example:
        // 1. Access blincContext to call build_ui
        // 2. Get the render tree
        // 3. Render to the Metal layer
    }
}
```

## Touch Phase Values

The `blinc_handle_touch` function accepts a `phase` parameter:

| Value | Phase | Description |
|-------|-------|-------------|
| 0 | Began | Touch started |
| 1 | Moved | Touch position changed |
| 2 | Ended | Touch lifted |
| 3 | Cancelled | Touch cancelled by system |

## Thread Safety

- All Blinc FFI functions must be called from the main thread
- CADisplayLink callbacks run on the main thread by default
- The render context is not thread-safe

## Memory Management

- Call `blinc_create_context` once during initialization
- Call `blinc_destroy_context` when done (e.g., in `deinit`)
- The context pointer is owned by Swift after creation
