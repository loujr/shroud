//! Kill switch module
//!
//! Provides VPN kill switch functionality using iptables.
//! When enabled, blocks all traffic except through the VPN tunnel.

pub mod boot;
pub mod cleanup;
#[allow(dead_code)]
pub mod cleanup_logic;
pub mod firewall;
pub mod paths;
#[allow(dead_code)]
pub mod rules;
pub mod sudo_check;

#[cfg(test)]
mod tests;

pub use cleanup::{cleanup_stale_on_startup, cleanup_with_fallback, CleanupResult};

// Re-export cleanup_all for headless mode
#[allow(unused_imports)]
pub use cleanup::cleanup_all;
pub use firewall::{KillSwitch, KillSwitchError};
#[allow(unused_imports)]
pub use paths::{ip6tables, ip6tables_path, iptables, iptables_path, nft, nft_path};
#[allow(unused_imports)]
pub use sudo_check::{
    check_sudo_access, check_sudo_access_with_message, validate_sudoers_on_startup,
    SudoAccessStatus,
};
