# Changelog

All notable changes to `blinc_image` will be documented in this file.

## [Unreleased]

## [0.6.0] - 2026-11-04

### Added
- **Pluggable `ImageDecoder` trait + `DecoderRegistry`** for BYO format support. Apps can register a custom decoder (zune-image, libvips, a WebGPU compute decoder, a stub for headless tests) via `decoder::set_global_registry`; the loader picks it up. Built-in decoders ship as `PngDecoder`, `JpegDecoder`, etc., each gated by its matching cargo feature.
- Per-format cargo features: `png`, `jpeg`, `gif`, `webp`, `bmp`, `tiff`, `avif` — each turns on the matching `image` crate codec and registers its built-in `ImageDecoder`. The `all-formats` umbrella enables every previously-bundled format.
- `ImageFormat::Tiff` and `ImageFormat::Avif` variants (plus matching extension/MIME detection).
- `ImageData::from_decoded(DecodedImage)` for callers that decode through the registry directly.

### Changed
- **Default `image` crate decoders cut to PNG + JPEG.** Previously the crate hard-coded `png + jpeg + gif + webp + bmp` regardless of what the app actually needed. Apps that use those extra formats turn on the matching feature (`gif` / `webp` / `bmp`) or use the `all-formats` umbrella to restore the prior behaviour. Shaved 5 transitive crates from the default Linux dep tree (79 → 74); apps that build with `default-features = false` and BYO a decoder drop another 54 (74 → 20).
- `ImageData::from_bytes` now routes through the global `DecoderRegistry` instead of calling `image::load_from_memory` directly. The default registry materialises lazily via `DecoderRegistry::with_builtins()`, so apps that don't customise it see no behaviour change.
