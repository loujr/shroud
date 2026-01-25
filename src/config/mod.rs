//! Configuration module
//!
//! Provides persistent configuration storage for user preferences.

pub mod settings;

pub use settings::{Config, ConfigManager, DnsMode, Ipv6Mode};
