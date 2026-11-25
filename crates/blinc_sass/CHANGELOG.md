# Changelog

All notable changes to `blinc_sass` will be documented in this file.

## [Unreleased]

### Added
- `blinc_sass` compiles a crate's SCSS sources into the CSS it embeds, from a `build.rs`: `build`, `compile` and `sources`. It carries shared partials, `blinc/mixins` and `blinc/tokens`, and wraps the result in an `@layer` where asked. The compiler is `grass`.
