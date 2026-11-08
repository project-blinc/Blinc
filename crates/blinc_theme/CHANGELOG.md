# Changelog

All notable changes to `blinc_theme` will be documented in this file.

## [Unreleased]

### Fixed
- The Linux theme names its UI sans from the shared `blinc_core::system_fonts::LINUX_SANS` list (Adwaita Sans first), so it and the text registry resolve the same families in the same order. The previous list named Cantarell first and left out Noto Sans and Adwaita Sans.
- Runtime overrides apply to `ThemeState::colors()`, `spacing()` and `radii()` and to the CSS variable map. Before, spacing and radius overrides changed only `spacing_value(token)` and `radius(token)`, and colour overrides skipped `colors()`, so `--space-*` and `--radius-*` kept the theme's values.

## [0.6.0] - 2026-11-04

### Added
- `--chart-1` to `--chart-5` and `--chart-baseline`: a palette for charts, one per scheme, in the CSS variable map (`chart_palette(scheme)`).
- **Universal HID variants — `RestrainedTheme`, `HybridTheme`, `ExpressiveTheme`** plus `DefaultTheme` alias (= `HybridTheme`). Three considered cross-platform synthesis variants with per-variant `RadiusTokens` ladders, `ShapeTokens` (superellipse smoothing), and semantic-easing assignments. `cn_bundle()` defaults to `HybridTheme` when the platform doesn't match a built-in.
- **`ShapeTokens`** (`corner_smoothing`, `corner_exponent`, `smoothing_threshold`). Theme-driven squircle smoothing applied per-corner at paint time via a new `resolve_corner_shape` helper. Defaults to `is_off()` so existing themes (BlincTheme, platform themes) keep circular corners unchanged.
- **`--focus-ring`, `--focus-ring-error`, `--focus-ring-success` CSS variables** — alpha-tinted (35 %) variants of `BorderFocus` / `BorderError` / `Success`, derived at frame time so user-written CSS can reference the same soft halo colour the focus ring uses.
- **Semantic easing CSS variables** — `--ease-default`, `--ease-in`, `--ease-out`, `--ease-in-out`, `--ease-emphasized`, `--ease-decelerated`, `--ease-accelerated` derived from the active variant's `AnimationTokens`. Lets framework-level CSS rules (in `cn`) and user stylesheets resolve to the same easings the Rust API uses.
- **`cubic-bezier()` parsing** in the CSS animation-timing path, so `transition: opacity 200ms cubic-bezier(0.4, 0.0, 0.2, 1.0)` resolves correctly against the variant tokens.

### Changed
- `radius_default` bumped per variant so it lands at or above each variant's `smoothing_threshold`. Pre-fix, components consuming `--radius-default` (cn::button / cn::input / cn::card) all fell back to a true circle and never picked up the variant squircle.
- `HybridTheme` swapped in as the no-match fallback (was `BlincTheme`). `BlincTheme` is still available as a named opt-in theme.

## [0.4.0] - 2026-04-05

### Changed
- Version bump to align with workspace 0.4.0 release
