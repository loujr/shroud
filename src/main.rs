//! NetworkManager VPN Supervisor with System Tray
//!
//! A production-ready system tray application for managing VPN connections via NetworkManager
//! with auto-reconnect capabilities for Arch Linux / KDE Plasma.
//!
//! # Architecture
//!
//! - `state/` - State machine types and transitions
//! - `nm/` - NetworkManager interface (nmcli, future: D-Bus)
//! - `tray/` - System tray UI (ksni)
//!
//! # Building
//!
//! ```bash
//! cargo build --release
//! ```

mod nm;
mod state;
mod tray;

use log::{debug, error, info, warn};
use notify_rust::Notification;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{sleep, Duration};

use crate::nm::{
    connect as nm_connect, disconnect as nm_disconnect, get_active_vpn as nm_get_active_vpn,
    get_active_vpn_with_state as nm_get_active_vpn_with_state, get_vpn_state as nm_get_vpn_state,
    kill_orphan_openvpn_processes, list_vpn_connections as nm_list_vpn_connections,
};
use crate::state::{NmVpnState, VpnState};
use crate::tray::{SharedState, VpnCommand, VpnTray};

// ============================================================================
// Configuration Constants
// ============================================================================

/// Poll NetworkManager state every 2 seconds
const NM_POLL_INTERVAL_SECS: u64 = 2;

/// Wait after nmcli con up before verifying connection
const CONNECTION_VERIFY_DELAY_SECS: u64 = 5;

/// Maximum number of reconnection attempts before giving up
const MAX_RECONNECT_ATTEMPTS: u32 = 10;

/// Maximum number of connection attempts during handle_connect
const MAX_CONNECT_ATTEMPTS: u32 = 3;

/// Base delay for exponential backoff in seconds
const RECONNECT_BASE_DELAY_SECS: u64 = 2;

/// Cap on reconnect delay in seconds
const RECONNECT_MAX_DELAY_SECS: u64 = 30;

/// Grace period after intentional disconnect to prevent false drop detection
const POST_DISCONNECT_GRACE_SECS: u64 = 5;

/// Maximum attempts to verify disconnect completion
const DISCONNECT_VERIFY_MAX_ATTEMPTS: u32 = 30;

/// Maximum attempts to verify connection after nmcli con up
const CONNECTION_MONITOR_MAX_ATTEMPTS: u32 = 60;

/// Interval between connection monitoring attempts in milliseconds
const CONNECTION_MONITOR_INTERVAL_MS: u64 = 500;

/// Interval between disconnect verification attempts in milliseconds
const DISCONNECT_VERIFY_INTERVAL_MS: u64 = 500;

/// Settle time after disconnect is verified before connecting to new VPN
const POST_DISCONNECT_SETTLE_SECS: u64 = 3;

// ============================================================================
// VPN Supervisor
// ============================================================================

/// VPN Supervisor that manages VPN connections via NetworkManager
pub struct VpnSupervisor {
    /// Shared state accessible by the tray
    state: Arc<RwLock<SharedState>>,
    /// Channel receiver for commands from the tray
    rx: mpsc::Receiver<VpnCommand>,
    /// Tray handle for updating the icon
    tray_handle: Arc<std::sync::Mutex<Option<ksni::blocking::Handle<VpnTray>>>>,
    /// Timestamp of last intentional disconnect
    last_disconnect_time: Option<Instant>,
    /// Timestamp of last polling tick (for detecting time jumps/sleep/wake)
    last_poll_time: Instant,
}

impl VpnSupervisor {
    /// Create a new VPN supervisor
    pub fn new(
        state: Arc<RwLock<SharedState>>,
        rx: mpsc::Receiver<VpnCommand>,
        tray_handle: Arc<std::sync::Mutex<Option<ksni::blocking::Handle<VpnTray>>>>,
    ) -> Self {
        Self {
            state,
            rx,
            tray_handle,
            last_disconnect_time: None,
            last_poll_time: Instant::now(),
        }
    }

