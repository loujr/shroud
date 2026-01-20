//! NetworkManager module
//!
//! Provides the interface to NetworkManager for managing VPN connections.
//! Currently uses nmcli subprocess calls; future work will add D-Bus event subscription.

pub mod client;

pub use client::{
    connect, disconnect, get_active_vpn, get_active_vpn_with_state, get_vpn_state,
    get_vpn_uuid, kill_orphan_openvpn_processes, list_vpn_connections,
};
