//! Zircon status codes (zx_status_t)

use std::fmt;

/// Zircon status code
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Status(i32);

impl Status {
    /// Create a status from a raw value
    pub const fn from_raw(raw: i32) -> Self {
        Status(raw)
    }

    /// Get the raw status value
    pub const fn into_raw(self) -> i32 {
        self.0
    }

    /// Check if this is a success status
    pub const fn is_ok(self) -> bool {
        self.0 == 0
    }

    /// Check if this is an error status
    pub const fn is_error(self) -> bool {
        self.0 != 0
    }
}

// Status code constants
impl Status {
    pub const OK: Status = Status(0);
    pub const ERR_INTERNAL: Status = Status(-1);
    pub const ERR_NOT_SUPPORTED: Status = Status(-2);
    pub const ERR_NO_RESOURCES: Status = Status(-3);
    pub const ERR_NO_MEMORY: Status = Status(-4);
    pub const ERR_INVALID_ARGS: Status = Status(-10);
    pub const ERR_BAD_HANDLE: Status = Status(-11);
    pub const ERR_WRONG_TYPE: Status = Status(-12);
    pub const ERR_BAD_SYSCALL: Status = Status(-13);
    pub const ERR_OUT_OF_RANGE: Status = Status(-14);
    pub const ERR_BUFFER_TOO_SMALL: Status = Status(-15);
    pub const ERR_BAD_STATE: Status = Status(-20);
    pub const ERR_TIMED_OUT: Status = Status(-21);
    pub const ERR_SHOULD_WAIT: Status = Status(-22);
    pub const ERR_CANCELED: Status = Status(-23);
    pub const ERR_PEER_CLOSED: Status = Status(-24);
    pub const ERR_NOT_FOUND: Status = Status(-25);
    pub const ERR_ALREADY_EXISTS: Status = Status(-26);
    pub const ERR_ALREADY_BOUND: Status = Status(-27);
    pub const ERR_UNAVAILABLE: Status = Status(-28);
    pub const ERR_ACCESS_DENIED: Status = Status(-30);
    pub const ERR_IO: Status = Status(-40);
    pub const ERR_IO_REFUSED: Status = Status(-41);
    pub const ERR_IO_DATA_INTEGRITY: Status = Status(-42);
    pub const ERR_IO_DATA_LOSS: Status = Status(-43);
    pub const ERR_IO_NOT_PRESENT: Status = Status(-44);
    pub const ERR_IO_OVERRUN: Status = Status(-45);
    pub const ERR_IO_MISSED_DEADLINE: Status = Status(-46);
    pub const ERR_IO_INVALID: Status = Status(-47);
    pub const ERR_BAD_PATH: Status = Status(-50);
    pub const ERR_NOT_DIR: Status = Status(-51);
    pub const ERR_NOT_FILE: Status = Status(-52);
    pub const ERR_FILE_BIG: Status = Status(-53);
    pub const ERR_NO_SPACE: Status = Status(-54);
    pub const ERR_NOT_EMPTY: Status = Status(-55);
    pub const ERR_STOP: Status = Status(-60);
    pub const ERR_NEXT: Status = Status(-61);
    pub const ERR_ASYNC: Status = Status(-62);
    pub const ERR_PROTOCOL_NOT_SUPPORTED: Status = Status(-70);
    pub const ERR_ADDRESS_UNREACHABLE: Status = Status(-71);
    pub const ERR_ADDRESS_IN_USE: Status = Status(-72);
    pub const ERR_NOT_CONNECTED: Status = Status(-73);
    pub const ERR_CONNECTION_REFUSED: Status = Status(-74);
    pub const ERR_CONNECTION_RESET: Status = Status(-75);
    pub const ERR_CONNECTION_ABORTED: Status = Status(-76);
}

impl fmt::Debug for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Status::OK => write!(f, "OK"),
            Status::ERR_INTERNAL => write!(f, "ERR_INTERNAL"),
            Status::ERR_NOT_SUPPORTED => write!(f, "ERR_NOT_SUPPORTED"),
            Status::ERR_NO_RESOURCES => write!(f, "ERR_NO_RESOURCES"),
            Status::ERR_NO_MEMORY => write!(f, "ERR_NO_MEMORY"),
            Status::ERR_INVALID_ARGS => write!(f, "ERR_INVALID_ARGS"),
            Status::ERR_BAD_HANDLE => write!(f, "ERR_BAD_HANDLE"),
            Status::ERR_WRONG_TYPE => write!(f, "ERR_WRONG_TYPE"),
            Status::ERR_BAD_STATE => write!(f, "ERR_BAD_STATE"),
            Status::ERR_TIMED_OUT => write!(f, "ERR_TIMED_OUT"),
            Status::ERR_SHOULD_WAIT => write!(f, "ERR_SHOULD_WAIT"),
            Status::ERR_CANCELED => write!(f, "ERR_CANCELED"),
            Status::ERR_PEER_CLOSED => write!(f, "ERR_PEER_CLOSED"),
            Status::ERR_NOT_FOUND => write!(f, "ERR_NOT_FOUND"),
            Status::ERR_ACCESS_DENIED => write!(f, "ERR_ACCESS_DENIED"),
            Status::ERR_IO => write!(f, "ERR_IO"),
            other => write!(f, "Status({})", other.0),
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl std::error::Error for Status {}

impl From<Status> for i32 {
    fn from(status: Status) -> i32 {
        status.0
    }
}

impl From<i32> for Status {
    fn from(raw: i32) -> Status {
        Status(raw)
    }
}

/// Convert a raw status to a Result
pub fn ok(status: i32) -> crate::Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(Status(status))
    }
}
