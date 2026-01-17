//! FIDL message decoder

use crate::{
    Error, Result, MessageHeader, HandleInfo,
    handle::{HANDLE_ABSENT, HANDLE_PRESENT, ObjectType},
    MAX_RECURSION_DEPTH,
};
use blinc_fuchsia_zircon::{Handle, Rights};

/// FIDL message decoder
///
/// Decodes FIDL messages from bytes + handles received from a channel.
#[derive(Debug)]
pub struct Decoder<'a> {
    /// Message bytes
    bytes: &'a [u8],
    /// Current read position
    offset: usize,
    /// End of inline data (start of out-of-line)
    inline_end: usize,
    /// Handles received with message
    handles: Vec<Handle>,
    /// Current handle index
    handle_index: usize,
    /// Recursion depth
    depth: usize,
}

impl<'a> Decoder<'a> {
    /// Create a new decoder
    ///
    /// # Arguments
    /// * `bytes` - Message bytes (inline + out-of-line)
    /// * `handles` - Handles received with the message
    pub fn new(bytes: &'a [u8], handles: Vec<Handle>) -> Self {
        Self {
            bytes,
            offset: 0,
            inline_end: bytes.len(), // Will be updated when we know the structure
            handles,
            handle_index: 0,
            depth: 0,
        }
    }

    /// Create a decoder with known inline size
    pub fn with_inline_size(bytes: &'a [u8], handles: Vec<Handle>, inline_size: usize) -> Self {
        Self {
            bytes,
            offset: 0,
            inline_end: inline_size.min(bytes.len()),
            handles,
            handle_index: 0,
            depth: 0,
        }
    }

    /// Read the message header
    pub fn read_header(&mut self) -> Result<MessageHeader> {
        if self.bytes.len() < MessageHeader::SIZE {
            return Err(Error::BufferUnderflow {
                needed: MessageHeader::SIZE,
                available: self.bytes.len(),
            });
        }
        let header = MessageHeader::decode(&self.bytes[..MessageHeader::SIZE])?;
        self.offset = MessageHeader::SIZE;
        Ok(header)
    }

    /// Remaining bytes
    pub fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    /// Current offset
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Check if we have enough bytes
    fn check_remaining(&self, needed: usize) -> Result<()> {
        if self.remaining() < needed {
            return Err(Error::BufferUnderflow {
                needed,
                available: self.remaining(),
            });
        }
        Ok(())
    }

    /// Read a u8
    pub fn read_u8(&mut self) -> Result<u8> {
        self.check_remaining(1)?;
        let value = self.bytes[self.offset];
        self.offset += 1;
        Ok(value)
    }

    /// Read an i8
    pub fn read_i8(&mut self) -> Result<i8> {
        Ok(self.read_u8()? as i8)
    }

    /// Read a u16
    pub fn read_u16(&mut self) -> Result<u16> {
        self.check_remaining(2)?;
        let value = u16::from_le_bytes([
            self.bytes[self.offset],
            self.bytes[self.offset + 1],
        ]);
        self.offset += 2;
        Ok(value)
    }

    /// Read an i16
    pub fn read_i16(&mut self) -> Result<i16> {
        self.check_remaining(2)?;
        let value = i16::from_le_bytes([
            self.bytes[self.offset],
            self.bytes[self.offset + 1],
        ]);
        self.offset += 2;
        Ok(value)
    }

    /// Read a u32
    pub fn read_u32(&mut self) -> Result<u32> {
        self.check_remaining(4)?;
        let value = u32::from_le_bytes([
            self.bytes[self.offset],
            self.bytes[self.offset + 1],
            self.bytes[self.offset + 2],
            self.bytes[self.offset + 3],
        ]);
        self.offset += 4;
        Ok(value)
    }

    /// Read an i32
    pub fn read_i32(&mut self) -> Result<i32> {
        self.check_remaining(4)?;
        let value = i32::from_le_bytes([
            self.bytes[self.offset],
            self.bytes[self.offset + 1],
            self.bytes[self.offset + 2],
            self.bytes[self.offset + 3],
        ]);
        self.offset += 4;
        Ok(value)
    }

    /// Read a u64
    pub fn read_u64(&mut self) -> Result<u64> {
        self.check_remaining(8)?;
        let value = u64::from_le_bytes([
            self.bytes[self.offset],
            self.bytes[self.offset + 1],
            self.bytes[self.offset + 2],
            self.bytes[self.offset + 3],
            self.bytes[self.offset + 4],
            self.bytes[self.offset + 5],
            self.bytes[self.offset + 6],
            self.bytes[self.offset + 7],
        ]);
        self.offset += 8;
        Ok(value)
    }

    /// Read an i64
    pub fn read_i64(&mut self) -> Result<i64> {
        self.check_remaining(8)?;
        let value = i64::from_le_bytes([
            self.bytes[self.offset],
            self.bytes[self.offset + 1],
            self.bytes[self.offset + 2],
            self.bytes[self.offset + 3],
            self.bytes[self.offset + 4],
            self.bytes[self.offset + 5],
            self.bytes[self.offset + 6],
            self.bytes[self.offset + 7],
        ]);
        self.offset += 8;
        Ok(value)
    }

    /// Read an f32
    pub fn read_f32(&mut self) -> Result<f32> {
        self.check_remaining(4)?;
        let value = f32::from_le_bytes([
            self.bytes[self.offset],
            self.bytes[self.offset + 1],
            self.bytes[self.offset + 2],
            self.bytes[self.offset + 3],
        ]);
        self.offset += 4;
        Ok(value)
    }

