# Changelog

All notable changes to `blinc_text` will be documented in this file.

## [Unreleased]

### Fixed
- Resolve iOS font files by their native CoreText locations and use Apple’s CJK fallback families for candidate text.
- Discover Android and iOS system colour-emoji fonts on first emoji use, including CoreText font locations in Simulator. Decode Apple’s compressed colour-emoji strikes through CoreText on iOS.

### Added
- COLRv1 colour glyph rasterization supports gradients, transforms, clipping and compositing through the existing colour atlas. Bitmap and COLRv0 glyphs retain their existing rasterization path.

## [0.6.0] - 2026-11-04

### Changed
- `ColorGlyphAtlas` lazy-allocates its CPU pixel shadow buffer (`pixels: Option<Vec<u8>>`) on first `insert_glyph`. Apps that never render a color emoji save the ~1 MB heap allocation. `pixels()` returns an empty slice when not yet allocated; `clear` / `grow` short-circuit. The matching GPU texture in `blinc_gpu::text` stays eagerly allocated because the bind group requires both atlases to be `Some` — sampling the wgpu-zeroed texture for non-color glyphs is harmless.
