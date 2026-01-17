//! Zircon handle rights

use bitflags::bitflags;

bitflags! {
    /// Rights associated with a handle
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct Rights: u32 {
        const NONE = 0;
        const DUPLICATE = 1 << 0;
        const TRANSFER = 1 << 1;
        const READ = 1 << 2;
        const WRITE = 1 << 3;
        const EXECUTE = 1 << 4;
        const MAP = 1 << 5;
        const GET_PROPERTY = 1 << 6;
        const SET_PROPERTY = 1 << 7;
        const ENUMERATE = 1 << 8;
        const DESTROY = 1 << 9;
        const SET_POLICY = 1 << 10;
        const GET_POLICY = 1 << 11;
        const SIGNAL = 1 << 12;
        const SIGNAL_PEER = 1 << 13;
        const WAIT = 1 << 14;
        const INSPECT = 1 << 15;
        const MANAGE_JOB = 1 << 16;
        const MANAGE_PROCESS = 1 << 17;
        const MANAGE_THREAD = 1 << 18;
        const APPLY_PROFILE = 1 << 19;
        const MANAGE_SOCKET = 1 << 20;

        // Common combinations
        const SAME_RIGHTS = 1 << 31;
        const BASIC = Self::TRANSFER.bits() | Self::DUPLICATE.bits() |
                      Self::WAIT.bits() | Self::INSPECT.bits();

        const IO = Self::READ.bits() | Self::WRITE.bits();

        const CHANNEL_DEFAULT = Self::TRANSFER.bits() | Self::READ.bits() |
                                Self::WRITE.bits() | Self::SIGNAL.bits() |
                                Self::SIGNAL_PEER.bits() | Self::WAIT.bits() |
                                Self::INSPECT.bits();

        const VMO_DEFAULT = Self::DUPLICATE.bits() | Self::TRANSFER.bits() |
                           Self::READ.bits() | Self::WRITE.bits() |
                           Self::MAP.bits() | Self::GET_PROPERTY.bits() |
                           Self::SET_PROPERTY.bits();
    }
}