    /// Read an f64
    pub fn read_f64(&mut self) -> Result<f64> {
        self.check_remaining(8)?;
        let value = f64::from_le_bytes([
            self.bytes[self.offset],
            self.bytes[self.offset + 1],
            self.bytes[self.offset + 2],
            self.bytes[self.offset + 3],
            self.bytes[self.offset + 4],
            self.bytes[self.offset + 5],
            self.bytes[self.offset + 6],
            self.bytes[self.offset + 7],
        ]);
        self.offset += 8;
        Ok(value)
    }

    /// Read a bool
    pub fn read_bool(&mut self) -> Result<bool> {
        let value = self.read_u8()?;
        match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::InvalidBool { value }),
        }
    }

    /// Read raw bytes
    pub fn read_bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        self.check_remaining(len)?;
        let data = &self.bytes[self.offset..self.offset + len];
        self.offset += len;
        Ok(data)
    }

    /// Skip padding to align to boundary
    pub fn skip_padding(&mut self, alignment: usize) -> Result<()> {
        let remainder = self.offset % alignment;
        if remainder != 0 {
            let padding = alignment - remainder;
            self.check_remaining(padding)?;
            self.offset += padding;
        }
        Ok(())
    }

    /// Skip padding for 8-byte alignment
    pub fn skip_padding_8(&mut self) -> Result<()> {
        self.skip_padding(8)
    }

    /// Read a string
    ///
    /// Returns None if the string is absent (null pointer).
    pub fn read_string(&mut self) -> Result<Option<String>> {
        let count = self.read_u64()? as usize;
        let pointer = self.read_u64()?;

        if pointer == 0 {
            // Null pointer = absent
            return Ok(None);
        }

        // Read from current position (out-of-line data follows inline)
        self.check_remaining(count)?;
        let bytes = &self.bytes[self.offset..self.offset + count];

        // Advance past string data + padding
        let padded_len = (count + 7) & !7; // Round up to 8
        self.offset += padded_len;

        let s = std::str::from_utf8(bytes)
            .map_err(|_| Error::InvalidUtf8)?;

        Ok(Some(s.to_string()))
    }

    /// Read a vector header
    ///
    /// Returns (count, present).
    pub fn read_vector_header(&mut self) -> Result<(usize, bool)> {
        let count = self.read_u64()? as usize;
        let pointer = self.read_u64()?;
        Ok((count, pointer != 0))
    }

    /// Read a handle
    pub fn read_handle(&mut self) -> Result<Option<Handle>> {
        let marker = self.read_u32()?;

        match marker {
            HANDLE_ABSENT => Ok(None),
            HANDLE_PRESENT => {
                if self.handle_index >= self.handles.len() {
                    return Err(Error::InvalidHandle);
                }
                let handle = std::mem::take(&mut self.handles[self.handle_index]);
                self.handle_index += 1;
                Ok(Some(handle))
            }
            _ => Err(Error::InvalidHandle),
        }
    }

    /// Read a handle with type info
    pub fn read_handle_info(&mut self, expected_type: ObjectType) -> Result<Option<HandleInfo>> {
        match self.read_handle()? {
            Some(handle) => {
                Ok(Some(HandleInfo::new(handle, expected_type, Rights::SAME_RIGHTS)))
            }
            None => Ok(None),
        }
    }

    /// Enter a nested structure (recursion check)
    pub fn enter_recursion(&mut self) -> Result<()> {
        self.depth += 1;
        if self.depth > MAX_RECURSION_DEPTH {
            return Err(Error::RecursionDepthExceeded);
        }
        Ok(())
    }

    /// Leave a nested structure
    pub fn leave_recursion(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// Get remaining handles
    pub fn remaining_handles(&self) -> usize {
        self.handles.len().saturating_sub(self.handle_index)
    }

    /// Take all remaining handles
    pub fn take_remaining_handles(&mut self) -> Vec<Handle> {
        self.handles.drain(self.handle_index..).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_primitives() {
        let bytes = [
            1u8, // u8
            2, 0, // u16
            3, 0, 0, 0, // u32
            4, 0, 0, 0, 0, 0, 0, 0, // u64
        ];

        let mut decoder = Decoder::new(&bytes, vec![]);
        assert_eq!(decoder.read_u8().unwrap(), 1);
        assert_eq!(decoder.read_u16().unwrap(), 2);
        assert_eq!(decoder.read_u32().unwrap(), 3);
        assert_eq!(decoder.read_u64().unwrap(), 4);
    }

    #[test]
    fn test_decode_header() {
        let header = MessageHeader::new_request(123, 0x5678);
        let bytes = header.encode();

        let mut decoder = Decoder::new(&bytes, vec![]);
        let decoded = decoder.read_header().unwrap();
        assert_eq!(decoded.txid, 123);
        assert_eq!(decoded.ordinal, 0x5678);
    }

    #[test]
    fn test_buffer_underflow() {
        let bytes = [1, 2, 3];
        let mut decoder = Decoder::new(&bytes, vec![]);

        assert!(decoder.read_u8().is_ok());
        assert!(decoder.read_u8().is_ok());
        assert!(decoder.read_u8().is_ok());
        assert!(matches!(decoder.read_u8(), Err(Error::BufferUnderflow { .. })));
    }
}