    /// Run the supervisor's main loop
    pub async fn run(mut self) {
        info!("VPN supervisor starting");

        // Initial connection refresh and state sync
        self.refresh_connections().await;
        self.sync_with_nm().await;
        self.last_poll_time = Instant::now();

        // Create an interval for NM polling
        let mut nm_poll_interval = tokio::time::interval(Duration::from_secs(NM_POLL_INTERVAL_SECS));

        loop {
            tokio::select! {
                // Handle commands from the tray
                Some(cmd) = self.rx.recv() => {
                    debug!("Received command: {:?}", cmd);
                    match cmd {
                        VpnCommand::Connect(server) => {
                            self.handle_connect(&server).await;
                        }
                        VpnCommand::Disconnect => {
                            self.handle_disconnect().await;
                        }
                        VpnCommand::ToggleAutoReconnect => {
                            self.toggle_auto_reconnect().await;
                        }
                        VpnCommand::RefreshConnections => {
                            self.refresh_connections().await;
                        }
                    }
                }

                // Poll NetworkManager state periodically
                _ = nm_poll_interval.tick() => {
                    // Detect time jumps (sleep/wake events)
                    let elapsed = self.last_poll_time.elapsed();
                    if elapsed > Duration::from_secs(NM_POLL_INTERVAL_SECS * 3) {
                        warn!(
                            "Time jump detected ({:.1}s since last poll), forcing state resync",
                            elapsed.as_secs_f32()
                        );
                        self.force_state_resync().await;
                    } else {
                        debug!("Polling NetworkManager state");
                        self.sync_with_nm().await;
                    }
                    self.last_poll_time = Instant::now();
                }
            }
        }
    }

