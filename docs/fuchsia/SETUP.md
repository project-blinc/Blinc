# Fuchsia Development Setup for Blinc

This guide covers setting up a Fuchsia development environment for building and testing Blinc applications.

## Quick Start

```bash
# 1. Install Fuchsia SDK
./scripts/setup-fuchsia-sdk.sh

# 2. Add Rust targets
rustup target add x86_64-unknown-fuchsia
rustup target add aarch64-unknown-fuchsia

# 3. Setup emulator (optional but recommended for testing)
./scripts/setup-fuchsia-emulator.sh

# 4. Build for Fuchsia
cargo build --target x86_64-unknown-fuchsia --features fuchsia --release
```

## Prerequisites

- **OS**: macOS (Intel/Apple Silicon) or Linux (x86_64)
- **RAM**: 16GB+ recommended (8GB minimum)
- **Disk**: 20GB+ free space
- **Rust**: 1.75+ with nightly toolchain
- **Virtualization**: KVM (Linux) or Hypervisor.framework (macOS) for emulator

## Step-by-Step Setup

### 1. Install the Fuchsia SDK

The SDK provides build tools, libraries, and the `ffx` CLI:

```bash
./scripts/setup-fuchsia-sdk.sh
```

This downloads:
- Fuchsia IDK (Interface Definition Kit)
- Build tools (fx, ffx)
- System headers and libraries
- Vulkan ICD (Installable Client Driver)

**Environment Setup** (add to `~/.bashrc` or `~/.zshrc`):

```bash
export FUCHSIA_DIR="$HOME/.fuchsia"
export PATH="$FUCHSIA_DIR/sdk/tools:$PATH"
```

### 2. Install Rust Targets

Add the Fuchsia cross-compilation targets:

```bash
# x86_64 (emulator, desktop Fuchsia devices)
rustup target add x86_64-unknown-fuchsia

# ARM64 (most Fuchsia devices)
rustup target add aarch64-unknown-fuchsia
```

### 3. Configure Cargo for Cross-Compilation

Create or update `~/.cargo/config.toml`:

```toml
[target.x86_64-unknown-fuchsia]
linker = "lld"
rustflags = ["-C", "link-arg=--target=x86_64-unknown-fuchsia"]

[target.aarch64-unknown-fuchsia]
linker = "lld"
rustflags = ["-C", "link-arg=--target=aarch64-unknown-fuchsia"]
```

**Note**: The Fuchsia SDK setup script creates this configuration automatically.

### 4. Build Blinc for Fuchsia

```bash
# Debug build (faster)
cargo build --target x86_64-unknown-fuchsia --features fuchsia

# Release build (optimized)
cargo build --target x86_64-unknown-fuchsia --features fuchsia --release

# Build an example
cargo build --example fuchsia_hello --target x86_64-unknown-fuchsia --features fuchsia
```

### 5. Set Up the Emulator

For testing without physical hardware:

```bash
./scripts/setup-fuchsia-emulator.sh
```

This downloads the emulator images and configures `ffx`.

**Start the emulator**:

```bash
# With graphics (for UI development)
ffx emu start workbench_eng.x64

# Headless (for CI/testing)
ffx emu start workbench_eng.x64 --headless
```

**Stop the emulator**:

```bash
ffx emu stop
```

## Running on the Emulator

### Package Your App

Fuchsia apps are distributed as packages. Create a package:

```bash
# Build the binary
cargo build --example fuchsia_hello --target x86_64-unknown-fuchsia --features fuchsia --release

# Create package structure
mkdir -p pkg/bin pkg/meta
cp target/x86_64-unknown-fuchsia/release/examples/fuchsia_hello pkg/bin/
cp examples/counter/platforms/fuchsia/meta/counter.cml pkg/meta/

# Create the package (requires Fuchsia SDK tools)
ffx package build pkg/
```

### Deploy and Run

```bash
# Add package repository
ffx repository add-from-pm ./pkg

# Run the component
ffx component run fuchsia-pkg://fuchsia.com/fuchsia_hello#meta/fuchsia_hello.cm
```

