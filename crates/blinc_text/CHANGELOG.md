# Changelog

All notable changes to `blinc_text` will be documented in this file.

## [Unreleased]

## [0.6.0] - 2026-11-03

### Changed
- `ColorGlyphAtlas` lazy-allocates its CPU pixel shadow buffer (`pixels: Option<Vec<u8>>`) on first `insert_glyph`. Apps that never render a color emoji save the ~1 MB heap allocation. `pixels()` returns an empty slice when not yet allocated; `clear` / `grow` short-circuit. The matching GPU texture in `blinc_gpu::text` stays eagerly allocated because the bind group requires both atlases to be `Some` — sampling the wgpu-zeroed texture for non-color glyphs is harmless.
