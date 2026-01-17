//! Stub syscall implementations for non-Fuchsia platforms
//!
//! These allow the crate to compile on macOS/Linux for development,
//! but operations return ERR_NOT_SUPPORTED at runtime.

#![allow(dead_code)]

use crate::{Handle, Status, RawHandle, channel::ChannelReadResult, channel::MessageBuf};

// Handle operations
pub fn handle_close(_handle: RawHandle) {
    // No-op on non-Fuchsia - handles don't exist
}

pub fn handle_duplicate(_handle: RawHandle, _rights: u32) -> crate::Result<Handle> {
    Err(Status::ERR_NOT_SUPPORTED)
}

pub fn handle_replace(_handle: RawHandle, _rights: u32) -> crate::Result<Handle> {
    Err(Status::ERR_NOT_SUPPORTED)
}

// Channel operations
pub fn channel_create() -> crate::Result<(Handle, Handle)> {
    // For development, we could return mock handles
    // But for now, indicate not supported
    Err(Status::ERR_NOT_SUPPORTED)
}

pub fn channel_write(
    _handle: RawHandle,
    _bytes: &[u8],
    _handles: &mut [Handle],
) -> crate::Result<()> {
    Err(Status::ERR_NOT_SUPPORTED)
}

pub fn channel_read(
    _handle: RawHandle,
    _buf: &mut MessageBuf,
) -> crate::Result<ChannelReadResult> {
    Err(Status::ERR_NOT_SUPPORTED)
}

// EventPair operations
pub fn eventpair_create() -> crate::Result<(Handle, Handle)> {
    Err(Status::ERR_NOT_SUPPORTED)
}

pub fn eventpair_signal_peer(_handle: RawHandle, _clear: u32, _set: u32) -> crate::Result<()> {
    Err(Status::ERR_NOT_SUPPORTED)
}

// VMO operations
pub fn vmo_create(_size: u64, _options: u32) -> crate::Result<Handle> {
    Err(Status::ERR_NOT_SUPPORTED)
}

pub fn vmo_get_size(_handle: RawHandle) -> crate::Result<u64> {
    Err(Status::ERR_NOT_SUPPORTED)
}

pub fn vmo_set_size(_handle: RawHandle, _size: u64) -> crate::Result<()> {
    Err(Status::ERR_NOT_SUPPORTED)
}

pub fn vmo_read(_handle: RawHandle, _data: &mut [u8], _offset: u64) -> crate::Result<()> {
    Err(Status::ERR_NOT_SUPPORTED)
}

pub fn vmo_write(_handle: RawHandle, _data: &[u8], _offset: u64) -> crate::Result<()> {
    Err(Status::ERR_NOT_SUPPORTED)
}

// Clock operations
pub fn clock_get_monotonic() -> crate::time::Time {
    crate::time::Time::from_nanos(0)
}
