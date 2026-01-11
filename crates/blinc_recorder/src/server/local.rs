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
    /// Ping to keep connection alive.
    Ping,
}

impl ServerMessage {
    /// Serialize message to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Simple format: 4-byte length prefix + JSON payload
        let json = match self {
            ServerMessage::Hello { app_name, protocol_version } => {
                format!(r#"{{"type":"hello","app_name":"{}","protocol_version":{}}}"#,
                    app_name, protocol_version)
            }
            ServerMessage::Export(export) => {
                // For now, just send basic stats
                format!(r#"{{"type":"export","total_events":{},"total_snapshots":{}}}"#,
                    export.stats.total_events, export.stats.total_snapshots)
            }
            ServerMessage::StateChange { is_recording, is_paused } => {
                format!(r#"{{"type":"state_change","is_recording":{},"is_paused":{}}}"#,
                    is_recording, is_paused)
            }
            ServerMessage::Ping => {
                r#"{"type":"ping"}"#.to_string()
            }
        };

        let bytes = json.as_bytes();
        let len = bytes.len() as u32;
        let mut result = Vec::with_capacity(4 + bytes.len());
        result.extend_from_slice(&len.to_le_bytes());
        result.extend_from_slice(bytes);
        result
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

    // Main loop: periodically send state and respond to commands
    let mut buf = [0u8; 1024];
    loop {
        // Check for incoming commands
        match stream.read(&mut buf) {
            Ok(0) => {
                // Client disconnected
                return Ok(());
            }
            Ok(_n) => {
                // TODO: Parse and handle commands
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Timeout, continue
            }
            Err(e) => {
                return Err(e);
            }
        }

        // Send current state
        let state = ServerMessage::StateChange {
            is_recording: session.is_recording(),
            is_paused: session.is_paused(),
        };
        stream.write_all(&state.to_bytes())?;

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

    let mut buf = [0u8; 1024];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(_n) => {}
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e),
        }

        let state = ServerMessage::StateChange {
            is_recording: session.is_recording(),
            is_paused: session.is_paused(),
        };
        stream.write_all(&state.to_bytes())?;

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
}