    /// Handle connection to a server
    async fn handle_connect(&mut self, connection_name: &str) {
        info!("Connect requested: {}", connection_name);

        // Get current state and determine what to do
        let current_server = {
            let state = self.state.read().await;
            state.state.server_name().map(|s| s.to_string())
        };

        // If already connected to this server, do nothing
        if current_server.as_deref() == Some(connection_name) {
            info!("Already connected to {}", connection_name);
            return;
        }

        // If connected to different server, disconnect first with verification
        if let Some(ref current) = current_server {
            info!(
                "Disconnecting from {} before connecting to {}",
                current, connection_name
            );

            // Update state to show we're transitioning
            {
                let mut state = self.state.write().await;
                state.state = VpnState::Connecting {
                    server: connection_name.to_string(),
                };
            }
            self.update_tray();

            // Disconnect and wait for it to complete
            debug!("Calling nm_disconnect for: {}", current);
            if let Err(e) = nm_disconnect(current).await {
                warn!("Disconnect command failed (continuing anyway): {}", e);
            }

            // Wait and verify the connection is fully disconnected
            let mut disconnected = false;
            for attempt in 1..=DISCONNECT_VERIFY_MAX_ATTEMPTS {
                sleep(Duration::from_millis(DISCONNECT_VERIFY_INTERVAL_MS)).await;

                match nm_get_vpn_state(current).await {
                    Some(NmVpnState::Activated | NmVpnState::Deactivating | NmVpnState::Activating) => {
                        debug!("VPN '{}' still active, waiting... (attempt {})", current, attempt);
                    }
                    Some(NmVpnState::Inactive) | None => {
                        info!("Previous VPN '{}' disconnected successfully", current);
                        disconnected = true;
                        break;
                    }
                }
            }

            if !disconnected {
                warn!("Disconnect verification timed out, cleaning up orphan processes");
            }
            kill_orphan_openvpn_processes().await;

            // Extra settle time
            sleep(Duration::from_secs(POST_DISCONNECT_SETTLE_SECS)).await;
        }

        // Update state to Connecting
        {
            let mut state = self.state.write().await;
            state.state = VpnState::Connecting {
                server: connection_name.to_string(),
            };
        }
        self.update_tray();
        self.show_notification("VPN", &format!("Connecting to {}...", connection_name));

        // Attempt connection with retries
        let mut attempts = 0;

        while attempts < MAX_CONNECT_ATTEMPTS {
            attempts += 1;
            debug!("Connection attempt {} of {} for {}", attempts, MAX_CONNECT_ATTEMPTS, connection_name);

            match nm_connect(connection_name).await {
                Ok(_) => {
                    // Monitor the connection state
                    let mut connection_succeeded = false;
                    let mut failure_reason: Option<String> = None;

                    for monitor_attempt in 1..=CONNECTION_MONITOR_MAX_ATTEMPTS {
                        sleep(Duration::from_millis(CONNECTION_MONITOR_INTERVAL_MS)).await;

                        match nm_get_vpn_state(connection_name).await {
                            Some(NmVpnState::Activated) => {
                                info!("VPN '{}' successfully activated", connection_name);
                                connection_succeeded = true;
                                break;
                            }
                            Some(NmVpnState::Activating) => {
                                // Still connecting, keep waiting
                            }
                            Some(NmVpnState::Deactivating) => {
                                failure_reason = Some("Connection failed during activation".to_string());
                                break;
                            }
                            Some(NmVpnState::Inactive) | None => {
                                if monitor_attempt > 10 {
                                    failure_reason = Some("Connection never became active".to_string());
                                    break;
                                }
                            }
                        }
                    }

                    if connection_succeeded {
                        {
                            let mut state = self.state.write().await;
                            state.state = VpnState::Connected {
                                server: connection_name.to_string(),
                            };
                        }
                        self.sync_with_nm().await;
                        self.update_tray();
                        self.show_notification("VPN Connected", &format!("Connected to {}", connection_name));
                        return;
                    }

                    warn!("Connection monitoring failed: {}", failure_reason.as_deref().unwrap_or("timeout"));
                }
                Err(e) => {
                    warn!("Connection attempt {} failed: {}", attempts, e);
                }
            }

            if attempts < MAX_CONNECT_ATTEMPTS {
                sleep(Duration::from_secs(2)).await;
            }
        }

        // Connection failed after all attempts
        error!("Failed to connect to {} after {} attempts", connection_name, MAX_CONNECT_ATTEMPTS);
        {
            let mut state = self.state.write().await;
            state.state = VpnState::Failed {
                server: connection_name.to_string(),
                reason: "Connection verification failed".to_string(),
            };
        }
        self.update_tray();
        self.show_notification("VPN Failed", &format!("Could not connect to {}", connection_name));
    }

    /// Handle disconnect command
    async fn handle_disconnect(&mut self) {
        info!("Disconnecting VPN");

        let connection_name = {
            let state = self.state.read().await;
            state.state.server_name().map(|s| s.to_string())
        };

        if let Some(name) = connection_name {
            self.last_disconnect_time = Some(Instant::now());

            match nm_disconnect(&name).await {
                Ok(_) => {
                    info!("Disconnected successfully");
                    {
                        let mut state = self.state.write().await;
                        state.state = VpnState::Disconnected;
                    }
                    self.sync_with_nm().await;
                    self.update_tray();
                    self.show_notification("VPN Disconnected", "VPN connection closed");
                }
                Err(e) => {
                    error!("Failed to disconnect: {}", e);
                }
            }
        }
    }

