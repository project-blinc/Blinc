# Fuchsia Platform Template

This directory contains templates for building Blinc applications on Fuchsia OS.

## Quick Start

```bash
# 1. Run the complete setup (SDK + emulator + verification)
./scripts/setup-fuchsia-all.sh

# 2. Reload your shell
source ~/.zshrc  # or ~/.bashrc

# 3. Build for Fuchsia
cargo build --target x86_64-unknown-fuchsia

# 4. Start emulator and run
ffx emu start workbench_eng.x64 --headless
ffx component run fuchsia-pkg://fuchsia.com/your_app#meta/your_app.cm
```

## Directory Structure

```
fuchsia/
├── meta/
│   └── blinc_app.cml     # Component manifest template
├── BUILD.gn              # GN build file (fx build workflow)
├── BUILD.bazel           # Bazel build file (SDK samples workflow)
└── README.md             # This file
```

## Setup Scripts

| Script | Purpose |
|--------|---------|
| `setup-fuchsia-all.sh` | **Run this first** - complete setup |
| `setup-fuchsia-sdk.sh` | SDK installation only |
| `setup-fuchsia-emulator.sh` | Emulator setup |
| `verify-fuchsia-tools.sh` | Verify installation |

## Build Workflows

### Option 1: Cargo + Manual Packaging (Recommended for Blinc)

This is the simplest workflow for Rust-first development:

```bash
# Build
cargo build --target x86_64-unknown-fuchsia --release

# Package (using ffx)
ffx package build \
  --published-name my_app \
  --api-level HEAD \
  target/x86_64-unknown-fuchsia/release/my_app \
  meta/blinc_app.cml

# Run
ffx component run fuchsia-pkg://fuchsia.com/my_app#meta/blinc_app.cm
```

### Option 2: Bazel SDK Workflow

For integration with the Fuchsia SDK samples repository:

```bash
cd ~/.fuchsia/sdk-samples
# Copy your app into src/
tools/bazel build //src/my_app:pkg
tools/bazel run //src/my_app:pkg.component
```

### Option 3: GN/fx Workflow

For integration with a full Fuchsia source checkout:

```bash
fx set core.x64 --with //path/to/my_app
fx build
fx component run fuchsia-pkg://fuchsia.com/my_app#meta/blinc_app.cm
```

## Component Manifest

The `meta/blinc_app.cml` file defines your Fuchsia component. Key sections:

- **program**: Specifies the binary to run
- **capabilities**: What your component provides (ViewProvider for GUI apps)
- **use**: What system services you need (Flatland, input, Vulkan)
- **expose**: What you expose to parent components

### Required Capabilities

GUI applications **must** expose `fuchsia.ui.app.ViewProvider` and use:

| Protocol | Purpose |
|----------|---------|
| `fuchsia.ui.composition.Flatland` | Window compositing |
| `fuchsia.vulkan.loader.Loader` | GPU access |
| `fuchsia.sysmem2.Allocator` | GPU buffer allocation |

### Optional Capabilities

Add these based on your app's needs:

| Protocol | Purpose |
|----------|---------|
| `fuchsia.ui.pointer.TouchSource` | Touch input |
| `fuchsia.ui.pointer.MouseSource` | Mouse input |
| `fuchsia.ui.input3.Keyboard` | Keyboard input |
| `fuchsia.media.AudioRenderer` | Audio playback |
| `fuchsia.accessibility.semantics.SemanticsManager` | Accessibility |

## Environment Variables

After running setup, these are set automatically:

```bash
FUCHSIA_DIR=~/.fuchsia
FUCHSIA_SDK=~/.fuchsia/sdk-samples
FUCHSIA_BIN=~/.fuchsia/bin
PATH=$FUCHSIA_BIN:$PATH
```

## Troubleshooting

### "ffx not found"

```bash
# Re-run setup
./scripts/setup-fuchsia-all.sh

# Or manually add to PATH
export PATH="$HOME/.fuchsia/bin:$PATH"
```

### "target x86_64-unknown-fuchsia not found"

```bash
# Fuchsia targets may require nightly Rust
rustup target add x86_64-unknown-fuchsia
# If that fails:
rustup +nightly target add x86_64-unknown-fuchsia
```

### Emulator won't start

```bash
# Check virtualization support
sysctl -n kern.hv_support  # macOS - should be 1

# Try headless mode
ffx emu start workbench_eng.x64 --headless

# Check logs
ffx log
```

### Build errors with FIDL dependencies

The Fuchsia FIDL crates are NOT on crates.io. They're provided by:
- The Fuchsia SDK during `fx build`
- The `#[cfg(target_os = "fuchsia")]` gates in code

For development on macOS/Linux, the code compiles because Fuchsia-specific code is gated. The real FIDL bindings resolve when building within the Fuchsia tree.

## Resources

- [Fuchsia SDK](https://fuchsia.dev/fuchsia-src/development/sdk)
- [Component Manifests](https://fuchsia.dev/fuchsia-src/concepts/components/v2/component_manifests)
- [Flatland Guide](https://fuchsia.dev/fuchsia-src/development/graphics/flatland)
- [Input Protocol](https://fuchsia.dev/fuchsia-src/concepts/ui/input)
- [Rust on Fuchsia](https://fuchsia.dev/fuchsia-src/development/languages/rust)
