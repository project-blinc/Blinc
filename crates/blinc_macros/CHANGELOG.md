# Changelog

## [Unreleased]

### Added
- Named `BlincComponent` structs generate `NameProps` with reactive fields, a `Name::props(...)` constructor and fluent `IntoReactive<T>` setters. Values, Signal and Computed sources retain their binding identities.
- Generic components preserve bounds and isolate hook identities by concrete type.

### Changed
- Component and flow macros resolve direct or renamed dependencies and the unified `blinc` facade.
- Ordinary field hooks return persistent `Signal<T>` through `use_signal_keyed`.
- The derive guide and README describe prop bindings, Computed, Effect and CSS styling hooks.