    /// Sync internal state with NetworkManager
    async fn sync_with_nm(&mut self) {
        // Check if we're in grace period after intentional disconnect
        if let Some(disconnect_time) = self.last_disconnect_time {
            if disconnect_time.elapsed().as_secs() < POST_DISCONNECT_GRACE_SECS {
                debug!("In grace period after intentional disconnect");
                return;
            } else {
                self.last_disconnect_time = None;
            }
        }

        let active_vpn_info = nm_get_active_vpn_with_state().await;
        let (current_state, auto_reconnect) = {
            let state = self.state.read().await;
            (state.state.clone(), state.auto_reconnect)
        };

        let mut needs_tray_update = false;

        // Handle activating state
        if let Some(ref info) = active_vpn_info {
            if info.state == NmVpnState::Activating {
                match &current_state {
                    VpnState::Connecting { server } if server == &info.name => {
                        debug!("State synchronized: connecting to {}", info.name);
                    }
                    VpnState::Disconnected => {
                        info!("Detected external VPN activation: {}", info.name);
                        {
                            let mut state = self.state.write().await;
                            state.state = VpnState::Connecting { server: info.name.clone() };
                        }
                        needs_tray_update = true;
                    }
                    _ => {}
                }
                if needs_tray_update {
                    self.update_tray();
                }
                return;
            }
        }

        let active_vpn = active_vpn_info
            .filter(|info| info.state == NmVpnState::Activated)
            .map(|info| info.name);

        match (&current_state, &active_vpn) {
            // Connection dropped
            (VpnState::Connected { server }, None) => {
                warn!("Connection to {} dropped unexpectedly", server);
                if auto_reconnect {
                    info!("Auto-reconnect enabled, attempting to reconnect");
                    let server_clone = server.clone();
                    self.attempt_reconnect(&server_clone, 1).await;
                } else {
                    {
                        let mut state = self.state.write().await;
                        state.state = VpnState::Disconnected;
                    }
                    needs_tray_update = true;
                    self.show_notification("VPN Disconnected", "Connection dropped");
                }
            }

            // External switch
            (VpnState::Connected { server: our_server }, Some(nm_server)) if our_server != nm_server => {
                info!("VPN changed externally from {} to {}", our_server, nm_server);
                {
                    let mut state = self.state.write().await;
                    state.state = VpnState::Connected { server: nm_server.clone() };
                }
                needs_tray_update = true;
            }

            // External connection
            (VpnState::Disconnected, Some(vpn_name)) => {
                info!("Detected external VPN connection: {}", vpn_name);
                {
                    let mut state = self.state.write().await;
                    state.state = VpnState::Connected { server: vpn_name.clone() };
                }
                needs_tray_update = true;
            }

            // Connecting confirmed
            (VpnState::Connecting { server: target }, Some(active)) if target == active => {
                info!("Connection to {} confirmed by NM sync", target);
                {
                    let mut state = self.state.write().await;
                    state.state = VpnState::Connected { server: target.clone() };
                }
                needs_tray_update = true;
            }

            // Recovered from failed
            (VpnState::Failed { .. }, Some(vpn_name)) => {
                info!("VPN recovered, now connected to {}", vpn_name);
                {
                    let mut state = self.state.write().await;
                    state.state = VpnState::Connected { server: vpn_name.clone() };
                }
                needs_tray_update = true;
            }

            // Failed to disconnected
            (VpnState::Failed { server, .. }, None) => {
                info!("Failed connection to {} confirmed, transitioning to Disconnected", server);
                {
                    let mut state = self.state.write().await;
                    state.state = VpnState::Disconnected;
                }
                needs_tray_update = true;
            }

            // Already in sync
            _ => {}
        }

        if needs_tray_update {
            self.update_tray();
        }
    }

    /// Force a complete state resync with NetworkManager
    async fn force_state_resync(&mut self) {
        info!("Forcing complete state resync with NetworkManager");
        self.last_disconnect_time = None;
        self.refresh_connections().await;

        let active_vpn_info = nm_get_active_vpn_with_state().await;

        {
            let mut state = self.state.write().await;
            match active_vpn_info {
                Some(info) => match info.state {
                    NmVpnState::Activated => {
                        info!("Resync: VPN {} is fully active", info.name);
                        state.state = VpnState::Connected { server: info.name };
                    }
                    NmVpnState::Activating => {
                        info!("Resync: VPN {} is activating", info.name);
                        state.state = VpnState::Connecting { server: info.name };
                    }
                    _ => {
                        info!("Resync: No active VPN");
                        state.state = VpnState::Disconnected;
                    }
                },
                None => {
                    if !matches!(state.state, VpnState::Connecting { .. } | VpnState::Reconnecting { .. }) {
                        state.state = VpnState::Disconnected;
                    }
                }
            }
        }

        self.update_tray();
    }

