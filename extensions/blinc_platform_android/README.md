# blinc_platform_android

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

Android platform implementation for Blinc UI.

## Overview

`blinc_platform_android` provides native Android integration using the NDK, including activity lifecycle, touch input, and GPU rendering via Vulkan.

## Supported Platforms

- Android 7.0+ (API level 24+)
- ARM64 and x86_64 architectures

## Features

- **Native Activity**: Full NDK integration
- **JNI Bridge**: Java interoperability
- **Touch Input**: Multi-touch support
- **Vulkan Rendering**: Hardware-accelerated graphics
- **Asset Loading**: Load from APK resources
- **Lifecycle**: Proper activity state handling

## Quick Start

```rust
use blinc_platform_android::android_main;

#[no_mangle]
pub extern "C" fn ANativeActivity_onCreate(
    activity: *mut ANativeActivity,
    saved_state: *mut c_void,
    saved_state_size: usize,
) {
    android_main(activity, |ctx| {
        // Build your UI
        div()
            .w_full()
            .h_full()
            .child(text("Hello Android!"))
    });
}
```

## Project Setup

### Cargo.toml

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
blinc_platform_android = "0.1"
```

### build.gradle

```gradle
android {
    defaultConfig {
        ndk {
            abiFilters 'arm64-v8a', 'x86_64'
        }
    }

    externalNativeBuild {
        ndkBuild {
            path 'src/main/jni/Android.mk'
        }
    }
}
```

### AndroidManifest.xml

```xml
<application
    android:hasCode="false"
    android:allowBackup="true">
    <activity
        android:name="android.app.NativeActivity"
        android:exported="true"
        android:configChanges="orientation|screenSize|keyboardHidden">
        <meta-data
            android:name="android.app.lib_name"
            android:value="myapp" />
        <intent-filter>
            <action android:name="android.intent.action.MAIN" />
            <category android:name="android.intent.category.LAUNCHER" />
        </intent-filter>
    </activity>
</application>
```

## Touch Handling

```rust
fn handle_touch(event: TouchEvent) {
    for pointer in event.pointers() {
        match pointer.action {
            PointerAction::Down => {
                // Touch started
            }
            PointerAction::Move => {
                // Touch moved
            }
            PointerAction::Up => {
                // Touch ended
            }
        }
    }
}
```

## Asset Loading

```rust
use blinc_platform_android::AndroidAssetLoader;

let loader = AndroidAssetLoader::new(activity);

// Load from assets/ directory in APK
let data = loader.load("images/logo.png")?;

// Check if asset exists
if loader.exists("config.json") {
    let config = loader.load("config.json")?;
}
```

## Lifecycle

```rust
android_main(activity, |ctx| {
    ctx.on_start(|| {
        // Activity started
    });

    ctx.on_resume(|| {
        // Activity resumed
    });

    ctx.on_pause(|| {
        // Activity paused - save state
    });

    ctx.on_stop(|| {
        // Activity stopped
    });

    ctx.on_destroy(|| {
        // Cleanup
    });

    build_ui()
});
```

## Building

```bash
# Build for Android
cargo ndk -t arm64-v8a -t x86_64 -o ./app/src/main/jniLibs build --release

# Or using cargo-apk
cargo apk build --release
```

## Requirements

- Android SDK (API level 24+)
- Android NDK r21+
- Rust with Android targets:
  ```bash
  rustup target add aarch64-linux-android
  rustup target add x86_64-linux-android
  ```
- cargo-ndk:
  ```bash
  cargo install cargo-ndk
  ```

## License

MIT OR Apache-2.0
