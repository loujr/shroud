//! Process management for E2E tests

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::time::{sleep, timeout};

/// Managed shroud daemon process for testing
pub struct ShroudProcess {
    child: Option<Child>,
    socket_path: String,
    binary_path: String,
}

impl ShroudProcess {
    pub fn new(binary: impl AsRef<Path>, socket: impl AsRef<Path>) -> Self {
        Self {
            child: None,
            socket_path: socket.as_ref().to_string_lossy().to_string(),
            binary_path: binary.as_ref().to_string_lossy().to_string(),
        }
    }

    /// Start the daemon and wait for it to be ready
    pub async fn start(&mut self) -> Result<(), String> {
        self.start_with_args(&[]).await
    }

    /// Start with custom arguments
    pub async fn start_with_args(&mut self, args: &[&str]) -> Result<(), String> {
        let mut cmd = Command::new(&self.binary_path);
        cmd.args(args)
            .env("SHROUD_SOCKET", &self.socket_path)
            .env("SHROUD_TEST_MODE", "1")
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd.spawn().map_err(|e| format!("Failed to start: {}", e))?;
        self.child = Some(child);

        // Wait for daemon to be ready
        self.wait_ready(Duration::from_secs(10)).await
    }

    /// Start in headless mode
    pub async fn start_headless(&mut self) -> Result<(), String> {
        self.start_with_args(&["--headless"]).await
    }

    /// Wait for daemon to respond to ping
    pub async fn wait_ready(&self, max_wait: Duration) -> Result<(), String> {
        let start = std::time::Instant::now();

        while start.elapsed() < max_wait {
            if self.ping().await.is_ok() {
                return Ok(());
            }
            sleep(Duration::from_millis(100)).await;
        }

        Err("Daemon did not become ready".to_string())
    }

    /// Send ping command
    pub async fn ping(&self) -> Result<(), String> {
        self.run_command(&["status"]).await.map(|_| ())
    }

    /// Run a shroud command
    pub async fn run_command(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new(&self.binary_path)
            .args(args)
            .env("SHROUD_SOCKET", &self.socket_path)
            .output()
            .map_err(|e| e.to_string())?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    /// Send connect command
    pub async fn connect(&self, vpn: &str) -> Result<String, String> {
        self.run_command(&["connect", vpn]).await
    }

    /// Send disconnect command
    pub async fn disconnect(&self) -> Result<String, String> {
        self.run_command(&["disconnect"]).await
    }

    /// Get status
    pub async fn status(&self) -> Result<String, String> {
        self.run_command(&["status"]).await
    }

    /// Enable kill switch
    pub async fn ks_enable(&self) -> Result<String, String> {
        self.run_command(&["ks", "on"]).await
    }

    /// Disable kill switch
    pub async fn ks_disable(&self) -> Result<String, String> {
        self.run_command(&["ks", "off"]).await
    }

    /// Stop the daemon gracefully
    pub async fn stop(&mut self) -> Result<(), String> {
        // First try graceful quit command
        let _ = self.run_command(&["quit"]).await;

        if let Some(ref mut child) = self.child {
            // Wait for exit with timeout
            match timeout(Duration::from_secs(5), async {
                loop {
                    match child.try_wait() {
                        Ok(Some(_)) => break,
                        Ok(None) => sleep(Duration::from_millis(100)).await,
                        Err(_) => break,
                    }
                }
            })
            .await
            {
                Ok(_) => {}
                Err(_) => {
                    // Force kill if graceful shutdown failed
                    let _ = child.kill();
                }
            }
        }
        self.child = None;
        Ok(())
    }

    /// Kill the daemon immediately (for crash recovery tests)
    pub fn kill(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
        }
        self.child = None;
    }

    /// Check if daemon is running
    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }

    /// Get PID if running
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }
}

impl Drop for ShroudProcess {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Run a system command and capture output
pub fn run_system_command(cmd: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Run command with sudo
pub fn run_sudo(cmd: &str, args: &[&str]) -> Result<String, String> {
    let mut sudo_args = vec![cmd];
    sudo_args.extend(args);
    run_system_command("sudo", &sudo_args)
}