    /// Attempt to reconnect with exponential backoff
    async fn attempt_reconnect(&mut self, connection_name: &str, initial_attempt: u32) {
        let mut attempt = initial_attempt;

        loop {
            if attempt > MAX_RECONNECT_ATTEMPTS {
                error!("Max reconnection attempts reached for {}", connection_name);
                {
                    let mut state = self.state.write().await;
                    state.state = VpnState::Failed {
                        server: connection_name.to_string(),
                        reason: format!("Max attempts ({}) exceeded", MAX_RECONNECT_ATTEMPTS),
                    };
                }
                self.update_tray();
                self.show_notification("VPN Reconnection Failed", &format!("Failed after {} attempts", MAX_RECONNECT_ATTEMPTS));
                return;
            }

            info!("Reconnection attempt {}/{} for {}", attempt, MAX_RECONNECT_ATTEMPTS, connection_name);

            {
                let mut state = self.state.write().await;
                state.state = VpnState::Reconnecting {
                    server: connection_name.to_string(),
                    attempt,
                    max_attempts: MAX_RECONNECT_ATTEMPTS,
                };
            }
            self.update_tray();

            let delay = std::cmp::min(RECONNECT_BASE_DELAY_SECS * (attempt as u64), RECONNECT_MAX_DELAY_SECS);
            sleep(Duration::from_secs(delay)).await;

            match nm_connect(connection_name).await {
                Ok(_) => {
                    sleep(Duration::from_secs(CONNECTION_VERIFY_DELAY_SECS)).await;

                    if let Some(active_vpn) = nm_get_active_vpn().await {
                        if active_vpn == connection_name {
                            info!("Successfully reconnected to {}", connection_name);
                            {
                                let mut state = self.state.write().await;
                                state.state = VpnState::Connected { server: connection_name.to_string() };
                            }
                            self.update_tray();
                            self.show_notification("VPN Reconnected", &format!("Reconnected to {}", connection_name));
                            return;
                        }
                    }
                    warn!("Reconnection verification failed, retrying");
                }
                Err(e) => {
                    error!("Reconnection attempt {} failed: {}", attempt, e);
                }
            }
            attempt += 1;
        }
    }

    /// Toggle auto-reconnect setting
    async fn toggle_auto_reconnect(&mut self) {
        let new_value = {
            let mut state = self.state.write().await;
            state.auto_reconnect = !state.auto_reconnect;
            state.auto_reconnect
        };
        info!("Auto-reconnect toggled to: {}", new_value);
        self.update_tray();
        self.show_notification("Auto-Reconnect", if new_value { "Enabled" } else { "Disabled" });
    }

    /// Refresh the list of available VPN connections
    async fn refresh_connections(&mut self) {
        info!("Refreshing VPN connections");
        let connections = nm_list_vpn_connections().await;
        {
            let mut state = self.state.write().await;
            state.connections = connections;
        }
        self.update_tray();
    }

    /// Update the tray icon with current state
    fn update_tray(&self) {
        let current_state = match self.state.try_read() {
            Ok(guard) => guard.clone(),
            Err(_) => return,
        };

        let tray_handle = self.tray_handle.clone();
        std::thread::spawn(move || {
            if let Ok(handle_guard) = tray_handle.lock() {
                if let Some(handle) = handle_guard.as_ref() {
                    handle.update(move |tray: &mut VpnTray| {
                        if let Ok(mut cached) = tray.cached_state.write() {
                            *cached = current_state.clone();
                        }
                    });
                }
            }
        });
    }

