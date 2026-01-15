//! FIDL message header

use crate::{FIDL_MAGIC, Error, Result};
use bitflags::bitflags;

/// Transaction ID (identifies request/response pairs)
pub type TransactionId = u32;

/// Method ordinal (identifies which method is being called)
pub type Ordinal = u64;

bitflags! {
    /// At-rest flags in message header
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct MessageFlags: u16 {
        /// No flags set
        const NONE = 0;
        /// Message uses wire format V2
        const USE_V2_WIRE_FORMAT = 1 << 1;
    }
}

bitflags! {
    /// Dynamic flags in message header
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct DynamicFlags: u8 {
        /// No flags set
        const NONE = 0;
        /// Flexible method (unknown ordinals allowed)
        const FLEXIBLE = 1 << 7;
    }
}

/// FIDL message header (16 bytes)
///
/// All FIDL messages begin with this header.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct MessageHeader {
    /// Transaction ID (0 for events/one-way calls)
    pub txid: TransactionId,
    /// At-rest flags
    pub at_rest_flags: MessageFlags,
    /// Dynamic flags
    pub dynamic_flags: DynamicFlags,
    /// Magic number (must be FIDL_MAGIC)
    pub magic: u8,
    /// Method ordinal
    pub ordinal: Ordinal,
}

impl MessageHeader {
    /// Header size in bytes
    pub const SIZE: usize = 16;

    /// Create a new request header
    pub fn new_request(txid: TransactionId, ordinal: Ordinal) -> Self {
        Self {
            txid,
            at_rest_flags: MessageFlags::USE_V2_WIRE_FORMAT,
            dynamic_flags: DynamicFlags::NONE,
            magic: FIDL_MAGIC,
            ordinal,
        }
    }

    /// Create a new response header
    pub fn new_response(txid: TransactionId, ordinal: Ordinal) -> Self {
        Self {
            txid,
            at_rest_flags: MessageFlags::USE_V2_WIRE_FORMAT,
            dynamic_flags: DynamicFlags::NONE,
            magic: FIDL_MAGIC,
            ordinal,
        }
    }

    /// Create a new event header (txid = 0)
    pub fn new_event(ordinal: Ordinal) -> Self {
        Self {
            txid: 0,
            at_rest_flags: MessageFlags::USE_V2_WIRE_FORMAT,
            dynamic_flags: DynamicFlags::NONE,
            magic: FIDL_MAGIC,
            ordinal,
        }
    }

    /// Create an epitaph header (signals channel closure)
    pub fn new_epitaph() -> Self {
        Self {
            txid: 0,
            at_rest_flags: MessageFlags::USE_V2_WIRE_FORMAT,
            dynamic_flags: DynamicFlags::NONE,
            magic: FIDL_MAGIC,
            ordinal: crate::EPITAPH_ORDINAL,
        }
    }

    /// Check if this is a request (has non-zero txid)
    pub fn is_request(&self) -> bool {
        self.txid != 0
    }

    /// Check if this is an event (txid = 0)
    pub fn is_event(&self) -> bool {
        self.txid == 0
    }

    /// Check if this is an epitaph
    pub fn is_epitaph(&self) -> bool {
        self.ordinal == crate::EPITAPH_ORDINAL
    }

    /// Set flexible flag
    pub fn set_flexible(&mut self) {
        self.dynamic_flags |= DynamicFlags::FLEXIBLE;
    }

    /// Check if flexible
    pub fn is_flexible(&self) -> bool {
        self.dynamic_flags.contains(DynamicFlags::FLEXIBLE)
    }

    /// Validate the header
    pub fn validate(&self) -> Result<()> {
        if self.magic != FIDL_MAGIC {
            return Err(Error::InvalidMagic {
                expected: FIDL_MAGIC,
                actual: self.magic,
            });
        }
        Ok(())
    }

    /// Encode header to bytes
    pub fn encode(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.txid.to_le_bytes());
        buf[4..6].copy_from_slice(&self.at_rest_flags.bits().to_le_bytes());
        buf[6] = self.dynamic_flags.bits();
        buf[7] = self.magic;
        buf[8..16].copy_from_slice(&self.ordinal.to_le_bytes());
        buf
    }

    /// Decode header from bytes
    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() < Self::SIZE {
            return Err(Error::BufferUnderflow {
                needed: Self::SIZE,
                available: buf.len(),
            });
        }

        let txid = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let at_rest_flags = MessageFlags::from_bits_truncate(
            u16::from_le_bytes([buf[4], buf[5]])
        );
        let dynamic_flags = DynamicFlags::from_bits_truncate(buf[6]);
        let magic = buf[7];
        let ordinal = u64::from_le_bytes([
            buf[8], buf[9], buf[10], buf[11],
            buf[12], buf[13], buf[14], buf[15],
        ]);

        let header = Self {
            txid,
            at_rest_flags,
            dynamic_flags,
            magic,
            ordinal,
        };

        header.validate()?;
        Ok(header)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_encode_decode() {
        let header = MessageHeader::new_request(123, 0xABCD1234);
        let encoded = header.encode();
        let decoded = MessageHeader::decode(&encoded).unwrap();
        assert_eq!(header, decoded);
    }

    #[test]
    fn test_header_event() {
        let header = MessageHeader::new_event(0x5678);
        assert!(header.is_event());
        assert!(!header.is_request());
    }

    #[test]
    fn test_header_epitaph() {
        let header = MessageHeader::new_epitaph();
        assert!(header.is_epitaph());
    }

    #[test]
    fn test_invalid_magic() {
        let mut buf = MessageHeader::new_request(1, 1).encode();
        buf[7] = 0xFF; // Invalid magic
        let result = MessageHeader::decode(&buf);
        assert!(matches!(result, Err(Error::InvalidMagic { .. })));
    }
}
