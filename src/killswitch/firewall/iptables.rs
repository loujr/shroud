// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-Commercial
// Copyright (C) 2026 Louis Nelson Jr. <https://lousclues.com>

//! iptables (IPv4) backend execution layer.
//!
//! Owns the runtime concerns of the iptables backend:
//! - Executing the rule script line-by-line via `sudo -n iptables ...`
//!   (never via shell interpolation — every command is built with
//!   `Command::new().args()`).
//! - Detecting `iptables-nft` failures and falling back to `iptables-legacy`.
//! - Verifying that the SHROUD_KILLSWITCH chain is wired into the OUTPUT
//!   chain after the script runs.
//! - Probing for sudo/iptables access without prompting for a password.

use std::process::Stdio;
use tokio::process::Command;

use crate::killswitch::paths::iptables;

use super::{KillSwitch, KillSwitchError, CHAIN_NAME};

impl KillSwitch {
    /// Timeout for sudo/iptables commands (seconds)
    /// Long enough for normal operations, short enough to detect hangs
    pub(super) const SUDO_CMD_TIMEOUT_SECS: u64 = 30;

    pub(super) async fn run_single_script(&self, script: &str) -> Result<(), KillSwitchError> {
        use tokio::process::Command;
        use tokio::time::{timeout, Duration};

        for raw_line in script.lines() {
            let mut line = raw_line.trim().to_string();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let ignore_error = line.contains("|| true");
            if let Some(stripped) = line.strip_suffix("|| true") {
                line = stripped.trim().to_string();
            }
            line = line.replace("2>/dev/null", "").trim().to_string();
            if line.is_empty() || line == "exit 0" {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            let mut cmd = parts[0];
            if self.use_legacy
                && (parts[0].ends_with("iptables") || parts[0].ends_with("ip6tables"))
            {
                if let Some(legacy_cmd) = Self::legacy_variant(parts[0]).await {
                    cmd = legacy_cmd;
                }
            }

            // HARDENING: Add timeout to sudo commands to prevent hanging
            // on password prompts or frozen iptables kernel modules
            let output = match timeout(
                Duration::from_secs(Self::SUDO_CMD_TIMEOUT_SECS),
                Command::new("sudo")
                    .arg("-n") // non-interactive: fail immediately if password needed
                    .arg(cmd)
                    .args(&parts[1..])
                    .output(),
            )
            .await
            {
                Ok(Ok(output)) => output,
                Ok(Err(e)) => return Err(KillSwitchError::Spawn(e)),
                Err(_) => {
                    return Err(KillSwitchError::Command(format!(
                        "Command timed out after {}s (possible sudo prompt or frozen kernel module): {}",
                        Self::SUDO_CMD_TIMEOUT_SECS,
                        line
                    )));
                }
            };

            if !output.status.success() && !ignore_error {
                let code = output.status.code().unwrap_or(-1);
                if code == 126 || code == 127 {
                    return Err(KillSwitchError::Permission);
                }
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stderr_lower = stderr.to_lowercase();

                if (stderr_lower.contains("cache initialization failed")
                    || stderr_lower.contains("netlink: error")
                    || stderr_lower.contains("can't initialize iptables table")
                    || stderr_lower.contains("ip_tables"))
                    && !parts.is_empty()
                    && (parts[0].ends_with("iptables") || parts[0].ends_with("ip6tables"))
                {
                    if let Some(legacy_cmd) = Self::legacy_variant(parts[0]).await {
                        // HARDENING: Add timeout to legacy fallback as well
                        let legacy_output = match timeout(
                            Duration::from_secs(Self::SUDO_CMD_TIMEOUT_SECS),
                            Command::new("sudo")
                                .arg("-n")
                                .arg(legacy_cmd)
                                .args(&parts[1..])
                                .output(),
                        )
                        .await
                        {
                            Ok(Ok(output)) => output,
                            Ok(Err(e)) => return Err(KillSwitchError::Spawn(e)),
                            Err(_) => {
                                return Err(KillSwitchError::Command(format!(
                                    "Legacy command timed out after {}s: {}",
                                    Self::SUDO_CMD_TIMEOUT_SECS,
                                    line
                                )));
                            }
                        };

                        if legacy_output.status.success() {
                            continue;
                        }
                    }

                    let detail = if stderr.trim().is_empty() {
                        line.clone()
                    } else {
                        stderr.trim().to_string()
                    };

                    return Err(KillSwitchError::Command(format!(
                        "{} (iptables-nft failed; install iptables-legacy or nftables)",
                        detail
                    )));
                }

                let detail = if stderr.trim().is_empty() {
                    line.clone()
                } else {
                    stderr.trim().to_string()
                };
                return Err(KillSwitchError::Command(format!(
                    "Command failed (exit {}): {}",
                    code, detail
                )));
            }
        }

        Ok(())
    }

    pub(super) async fn legacy_variant(cmd: &str) -> Option<&'static str> {
        let (candidate, candidate_path) = if cmd.ends_with("iptables") {
            ("iptables-legacy", "/usr/sbin/iptables-legacy")
        } else if cmd.ends_with("ip6tables") {
            ("ip6tables-legacy", "/usr/sbin/ip6tables-legacy")
        } else {
            return None;
        };

        if Command::new(candidate)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Some(candidate);
        }

        if Command::new(candidate_path)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Some(candidate_path);
        }

        None
    }

    /// Check if sudo is available and configured for passwordless iptables.
    pub fn check_sudo_access() -> Result<(), KillSwitchError> {
        let output = std::process::Command::new("sudo")
            .args(["-n", iptables(), "-L", "-n"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(KillSwitchError::Spawn)?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_lower = stderr.to_lowercase();
        if stderr_lower.contains("ip_tables")
            || stderr_lower.contains("table does not exist")
            || stderr_lower.contains("can't initialize iptables table")
            || stderr_lower.contains("cache initialization failed")
            || stderr_lower.contains("netlink: error")
        {
            return Err(KillSwitchError::Command(format!(
                "Sudo check failed: {}",
                stderr.trim()
            )));
        }

        if output.status.success() {
            return Ok(());
        }

        if stderr_lower.contains("permission denied") || stderr_lower.contains("password") {
            return Err(KillSwitchError::Permission);
        }

        Err(KillSwitchError::Command(format!(
            "Sudo check failed: {}",
            stderr.trim()
        )))
    }

    pub(super) fn check_iptables_legacy_access() -> Result<bool, KillSwitchError> {
        let output = std::process::Command::new("sudo")
            .args(["-n", "iptables-legacy", "-L", "-n"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output();

        match output {
            Ok(out) => Ok(out.status.success()),
            Err(_) => Ok(false),
        }
    }

    /// Verify our rules are actually in place
    pub(super) async fn verify_rules_exist(&self) -> bool {
        // Check if our chain exists and has the jump rule
        // Use sudo -n to avoid password prompts
        let output = Command::new("sudo")
            .args(["-n", iptables(), "-C", "OUTPUT", "-j", CHAIN_NAME])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        matches!(output, Ok(status) if status.success())
    }
}
