# iOS Development

This guide covers setting up your environment and building Blinc apps for iOS.

## Prerequisites

### 1. Xcode

Install Xcode 15+ from the Mac App Store or Apple Developer website.

```bash
# Verify installation
xcode-select -p
```

### 2. Rust iOS Targets

```bash
rustup target add aarch64-apple-ios        # Device (arm64)
rustup target add aarch64-apple-ios-sim    # Simulator (Apple Silicon)
rustup target add x86_64-apple-ios         # Simulator (Intel)
```

## Building

### Build Script

Create a build script `build-ios.sh`:

```bash
#!/bin/bash
set -e

MODE=${1:-debug}
PROJECT_NAME="my_app"

if [ "$MODE" = "release" ]; then
    CARGO_FLAGS="--release"
    TARGET_DIR="release"
else
    CARGO_FLAGS=""
    TARGET_DIR="debug"
fi

# Build for device
cargo build --target aarch64-apple-ios $CARGO_FLAGS

# Build for simulator (Apple Silicon)
cargo build --target aarch64-apple-ios-sim $CARGO_FLAGS

# Copy to libs directory
mkdir -p platforms/ios/libs/device
mkdir -p platforms/ios/libs/simulator

cp target/aarch64-apple-ios/$TARGET_DIR/lib${PROJECT_NAME}.a \
   platforms/ios/libs/device/

cp target/aarch64-apple-ios-sim/$TARGET_DIR/lib${PROJECT_NAME}.a \
   platforms/ios/libs/simulator/
```

### Building

```bash
# Debug build
./build-ios.sh

# Release build
./build-ios.sh release
```

### Xcode

1. Open `platforms/ios/BlincApp.xcodeproj`
2. Select your target (device or simulator)
3. Press Cmd+R to build and run

## Project Configuration

### Cargo.toml

```toml
[lib]
name = "my_app"
crate-type = ["cdylib", "staticlib"]

[target.'cfg(target_os = "ios")'.dependencies]
blinc_app = { version = "0.1", features = ["ios"] }
blinc_platform_ios = "0.1"
```

### Xcode Build Settings

In your Xcode project:

1. **Link the static library**:
   - Build Phases → Link Binary With Libraries
   - Add `libmy_app.a` from `libs/device/` or `libs/simulator/`

2. **Set the bridging header**:
   - Build Settings → Swift Compiler - General
   - Objective-C Bridging Header: `BlincApp/Blinc-Bridging-Header.h`

3. **Add required frameworks**:
   - Metal.framework
   - MetalKit.framework
   - QuartzCore.framework

## Swift Integration

### Bridging Header

The bridging header (`Blinc-Bridging-Header.h`) declares the C FFI functions:

```c
// Context lifecycle
IOSRenderContext* blinc_create_context(uint32_t width, uint32_t height, double scale);
void blinc_destroy_context(IOSRenderContext* ctx);

// Rendering
bool blinc_needs_render(IOSRenderContext* ctx);
void blinc_build_frame(IOSRenderContext* ctx);
bool blinc_render_frame(IOSGpuRenderer* gpu);

// Input
void blinc_handle_touch(IOSRenderContext* ctx, uint64_t id, float x, float y, int32_t phase);
```

### View Controller

The `BlincViewController` manages:

- CADisplayLink for 60fps frame timing
- Metal layer for GPU rendering
- Touch event forwarding to Rust

## Touch Event Handling

iOS touch events are routed through the view controller:

| iOS Phase | Blinc Event |
|-----------|-------------|
| touchesBegan | pointer_down |
| touchesMoved | pointer_move |
| touchesEnded | pointer_up + pointer_leave |
| touchesCancelled | pointer_leave |

The `pointer_leave` after `pointer_up` is important for proper button state transitions on touch devices.

## Debugging

### Console Logs

View Rust logs in Xcode's console or use Console.app with a filter:

```
subsystem:com.blinc.my_app
```

### Common Issues

**"Library not found: -lmy_app"**

Run the build script first:

```bash
./build-ios.sh
```

**Black screen on simulator**

1. Ensure you built for the correct simulator target (`aarch64-apple-ios-sim`)
2. Verify the library is in `libs/simulator/`
3. Check Xcode console for Metal initialization errors

**Touch events not working**

1. Verify `blinc_create_context` succeeds (check console logs)
2. Ensure `ios_app_init()` is called before creating the context
3. Check that touch coordinates are in logical points, not pixels

## Performance Tips

1. **Use release builds** for performance testing:
   ```bash
   ./build-ios.sh release
   ```

2. **Enable LTO** in Cargo.toml:
   ```toml
   [profile.release]
   lto = "thin"
   opt-level = "z"
   strip = true
   ```

3. **Test on real devices** - simulators use software rendering for some operations

4. **Profile with Instruments** - use Xcode's Metal debugger for GPU analysis
