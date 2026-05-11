// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-Commercial
// Copyright (C) 2026 Louis Nelson Jr. <https://lousclues.com>

//! Error types returned by the kill switch firewall backends.
//!
//! All firewall operations (iptables, ip6tables, nftables) funnel into the
//! [`KillSwitchError`] enum so that callers can react to permission failures,
//! missing binaries, command failures, and process I/O errors uniformly.

use thiserror::Error;

/// Errors that can occur during kill switch operations.
#[derive(Error, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum KillSwitchError {
    /// iptables is not installed or not in PATH
    #[error("iptables is not available. Install with: sudo apt install iptables")]
    #[allow(dead_code)]
    NotFound,

    /// Permission denied - need elevated privileges
    #[error(
        "Permission denied. Kill switch requires sudo access. Run: ./setup.sh --install-sudoers"
    )]
    Permission,

    /// Failed to spawn iptables/sudo process
    #[error("Failed to spawn iptables process: {0}")]
    Spawn(#[source] std::io::Error),

    /// iptables command returned error
    #[error("iptables command failed: {0}")]
    Command(String),

    /// Failed to write to process stdin
    #[error("Failed to write to process: {0}")]
    Write(#[source] std::io::Error),

    /// Failed waiting for iptables process
    #[error("Failed waiting for iptables process: {0}")]
    Wait(#[source] std::io::Error),
}
