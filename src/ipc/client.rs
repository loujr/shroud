//! IPC client for CLI communication with daemon.
//!
//! Provides functions for connecting to the Shroud daemon and sending commands.
//!
//! # Example
//!
//! ```ignore
//! use shroud::ipc::client;
//! use shroud::ipc::protocol::IpcCommand;
//!
//! let response = client::send_command(IpcCommand::Status).await?;
//! println!("Status: {:?}", response);
//! ```

use log::debug;
use std::io;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use super::protocol::{socket_path, IpcCommand, IpcResponse};

use thiserror::Error;

/// Error type for IPC client operations.
#[derive(Error, Debug)]
pub enum ClientError {
    /// Failed to connect to daemon socket
    #[error("Failed to connect to daemon: {0}")]
    ConnectionFailed(#[source] io::Error),
    /// Failed to send command
    #[error("Failed to send command: {0}")]
    SendFailed(#[source] io::Error),
    /// Failed to receive response
    #[error("Failed to receive response: {0}")]
    ReceiveFailed(#[source] io::Error),
    /// Failed to parse response
    #[error("Failed to parse response: {0}")]
    ParseError(#[from] serde_json::Error),
    /// Daemon is not running
    #[error("Daemon is not running. Start it with: shroud --daemon")]
    DaemonNotRunning,
    /// Unexpected response from daemon
    #[error("Unexpected response: {0}")]
    UnexpectedResponse(String),
}

/// Connect to the Shroud daemon.
///
/// Returns a connected Unix stream, or an error if the daemon is not running.
pub async fn connect_to_daemon() -> Result<UnixStream, ClientError> {
    let path = socket_path();

    if !path.exists() {
        return Err(ClientError::DaemonNotRunning);
    }

    UnixStream::connect(&path).await.map_err(|e| {
        if e.kind() == io::ErrorKind::ConnectionRefused {
            ClientError::DaemonNotRunning
        } else {
            ClientError::ConnectionFailed(e)
        }
    })
}

/// Send a command to the daemon and receive the response.
///
/// # Arguments
///
/// * `command` - The command to send
///
/// # Returns
///
/// The response from the daemon, or an error if communication failed.
pub async fn send_command(command: IpcCommand) -> Result<IpcResponse, ClientError> {
    let stream = connect_to_daemon().await?;
    send_command_on_stream(stream, command).await
}

/// Send a command on an existing stream.
///
/// This is useful when you want to reuse a connection for multiple commands.
pub async fn send_command_on_stream(
    stream: UnixStream,
    command: IpcCommand,
) -> Result<IpcResponse, ClientError> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Serialize and send command
    let command_json = serde_json::to_string(&command).map_err(ClientError::ParseError)?;

    debug!("Sending command: {}", command_json);

    writer
        .write_all(command_json.as_bytes())
        .await
        .map_err(ClientError::SendFailed)?;
    writer
        .write_all(b"\n")
        .await
        .map_err(ClientError::SendFailed)?;
    writer.flush().await.map_err(ClientError::SendFailed)?;

    // Read response
    let mut response_line = String::new();
    reader
        .read_line(&mut response_line)
        .await
        .map_err(ClientError::ReceiveFailed)?;

    debug!("Received response: {}", response_line.trim());

    if response_line.trim().is_empty() {
        return Err(ClientError::ReceiveFailed(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Empty response from daemon",
        )));
    }

    // Parse response
    serde_json::from_str(response_line.trim()).map_err(ClientError::ParseError)
}

/// Check if the daemon is running.
///
/// Returns `true` if the daemon socket exists and is connectable.
#[allow(dead_code)]
pub async fn is_daemon_running() -> bool {
    connect_to_daemon().await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_error_display() {
        let err = ClientError::DaemonNotRunning;
        assert!(err.to_string().contains("not running"));
    }

    #[test]
    fn test_client_error_connection() {
        let io_err = io::Error::new(io::ErrorKind::ConnectionRefused, "refused");
        let err = ClientError::ConnectionFailed(io_err);
        assert!(err.to_string().contains("connect"));
    }

    #[tokio::test]
    async fn test_daemon_not_running() {
        let result = is_daemon_running().await;
        // Don't assert value as it depends on system state, just ensure it runs
        let _ = result;
    }
}
