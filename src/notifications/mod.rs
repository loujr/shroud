//! Desktop notification system
//!
//! Provides categorized, configurable notifications for VPN events
//! with throttling, deduplication, and per-category enable/disable.

#[allow(dead_code)]
pub mod manager;
#[allow(dead_code)]
pub mod types;

#[allow(unused_imports)]
pub use manager::NotificationManager;
#[allow(unused_imports)]
pub use types::{Notification, NotificationAction, NotificationCategory, Urgency};
