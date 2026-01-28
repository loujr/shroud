//! IPC server for the Shroud daemon.
//!
//! Listens on a Unix domain socket for commands from CLI clients.
//!
//! # Architecture
//!
//! The server runs in a dedicated tokio task and forwards received commands
//! to the supervisor via a channel. Responses are sent back through the socket.

use log::{debug, error, info, warn};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

use super::protocol::{socket_path, IpcCommand, IpcResponse};
use thiserror::Error;

/// Errors that can occur in the IPC server.
#[derive(Error, Debug)]
pub enum ServerError {
    /// Failed to bind to socket
    #[error("Failed to bind to socket at {path}: {source}")]
    BindFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// Failed to accept connection
    #[error("Failed to accept connection: {0}")]
    AcceptFailed(#[source] std::io::Error),

    /// Failed to remove stale socket
    #[error("Failed to remove stale socket: {0}")]
    CleanupFailed(#[source] std::io::Error),

    /// Channel closed unexpectedly
    #[error("Command channel closed unexpectedly")]
    ChannelClosed,
}

/// Unix socket server for IPC communication.
pub struct IpcServer {
    /// Channel to send received commands to the supervisor
    command_tx: mpsc::Sender<(IpcCommand, mpsc::Sender<IpcResponse>)>,
}

impl IpcServer {
    /// Create a new IPC server.
    ///
    /// # Arguments
    ///
    /// * `command_tx` - Channel sender for forwarding commands to supervisor
    pub fn new(command_tx: mpsc::Sender<(IpcCommand, mpsc::Sender<IpcResponse>)>) -> Self {
        Self { command_tx }
    }

    /// Run the IPC server.
    ///
    /// Binds to the Unix socket and accepts client connections.
    /// This method runs indefinitely until an error occurs.
    pub async fn run(self) -> Result<(), ServerError> {
        let path = socket_path();

        // Remove stale socket file if it exists
        if path.exists() {
            std::fs::remove_file(&path).map_err(ServerError::CleanupFailed)?;
        }

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ServerError::BindFailed {
                path: parent.to_string_lossy().to_string(),
                source: e,
            })?;
        }

        let listener = UnixListener::bind(&path).map_err(|e| ServerError::BindFailed {
            path: path.to_string_lossy().to_string(),
            source: e,
        })?;

        // Set secure permissions (600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path)
                .map_err(|e| ServerError::BindFailed {
                    path: path.to_string_lossy().to_string(),
                    source: e,
                })?
                .permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&path, perms).map_err(|e| ServerError::BindFailed {
                path: path.to_string_lossy().to_string(),
                source: e,
            })?;
        }

        info!("IPC server listening on {:?}", path);

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let tx = self.command_tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, tx).await {
                            warn!("Client connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// Handle a single client connection.
    async fn handle_connection(
        stream: UnixStream,
        command_tx: mpsc::Sender<(IpcCommand, mpsc::Sender<IpcResponse>)>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        while reader.read_line(&mut line).await? > 0 {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                line.clear();
                continue;
            }

            debug!("Received command: {}", trimmed);

            let response = match serde_json::from_str::<IpcCommand>(trimmed) {
                Ok(cmd) => {
                    // Create a one-shot channel for the response
                    let (resp_tx, mut resp_rx) = mpsc::channel(1);

                    // Send command to supervisor
                    if command_tx.send((cmd, resp_tx)).await.is_err() {
                        IpcResponse::Error {
                            message: "Supervisor channel closed".to_string(),
                        }
                    } else {
                        // Wait for response from supervisor
                        // We use a timeout to prevent hanging forever if supervisor is busy/deadlocked
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(5),
                            resp_rx.recv(),
                        )
                        .await
                        {
                            Ok(Some(resp)) => resp,
                            Ok(None) => IpcResponse::Error {
                                message: "Supervisor dropped the response channel".to_string(),
                            },
                            Err(_) => IpcResponse::Error {
                                message: "Timeout waiting for supervisor response".to_string(),
                            },
                        }
                    }
                }
                Err(e) => {
                    warn!("Invalid command: {}", e);
                    IpcResponse::Error {
                        message: format!("Invalid command: {}", e),
                    }
                }
            };

            let response_json = serde_json::to_string(&response)?;
            writer.write_all(response_json.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;

            line.clear();
        }

        Ok(())
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        // Clean up socket file
        let path = socket_path();
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_creation() {
        let (tx, _rx) = mpsc::channel(1);
        let _server = IpcServer::new(tx);
    }
}
