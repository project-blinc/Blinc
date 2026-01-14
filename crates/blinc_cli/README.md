# blinc_cli

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

Command-line interface for the Blinc UI framework.

## Overview

`blinc_cli` provides commands for building, running, and developing Blinc applications with features like hot-reload.

## Installation

```bash
cargo install blinc_cli
```

Or build from source:

```bash
cargo build -p blinc_cli --release
```

## Commands

### Build

Compile your Blinc application:

```bash
# Build for current platform
blinc build

# Build for specific platform
blinc build --platform macos
blinc build --platform windows
blinc build --platform linux
blinc build --platform android
blinc build --platform ios

# Release build
blinc build --release

# Specify target directory
blinc build --target-dir ./build
```

### Dev

Run in development mode with hot-reload:

```bash
# Start dev server
blinc dev

# Specify port
blinc dev --port 3000

# Watch specific directories
blinc dev --watch src --watch assets

# Disable hot-reload
blinc dev --no-hot-reload
```

### Doctor

Check your development environment:

```bash
blinc doctor
```

Output:
```
Checking Blinc development environment...

✓ Rust toolchain: 1.75.0
✓ Cargo: 1.75.0
✓ wgpu supported: Yes
✓ Platform SDK: macOS 14.0
✓ Android SDK: Not found (optional)
✓ iOS SDK: Xcode 15.0

Environment is ready for Blinc development!
```

## Configuration

Create a `Blinc.toml` in your project root:

```toml
[package]
name = "my-app"
version = "0.1.0"

[build]
target-dir = "target"
assets = ["assets"]

[dev]
port = 3000
hot-reload = true
watch = ["src", "assets"]

[platforms.macos]
bundle-id = "com.example.myapp"
min-version = "11.0"

[platforms.ios]
bundle-id = "com.example.myapp"
min-version = "14.0"

[platforms.android]
package = "com.example.myapp"
min-sdk = 24
target-sdk = 34
```

## Project Structure

```
my-app/
├── Blinc.toml          # Project configuration
├── Cargo.toml          # Rust dependencies
├── src/
│   └── main.rs         # Application entry point
└── assets/             # Images, fonts, etc.
```

## Platform Requirements

### macOS
- Xcode Command Line Tools
- macOS 11.0+

### Windows
- Visual Studio Build Tools
- Windows 10+

### Linux
- GCC or Clang
- X11 or Wayland development packages

### Android
- Android SDK
- Android NDK
- Java JDK

### iOS
- Xcode
- iOS Simulator or device

## License

MIT OR Apache-2.0
