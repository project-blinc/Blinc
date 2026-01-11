//! Local socket server for debug connections.
//!
//! Provides a cross-platform server that listens for debugger connections
//! and streams recording data in real-time.

use crate::{RecordingExport, SharedRecordingSession};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// Configuration for the debug server.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DebugServerConfig {
    /// Application name used in socket path.
    pub app_name: String,
    /// Custom socket path (overrides default if set).
    pub socket_path: Option<PathBuf>,
    /// Whether to auto-start recording when a client connects.
    pub auto_start_recording: bool,
}

impl Default for DebugServerConfig {
    fn default() -> Self {
        Self {
            app_name: "blinc_app".to_string(),
            socket_path: None,
            auto_start_recording: true,
        }
    }
}

impl DebugServerConfig {
    /// Create a new config with the given app name.
    pub fn new(app_name: impl Into<String>) -> Self {
        Self {
            app_name: app_name.into(),
            ..Default::default()
        }
    }

    /// Get the socket path for this config.
    pub fn socket_path(&self) -> PathBuf {
        if let Some(ref path) = self.socket_path {
            return path.clone();
        }

        #[cfg(unix)]
        {
            PathBuf::from(format!("/tmp/blinc/{}.sock", self.app_name))
        }

        #[cfg(windows)]
        {
            PathBuf::from(format!(r"\\.\pipe\blinc\{}", self.app_name))
        }
    }
}

/// Handle to a running debug server.
pub struct ServerHandle {
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    socket_path: PathBuf,
}

impl ServerHandle {
    /// Request the server to shut down.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    /// Get the socket path the server is listening on.
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Check if the server is still running.
    pub fn is_running(&self) -> bool {
        self.thread
            .as_ref()
            .map(|t| !t.is_finished())
            .unwrap_or(false)
    }

