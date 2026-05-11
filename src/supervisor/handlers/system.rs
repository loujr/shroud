// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-Commercial
// Copyright (C) 2026 Louis Nelson Jr. <https://lousclues.com>

//! System lifecycle event handlers

use tokio::time::{sleep, Duration};
use tracing::{debug, info};

use crate::daemon::lock::release_instance_lock;

impl super::super::VpnSupervisor {
    /// Handle quit command - clean shutdown
    pub(crate) async fn handle_quit(&mut self) {
        info!("Quit requested, cleaning up...");

        // Non-blocking kill switch cleanup with timeout
        if self.kill_switch.is_enabled() {
            info!("Cleaning up kill switch before shutdown");
            match crate::killswitch::cleanup_with_fallback() {
                crate::killswitch::CleanupResult::Cleaned => {
                    info!("Kill switch cleanup successful");
                }
                crate::killswitch::CleanupResult::NothingToClean => {
                    debug!("No kill switch rules to clean");
                }
                crate::killswitch::CleanupResult::Failed(_) => {
                    self.tray.notify(
                        "Cleanup Failed",
                        "Firewall rules may need manual cleanup. See logs.",
                    );
                }
            }
            self.kill_switch.sync_state();
        }

        // Show notification
        self.tray.notify("VPN Shroud", "Shutting down...");

        // Give notification time to show
        sleep(Duration::from_millis(300)).await;

        info!("Shutdown complete");

        // Clean up and signal exit
        release_instance_lock();
        let socket_path = crate::ipc::protocol::socket_path();
        let _ = std::fs::remove_file(&socket_path);
        self.exit_state.request("User quit");
    }

    pub(crate) async fn graceful_shutdown(&mut self) {
        info!("Performing graceful shutdown");

        if self.kill_switch.is_enabled() {
            info!("Cleaning up kill switch before shutdown");
            match crate::killswitch::cleanup_with_fallback() {
                crate::killswitch::CleanupResult::Cleaned => {
                    info!("Kill switch cleanup successful");
                }
                crate::killswitch::CleanupResult::NothingToClean => {
                    debug!("No kill switch rules to clean");
                }
                crate::killswitch::CleanupResult::Failed(_) => {
                    self.tray.notify(
                        "Cleanup Failed",
                        "Firewall rules may need manual cleanup. See logs.",
                    );
                }
            }
            self.kill_switch.sync_state();
        }

        release_instance_lock();

        let socket_path = crate::ipc::protocol::socket_path();
        if socket_path.exists() {
            let _ = std::fs::remove_file(&socket_path);
        }

        info!("Graceful shutdown complete");
    }
}

pub(super) fn resolve_restart_path() -> Result<std::path::PathBuf, String> {
    let exe_path =
        std::env::current_exe().map_err(|e| format!("Failed to get executable path: {}", e))?;

    let exe_display = exe_path.to_string_lossy();
    if exe_path.exists() && !exe_display.contains(" (deleted)") {
        return Ok(exe_path);
    }

    // Handle the update scenario: binary was replaced at the same path.
    // On Linux, /proc/self/exe shows "/path/to/shroud (deleted)" when the
    // original inode is removed, even if a new binary exists at the same path.
    // This is the normal flow during `scripts/update.sh` (rm + cp).
    if exe_display.contains(" (deleted)") {
        let original_path = exe_display.trim_end_matches(" (deleted)");
        let original = std::path::PathBuf::from(original_path);
        if original.exists() {
            info!(
                "Running binary was replaced (update). Using new binary at: {}",
                original.display()
            );
            return Ok(original);
        }
    }

    // SECURITY: Do NOT fall back to arbitrary user-writable paths.
    // If the running binary is deleted and no replacement exists at the
    // same path, refuse to restart (SHROUD-VULN-036).
    Err("Running binary has been deleted. Cannot safely restart. \
         Please restart manually: shroud"
        .to_string())
}
