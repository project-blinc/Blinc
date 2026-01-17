//! Zircon event pairs for signaling

use crate::{Handle, HandleBased, HandleRef, AsHandleRef, sys};

/// An event pair - two linked event objects
///
/// Event pairs are used for ViewRef/ViewRefControl in Fuchsia UI.
/// Signals on one endpoint can be observed on the peer.
#[derive(Debug)]
#[repr(transparent)]
pub struct EventPair(Handle);

impl EventPair {
    /// Create a new event pair
    pub fn create() -> crate::Result<(EventPair, EventPair)> {
        let (h0, h1) = sys::eventpair_create()?;
        Ok((EventPair(h0), EventPair(h1)))
    }

    /// Signal this endpoint
    pub fn signal_peer(&self, clear: u32, set: u32) -> crate::Result<()> {
        sys::eventpair_signal_peer(self.0.raw_handle(), clear, set)
    }
}

impl AsHandleRef for EventPair {
    fn as_handle_ref(&self) -> HandleRef<'_> {
        self.0.as_handle_ref()
    }
}

impl From<Handle> for EventPair {
    fn from(handle: Handle) -> Self {
        EventPair(handle)
    }
}

impl From<EventPair> for Handle {
    fn from(ep: EventPair) -> Self {
        ep.0
    }
}

impl HandleBased for EventPair {}
