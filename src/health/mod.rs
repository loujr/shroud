//! Health check module
//!
//! Provides connectivity verification for VPN tunnels to detect degraded states.

pub mod checker;

#[cfg(test)]
mod tests;

pub use checker::{HealthChecker, HealthResult};
