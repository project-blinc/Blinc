//! Debug server module for blinc_recorder.
//!
//! This module provides a local socket server that allows external tools
//! (like blinc_debugger) to connect and receive live recording data.
//!
//! Platform support:
//! - Unix (Linux/macOS): Unix domain sockets at `/tmp/blinc/{app_name}.sock`
//! - Windows: Named pipes at `\\.\pipe\blinc\{app_name}`

mod local;

pub use local::{
    start_local_server, start_local_server_named, ClientCommand, DebugServer, DebugServerConfig,
    ServerHandle, ServerMessage,
};
