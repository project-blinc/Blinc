# Changelog

All notable changes to `blinc_text` will be documented in this file.

## [Unreleased]

### Added
- `FaceChoice::Joined` and `FallbackFaces::apply_to`; see the emoji sequence fix below. A `match` on `FaceChoice` needs the new arm.
- `use_fontconfig_sans()`, opt-in: on Linux it asks `fc-match` for fontconfig's default sans and makes it the face `system-ui` resolves to, through the new `FontRegistry::set_system_ui_family`. Nothing changes unless an app calls it; the default stays the built-in list, which costs nothing at startup. Call it once before drawing text.

### Changed
- `FontFace::from_file` memory-maps the file instead of reading it, so a loaded system font is clean, shared memory rather than a heap copy. New dependency off wasm: `memmap2`.

### Fixed
- An emoji sequence is one glyph. A skin tone (`👋🏾`), a joined family and a flag drew as the plain base emoji followed by a blank advance, one em per extra code point, and a typed one left a gap before the next word. `FallbackFaces::apply_to(text, ..)` shapes each such cluster with the emoji face and keeps the single glyph it returns; the other code points become `FaceChoice::Joined`, which has no advance and draws nothing, and the layout engine spaces letters once per sequence. `apply` is unchanged. A sequence the face does not combine keeps a glyph per code point.
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
