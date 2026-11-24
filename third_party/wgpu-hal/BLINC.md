# wgpu-hal 26.0.5, with one change for Blinc

This folder is the published `wgpu-hal` 26.0.5 crate. The root `Cargo.toml` uses it in place of
the crates.io copy, through `[patch.crates-io]`. Only workspace builds use it. A crate that
depends on `blinc_gpu` from crates.io gets the published `wgpu-hal`.

## The change

wgpu records each render pass into its own command buffer, and adds one more before it for the
pass's resource transitions. On Metal, a transition encodes nothing, but the published crate makes
an `MTLCommandBuffer` in `begin_encoding` anyway. Each of these empty buffers is created,
committed and completed, and each costs CPU time in the app and in Metal's queue thread.

In this copy, the Metal command encoder waits for the first command that needs an
`MTLCommandBuffer`, and makes it then. If nothing was encoded, `end_encoding` returns no buffer and
`submit` skips it. The fence signal goes on the last buffer that has commands. If no buffer has
commands, the signal goes on a new buffer, as before.

The change is in `src/metal/command.rs`, `src/metal/mod.rs` and `src/metal/device.rs`. The other
backends are unchanged. `Cargo.toml` also allows the `mismatched_lifetime_syntaxes` lint. Cargo
hides the warnings of a registry crate, but not of a path dependency such as this one.

## When wgpu is upgraded

1. Copy the new `wgpu-hal` from the cargo registry into this folder.
2. Remove `Cargo.toml.orig`, `Cargo.lock`, `.cargo_vcs_info.json` and `.cargo-ok`.
3. If the new release makes Metal command buffers on first use, remove this folder and the
   `[patch.crates-io]` entry. Otherwise, apply the change again.
4. Update `ci/workspace.lock`.

The measurements are in git-bug issue
7936e4bcdce1ff52853aebec21d68df924e60aa5f04f56d7e59f0479d62720e4.
