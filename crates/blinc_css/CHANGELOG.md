# Changelog

## [Unreleased]

### Added
- Declaration-level `!important` support for background, text and uniform border colours, with component-layer precedence, specificity and source order. Unsupported importance properties report an error.
- `ElementStyle` gains `aspect_ratio` and optional colour-priority metadata (`important_colors`). Exhaustive struct literals must initialize both; builder and `..Default::default()` construction remain valid.
- CSS `aspect-ratio` accepts positive numbers, fractions and `auto`, and participates in layout-style merging.

### Fixed
- `font-family` keeps every named family before the first generic keyword, in order (`StyleFontFamily::name` is the stack joined with `, `), and the generic keyword ends the list as in CSS. A quoted name may hold a comma and a quoted `serif` is a family called serif, not the generic. Before, only the first name was kept, so `font-family: "Inter", "Helvetica Neue", Arial, sans-serif` went from Inter straight to the generic sans.
- Root variables apply to rules declared earlier, including rules in component layers.
- Incomplete blocks, strings and comments report syntax errors so live editors can retain the last valid stylesheet.
