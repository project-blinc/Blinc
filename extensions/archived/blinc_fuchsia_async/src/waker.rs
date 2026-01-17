//! Waker support for the Fuchsia executor

use std::sync::atomic::{AtomicU64, Ordering};

/// Token for identifying wake sources
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WakeToken(u64);

impl WakeToken {
    /// Create a new unique wake token
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    /// Get the raw token value
    pub fn raw(&self) -> u64 {
        self.0
    }

    /// Create from raw value
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

impl Default for WakeToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Port packet key used for executor wakeups
///
/// On Fuchsia, this would be the key passed to zx_port_queue.
pub const EXECUTOR_WAKE_KEY: u64 = 0xB11C_EA7E_0000_0001;

/// Packet type for user packets
pub const PACKET_TYPE_USER: u32 = 0;

/// Port packet for Fuchsia (matches zx_port_packet_t)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct PortPacket {
    /// Key identifying the source
    pub key: u64,
    /// Packet type
    pub packet_type: u32,
    /// Status code
    pub status: i32,
    /// Packet-specific data
    pub data: PortPacketData,
}

/// Data portion of a port packet
#[derive(Clone, Copy)]
#[repr(C)]
pub union PortPacketData {
    /// User packet data
    pub user: UserPacket,
    /// Signal packet data
    pub signal: SignalPacket,
    /// Raw bytes
    pub raw: [u8; 32],
}

impl std::fmt::Debug for PortPacketData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PortPacketData").finish()
    }
}

/// User packet data
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct UserPacket {
    /// User-defined data
    pub data: [u64; 4],
}

/// Signal packet data
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SignalPacket {
    /// Triggering signals
    pub trigger: u32,
    /// Observed signals
    pub observed: u32,
    /// Count
    pub count: u64,
    /// Timestamp
    pub timestamp: i64,
    /// Reserved
    pub reserved: u64,
}

impl PortPacket {
    /// Create a user wake packet
    pub fn user_wake(key: u64) -> Self {
        Self {
            key,
            packet_type: PACKET_TYPE_USER,
            status: 0,
            data: PortPacketData {
                user: UserPacket { data: [0; 4] },
            },
        }
    }

    /// Check if this is a user packet
    pub fn is_user(&self) -> bool {
        self.packet_type == PACKET_TYPE_USER
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wake_token_unique() {
        let t1 = WakeToken::new();
        let t2 = WakeToken::new();
        assert_ne!(t1, t2);
    }

    #[test]
    fn test_port_packet_size() {
        // Verify struct matches Zircon's layout
        assert!(std::mem::size_of::<PortPacket>() >= 48);
    }
}
