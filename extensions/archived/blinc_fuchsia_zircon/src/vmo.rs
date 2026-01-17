//! Zircon Virtual Memory Objects (VMO)

use crate::{Handle, HandleBased, HandleRef, AsHandleRef, sys};

/// A Virtual Memory Object
///
/// VMOs are used for sharing memory between processes, including
/// GPU buffer sharing via sysmem.
#[derive(Debug)]
#[repr(transparent)]
pub struct Vmo(Handle);

impl Vmo {
    /// Create a new VMO with the given size
    pub fn create(size: u64) -> crate::Result<Vmo> {
        let handle = sys::vmo_create(size, 0)?;
        Ok(Vmo(handle))
    }

    /// Create a VMO with options
    pub fn create_with_opts(size: u64, opts: VmoOptions) -> crate::Result<Vmo> {
        let handle = sys::vmo_create(size, opts.bits())?;
        Ok(Vmo(handle))
    }

    /// Get the size of the VMO
    pub fn get_size(&self) -> crate::Result<u64> {
        sys::vmo_get_size(self.0.raw_handle())
    }

    /// Set the size of the VMO
    pub fn set_size(&self, size: u64) -> crate::Result<()> {
        sys::vmo_set_size(self.0.raw_handle(), size)
    }

    /// Read from the VMO
    pub fn read(&self, data: &mut [u8], offset: u64) -> crate::Result<()> {
        sys::vmo_read(self.0.raw_handle(), data, offset)
    }

    /// Write to the VMO
    pub fn write(&self, data: &[u8], offset: u64) -> crate::Result<()> {
        sys::vmo_write(self.0.raw_handle(), data, offset)
    }
}

impl AsHandleRef for Vmo {
    fn as_handle_ref(&self) -> HandleRef<'_> {
        self.0.as_handle_ref()
    }
}

impl From<Handle> for Vmo {
    fn from(handle: Handle) -> Self {
        Vmo(handle)
    }
}

impl From<Vmo> for Handle {
    fn from(vmo: Vmo) -> Self {
        vmo.0
    }
}

impl HandleBased for Vmo {}

use bitflags::bitflags;

bitflags! {
    /// Options for VMO creation
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct VmoOptions: u32 {
        const NONE = 0;
        /// VMO can be resized
        const RESIZABLE = 1 << 1;
    }
}
