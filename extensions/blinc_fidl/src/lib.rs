//! Blinc FIDL - Wire Format Encoding/Decoding
//!
//! This crate implements the FIDL (Fuchsia Interface Definition Language) wire format
//! for encoding and decoding messages sent over Zircon channels.
//!
//! # Wire Format
//!
//! FIDL uses a binary wire format with:
//! - Little-endian byte order
//! - 8-byte alignment for all out-of-line data
//! - Handles passed out-of-band (separate from bytes)
//!
//! # Message Structure
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │                    Message Header (16 bytes)              │
//! │  ┌─────────┬─────────────┬──────────┬─────┬───────────┐  │
//! │  │ txid(4) │ at_rest(2)  │ dyn(1)   │magic│ ordinal(8)│  │
//! │  └─────────┴─────────────┴──────────┴─────┴───────────┘  │
//! ├──────────────────────────────────────────────────────────┤
//! │                    Message Body                           │
//! │  (inline data + out-of-line data, 8-byte aligned)        │
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use blinc_fidl::{Encoder, Decoder, MessageHeader};
//!
//! // Encode a request
//! let mut encoder = Encoder::new();
//! encoder.write_header(MessageHeader::new_request(1, 0x12345678));
//! encoder.write_u32(42);
//! let (bytes, handles) = encoder.finish();
//!
//! // Send over channel...
//!
//! // Decode response
//! let mut decoder = Decoder::new(&bytes, handles);
//! let header = decoder.read_header()?;
//! let value = decoder.read_u32()?;
//! ```

mod encoding;
mod decoding;
mod header;
mod error;
mod handle;

pub use encoding::Encoder;
pub use decoding::Decoder;
pub use header::{MessageHeader, TransactionId, Ordinal, MessageFlags, DynamicFlags};
pub use error::{Error, Result};
pub use handle::{HandleDisposition, HandleInfo, HandleOp, ObjectType};

/// FIDL wire format magic number
pub const FIDL_MAGIC: u8 = 1;

/// Maximum message size (64KB inline + handles)
pub const MAX_MESSAGE_SIZE: usize = 65536;

/// Maximum number of handles per message
pub const MAX_HANDLES: usize = 64;

/// Maximum recursion depth for nested types
pub const MAX_RECURSION_DEPTH: usize = 32;

/// FIDL epitaph ordinal (indicates channel closure)
pub const EPITAPH_ORDINAL: u64 = 0xFFFFFFFFFFFFFFFF;

/// Prelude for common imports
pub mod prelude {
    pub use super::{
        Encoder, Decoder, MessageHeader, TransactionId, Ordinal,
        Error, Result, HandleDisposition, HandleInfo,
        FIDL_MAGIC, MAX_MESSAGE_SIZE, MAX_HANDLES,
    };
}
