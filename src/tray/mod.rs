//! System tray module
//!
//! Provides the system tray UI for the VPN manager.

#[allow(dead_code)]
pub mod drawing;
pub mod icons;
pub mod service;
#[allow(dead_code)]
pub mod state;

#[cfg(test)]
mod tests;

pub use service::{SharedState, VpnCommand, VpnTray};
