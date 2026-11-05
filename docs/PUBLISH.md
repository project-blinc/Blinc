# Cargo Publish Strategy

This document outlines the publishing strategy for Blinc crates to crates.io.

## Dependency Graph

```text
Level 0 (no internal deps):
├── blinc_macros      (proc-macros)
├── blinc_platform    (platform traits)
├── blinc_icons       (icon set)
└── blinc_core        (reactive system)

Level 1 (depends on Level 0):
├── blinc_animation   → blinc_core
├── blinc_paint       → blinc_core
├── blinc_recorder    → blinc_core
├── blinc_svg         → blinc_core
├── blinc_text        → blinc_core
└── blinc_platform_desktop → blinc_platform

Level 2:
├── blinc_theme       → blinc_core, blinc_animation
└── blinc_image       → blinc_platform?, blinc_text?

Level 3:
└── blinc_layout      → blinc_core, blinc_animation, blinc_theme

Level 4:
├── blinc_gpu         → blinc_core, blinc_layout, blinc_paint, blinc_text
├── blinc_cn          → blinc_layout, blinc_core, blinc_animation, blinc_theme, blinc_macros, blinc_icons
├── blinc_platform_android → blinc_core, blinc_animation, blinc_platform, blinc_gpu?
└── blinc_platform_ios     → blinc_core, blinc_animation, blinc_platform, blinc_gpu

Level 5:
└── blinc_app         → (many deps including platform extensions)

Level 6 (top-level):
├── blinc             → facade over the public crates (publish after blinc_app)
├── blinc_debugger    → blinc_app, blinc_layout, blinc_theme, blinc_cn, blinc_icons, blinc_recorder
├── blinc_cli         → blinc_core, blinc_animation
├── blinc_runtime     → optional deps
└── blinc_test_suite  → internal testing
```

## Crate Categories

### Public API (publish to crates.io)

These crates form the public Blinc API:

| Crate | Description | Priority |
|-------|-------------|----------|
| `blinc_core` | Reactive signals, state machines, types | High |
| `blinc_animation` | Spring physics, keyframes, timelines | High |
| `blinc_layout` | Layout engine (flexbox, event routing) | High |
| `blinc_theme` | Theming system | High |
| `blinc_macros` | Procedural macros | High |
| `blinc_app` | Application framework | High |
| `blinc` | Unified facade, with optional components, SVG and icons | High |
| `blinc_gpu` | GPU renderer (wgpu) - for custom renderers | High |
| `blinc_paint` | Paint primitives and operations | High |
| `blinc_cn` | Component library (buttons, inputs, etc.) | Medium |
| `blinc_icons` | Icon set (Lucide icons) | Medium |

### Support Crates (publish as dependencies)

These are needed by public crates and may be useful for advanced users:

| Crate | Description | Needed By |
| ----- | ----------- | --------- |
| `blinc_text` | Text shaping/rendering | blinc_gpu |
| `blinc_svg` | SVG parsing | blinc_app |
| `blinc_image` | Image loading | blinc_app |
| `blinc_platform` | Platform abstraction traits | extensions |

### Platform Extensions (publish with target gates)

| Crate | Target | Backend |
|-------|--------|---------|
| `blinc_platform_desktop` | macOS/Windows/Linux | wgpu (all backends) |
| `blinc_platform_android` | Android | Vulkan |
| `blinc_platform_ios` | iOS | Metal |

### Internal Only (do NOT publish)

| Crate | Reason |
|-------|--------|
| `blinc_recorder` | Internal debugging tool |
| `blinc_debugger` | Internal debugging UI |
| `blinc_runtime` | Experimental runtime |
| `blinc_test_suite` | Internal test utilities |

## Publish Order

Execute in this exact order (respects dependency graph):

```bash
# Phase 1: Foundation (no internal deps)
cargo publish -p blinc_macros
cargo publish -p blinc_platform
cargo publish -p blinc_icons
cargo publish -p blinc_core

# Phase 2: Core systems
cargo publish -p blinc_animation
cargo publish -p blinc_paint
cargo publish -p blinc_svg
cargo publish -p blinc_text

# Phase 3: Higher-level systems
cargo publish -p blinc_theme
cargo publish -p blinc_image
cargo publish -p blinc_layout

# Phase 4: GPU and components
cargo publish -p blinc_gpu
cargo publish -p blinc_cn

# Phase 5: Platform extensions
cargo publish -p blinc_platform_desktop
cargo publish -p blinc_platform_android
cargo publish -p blinc_platform_ios

# Phase 6: Application framework
cargo publish -p blinc_app

# Unified facade (after all its dependencies, including blinc_macros)
cargo publish -p blinc

# Phase 7: CLI (binary)
cargo publish -p blinc_cli
```

