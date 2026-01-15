//! Zircon channels for IPC (FIDL communication)

use crate::{Handle, HandleBased, HandleRef, AsHandleRef, Status, sys};

/// A Zircon channel endpoint
///
/// Channels are bidirectional, message-oriented IPC primitives.
/// They are the foundation of FIDL communication.
#[derive(Debug)]
#[repr(transparent)]
pub struct Channel(Handle);

impl Channel {
    /// Create a new channel pair
    pub fn create() -> crate::Result<(Channel, Channel)> {
        let (h0, h1) = sys::channel_create()?;
        Ok((Channel(h0), Channel(h1)))
    }

    /// Write a message to the channel
    ///
    /// # Arguments
    /// * `bytes` - The message data
    /// * `handles` - Handles to transfer with the message
    pub fn write(&self, bytes: &[u8], handles: &mut [Handle]) -> crate::Result<()> {
        if self.as_handle_ref().raw_handle() == crate::HANDLE_INVALID {
            return Err(Status::ERR_BAD_HANDLE);
        }
        sys::channel_write(self.0.raw_handle(), bytes, handles)
    }

    /// Read a message from the channel
    ///
    /// # Arguments
    /// * `buf` - Buffer to receive the message
    pub fn read(&self, buf: &mut MessageBuf) -> crate::Result<ChannelReadResult> {
        if self.as_handle_ref().raw_handle() == crate::HANDLE_INVALID {
            return Err(Status::ERR_BAD_HANDLE);
        }
        sys::channel_read(self.0.raw_handle(), buf)
    }

    /// Read a message, returning Ok(None) if the channel is empty
    pub fn read_opt(&self, buf: &mut MessageBuf) -> crate::Result<Option<ChannelReadResult>> {
        match self.read(buf) {
            Ok(result) => Ok(Some(result)),
            Err(Status::ERR_SHOULD_WAIT) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Check if the peer endpoint has been closed
    pub fn is_peer_closed(&self) -> bool {
        // In real implementation, this would check signals
        false
    }
}

impl AsHandleRef for Channel {
    fn as_handle_ref(&self) -> HandleRef<'_> {
        self.0.as_handle_ref()
    }
}

impl From<Handle> for Channel {
    fn from(handle: Handle) -> Self {
        Channel(handle)
    }
}

impl From<Channel> for Handle {
    fn from(channel: Channel) -> Self {
        channel.0
    }
}

impl HandleBased for Channel {}

/// Result of a channel read operation
#[derive(Debug)]
pub struct ChannelReadResult {
    /// Number of bytes read
    pub bytes: usize,
    /// Number of handles read
    pub handles: usize,
}

/// Buffer for channel messages
#[derive(Debug, Default)]
pub struct MessageBuf {
    /// Message bytes
    pub bytes: Vec<u8>,
    /// Handles transferred with the message
    pub handles: Vec<Handle>,
}

impl MessageBuf {
    /// Create a new empty message buffer
    pub fn new() -> Self {
        MessageBuf {
            bytes: Vec::new(),
            handles: Vec::new(),
        }
    }

    /// Create a message buffer with pre-allocated capacity
    pub fn with_capacity(bytes: usize, handles: usize) -> Self {
        MessageBuf {
            bytes: Vec::with_capacity(bytes),
            handles: Vec::with_capacity(handles),
        }
    }

    /// Clear the buffer for reuse
    pub fn clear(&mut self) {
        self.bytes.clear();
        // Note: handles are NOT dropped, caller should handle them
        self.handles.clear();
    }

    /// Ensure capacity for at least the given sizes
    pub fn ensure_capacity(&mut self, bytes: usize, handles: usize) {
        self.bytes.reserve(bytes.saturating_sub(self.bytes.capacity()));
        self.handles.reserve(handles.saturating_sub(self.handles.capacity()));
    }
}
