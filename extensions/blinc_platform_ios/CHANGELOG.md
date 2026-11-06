# Changelog

All notable changes to `blinc_platform_ios` will be documented in this file.

## [Unreleased]

### Input
- Native marked-text composition, candidate replacement and UTF-16 selection synchronize with Blinc editable widgets through the UIKit keyboard connection.

### Accessibility
- UIKit accessibility container and text-field proxies for GPU-rendered controls, with stable identities, semantic updates and assistive actions. Dynamic Type scales the UI and coordinates together.

### Changed
- `lookup_extern_fn` (the `dlsym` helper for iOS template hooks) wraps its `extern "C"` import as `unsafe extern "C" { … }` and adds explicit `unsafe { }` blocks around the `dlsym` + `transmute` calls so the body satisfies edition 2024's `unsafe_op_in_unsafe_fn`. Per-export `#[unsafe(no_mangle)]` syntax applied to all iOS native-bridge symbols.

## [0.4.0] - 2026-04-05

### Added
- Camera capture via AVCaptureSession with RGBA frame streaming to Rust (`blinc_dispatch_stream_data`)
- Audio recording via AVAudioEngine with PCM float streaming to Rust
- `BlincNativeBridge.swift` template: camera, audio, device, haptics, clipboard handlers

## [0.1.12] - 2025-01-19

### Changed
- Version bump to align with blinc_app improvements
- Improved integration with momentum scrolling system

## [0.1.1] - Initial Release

- Initial public release with iOS/UIKit integration
- Metal rendering support
- Touch input handling and asset loading