### View Output

```bash
# View logs
ffx log --filter blinc

# VNC for graphical output
ffx target vnc
```

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        Blinc App                             │
│  ┌─────────────────────────────────────────────────────────┐│
│  │                    FuchsiaApp::run()                    ││
│  │  - UI builder callback                                  ││
│  │  - Reactive state management                            ││
│  │  - Animation scheduling                                 ││
│  └────────────────────────┬────────────────────────────────┘│
│                           │                                  │
│  ┌────────────────────────┴────────────────────────────────┐│
│  │              blinc_platform_fuchsia                      ││
│  │  - FuchsiaPlatform (lifecycle)                          ││
│  │  - FuchsiaWindow (Scenic View)                          ││
│  │  - FuchsiaEventLoop (FIDL events)                       ││
│  │  - Input handling (touch, mouse, keyboard)              ││
│  └────────────────────────┬────────────────────────────────┘│
└───────────────────────────┼─────────────────────────────────┘
                            │
         ┌──────────────────┼──────────────────┐
         ▼                  ▼                  ▼
┌─────────────────┐ ┌──────────────┐ ┌─────────────────┐
│     Scenic      │ │    Vulkan    │ │   FIDL IPC      │
│  (Compositor)   │ │  (via Magma) │ │ (System Svcs)   │
└─────────────────┘ └──────────────┘ └─────────────────┘
```

## Component Manifest

Every Fuchsia app needs a component manifest (`.cml` file):

```json5
{
    program: {
        runner: "elf",
        binary: "bin/your_app",
    },
    use: [
        { protocol: "fuchsia.ui.scenic.Scenic" },
        { protocol: "fuchsia.vulkan.loader.Loader" },
    ],
    expose: [
        {
            protocol: "fuchsia.ui.app.ViewProvider",
            from: "self",
        },
    ],
}
```

See [examples/counter/platforms/fuchsia/meta/counter.cml](../../examples/counter/platforms/fuchsia/meta/counter.cml) for a complete example.

## Troubleshooting

### Build Errors

**"failed to resolve: use of unresolved module"**

Make sure you're using the correct feature flags:
```bash
cargo build --features fuchsia --no-default-features --target x86_64-unknown-fuchsia
```

**Linker errors**

Ensure the Fuchsia SDK sysroot is available:
```bash
ls $FUCHSIA_DIR/sdk/arch/x64/sysroot/
```

### Emulator Issues

**"KVM not available"**

Enable hardware virtualization in your BIOS, or run with software emulation (slower):
```bash
ffx emu start --gpu swiftshader_indirect workbench_eng.x64
```

**Emulator doesn't start**

Check for existing instances:
```bash
ffx emu list
ffx emu stop --all
```

### Debugging

**View component state**:
```bash
ffx component show your_component
```

**Capture system logs**:
```bash
ffx log --since now
```

**Profile GPU**:
```bash
ffx trace start --categories vulkan
# ... run app ...
ffx trace stop
```

## Resources

- [Fuchsia Documentation](https://fuchsia.dev)
- [Fuchsia SDK](https://fuchsia.dev/fuchsia-src/development/sdk)
- [Component Model](https://fuchsia.dev/fuchsia-src/concepts/components/v2)
- [Scenic Graphics](https://fuchsia.dev/fuchsia-src/concepts/graphics/scenic)
- [Flatland](https://fuchsia.dev/fuchsia-src/development/graphics/flatland)
- [Vulkan on Fuchsia](https://fuchsia.dev/fuchsia-src/development/graphics/vulkan)

## Status

The Fuchsia platform support is currently in **early development**:

- [x] Platform traits implemented (stub)
- [x] Cross-compilation working
- [x] GPU configuration (Vulkan)
- [x] Input types defined
- [ ] Scenic integration (TODO - requires FIDL)
- [ ] Event loop (TODO - requires fuchsia-async)
- [ ] Full testing on emulator

Contributions welcome! See the [platform plan](../../.claude/plans/keen-roaming-bentley.md) for details.
