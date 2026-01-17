# blinc_platform_fuchsia

Fuchsia OS platform support for the Blinc UI framework.

## Overview

This crate provides Fuchsia-specific implementations of the `blinc_platform` traits:

- **FuchsiaPlatform** - Main platform implementation using Scenic compositor
- **FuchsiaWindow** - Window wrapper using Scenic Views
- **FuchsiaEventLoop** - Event loop using fuchsia-async executor
- **FuchsiaAssetLoader** - Asset loading from Fuchsia packages

## Requirements

- Fuchsia SDK (`fx` toolchain)
- Rust targets: `x86_64-unknown-fuchsia` or `aarch64-unknown-fuchsia`
- FEMU emulator or Fuchsia device

## Building

```bash
# Add Fuchsia Rust targets
rustup target add x86_64-unknown-fuchsia
rustup target add aarch64-unknown-fuchsia

# Build with Fuchsia SDK
fx set core.x64 --with //third_party/blinc
fx build
```

## Component Manifest

Example `meta/your_app.cml`:

```json5
{
    program: {
        runner: "elf",
        binary: "bin/your_app",
    },
    capabilities: [
        { protocol: "fuchsia.ui.app.ViewProvider" },
    ],
    use: [
        { protocol: "fuchsia.ui.scenic.Scenic" },
        { protocol: "fuchsia.ui.composition.Flatland" },
        { protocol: "fuchsia.vulkan.loader.Loader" },
    ],
}
```

## Status

**Work in Progress** - This platform extension is scaffolded but requires full implementation:

- [ ] Scenic View integration
- [ ] FIDL event handling
- [ ] Vulkan surface via ImagePipe
- [ ] Touch input from fuchsia.ui.pointer
- [ ] Keyboard input from fuchsia.ui.input3

## License

Apache-2.0 OR MIT
