# Android Development

This guide covers setting up your environment and building Blinc apps for Android.

## Prerequisites

### 1. Android SDK & NDK

Install Android Studio or the standalone SDK:

```bash
# macOS (via Homebrew)
brew install --cask android-studio

# Or download from https://developer.android.com/studio
```

Set up environment variables:

```bash
export ANDROID_HOME=$HOME/Library/Android/sdk
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/26.1.10909125
export PATH=$PATH:$ANDROID_HOME/platform-tools
```

### 2. Rust Android Targets

```bash
rustup target add aarch64-linux-android
rustup target add armv7-linux-androideabi
rustup target add x86_64-linux-android
rustup target add i686-linux-android
```

### 3. cargo-ndk

```bash
cargo install cargo-ndk
```

## Building

### Debug Build

```bash
# Build for arm64 (most modern devices)
cargo ndk -t arm64-v8a build

# Build for multiple architectures
cargo ndk -t arm64-v8a -t armeabi-v7a build
```

### Release Build

```bash
cargo ndk -t arm64-v8a build --release
```

### Using Gradle

From the `platforms/android` directory:

```bash
./gradlew assembleDebug
```

The APK will be at `app/build/outputs/apk/debug/app-debug.apk`.

## Project Configuration

### Cargo.toml

```toml
[lib]
name = "my_app"
crate-type = ["cdylib", "staticlib"]

[target.'cfg(target_os = "android")'.dependencies]
blinc_app = { version = "0.1", features = ["android"] }
blinc_platform_android = "0.1"
android-activity = { version = "0.6", features = ["native-activity"] }
log = "0.4"
android_logger = "0.14"
```

### AndroidManifest.xml

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <uses-feature android:glEsVersion="0x00030000" android:required="true" />

    <application
        android:label="My App"
        android:theme="@android:style/Theme.DeviceDefault.NoActionBar.Fullscreen"
        android:hardwareAccelerated="true">

        <activity
            android:name=".MainActivity"
            android:configChanges="orientation|screenSize|keyboardHidden"
            android:exported="true">

            <meta-data
                android:name="android.app.lib_name"
                android:value="my_app" />

            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
```

## Touch Event Handling

Android touch events are automatically routed to your UI. The touch phases map as follows:

| Android Action | Blinc Event |
|---------------|-------------|
| ACTION_DOWN   | pointer_down |
| ACTION_MOVE   | pointer_move |
| ACTION_UP     | pointer_up + pointer_leave |
| ACTION_CANCEL | pointer_leave |

## Debugging

### View Logs

```bash
adb logcat | grep -E "(blinc|BlincApp)"
```

### Common Issues

**"Library not found"**

Ensure the native library is built and copied to `app/src/main/jniLibs/`:

```bash
cargo ndk -t arm64-v8a build
cp target/aarch64-linux-android/debug/libmy_app.so \
   platforms/android/app/src/main/jniLibs/arm64-v8a/
```

**"Vulkan not supported"**

Check device compatibility:

```bash
adb shell getprop ro.hardware.vulkan
```

Most devices with API 24+ support Vulkan, but some older devices may not.

**Touch events not working**

1. Verify the render context is created successfully
2. Check that `android.app.lib_name` in manifest matches your library name
3. Look for errors in logcat

## Performance Tips

1. **Use release builds** for performance testing
2. **Enable LTO** in Cargo.toml:
   ```toml
   [profile.release]
   lto = "thin"
   opt-level = "z"
   ```
3. **Test on real devices** - emulators have different GPU characteristics
