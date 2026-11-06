# Changelog

All notable changes to `blinc_platform_android` will be documented in this file.

## [Unreleased]

### Added
- `BlincIme.kt` connects a native editor to the focused GPU document, preserving composing spans, candidate replacement, selection and input-connection batch edits.

### Changed
- FFI declaration block for `ANativeWindow_*` (NDK) wrapped as `unsafe extern "C" { … }` to satisfy edition 2024's required-unsafe-on-extern rule. Per-export `#[unsafe(no_mangle)]` syntax applied to every JNI native function in the bridge.

## [0.4.0] - 2026-04-05

### Added
- Camera preview via Camera2 API with RGBA frame streaming to Rust (`nativeDispatchStreamData`)
- Audio recording via AudioRecord with PCM float streaming to Rust
- `BlincNativeBridge` template: camera, audio, device, haptics, clipboard, keyboard handlers

## [0.1.12] - 2025-01-19

### Changed
- Version bump to align with blinc_app improvements
- Improved integration with new momentum scrolling system

## [0.1.1] - Initial Release

- Initial public release with Android NDK integration
- Touch input handling and display density support
- Asset loading from Android APK
