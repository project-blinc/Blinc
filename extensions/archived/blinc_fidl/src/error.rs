//! FIDL error types

use std::fmt;

/// FIDL encoding/decoding errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Message too large
    MessageTooLarge {
        size: usize,
        max: usize,
    },
    /// Too many handles
    TooManyHandles {
        count: usize,
        max: usize,
    },
    /// Invalid magic number
    InvalidMagic {
        expected: u8,
        actual: u8,
    },
    /// Buffer underflow (tried to read past end)
    BufferUnderflow {
        needed: usize,
        available: usize,
    },
    /// Invalid string (not UTF-8)
    InvalidUtf8,
    /// Invalid enum value
    InvalidEnumValue {
        ordinal: u64,
    },
    /// Invalid union ordinal
    InvalidUnionOrdinal {
        ordinal: u64,
    },
    /// Missing required field
    MissingRequiredField {
        name: &'static str,
    },
    /// Unexpected null reference
    UnexpectedNull,
    /// Invalid handle
    InvalidHandle,
    /// Recursion depth exceeded
    RecursionDepthExceeded,
    /// Invalid alignment
    InvalidAlignment {
        offset: usize,
        alignment: usize,
    },
    /// Invalid boolean value
    InvalidBool {
        value: u8,
    },
    /// Zircon error
    Zircon(blinc_fuchsia_zircon::Status),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::MessageTooLarge { size, max } => {
                write!(f, "Message too large: {} bytes (max {})", size, max)
            }
            Error::TooManyHandles { count, max } => {
                write!(f, "Too many handles: {} (max {})", count, max)
            }
            Error::InvalidMagic { expected, actual } => {
                write!(f, "Invalid magic: expected {}, got {}", expected, actual)
            }
            Error::BufferUnderflow { needed, available } => {
                write!(f, "Buffer underflow: needed {} bytes, {} available", needed, available)
            }
            Error::InvalidUtf8 => write!(f, "Invalid UTF-8 string"),
            Error::InvalidEnumValue { ordinal } => {
                write!(f, "Invalid enum value: {}", ordinal)
            }
            Error::InvalidUnionOrdinal { ordinal } => {
                write!(f, "Invalid union ordinal: {}", ordinal)
            }
            Error::MissingRequiredField { name } => {
                write!(f, "Missing required field: {}", name)
            }
            Error::UnexpectedNull => write!(f, "Unexpected null reference"),
            Error::InvalidHandle => write!(f, "Invalid handle"),
            Error::RecursionDepthExceeded => write!(f, "Recursion depth exceeded"),
            Error::InvalidAlignment { offset, alignment } => {
                write!(f, "Invalid alignment: offset {} not aligned to {}", offset, alignment)
            }
            Error::InvalidBool { value } => {
                write!(f, "Invalid boolean value: {}", value)
            }
            Error::Zircon(status) => write!(f, "Zircon error: {:?}", status),
        }
    }
}

impl std::error::Error for Error {}

impl From<blinc_fuchsia_zircon::Status> for Error {
    fn from(status: blinc_fuchsia_zircon::Status) -> Self {
        Error::Zircon(status)
    }
}

/// Result type for FIDL operations
pub type Result<T> = std::result::Result<T, Error>;
