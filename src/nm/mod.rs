//! NetworkManager module
//!
//! Provides the interface to NetworkManager for managing VPN connections.
//! Currently uses nmcli subprocess calls; future work will add D-Bus event subscription.

pub mod client;
pub mod connections;
#[cfg(test)]
pub mod mock;
pub mod parsing;
pub mod traits;

/// Get the nmcli command path (centralized for all NM modules).
///
/// Supports `SHROUD_NMCLI` env override for non-standard installations (NixOS,
/// custom prefix) and for test mocking.
pub(crate) fn nmcli_command() -> String {
    if let Ok(path) = std::env::var("SHROUD_NMCLI") {
        return path;
    }
    "nmcli".to_string()
}

#[allow(unused_imports)]
pub use client::{
    connect, disconnect, get_active_vpn, get_active_vpn_with_state, get_all_active_vpns,
    get_vpn_state, kill_orphan_openvpn_processes, list_vpn_connections,
};
#[allow(unused_imports)]
pub use connections::{get_vpn_type, list_vpn_connections_with_types, VpnConnection, VpnType};
#[cfg(test)]
pub use mock::{MockNmClient, NmCall};
#[allow(unused_imports)]
pub use traits::{NmCliClient, NmClient, NmError};