    /// Wait for the server to finish.
    pub fn join(mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.shutdown();
        // Clean up socket file on Unix
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

/// Debug server that streams recording data to connected clients.
pub struct DebugServer {
    config: DebugServerConfig,
    session: Arc<SharedRecordingSession>,
    clients: Arc<Mutex<Vec<ClientConnection>>>,
}

struct ClientConnection {
    #[cfg(unix)]
    stream: std::os::unix::net::UnixStream,
    #[cfg(windows)]
    stream: std::net::TcpStream, // Fallback for Windows (TODO: named pipes)
}

impl DebugServer {
    /// Create a new debug server with the given config and recording session.
    pub fn new(config: DebugServerConfig, session: Arc<SharedRecordingSession>) -> Self {
        Self {
            config,
            session,
            clients: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Start the server in a background thread.
    ///
    /// Returns a handle that can be used to shut down the server.
    pub fn start(self) -> io::Result<ServerHandle> {
        let socket_path = self.config.socket_path();

        // Ensure parent directory exists
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Remove existing socket file
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&socket_path);
        }

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();
        let socket_path_clone = socket_path.clone();

        let thread = thread::spawn(move || {
            if let Err(e) = self.run_server(&socket_path_clone, shutdown_clone) {
                tracing::error!("Debug server error: {}", e);
            }
        });

        Ok(ServerHandle {
            shutdown,
            thread: Some(thread),
            socket_path,
        })
    }

    #[cfg(unix)]
    fn run_server(&self, socket_path: &PathBuf, shutdown: Arc<AtomicBool>) -> io::Result<()> {
        use std::os::unix::net::UnixListener;
        use std::time::Duration;

        let listener = UnixListener::bind(socket_path)?;
        listener.set_nonblocking(true)?;

        tracing::info!(
            "Debug server listening on {}",
            socket_path.display()
        );

        while !shutdown.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    tracing::info!("Debug client connected");

                    if self.config.auto_start_recording && !self.session.is_recording() {
                        self.session.start();
                    }

                    // Handle client in a new thread
                    let session = self.session.clone();
                    thread::spawn(move || {
                        if let Err(e) = handle_client(stream, session) {
                            tracing::debug!("Client disconnected: {}", e);
                        }
                    });
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // No pending connection, sleep briefly
                    thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    #[cfg(windows)]
    fn run_server(&self, _socket_path: &PathBuf, shutdown: Arc<AtomicBool>) -> io::Result<()> {
        use std::net::TcpListener;
        use std::time::Duration;

        // Fallback to TCP on Windows (TODO: implement named pipes)
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let local_addr = listener.local_addr()?;
        listener.set_nonblocking(true)?;

        tracing::info!("Debug server listening on {} (TCP fallback)", local_addr);

        while !shutdown.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    tracing::info!("Debug client connected");

                    if self.config.auto_start_recording && !self.session.is_recording() {
                        self.session.start();
                    }

                    let session = self.session.clone();
                    thread::spawn(move || {
                        if let Err(e) = handle_client_tcp(stream, session) {
                            tracing::debug!("Client disconnected: {}", e);
                        }
                    });
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }
}

/// Commands that clients can send to the server.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientCommand {
    /// Start recording.
    Start,
    /// Pause recording.
    Pause,
    /// Resume recording (same as Start when paused).
    Resume,
    /// Stop recording.
    Stop,
    /// Reset the session.
    Reset,
    /// Request a full export of recorded data.
    RequestExport,
    /// Request current session stats.
    RequestStats,
    /// Ping to keep connection alive.
    Ping,
}

impl ClientCommand {
    /// Parse a command from JSON bytes.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        // Skip length prefix if present
        let json_data = if data.len() > 4 {
            let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
            if data.len() >= 4 + len {
                &data[4..4 + len]
            } else {
                data
            }
        } else {
            data
        };

        // Try to parse as JSON
        let s = std::str::from_utf8(json_data).ok()?;

        // Simple JSON parsing for command type
        if s.contains("\"start\"") || s.contains("\"Start\"") {
            Some(ClientCommand::Start)
        } else if s.contains("\"pause\"") || s.contains("\"Pause\"") {
            Some(ClientCommand::Pause)
        } else if s.contains("\"resume\"") || s.contains("\"Resume\"") {
            Some(ClientCommand::Resume)
        } else if s.contains("\"stop\"") || s.contains("\"Stop\"") {
            Some(ClientCommand::Stop)
        } else if s.contains("\"reset\"") || s.contains("\"Reset\"") {
            Some(ClientCommand::Reset)
        } else if s.contains("\"request_export\"") || s.contains("\"RequestExport\"") {
            Some(ClientCommand::RequestExport)
        } else if s.contains("\"request_stats\"") || s.contains("\"RequestStats\"") {
            Some(ClientCommand::RequestStats)
        } else if s.contains("\"ping\"") || s.contains("\"Ping\"") {
            Some(ClientCommand::Ping)
        } else {
            None
        }
    }
}

/// Message types sent to clients.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Initial handshake with session info.
    Hello {
        app_name: String,
        protocol_version: u32,
    },
    /// Full recording export.
    Export(RecordingExport),
    /// Session state changed.
    StateChange {
        is_recording: bool,
        is_paused: bool,
    },
    /// Session statistics.
    Stats {
        total_events: u64,
        total_snapshots: u64,
        events_dropped: u64,
        snapshots_dropped: u64,
    },
    /// Acknowledgment of a command.
    Ack { command: String },
    /// Error response.
    Error { message: String },
    /// Ping response (pong).
    Pong,
}

