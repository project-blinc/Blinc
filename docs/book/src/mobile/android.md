# Android Project Setup

This guide covers setting up an Android Blinc project — toolchain, build commands, and the platform-specific files (`AndroidManifest.xml`, Gradle config, debugging).

For the cross-platform Blinc API (native bridge, camera, deep linking, lifecycle, etc.), see the [Mobile Development overview](./overview.md).

## Prerequisites

### 1. Android SDK & NDK

```bash
# macOS
brew install --cask android-studio

export ANDROID_HOME=$HOME/Library/Android/sdk
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/26.1.10909125
export PATH=$PATH:$ANDROID_HOME/platform-tools
```

### 2. Rust Targets

```bash
rustup target add aarch64-linux-android
rustup target add armv7-linux-androideabi
rustup target add x86_64-linux-android
cargo install cargo-ndk
```

## Building

```bash
# Debug — single arch
cargo ndk -t arm64-v8a build

# Release — multi-arch
cargo ndk -t arm64-v8a -t armeabi-v7a build --release

# Or via Gradle (from platforms/android/)
./gradlew assembleDebug
```

The APK lands in `app/build/outputs/apk/debug/app-debug.apk`.

## Project Configuration

### Cargo.toml

```toml
[lib]
name = "my_app"
crate-type = ["cdylib", "staticlib"]

[target.'cfg(target_os = "android")'.dependencies]
blinc_app = { version = "0.6", features = ["android"] }
blinc_platform_android = "0.6"
android-activity = { version = "0.6", features = ["native-activity"] }
log = "0.4"
android_logger = "0.14"
```

### AndroidManifest.xml

```xml
<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <uses-feature android:glEsVersion="0x00030000" android:required="true" />

    <!-- Permissions for native bridge features -->
    <uses-permission android:name="android.permission.CAMERA" />
    <uses-permission android:name="android.permission.RECORD_AUDIO" />
    <uses-permission android:name="android.permission.VIBRATE" />
    <uses-permission android:name="android.permission.INTERNET" />

    <application
        android:label="My App"
        android:theme="@android:style/Theme.DeviceDefault.NoActionBar.Fullscreen"
        android:hardwareAccelerated="true">

        <activity
            android:name=".MainActivity"
            android:launchMode="singleTask"
            android:configChanges="orientation|screenSize|screenLayout|smallestScreenSize|keyboardHidden|keyboard|navigation|uiMode|density|colorMode|fontScale|locale|layoutDirection|touchscreen|mcc|mnc"
            android:exported="true">

            <meta-data
                android:name="android.app.lib_name"
                android:value="my_app" />

            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>

            <!-- Deep link: myapp://path/to/route -->
            <intent-filter android:autoVerify="true">
                <action android:name="android.intent.action.VIEW" />
                <category android:name="android.intent.category.DEFAULT" />
                <category android:name="android.intent.category.BROWSABLE" />
                <data android:scheme="myapp" />
            </intent-filter>
        </activity>
    </application>
</manifest>
```

> **Critical — do not trim `launchMode` or `configChanges`.** Blinc runs on a
> `NativeActivity`, whose native context (`ndk-context`) can only be
> initialized once per process. If Android destroys and recreates the activity
> (a config change the manifest doesn't declare, or a launcher-icon resume that
> spawns a second activity instance), the second init hits
> `assert!(previous.is_none())` and the process aborts (SIGABRT) — Android then
> cold-restarts the app, which the user sees as "resume flashes black and comes
> back with fresh state." `launchMode="singleTask"` makes an icon tap resume the
> single instance instead of creating a new one, and the broad `configChanges`
> list keeps config changes (dark mode, density, font scale, insets, …) from
> destroying the activity. Both are required, not optional.

## Debugging

```bash
# View Rust logs
adb logcat | grep -E "(blinc|BlincApp)"

# Filter for native bridge calls
adb logcat | grep BlincNativeBridge
```

### Common Issues

**"Library not found"** — ensure the native library is built and copied to `app/src/main/jniLibs/<arch>/`:

```bash
cargo ndk -t arm64-v8a build
cp target/aarch64-linux-android/debug/libmy_app.so \
   platforms/android/app/src/main/jniLibs/arm64-v8a/
```

**"Vulkan not supported"** — check device capability:

```bash
adb shell getprop ro.hardware.vulkan
```

API 24+ devices generally support Vulkan, but some emulators may not.

**"Native call failed"** — verify the namespace+name matches between Kotlin and Rust handlers. Check logcat for `BlincNativeBridge: handler not found for X.Y`.

**Touch events not working** — verify the render context is created successfully and `android.app.lib_name` in the manifest matches your library name.

## Performance

```toml
[profile.release]
lto = "fat"
opt-level = "z"      # optimize for size on mobile
panic = "abort"
strip = true
codegen-units = 1
```

- **Test on real devices** — emulators have different GPU characteristics
- **Profile with Android Studio Profiler** for CPU/GPU/memory
- **Bundle assets via `assets/`** — `AndroidAssetLoader` auto-resolves them through the platform `AssetLoader` trait

## Next Steps

- [Mobile Development overview](./overview.md) — native bridge, camera, deep linking, lifecycle, safe area APIs
- [iOS Project Setup](./ios.md) — build the iOS counterpart
- [CLI Reference](./cli.md)