## Pre-Publish Checklist

### 1. Update Cargo.toml metadata

Each crate needs proper metadata:

```toml
[package]
name = "blinc_core"
version = "0.1.1"
edition = "2021"
license = "Apache-2.0"
repository = "https://github.com/project-blinc/Blinc"
documentation = "https://docs.rs/blinc_core"
readme = "README.md"
keywords = ["ui", "gui", "reactive", "framework"]
categories = ["gui", "graphics", "rendering"]
description = "Blinc core runtime - reactive signals, state machines, and event dispatch"
```

### 2. Convert path dependencies to version deps

Before publishing, convert path dependencies to version dependencies:

```toml
# Before (development)
blinc_core = { path = "../blinc_core" }

# After (publish)
blinc_core = { version = "0.1.1" }
```

Script to automate this (run before publish):

```bash
# scripts/prepare-publish.sh
for crate in crates/*/Cargo.toml extensions/*/Cargo.toml; do
    sed -i '' 's/path = "\.\.\/blinc_\([^"]*\)"/version = "0.1.1"/g' "$crate"
    sed -i '' 's/path = "\.\.\/\.\.\/crates\/blinc_\([^"]*\)"/version = "0.1.1"/g' "$crate"
    sed -i '' 's/path = "\.\.\/\.\.\/extensions\/blinc_\([^"]*\)"/version = "0.1.1"/g' "$crate"
done
```

### 3. Create README for each crate

Each published crate should have a README.md with:
- Brief description
- Installation instructions
- Basic usage example
- Link to main documentation

### 4. Verify each crate builds standalone

```bash
# Test each crate can build independently
for crate in blinc_core blinc_animation blinc_layout; do
    cargo build -p $crate
done
```

### 5. Run cargo publish --dry-run

```bash
# Verify publish will succeed
cargo publish -p blinc_core --dry-run
```

## Version Strategy

### Initial Release (v0.1.1)

- All crates start at `0.1.1`
- Use `0.x.y` to indicate API instability
- Document breaking changes in CHANGELOG.md

### Version Synchronization

Keep all Blinc crates at the same version for simplicity:

```toml
# Cargo.toml (workspace)
[workspace.package]
version = "0.1.1"
```

### Semantic Versioning

Once stable (1.0.0+):
- MAJOR: Breaking API changes
- MINOR: New features, backward compatible
- PATCH: Bug fixes only

## Automation

### GitHub Actions Workflow

Create `.github/workflows/publish.yml`:

```yaml
name: Publish to crates.io

on:
  push:
    tags:
      - 'v*'

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Login to crates.io
        run: cargo login ${{ secrets.CARGO_REGISTRY_TOKEN }}

      - name: Publish crates
        run: |
          # Phase 1
          cargo publish -p blinc_macros --no-verify
          cargo publish -p blinc_platform --no-verify
          cargo publish -p blinc_icons --no-verify
          cargo publish -p blinc_core --no-verify
          sleep 30  # Wait for crates.io to index

          # Phase 2
          cargo publish -p blinc_animation --no-verify
          cargo publish -p blinc_paint --no-verify
          cargo publish -p blinc_svg --no-verify
          cargo publish -p blinc_text --no-verify
          sleep 30

          # ... continue phases
```

### cargo-release Integration

Consider using [cargo-release](https://github.com/crate-ci/cargo-release) for automated releases:

```bash
cargo install cargo-release
cargo release --workspace 0.1.1
```

## Notes

### Circular Dependencies

There is a dev-dependency cycle between `blinc_core` and `blinc_animation`:
- `blinc_animation` depends on `blinc_core` (runtime)
- `blinc_core` has dev-dependency on `blinc_animation` (tests only)

This does NOT block publishing since dev-dependencies are not considered for the dependency graph.

### Platform-Specific Crates

Platform extensions have target-gated dependencies. When publishing:
- `blinc_platform_android` only builds on Android targets
- `blinc_platform_ios` only builds on iOS targets
- Use `--no-verify` flag for cross-compilation requirements

### Feature Flags

`blinc_app` has feature flags for platforms:
- `default = ["windowed"]` - Desktop with winit
- `android` - Android with Vulkan
- `ios` - iOS with Metal

Consumers select the appropriate feature:

```toml
# Desktop
blinc_app = "0.1.1"

# Android
blinc_app = { version = "0.1.1", default-features = false, features = ["android"] }

# iOS
blinc_app = { version = "0.1.1", default-features = false, features = ["ios"] }
```
