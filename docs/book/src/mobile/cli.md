# CLI Reference

The Blinc CLI simplifies creating and building mobile projects.

## Creating a Project

### New Project

```bash
blinc new my-app --template rust
```

This creates a new Blinc project with:
- Cargo.toml configured for mobile targets
- Platform directories for Android and iOS
- Build scripts for each platform
- Example UI code

### Options

```bash
blinc new <name> [options]

Options:
  --template <type>   Project template (rust, swift, kotlin)
  --platforms <list>  Target platforms (desktop,android,ios)
  --no-git            Skip git initialization
```

## Building

### Build for Android

```bash
blinc build android
```

Options:

```bash
blinc build android [options]

Options:
  --release           Build in release mode
  --target <arch>     Target architecture (arm64-v8a, armeabi-v7a, x86_64, x86)
  --all-targets       Build for all architectures
```

### Build for iOS

```bash
blinc build ios
```

Options:

```bash
blinc build ios [options]

Options:
  --release           Build in release mode
  --device            Build for physical device only
  --simulator         Build for simulator only
```

## Running

### Run on Android

```bash
blinc run android
```

This will:
1. Build the native library
2. Build the APK with Gradle
3. Install on connected device/emulator
4. Launch the app

Options:

```bash
blinc run android [options]

Options:
  --release           Run release build
  --device <id>       Target specific device (from adb devices)
  --no-install        Build only, don't install
```

### Run on iOS

```bash
blinc run ios
```

This will:
1. Build the static library
2. Open Xcode project
3. Build and run on selected target

Options:

```bash
blinc run ios [options]

Options:
  --release           Run release build
  --simulator <name>  Target specific simulator
  --device            Run on physical device
```

## Project Configuration

### blinc.toml

The project configuration file:

```toml
[project]
name = "my-app"
version = "0.1.0"
template = "rust"
entry = "Cargo.toml"

[targets]
default = "desktop"
supported = ["desktop", "android", "ios"]

[targets.desktop]
enabled = true
command = "cargo run --features desktop"

[targets.android]
enabled = true
platform_dir = "platforms/android"

[targets.ios]
enabled = true
platform_dir = "platforms/ios"

[build]
blinc_path = "../.."  # Path to Blinc framework
```

### Configuration Options

| Key | Description | Default |
|-----|-------------|---------|
| `project.name` | Project name | Required |
| `project.version` | Version string | "0.1.0" |
| `project.template` | Template type | "rust" |
| `targets.default` | Default build target | "desktop" |
| `targets.supported` | List of supported platforms | ["desktop"] |
| `build.blinc_path` | Path to Blinc framework | "../.." |

## Cleaning

```bash
# Clean all build artifacts
blinc clean

# Clean specific platform
blinc clean android
blinc clean ios
```

## Checking Configuration

```bash
# Validate project configuration
blinc check

# Check specific platform setup
blinc check android
blinc check ios
```

This verifies:
- Required tools are installed
- Environment variables are set
- Project configuration is valid
