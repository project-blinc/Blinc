//! Generic handle wrapper for Zircon kernel objects

use crate::{RawHandle, HANDLE_INVALID, Status, Rights, sys};

/// A borrowed reference to a handle
#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
pub struct HandleRef<'a> {
    raw: RawHandle,
    _phantom: std::marker::PhantomData<&'a Handle>,
}

impl<'a> HandleRef<'a> {
    /// Create a HandleRef from a raw handle
    ///
    /// # Safety
    /// The handle must be valid for the lifetime 'a
    pub unsafe fn from_raw(raw: RawHandle) -> Self {
        HandleRef {
            raw,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get the raw handle value
    pub fn raw_handle(&self) -> RawHandle {
        self.raw
    }
}

/// Trait for types that can be converted to a HandleRef
pub trait AsHandleRef {
    /// Get a reference to the underlying handle
    fn as_handle_ref(&self) -> HandleRef<'_>;

    /// Get the raw handle value
    fn raw_handle(&self) -> RawHandle {
        self.as_handle_ref().raw_handle()
    }

    /// Check if this handle is valid
    fn is_invalid(&self) -> bool {
        self.raw_handle() == HANDLE_INVALID
    }
}

/// Trait for types backed by a Zircon handle
pub trait HandleBased: AsHandleRef + From<Handle> + Into<Handle> {
    /// Create an instance from a raw handle
    ///
    /// # Safety
    /// The caller must ensure the handle is valid and of the correct type
    unsafe fn from_raw(raw: RawHandle) -> Self {
        Handle::from_raw(raw).into()
    }

    /// Convert into a raw handle, consuming self
    fn into_raw(self) -> RawHandle {
        let handle: Handle = self.into();
        handle.into_raw()
    }
}

/// A generic handle to a Zircon kernel object
///
/// When dropped, the handle is automatically closed.
#[derive(Debug)]
#[repr(transparent)]
pub struct Handle(RawHandle);

impl Handle {
    /// Create a Handle from a raw handle value
    ///
    /// # Safety
    /// The caller must ensure the handle is valid
    pub unsafe fn from_raw(raw: RawHandle) -> Self {
        Handle(raw)
    }

    /// Get the raw handle value without consuming self
    pub fn raw_handle(&self) -> RawHandle {
        self.0
    }

    /// Convert into a raw handle, consuming self without closing
    pub fn into_raw(self) -> RawHandle {
        let raw = self.0;
        std::mem::forget(self);
        raw
    }

    /// Check if this is an invalid handle
    pub fn is_invalid(&self) -> bool {
        self.0 == HANDLE_INVALID
    }

    /// Replace this handle with an invalid handle, returning the old value
    pub fn take(&mut self) -> Handle {
        let raw = self.0;
        self.0 = HANDLE_INVALID;
        Handle(raw)
    }

    /// Create an invalid handle
    pub const fn invalid() -> Self {
        Handle(HANDLE_INVALID)
    }

    /// Duplicate this handle with the given rights
    pub fn duplicate(&self, rights: Rights) -> crate::Result<Handle> {
        if self.is_invalid() {
            return Err(Status::ERR_BAD_HANDLE);
        }
        sys::handle_duplicate(self.0, rights.bits())
    }

    /// Replace this handle with one that has reduced rights
    pub fn replace(self, rights: Rights) -> crate::Result<Handle> {
        if self.is_invalid() {
            return Err(Status::ERR_BAD_HANDLE);
        }
        let raw = self.into_raw();
        sys::handle_replace(raw, rights.bits())
    }
}

impl AsHandleRef for Handle {
    fn as_handle_ref(&self) -> HandleRef<'_> {
        HandleRef {
            raw: self.0,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.is_invalid() {
            sys::handle_close(self.0);
        }
    }
}

impl Default for Handle {
    fn default() -> Self {
        Handle::invalid()
    }
}

// Note: Handle intentionally does not implement Clone.
// Use duplicate() to create a copy with explicit rights.
