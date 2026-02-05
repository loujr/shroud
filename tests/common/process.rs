//! Process management for E2E tests

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::{sleep, timeout};

// Track all spawned PIDs for cleanup
static SPAWNED_PIDS: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// Kill ALL shroud processes (global cleanup)
/// This function is designed to be fast and never hang
pub fn cleanup_all_shroud_processes() {
    // Kill tracked PIDs first (fast, direct syscall - never hangs)
    if let Ok(pids) = SPAWNED_PIDS.lock() {
        for pid in pids.iter() {
            unsafe {
                libc::kill(*pid as i32, libc::SIGKILL);
                // Also try to reap zombie immediately
                libc::waitpid(*pid as i32, std::ptr::null_mut(), libc::WNOHANG);
            }
        }
    }

    // Use pkill directly (don't spawn timeout subprocess which can cause issues)
    // These are fire-and-forget - we don't wait for them
    if let Ok(mut child) = Command::new("pkill")
        .args(["-9", "-f", "shroud --headless"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        // Give it 100ms max then forget about it
        std::thread::sleep(Duration::from_millis(100));
        let _ = child.try_wait();
    }

    if let Ok(mut child) = Command::new("pkill")
        .args(["-9", "-x", "shroud"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        std::thread::sleep(Duration::from_millis(100));
        let _ = child.try_wait();
    }
}

/// Managed shroud daemon process for testing
pub struct ShroudProcess {
    child: Option<Child>,
    socket_path: String,
    binary_path: String,
    start_timeout: Duration,
    stop_timeout: Duration,
}

impl ShroudProcess {
    pub fn new(binary: impl AsRef<Path>, socket: impl AsRef<Path>) -> Self {
        Self {
            child: None,
            socket_path: socket.as_ref().to_string_lossy().to_string(),
            binary_path: binary.as_ref().to_string_lossy().to_string(),
            start_timeout: Duration::from_secs(10),
            stop_timeout: Duration::from_secs(5),
        }
    }

    /// Set shorter timeouts for CI
    pub fn with_ci_timeouts(mut self) -> Self {
        self.start_timeout = Duration::from_secs(5);
        self.stop_timeout = Duration::from_secs(2);
        self
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
            .env("SHROUD_LOG_LEVEL", "warn")
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|e| format!("Failed to start: {}", e))?;

        // Track PID for cleanup
        let pid = child.id();
        if let Ok(mut pids) = SPAWNED_PIDS.lock() {
            pids.push(pid);
        }

        self.child = Some(child);

        // Wait for daemon to be ready with timeout
        match timeout(self.start_timeout, self.wait_ready_internal()).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => {
                self.force_kill();
                Err(format!("Daemon failed to start: {}", e))
            }
            Err(_) => {
                self.force_kill();
                Err("Daemon start timed out".to_string())
            }
        }
    }

    /// Start in headless mode
    pub async fn start_headless(&mut self) -> Result<(), String> {
        self.start_with_args(&["--headless"]).await
    }

    /// Internal wait ready (no timeout - caller handles it)
    async fn wait_ready_internal(&self) -> Result<(), String> {
        for _ in 0..50 {
            if self.ping_sync() {
                return Ok(());
            }
            sleep(Duration::from_millis(100)).await;
        }
        Err("Daemon not responding".to_string())
    }

    /// Synchronous ping check
    fn ping_sync(&self) -> bool {
        Command::new(&self.binary_path)
            .args(["status"])
            .env("SHROUD_SOCKET", &self.socket_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Wait for daemon to respond to ping
    pub async fn wait_ready(&self, max_wait: Duration) -> Result<(), String> {
        match timeout(max_wait, self.wait_ready_internal()).await {
            Ok(result) => result,
            Err(_) => Err("Daemon did not become ready".to_string()),
        }
    }

    /// Send ping command
    pub async fn ping(&self) -> Result<(), String> {
        self.run_command(&["status"]).await.map(|_| ())
    }

    /// Run a shroud command with timeout
    pub async fn run_command(&self, args: &[&str]) -> Result<String, String> {
        let result = timeout(Duration::from_secs(5), async {
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
        })
        .await;

        match result {
            Ok(r) => r,
            Err(_) => Err("Command timed out".to_string()),
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
        if self.child.is_none() {
            return Ok(());
        }

        // First try graceful quit command
        let _ = self.run_command(&["quit"]).await;

        if let Some(ref mut child) = self.child {
            // Wait for exit with timeout
            match timeout(self.stop_timeout, async {
                loop {
                    match child.try_wait() {
                        Ok(Some(_)) => break Ok(()),
                        Ok(None) => sleep(Duration::from_millis(50)).await,
                        Err(e) => break Err(e.to_string()),
                    }
                }
            })
            .await
            {
                Ok(Ok(())) => {
                    self.cleanup_pid();
                    self.child = None;
                    return Ok(());
                }
                _ => {
                    // Force kill
                    self.force_kill();
                    return Err("Had to force kill daemon".to_string());
                }
            }
        }
        self.child = None;
        Ok(())
    }

    /// Force kill the daemon immediately
    fn force_kill(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            // Don't use wait() as it can block - use try_wait with a brief sleep
            for _ in 0..10 {
                if let Ok(Some(_)) = child.try_wait() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            // Final try_wait to reap if possible, but don't block
            let _ = child.try_wait();
        }
        self.cleanup_pid();
        self.child = None;
    }

    /// Remove our PID from the tracked list
    fn cleanup_pid(&self) {
        if let Some(ref child) = self.child {
            let pid = child.id();
            if let Ok(mut pids) = SPAWNED_PIDS.lock() {
                pids.retain(|&p| p != pid);
            }
        }
    }

    /// Kill the daemon immediately (for crash recovery tests)
    pub fn kill(&mut self) {
        self.force_kill();
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
        // ALWAYS force kill on drop
        self.force_kill();

        // Also clean up socket
        let _ = std::fs::remove_file(&self.socket_path);
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
