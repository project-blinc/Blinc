# Blinc Rust Fuchsia SDK Plan

Build a standalone Rust SDK for Fuchsia development without requiring the full Fuchsia source tree.

## Existing Resources

| Resource | Status | Notes |
|----------|--------|-------|
| [fuchsia-zircon](https://crates.io/crates/fuchsia-zircon) | Old (v0.3.3, 2017) | Basic Zircon bindings on crates.io |
| [fuchsia-zircon-sys](https://crates.io/crates/fuchsia-zircon-sys) | Old (v0.3.3) | Low-level syscall bindings |
| [FuchsiaRustSDK](https://github.com/ArnaudWald/ArnaudWald/FuchsiaRustSDK) | 2019 | Minimal SDK, good reference |
| Fuchsia SDK (vendor/) | Current | Has fidlc, fidlgen_rust, sysroot |

## SDK Components to Build

### 1. blinc_fuchsia_zircon (Zircon Runtime)
Modernized Zircon kernel bindings.

```
extensions/blinc_fuchsia_zircon/
├── src/
│   ├── lib.rs          # Re-exports
│   ├── status.rs       # zx_status_t error codes
│   ├── handle.rs       # Handle wrapper
│   ├── channel.rs      # Channel IPC
│   ├── eventpair.rs    # EventPair for ViewRef
│   ├── vmo.rs          # Virtual Memory Objects
│   ├── port.rs         # Async port
│   └── time.rs         # Time types
└── Cargo.toml
```

Key types needed:
- `Handle` - Generic kernel object handle
- `Channel` - IPC channel for FIDL
- `EventPair` - For ViewRef/ViewRefControl
- `Vmo` - For buffer sharing with GPU
- `Status` - Error codes

### 2. blinc_fuchsia_async (Async Runtime)
Fuchsia-compatible async executor.

```
extensions/blinc_fuchsia_async/
├── src/
│   ├── lib.rs
│   ├── executor.rs     # Single-threaded executor
│   ├── timer.rs        # Async timers
│   └── channel.rs      # Async channel operations
└── Cargo.toml
```

### 3. blinc_fidl (FIDL Runtime)
FIDL encoding/decoding runtime.

```
extensions/blinc_fidl/
├── src/
│   ├── lib.rs
│   ├── encoding.rs     # Wire format encoding
│   ├── decoding.rs     # Wire format decoding
│   ├── endpoints.rs    # Client/Server endpoints
│   ├── epitaph.rs      # Channel close reasons
│   └── error.rs        # FIDL errors
└── Cargo.toml
```

### 4. blinc_fuchsia_bindings (Generated - DONE)
Already created - FIDL type definitions.

## Implementation Strategy

### Phase 1: Minimal Zircon Types (Stubs)
Create type definitions that compile on all platforms but only work on Fuchsia.

```rust
// On non-Fuchsia: stubs that panic
// On Fuchsia: real syscall wrappers

#[cfg(target_os = "fuchsia")]
mod sys {
    extern "C" {
        pub fn zx_channel_create(...) -> i32;
        pub fn zx_channel_write(...) -> i32;
        pub fn zx_channel_read(...) -> i32;
    }
}

#[cfg(not(target_os = "fuchsia"))]
mod sys {
    pub fn zx_channel_create(...) -> i32 { panic!("Fuchsia only") }
}
```

### Phase 2: FIDL Runtime
Implement FIDL wire format encoding/decoding based on the spec:
https://fuchsia.dev/fuchsia-src/reference/fidl/language/wire-format

Key concepts:
- 8-byte alignment
- Little-endian
- Out-of-line data for strings/vectors
- Handle transfer via channel

### Phase 3: Update FIDL Code Generator
Modify `generate-fuchsia-fidl.sh` to use our runtime crates:

```rust
// Instead of:
use fidl::...;
use fuchsia_zircon as zx;

// Generate:
use blinc_fidl::...;
use blinc_fuchsia_zircon as zx;
```

### Phase 4: Integration
Update `blinc_platform_fuchsia` to use the SDK:

```toml
[dependencies]
blinc_fuchsia_zircon = { path = "..." }
blinc_fuchsia_async = { path = "..." }
blinc_fidl = { path = "..." }
blinc_fuchsia_bindings = { path = "..." }
```

## Files to Create

| Crate | Files | Priority |
|-------|-------|----------|
| blinc_fuchsia_zircon | 8 | High |
| blinc_fidl | 6 | High |
| blinc_fuchsia_async | 4 | Medium |

## Build Workflow

```bash
# 1. Generate FIDL bindings (already works)
./scripts/generate-fuchsia-fidl.sh

# 2. Build for host (stubs)
cargo build -p blinc_platform_fuchsia

# 3. Build for Fuchsia (real syscalls)
cargo build --target x86_64-unknown-fuchsia -p blinc_platform_fuchsia \
    -Z build-std=std,panic_abort
```

## Testing Strategy

1. **Host testing**: Compile and run unit tests with stub implementations
2. **Fuchsia testing**: Use emulator to run integration tests
3. **CI**: Test compilation for both host and Fuchsia targets

## References

- [FIDL Wire Format](https://fuchsia.dev/fuchsia-src/reference/fidl/language/wire-format)
- [Zircon Syscalls](https://fuchsia.dev/fuchsia-src/reference/syscalls)
- [FuchsiaRustSDK](https://github.com/ArnaudWald/ArnaudWald/FuchsiaRustSDK) (reference implementation)
- [fuchsia-zircon crate](https://docs.rs/fuchsia-zircon/0.3.1/fuchsia_zircon/) (old but useful)

## Timeline Estimate

This is a significant undertaking. The FIDL runtime alone is complex.
Consider prioritizing the minimal subset needed for Blinc:
- Channel (for FIDL IPC)
- EventPair (for ViewRef)
- Vmo (for GPU buffers)
- Basic FIDL encoding for UI protocols
