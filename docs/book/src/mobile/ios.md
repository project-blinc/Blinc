# iOS Project Setup

This guide covers setting up an iOS Blinc project — toolchain, build commands, and the platform-specific files (`Info.plist`, Xcode configuration, debugging).

For the cross-platform Blinc API (native bridge, camera, deep linking, lifecycle, etc.), see the [Mobile Development overview](./overview.md).

See [Accessibility](../core/accessibility.md) for native semantic elements, text-field editing and Simulator validation.

## Prerequisites

### 1. Xcode

Install Xcode 15+ from the App Store.

```bash
xcode-select -p
```

### 2. Rust Targets

```bash
rustup target add aarch64-apple-ios        # Device
rustup target add aarch64-apple-ios-sim    # Simulator (Apple Silicon)
rustup target add x86_64-apple-ios         # Simulator (Intel)
```

## Building

Create a build script `build-ios.sh`:

```bash
#!/bin/bash
set -e
MODE=${1:-debug}
PROJECT_NAME="my_app"
[ "$MODE" = "release" ] && CARGO_FLAGS="--release" || CARGO_FLAGS=""
TARGET_DIR=$([ "$MODE" = "release" ] && echo "release" || echo "debug")

cargo build --target aarch64-apple-ios $CARGO_FLAGS
cargo build --target aarch64-apple-ios-sim $CARGO_FLAGS

mkdir -p platforms/ios/libs/{device,simulator}
cp target/aarch64-apple-ios/$TARGET_DIR/lib${PROJECT_NAME}.a \
   platforms/ios/libs/device/
cp target/aarch64-apple-ios-sim/$TARGET_DIR/lib${PROJECT_NAME}.a \
   platforms/ios/libs/simulator/
```

```bash
./build-ios.sh         # debug
./build-ios.sh release
```

Then open `platforms/ios/BlincApp.xcodeproj` in Xcode and press Cmd+R.

## Project Configuration

### Cargo.toml

```toml
[lib]
name = "my_app"
crate-type = ["cdylib", "staticlib"]

[target.'cfg(target_os = "ios")'.dependencies]
blinc = { version = "0.6", default-features = false, features = ["ios"] }
```

### Xcode Build Settings

1. **Link static library**: Build Phases → Link Binary With Libraries → add `libmy_app.a` from `libs/device/` or `libs/simulator/`
2. **Bridging header**: Build Settings → Objective-C Bridging Header → `BlincApp/Blinc-Bridging-Header.h`
3. **Frameworks**:
   - `Metal.framework`
   - `MetalKit.framework`
   - `QuartzCore.framework`
   - `AVFoundation.framework` (camera/audio)
   - `CoreHaptics.framework` (haptics)

### Info.plist

```xml
<!-- Camera + microphone permissions for native bridge features -->
<key>NSCameraUsageDescription</key>
<string>This app uses the camera for photo capture.</string>
<key>NSMicrophoneUsageDescription</key>
<string>This app records audio.</string>

<!-- Deep link URL scheme -->
<key>CFBundleURLTypes</key>
<array>
    <dict>
        <key>CFBundleURLSchemes</key>
        <array><string>myapp</string></array>
    </dict>
</array>
```

### Bridging Header

The bridging header (`Blinc-Bridging-Header.h`) declares the C FFI surface Swift uses to call Rust:

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

// Deep linking + keyboard (see Mobile overview for usage)
void blinc_ios_handle_deep_link(const char* uri);
void blinc_ios_set_keyboard_inset(IOSRenderContext* ctx, float inset);
```

The `BlincViewController` template manages the `CADisplayLink`, `CAMetalLayer`, and touch event forwarding to Rust.

## Debugging

### Console Logs

View Rust logs in Xcode's console or `Console.app` with a filter:

```
subsystem:com.blinc.my_app
```

### Common Issues

**"Library not found: -lmy_app"** — run `./build-ios.sh` first.

**Black screen on simulator** —
1. Verify the right simulator target (`aarch64-apple-ios-sim` for Apple Silicon, `x86_64-apple-ios` for Intel)
2. Verify the static library is in `libs/simulator/`
3. Check Xcode console for Metal initialization errors

**Touch events not working** —
1. Verify `blinc_create_context` succeeds (check console)
2. Ensure `ios_app_init()` is called before creating the context
3. Touch coordinates must be in logical points, not physical pixels

**Native call failed** — verify Swift handler is registered with matching `namespace.name`. Check that `BlincNativeBridge.shared.connectToRust()` was called at app launch.

## Performance

```toml
[profile.release]
lto = "fat"
opt-level = "z"
panic = "abort"
strip = true
codegen-units = 1
```

- **Test on real devices** — simulators use software rendering for some Metal operations
- **Profile with Instruments** — use the Metal System Trace template for GPU analysis

## Next Steps

- [Mobile Development overview](./overview.md) — native bridge, camera, deep linking, lifecycle, safe area APIs
- [Android Project Setup](./android.md) — build the Android counterpart
- [CLI Reference](./cli.md)