impl ServerMessage {
    /// Serialize message to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Simple format: 4-byte length prefix + JSON payload
        let json = match self {
            ServerMessage::Hello {
                app_name,
                protocol_version,
            } => {
                format!(
                    r#"{{"type":"hello","app_name":"{}","protocol_version":{}}}"#,
                    app_name, protocol_version
                )
            }
            ServerMessage::Export(export) => {
                format!(
                    r#"{{"type":"export","total_events":{},"total_snapshots":{}}}"#,
                    export.stats.total_events, export.stats.total_snapshots
                )
            }
            ServerMessage::StateChange {
                is_recording,
                is_paused,
            } => {
                format!(
                    r#"{{"type":"state_change","is_recording":{},"is_paused":{}}}"#,
                    is_recording, is_paused
                )
            }
            ServerMessage::Stats {
                total_events,
                total_snapshots,
                events_dropped,
                snapshots_dropped,
            } => {
                format!(
                    r#"{{"type":"stats","total_events":{},"total_snapshots":{},"events_dropped":{},"snapshots_dropped":{}}}"#,
                    total_events, total_snapshots, events_dropped, snapshots_dropped
                )
            }
            ServerMessage::Ack { command } => {
                format!(r#"{{"type":"ack","command":"{}"}}"#, command)
            }
            ServerMessage::Error { message } => {
                format!(r#"{{"type":"error","message":"{}"}}"#, message)
            }
            ServerMessage::Pong => r#"{"type":"pong"}"#.to_string(),
        };

        let bytes = json.as_bytes();
        let len = bytes.len() as u32;
        let mut result = Vec::with_capacity(4 + bytes.len());
        result.extend_from_slice(&len.to_le_bytes());
        result.extend_from_slice(bytes);
        result
    }
}

/// Process a client command and return a response message.
fn handle_command(cmd: ClientCommand, session: &Arc<SharedRecordingSession>) -> ServerMessage {
    match cmd {
        ClientCommand::Start | ClientCommand::Resume => {
            session.start();
            ServerMessage::Ack {
                command: "start".to_string(),
            }
        }
        ClientCommand::Pause => {
            session.pause();
            ServerMessage::Ack {
                command: "pause".to_string(),
            }
        }
        ClientCommand::Stop => {
            session.stop();
            ServerMessage::Ack {
                command: "stop".to_string(),
            }
        }
        ClientCommand::Reset => {
            session.reset();
            ServerMessage::Ack {
                command: "reset".to_string(),
            }
        }
        ClientCommand::RequestExport => {
            let export = session.export();
            ServerMessage::Export(export)
        }
        ClientCommand::RequestStats => {
            let stats = session.stats();
            ServerMessage::Stats {
                total_events: stats.total_events,
                total_snapshots: stats.total_snapshots,
                events_dropped: stats.events_dropped,
                snapshots_dropped: stats.snapshots_dropped,
            }
        }
        ClientCommand::Ping => ServerMessage::Pong,
    }
}

#[cfg(unix)]
fn handle_client(
    mut stream: std::os::unix::net::UnixStream,
    session: Arc<SharedRecordingSession>,
) -> io::Result<()> {
    use std::time::Duration;

    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    // Send hello message
    let hello = ServerMessage::Hello {
        app_name: "blinc_app".to_string(),
        protocol_version: 1,
    };
    stream.write_all(&hello.to_bytes())?;

    // Track last state to avoid sending redundant updates
    let mut last_recording = session.is_recording();
    let mut last_paused = session.is_paused();

    // Main loop: respond to commands and send state updates
    let mut buf = [0u8; 1024];
    loop {
        // Check for incoming commands
        match stream.read(&mut buf) {
            Ok(0) => {
                // Client disconnected
                return Ok(());
            }
            Ok(n) => {
                // Parse and handle command
                if let Some(cmd) = ClientCommand::from_bytes(&buf[..n]) {
                    let response = handle_command(cmd, &session);
                    stream.write_all(&response.to_bytes())?;
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Timeout, continue
            }
            Err(e) => {
                return Err(e);
            }
        }

        // Send state change if state has changed
        let is_recording = session.is_recording();
        let is_paused = session.is_paused();
        if is_recording != last_recording || is_paused != last_paused {
            let state = ServerMessage::StateChange {
                is_recording,
                is_paused,
            };
            stream.write_all(&state.to_bytes())?;
            last_recording = is_recording;
            last_paused = is_paused;
        }

        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(windows)]
fn handle_client_tcp(
    mut stream: std::net::TcpStream,
    session: Arc<SharedRecordingSession>,
) -> io::Result<()> {
    use std::time::Duration;

    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    let hello = ServerMessage::Hello {
        app_name: "blinc_app".to_string(),
        protocol_version: 1,
    };
    stream.write_all(&hello.to_bytes())?;

    // Track last state to avoid sending redundant updates
    let mut last_recording = session.is_recording();
    let mut last_paused = session.is_paused();

    let mut buf = [0u8; 1024];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                // Parse and handle command
                if let Some(cmd) = ClientCommand::from_bytes(&buf[..n]) {
                    let response = handle_command(cmd, &session);
                    stream.write_all(&response.to_bytes())?;
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e),
        }

        // Send state change if state has changed
        let is_recording = session.is_recording();
        let is_paused = session.is_paused();
        if is_recording != last_recording || is_paused != last_paused {
            let state = ServerMessage::StateChange {
                is_recording,
                is_paused,
            };
            stream.write_all(&state.to_bytes())?;
            last_recording = is_recording;
            last_paused = is_paused;
        }

        thread::sleep(Duration::from_millis(100));
    }
}

/// Start a debug server with the default configuration.
///
/// This is a convenience function for quick setup.
pub fn start_local_server(session: Arc<SharedRecordingSession>) -> io::Result<ServerHandle> {
    let config = DebugServerConfig::default();
    DebugServer::new(config, session).start()
}

/// Start a debug server with a custom app name.
pub fn start_local_server_named(
    app_name: impl Into<String>,
    session: Arc<SharedRecordingSession>,
) -> io::Result<ServerHandle> {
    let config = DebugServerConfig::new(app_name);
    DebugServer::new(config, session).start()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_path_unix() {
        let config = DebugServerConfig::new("test_app");
        #[cfg(unix)]
        assert_eq!(
            config.socket_path(),
            PathBuf::from("/tmp/blinc/test_app.sock")
        );
    }

    #[test]
    fn test_server_message_serialization() {
        let msg = ServerMessage::Hello {
            app_name: "test".to_string(),
            protocol_version: 1,
        };
        let bytes = msg.to_bytes();
        assert!(bytes.len() > 4);

        // Check length prefix
        let len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        assert_eq!(len + 4, bytes.len());
    }

    #[test]
    fn test_command_parsing() {
        // Test parsing various command formats
        let start_cmd = br#"{"type":"start"}"#;
        assert!(matches!(
            ClientCommand::from_bytes(start_cmd),
            Some(ClientCommand::Start)
        ));

        let pause_cmd = br#"{"type":"pause"}"#;
        assert!(matches!(
            ClientCommand::from_bytes(pause_cmd),
            Some(ClientCommand::Pause)
        ));

        let stop_cmd = br#"{"type":"stop"}"#;
        assert!(matches!(
            ClientCommand::from_bytes(stop_cmd),
            Some(ClientCommand::Stop)
        ));

        let ping_cmd = br#"{"type":"ping"}"#;
        assert!(matches!(
            ClientCommand::from_bytes(ping_cmd),
            Some(ClientCommand::Ping)
        ));

        let export_cmd = br#"{"type":"request_export"}"#;
        assert!(matches!(
            ClientCommand::from_bytes(export_cmd),
            Some(ClientCommand::RequestExport)
        ));

        // Unknown command should return None
        let unknown_cmd = br#"{"type":"unknown"}"#;
        assert!(ClientCommand::from_bytes(unknown_cmd).is_none());
    }

    #[test]
    fn test_command_with_length_prefix() {
        // Commands can optionally have a 4-byte length prefix
        let json = br#"{"type":"start"}"#;
        let len = json.len() as u32;
        let mut prefixed = Vec::new();
        prefixed.extend_from_slice(&len.to_le_bytes());
        prefixed.extend_from_slice(json);

        assert!(matches!(
            ClientCommand::from_bytes(&prefixed),
            Some(ClientCommand::Start)
        ));
    }
}
