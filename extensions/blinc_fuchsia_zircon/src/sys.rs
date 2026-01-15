//! Real Zircon syscall bindings (Fuchsia only)
//!
//! These are the actual FFI bindings to the Zircon kernel.
//! On Fuchsia, we link against libzircon.so which provides these syscalls.

#![cfg(target_os = "fuchsia")]

use crate::{Handle, RawHandle, Status, channel::ChannelReadResult, channel::MessageBuf};

// Link against libzircon.so
// Zircon syscall declarations - these match vendor/fuchsia-sdk/arch/*/sysroot/include/zircon/syscalls.h
#[link(name = "zircon")]
extern "C" {
    fn zx_handle_close(handle: RawHandle) -> i32;
    fn zx_handle_duplicate(handle: RawHandle, rights: u32, out: *mut RawHandle) -> i32;
    fn zx_handle_replace(handle: RawHandle, rights: u32, out: *mut RawHandle) -> i32;

    fn zx_channel_create(options: u32, out0: *mut RawHandle, out1: *mut RawHandle) -> i32;
    fn zx_channel_write(
        handle: RawHandle,
        options: u32,
        bytes: *const u8,
        num_bytes: u32,
        handles: *const RawHandle,
        num_handles: u32,
    ) -> i32;
    fn zx_channel_read(
        handle: RawHandle,
        options: u32,
        bytes: *mut u8,
        handles: *mut RawHandle,
        num_bytes: u32,
        num_handles: u32,
        actual_bytes: *mut u32,
        actual_handles: *mut u32,
    ) -> i32;

    fn zx_eventpair_create(options: u32, out0: *mut RawHandle, out1: *mut RawHandle) -> i32;
    fn zx_object_signal_peer(handle: RawHandle, clear_mask: u32, set_mask: u32) -> i32;

    fn zx_vmo_create(size: u64, options: u32, out: *mut RawHandle) -> i32;
    fn zx_vmo_get_size(handle: RawHandle, size: *mut u64) -> i32;
    fn zx_vmo_set_size(handle: RawHandle, size: u64) -> i32;
    fn zx_vmo_read(handle: RawHandle, buffer: *mut u8, offset: u64, buffer_size: usize) -> i32;
    fn zx_vmo_write(handle: RawHandle, buffer: *const u8, offset: u64, buffer_size: usize) -> i32;

    fn zx_clock_get_monotonic() -> i64;
}

// Safe wrappers

pub fn handle_close(handle: RawHandle) {
    unsafe { zx_handle_close(handle) };
}

pub fn handle_duplicate(handle: RawHandle, rights: u32) -> crate::Result<Handle> {
    let mut out: RawHandle = 0;
    let status = unsafe { zx_handle_duplicate(handle, rights, &mut out) };
    crate::ok(status)?;
    Ok(unsafe { Handle::from_raw(out) })
}

pub fn handle_replace(handle: RawHandle, rights: u32) -> crate::Result<Handle> {
    let mut out: RawHandle = 0;
    let status = unsafe { zx_handle_replace(handle, rights, &mut out) };
    crate::ok(status)?;
    Ok(unsafe { Handle::from_raw(out) })
}

pub fn channel_create() -> crate::Result<(Handle, Handle)> {
    let mut h0: RawHandle = 0;
    let mut h1: RawHandle = 0;
    let status = unsafe { zx_channel_create(0, &mut h0, &mut h1) };
    crate::ok(status)?;
    Ok(unsafe { (Handle::from_raw(h0), Handle::from_raw(h1)) })
}

pub fn channel_write(
    handle: RawHandle,
    bytes: &[u8],
    handles: &mut [Handle],
) -> crate::Result<()> {
    // Convert handles to raw handles for syscall
    let raw_handles: Vec<RawHandle> = handles.iter().map(|h| h.raw_handle()).collect();

    let status = unsafe {
        zx_channel_write(
            handle,
            0,
            bytes.as_ptr(),
            bytes.len() as u32,
            raw_handles.as_ptr(),
            raw_handles.len() as u32,
        )
    };

    // On success, handles are transferred and should not be closed
    if status == 0 {
        for h in handles.iter_mut() {
            std::mem::forget(std::mem::take(h));
        }
    }

    crate::ok(status)
}

pub fn channel_read(handle: RawHandle, buf: &mut MessageBuf) -> crate::Result<ChannelReadResult> {
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;

    // First call to get sizes
    let status = unsafe {
        zx_channel_read(
            handle,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            0,
            &mut actual_bytes,
            &mut actual_handles,
        )
    };

    if status != Status::ERR_BUFFER_TOO_SMALL.into_raw() && status != 0 {
        return Err(Status::from_raw(status));
    }

    // Allocate space
    buf.bytes.resize(actual_bytes as usize, 0);
    let mut raw_handles: Vec<RawHandle> = vec![0; actual_handles as usize];

    // Second call to get data
    let status = unsafe {
        zx_channel_read(
            handle,
            0,
            buf.bytes.as_mut_ptr(),
            raw_handles.as_mut_ptr(),
            actual_bytes,
            actual_handles,
            &mut actual_bytes,
            &mut actual_handles,
        )
    };

    crate::ok(status)?;

    // Convert raw handles to Handle
    buf.handles = raw_handles
        .into_iter()
        .map(|h| unsafe { Handle::from_raw(h) })
        .collect();

    Ok(ChannelReadResult {
        bytes: actual_bytes as usize,
        handles: actual_handles as usize,
    })
}

pub fn eventpair_create() -> crate::Result<(Handle, Handle)> {
    let mut h0: RawHandle = 0;
    let mut h1: RawHandle = 0;
    let status = unsafe { zx_eventpair_create(0, &mut h0, &mut h1) };
    crate::ok(status)?;
    Ok(unsafe { (Handle::from_raw(h0), Handle::from_raw(h1)) })
}

pub fn eventpair_signal_peer(handle: RawHandle, clear: u32, set: u32) -> crate::Result<()> {
    let status = unsafe { zx_object_signal_peer(handle, clear, set) };
    crate::ok(status)
}

pub fn vmo_create(size: u64, options: u32) -> crate::Result<Handle> {
    let mut out: RawHandle = 0;
    let status = unsafe { zx_vmo_create(size, options, &mut out) };
    crate::ok(status)?;
    Ok(unsafe { Handle::from_raw(out) })
}

pub fn vmo_get_size(handle: RawHandle) -> crate::Result<u64> {
    let mut size: u64 = 0;
    let status = unsafe { zx_vmo_get_size(handle, &mut size) };
    crate::ok(status)?;
    Ok(size)
}

pub fn vmo_set_size(handle: RawHandle, size: u64) -> crate::Result<()> {
    let status = unsafe { zx_vmo_set_size(handle, size) };
    crate::ok(status)
}

pub fn vmo_read(handle: RawHandle, data: &mut [u8], offset: u64) -> crate::Result<()> {
    let status = unsafe { zx_vmo_read(handle, data.as_mut_ptr(), offset, data.len()) };
    crate::ok(status)
}

pub fn vmo_write(handle: RawHandle, data: &[u8], offset: u64) -> crate::Result<()> {
    let status = unsafe { zx_vmo_write(handle, data.as_ptr(), offset, data.len()) };
    crate::ok(status)
}

pub fn clock_get_monotonic() -> crate::time::Time {
    let nanos = unsafe { zx_clock_get_monotonic() };
    crate::time::Time::from_nanos(nanos)
}
