# Changelog

All notable changes to `blinc_text` will be documented in this file.

## [Unreleased]

### Added
- `use_fontconfig_sans()`, opt-in: on Linux it asks `fc-match` for fontconfig's default sans and makes it the face `system-ui` resolves to, through the new `FontRegistry::set_system_ui_family`. Nothing changes unless an app calls it; the default stays the built-in list, which costs nothing at startup. Call it once before drawing text.

### Fixed
- iOS draws glyphs from faces that keep their outlines in Apple's `hvgl` table, which the software rasterizer cannot read, through CoreText. Pinyin and other CJK text in PingFang no longer renders as blank space.
- `FontRegistry::resolve_styled` takes a font stack, the families of a `font-family` list joined with commas, and uses the first one that is installed before the generic family. A name with no comma resolves as before. `family_stack` splits a stack.
- On Linux the default UI face list is the shared `blinc_core::system_fonts::LINUX_SANS` and now starts with Adwaita Sans, the GNOME 48 interface font, before Cantarell, Ubuntu, Noto Sans and DejaVu Sans.
- Resolve iOS font files by their native CoreText locations and use Apple’s CJK fallback families for candidate text.
- Discover Android and iOS system colour-emoji fonts on first emoji use, including CoreText font locations in Simulator. Decode Apple’s compressed colour-emoji strikes through CoreText on iOS.

### Added
- COLRv1 colour glyph rasterization supports gradients, transforms, clipping and compositing through the existing colour atlas. Bitmap and COLRv0 glyphs retain their existing rasterization path.

## [0.6.0] - 2026-11-04

### Changed
- `ColorGlyphAtlas` lazy-allocates its CPU pixel shadow buffer (`pixels: Option<Vec<u8>>`) on first `insert_glyph`. Apps that never render a color emoji save the ~1 MB heap allocation. `pixels()` returns an empty slice when not yet allocated; `clear` / `grow` short-circuit. The matching GPU texture in `blinc_gpu::text` stays eagerly allocated because the bind group requires both atlases to be `Some` — sampling the wgpu-zeroed texture for non-color glyphs is harmless.
