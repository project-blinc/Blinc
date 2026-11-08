# Changelog

## [Unreleased]

### Added
- Declaration-level `!important` support for background, text and uniform border colours, with component-layer precedence, specificity and source order. Unsupported importance properties report an error.
- `ElementStyle` gains `aspect_ratio` and optional colour-priority metadata (`important_colors`). Exhaustive struct literals must initialize both; builder and `..Default::default()` construction remain valid.
- CSS `aspect-ratio` accepts positive numbers, fractions and `auto`, and participates in layout-style merging.

### Fixed
- Root variables apply to rules declared earlier, including rules in component layers.
- Incomplete blocks, strings and comments report syntax errors so live editors can retain the last valid stylesheet.
