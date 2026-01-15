//! Handle types for FIDL encoding

use blinc_fuchsia_zircon::{Handle, Rights};

/// Object type for handles
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum ObjectType {
    /// Unknown/none
    #[default]
    None = 0,
    /// Process
    Process = 1,
    /// Thread
    Thread = 2,
    /// VMO (Virtual Memory Object)
    Vmo = 3,
    /// Channel
    Channel = 4,
    /// Event
    Event = 5,
    /// Port
    Port = 6,
    /// Interrupt
    Interrupt = 9,
    /// Socket
    Socket = 14,
    /// Resource
    Resource = 15,
    /// EventPair
    EventPair = 16,
    /// Job
    Job = 17,
    /// VMAR (Virtual Memory Address Region)
    Vmar = 18,
    /// FIFO
    Fifo = 19,
    /// Timer
    Timer = 22,
    /// BTI (Bus Transaction Initiator)
    Bti = 24,
    /// Profile
    Profile = 25,
    /// PMT (Pinned Memory Token)
    Pmt = 26,
    /// Pager
    Pager = 28,
    /// Clock
    Clock = 30,
    /// Stream
    Stream = 31,
    /// MSI (Message Signaled Interrupt)
    Msi = 32,
    /// IOB (I/O Buffer)
    Iob = 33,
}

impl ObjectType {
    /// Create from raw u32
    pub fn from_raw(raw: u32) -> Self {
        match raw {
            0 => ObjectType::None,
            1 => ObjectType::Process,
            2 => ObjectType::Thread,
            3 => ObjectType::Vmo,
            4 => ObjectType::Channel,
            5 => ObjectType::Event,
            6 => ObjectType::Port,
            9 => ObjectType::Interrupt,
            14 => ObjectType::Socket,
            15 => ObjectType::Resource,
            16 => ObjectType::EventPair,
            17 => ObjectType::Job,
            18 => ObjectType::Vmar,
            19 => ObjectType::Fifo,
            22 => ObjectType::Timer,
            24 => ObjectType::Bti,
            25 => ObjectType::Profile,
            26 => ObjectType::Pmt,
            28 => ObjectType::Pager,
            30 => ObjectType::Clock,
            31 => ObjectType::Stream,
            32 => ObjectType::Msi,
            33 => ObjectType::Iob,
            _ => ObjectType::None,
        }
    }

    /// Convert to raw u32
    pub fn into_raw(self) -> u32 {
        self as u32
    }
}

/// Handle operation (how to transfer the handle)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum HandleOp {
    /// Move the handle
    #[default]
    Move = 0,
    /// Duplicate the handle
    Duplicate = 1,
}

impl HandleOp {
    /// Create from raw u32
    pub fn from_raw(raw: u32) -> Self {
        match raw {
            1 => HandleOp::Duplicate,
            _ => HandleOp::Move,
        }
    }
}

/// Handle disposition for encoding
///
/// Describes how a handle should be transferred.
#[derive(Debug)]
pub struct HandleDisposition {
    /// The handle operation
    pub op: HandleOp,
    /// The handle to transfer
    pub handle: Handle,
    /// Expected object type (0 = any)
    pub object_type: ObjectType,
    /// Required rights
    pub rights: Rights,
}

impl HandleDisposition {
    /// Create a new handle disposition for moving a handle
    pub fn move_handle(handle: Handle) -> Self {
        Self {
            op: HandleOp::Move,
            handle,
            object_type: ObjectType::None,
            rights: Rights::SAME_RIGHTS,
        }
    }

    /// Create a new handle disposition for duplicating a handle
    pub fn duplicate_handle(handle: Handle, rights: Rights) -> Self {
        Self {
            op: HandleOp::Duplicate,
            handle,
            object_type: ObjectType::None,
            rights,
        }
    }

    /// Set expected object type
    pub fn with_type(mut self, object_type: ObjectType) -> Self {
        self.object_type = object_type;
        self
    }

    /// Set required rights
    pub fn with_rights(mut self, rights: Rights) -> Self {
        self.rights = rights;
        self
    }
}

/// Handle info received from decoding
#[derive(Debug)]
pub struct HandleInfo {
    /// The received handle
    pub handle: Handle,
    /// Object type
    pub object_type: ObjectType,
    /// Rights on the handle
    pub rights: Rights,
}

impl HandleInfo {
    /// Create new handle info
    pub fn new(handle: Handle, object_type: ObjectType, rights: Rights) -> Self {
        Self {
            handle,
            object_type,
            rights,
        }
    }

    /// Create from just a handle (type and rights unknown)
    pub fn from_handle(handle: Handle) -> Self {
        Self {
            handle,
            object_type: ObjectType::None,
            rights: Rights::SAME_RIGHTS,
        }
    }

    /// Take the handle out
    pub fn take(self) -> Handle {
        self.handle
    }
}

/// Placeholder for absent handle
pub const HANDLE_ABSENT: u32 = 0;

/// Placeholder for present handle
pub const HANDLE_PRESENT: u32 = u32::MAX;
