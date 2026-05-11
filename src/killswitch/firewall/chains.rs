// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-Commercial
// Copyright (C) 2026 Louis Nelson Jr. <https://lousclues.com>

//! SHROUD_* chain creation and teardown shared across backends.
//!
//! When the kill switch is enabled and disabled repeatedly across crashes,
//! restarts, or signals, the kernel may accumulate duplicate jump rules and
//! orphaned chains. This module owns the **robust cleanup** loop that
//! removes every duplicate of every rule the kill switch may have added,
//! across all three backends:
//!
//! - iptables (IPv4) `OUTPUT -j SHROUD_KILLSWITCH` jump rules
//! - The `SHROUD_KILLSWITCH` chain itself (flush + delete)
//! - ip6tables (IPv6) direct rules and `SHROUD_KILLSWITCH` chain
//! - The nftables `shroud_killswitch` table
//!
//! The cleanup is best-effort and tolerant of failures: we don't know which
//! backend was active when the previous instance died, so we try them all.

use std::process::Stdio;
use tokio::process::Command;

use crate::killswitch::paths::{ip6tables, iptables, nft};

use super::ip6tables::IPV6_OUTPUT_RULES;
use super::{KillSwitch, CHAIN_NAME, NFT_TABLE};

impl KillSwitch {
    /// Robust cleanup that removes ALL duplicate rules (handles race conditions)
    pub(super) async fn robust_iptables_cleanup(&self) {
        // Remove ALL duplicate SHROUD_KILLSWITCH jump rules from OUTPUT chain
        for _ in 0..100 {
            // Safety limit
            let output = Command::new("sudo")
                .args(["-n", iptables(), "-D", "OUTPUT", "-j", CHAIN_NAME])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
            if !matches!(output, Ok(s) if s.success()) {
                break;
            }
        }

        // Flush and delete the chain
        let _ = Command::new("sudo")
            .args(["-n", iptables(), "-F", CHAIN_NAME])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        let _ = Command::new("sudo")
            .args(["-n", iptables(), "-X", CHAIN_NAME])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        // IPv6: Remove ALL duplicate rules inserted by Block/Tunnel modes
        // These are direct OUTPUT rules, not a chain
        for rule in IPV6_OUTPUT_RULES {
            // Remove ALL duplicates of each rule
            for _ in 0..100 {
                let mut cmd = Command::new("sudo");
                cmd.arg("-n").arg(ip6tables());
                for arg in *rule {
                    cmd.arg(arg);
                }
                let output = cmd
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .await;
                if !matches!(output, Ok(s) if s.success()) {
                    break;
                }
            }
        }

        // Also try to clean up IPv6 SHROUD_KILLSWITCH chain if it exists
        for _ in 0..100 {
            let output = Command::new("sudo")
                .args(["-n", ip6tables(), "-D", "OUTPUT", "-j", CHAIN_NAME])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
            if !matches!(output, Ok(s) if s.success()) {
                break;
            }
        }

        let _ = Command::new("sudo")
            .args(["-n", ip6tables(), "-F", CHAIN_NAME])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        let _ = Command::new("sudo")
            .args(["-n", ip6tables(), "-X", CHAIN_NAME])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        // Clean up nftables table too
        let _ = Command::new("sudo")
            .args(["-n", nft(), "delete", "table", "inet", NFT_TABLE])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
}
