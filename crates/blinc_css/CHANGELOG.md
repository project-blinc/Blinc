# Changelog

## [Unreleased]

### Added
- CSS `aspect-ratio` accepts positive numbers, fractions and `auto`, and participates in layout-style merging.

### Fixed
- Root variables apply to rules declared earlier, including rules in component layers.
- Incomplete blocks, strings and comments report syntax errors so live editors can retain the last valid stylesheet.