    /// Show a desktop notification
    fn show_notification(&self, title: &str, body: &str) {
        let title = title.to_string();
        let body = body.to_string();
        std::thread::spawn(move || {
            let _ = Notification::new()
                .summary(&title)
                .body(&body)
                .timeout(5000)
                .show();
        });
    }
}

// ============================================================================
// Instance Lock
// ============================================================================

/// Path to the lock file for single-instance enforcement
fn get_lock_file_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .expect("XDG_RUNTIME_DIR not set - cannot safely create lock file");
    PathBuf::from(runtime_dir).join("openvpn-tray.lock")
}

/// Acquire an exclusive lock on the instance lock file
fn acquire_instance_lock() -> Result<File, String> {
    let lock_path = get_lock_file_path();

    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| format!("Failed to open lock file: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&lock_path, std::fs::Permissions::from_mode(0o600));
    }

    let fd = file.as_raw_fd();
    let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };

    if result != 0 {
        let errno = std::io::Error::last_os_error();
        if errno.raw_os_error() == Some(libc::EWOULDBLOCK) {
            let mut contents = String::new();
            if let Ok(mut f) = File::open(&lock_path) {
                let _ = f.read_to_string(&mut contents);
            }
            let pid_info = contents.trim().parse::<u32>().map(|pid| format!(" (PID {})", pid)).unwrap_or_default();
            return Err(format!("Another instance is already running{}", pid_info));
        }
        return Err(format!("Failed to acquire lock: {}", errno));
    }

    let truncate_result = unsafe { libc::ftruncate(fd, 0) };
    if truncate_result != 0 {
        return Err(format!("Failed to truncate lock file: {}", std::io::Error::last_os_error()));
    }

    use std::io::Seek;
    let mut file = file;
    file.seek(std::io::SeekFrom::Start(0)).map_err(|e| format!("Failed to seek: {}", e))?;
    write!(file, "{}", std::process::id()).map_err(|e| format!("Failed to write PID: {}", e))?;
    file.sync_all().map_err(|e| format!("Failed to sync: {}", e))?;

    info!("Acquired instance lock (PID {})", std::process::id());
    Ok(file)
}

/// Clean up lock file on exit
fn release_instance_lock() {
    let lock_path = get_lock_file_path();
    if let Err(e) = fs::remove_file(&lock_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn!("Failed to remove lock file: {}", e);
        }
    } else {
        info!("Released instance lock");
    }
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() {
    env_logger::init();

    let _lock_file = match acquire_instance_lock() {
        Ok(file) => file,
        Err(msg) => {
            eprintln!("{}", msg);
            std::process::exit(1);
        }
    };

    ctrlc::set_handler(move || {
        info!("Shutdown signal received, cleaning up...");
        release_instance_lock();
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    info!("Starting NetworkManager VPN Supervisor");

    let state = Arc::new(RwLock::new(SharedState::default()));
    let (tx, rx) = mpsc::channel(16);
    let tray_handle = Arc::new(std::sync::Mutex::new(None));

    let supervisor = VpnSupervisor::new(state.clone(), rx, tray_handle.clone());
    tokio::spawn(supervisor.run());

    let tray_service = VpnTray::new(tx);

    info!("Starting system tray");
    let tray_handle_clone = tray_handle.clone();
    std::thread::spawn(move || {
        use ksni::blocking::TrayMethods;
        match tray_service.spawn() {
            Ok(handle) => {
                if let Ok(mut guard) = tray_handle_clone.lock() {
                    *guard = Some(handle);
                }
            }
            Err(e) => {
                error!("Failed to spawn tray: {}", e);
                std::process::exit(1);
            }
        }
    });

    std::future::pending::<()>().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vpn_state_server_name() {
        let state = VpnState::Connected { server: "test".to_string() };
        assert_eq!(state.server_name(), Some("test"));

        let state = VpnState::Disconnected;
        assert_eq!(state.server_name(), None);
    }
}
