//! FIDL message encoder

use crate::{
    Error, Result, MessageHeader, HandleDisposition,
    MAX_MESSAGE_SIZE, MAX_HANDLES,
    handle::{HANDLE_ABSENT, HANDLE_PRESENT},
};
use blinc_fuchsia_zircon::Handle;

/// FIDL message encoder
///
/// Encodes FIDL messages into bytes + handles for sending over a channel.
#[derive(Debug)]
pub struct Encoder {
    /// Inline bytes buffer
    bytes: Vec<u8>,
    /// Out-of-line bytes buffer
    out_of_line: Vec<u8>,
    /// Handles to transfer
    handles: Vec<HandleDisposition>,
    /// Current recursion depth
    depth: usize,
}

impl Encoder {
    /// Create a new encoder
    pub fn new() -> Self {
        Self {
            bytes: Vec::with_capacity(256),
            out_of_line: Vec::new(),
            handles: Vec::new(),
            depth: 0,
        }
    }

    /// Create an encoder with pre-allocated capacity
    pub fn with_capacity(bytes: usize, handles: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(bytes),
            out_of_line: Vec::new(),
            handles: Vec::with_capacity(handles),
            depth: 0,
        }
    }

    /// Write the message header
    pub fn write_header(&mut self, header: MessageHeader) {
        self.bytes.extend_from_slice(&header.encode());
    }

    /// Write a u8
    pub fn write_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    /// Write an i8
    pub fn write_i8(&mut self, value: i8) {
        self.bytes.push(value as u8);
    }

    /// Write a u16
    pub fn write_u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Write an i16
    pub fn write_i16(&mut self, value: i16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Write a u32
    pub fn write_u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Write an i32
    pub fn write_i32(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Write a u64
    pub fn write_u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Write an i64
    pub fn write_i64(&mut self, value: i64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Write an f32
    pub fn write_f32(&mut self, value: f32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Write an f64
    pub fn write_f64(&mut self, value: f64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Write a bool
    pub fn write_bool(&mut self, value: bool) {
        self.bytes.push(if value { 1 } else { 0 });
    }

    /// Write raw bytes
    pub fn write_bytes(&mut self, data: &[u8]) {
        self.bytes.extend_from_slice(data);
    }

    /// Write padding to align to boundary
    pub fn write_padding(&mut self, alignment: usize) {
        let remainder = self.bytes.len() % alignment;
        if remainder != 0 {
            let padding = alignment - remainder;
            self.bytes.extend(std::iter::repeat(0).take(padding));
        }
    }

    /// Write padding for 8-byte alignment
    pub fn write_padding_8(&mut self) {
        self.write_padding(8);
    }

    /// Write a string (inline pointer + out-of-line data)
    ///
    /// FIDL strings are encoded as:
    /// - count: u64 (number of bytes, not chars)
    /// - pointer: u64 (ALLOC_PRESENT or ALLOC_ABSENT)
    /// - data: out-of-line bytes (8-byte aligned)
    pub fn write_string(&mut self, s: &str) -> Result<()> {
        let bytes = s.as_bytes();
        self.write_u64(bytes.len() as u64); // count
        self.write_u64(u64::MAX); // ALLOC_PRESENT

        // Write to out-of-line buffer
        self.out_of_line.extend_from_slice(bytes);

        // Pad to 8-byte alignment
        let remainder = bytes.len() % 8;
        if remainder != 0 {
            self.out_of_line.extend(std::iter::repeat(0).take(8 - remainder));
        }

        Ok(())
    }

    /// Write an optional string
    pub fn write_optional_string(&mut self, s: Option<&str>) -> Result<()> {
        match s {
            Some(s) => self.write_string(s),
            None => {
                self.write_u64(0); // count = 0
                self.write_u64(0); // ALLOC_ABSENT
                Ok(())
            }
        }
    }

    /// Write a vector header (inline)
    ///
    /// FIDL vectors are encoded as:
    /// - count: u64 (number of elements)
    /// - pointer: u64 (ALLOC_PRESENT or ALLOC_ABSENT)
    pub fn write_vector_header(&mut self, count: usize, present: bool) {
        self.write_u64(count as u64);
        self.write_u64(if present { u64::MAX } else { 0 });
    }

    /// Write a handle (inline u32 placeholder)
    pub fn write_handle(&mut self, handle: Option<Handle>) -> Result<()> {
        match handle {
            Some(h) => {
                if self.handles.len() >= MAX_HANDLES {
                    return Err(Error::TooManyHandles {
                        count: self.handles.len() + 1,
                        max: MAX_HANDLES,
                    });
                }
                self.write_u32(HANDLE_PRESENT);
                self.handles.push(HandleDisposition::move_handle(h));
            }
            None => {
                self.write_u32(HANDLE_ABSENT);
            }
        }
        Ok(())
    }

    /// Write a handle disposition
    pub fn write_handle_disposition(&mut self, disposition: HandleDisposition) -> Result<()> {
        if self.handles.len() >= MAX_HANDLES {
            return Err(Error::TooManyHandles {
                count: self.handles.len() + 1,
                max: MAX_HANDLES,
            });
        }
        self.write_u32(HANDLE_PRESENT);
        self.handles.push(disposition);
        Ok(())
    }

    /// Write out-of-line data directly
    pub fn write_out_of_line(&mut self, data: &[u8]) {
        self.out_of_line.extend_from_slice(data);

        // Pad to 8-byte alignment
        let remainder = data.len() % 8;
        if remainder != 0 {
            self.out_of_line.extend(std::iter::repeat(0).take(8 - remainder));
        }
    }

    /// Current inline buffer length
    pub fn inline_len(&self) -> usize {
        self.bytes.len()
    }

    /// Current out-of-line buffer length
    pub fn out_of_line_len(&self) -> usize {
        self.out_of_line.len()
    }

    /// Finish encoding and return (bytes, handles)
    ///
    /// The bytes are: inline data + out-of-line data
    /// Handles are returned as HandleDisposition for transfer.
    pub fn finish(mut self) -> Result<(Vec<u8>, Vec<Handle>)> {
        // Combine inline and out-of-line
        self.bytes.append(&mut self.out_of_line);

        if self.bytes.len() > MAX_MESSAGE_SIZE {
            return Err(Error::MessageTooLarge {
                size: self.bytes.len(),
                max: MAX_MESSAGE_SIZE,
            });
        }

        // Extract handles
        let handles: Vec<Handle> = self.handles
            .into_iter()
            .map(|d| d.handle)
            .collect();

        Ok((self.bytes, handles))
    }

    /// Reset the encoder for reuse
    pub fn reset(&mut self) {
        self.bytes.clear();
        self.out_of_line.clear();
        self.handles.clear();
        self.depth = 0;
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_primitives() {
        let mut encoder = Encoder::new();
        encoder.write_u8(1);
        encoder.write_u16(2);
        encoder.write_u32(3);
        encoder.write_u64(4);

        let (bytes, _) = encoder.finish().unwrap();
        assert_eq!(bytes.len(), 1 + 2 + 4 + 8);
    }

    #[test]
    fn test_encode_string() {
        let mut encoder = Encoder::new();
        encoder.write_string("hello").unwrap();

        let (bytes, _) = encoder.finish().unwrap();
        // 16 bytes inline (count + pointer) + 8 bytes out-of-line ("hello" padded)
        assert_eq!(bytes.len(), 16 + 8);
    }

    #[test]
    fn test_encode_header() {
        let mut encoder = Encoder::new();
        let header = MessageHeader::new_request(1, 0x12345678);
        encoder.write_header(header);

        let (bytes, _) = encoder.finish().unwrap();
        assert_eq!(bytes.len(), MessageHeader::SIZE);
    }
}
