use log::{debug, warn};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::notifications::NotificationManager;
use crate::tray::{SharedState, VpnTray};

/// Bridge between the supervisor and the system tray + notification system.
///
/// Owns the tray handle and notification manager. The supervisor calls
/// `update()` after state changes, and `notify()` to show desktop messages.
pub(crate) struct TrayBridge {
    /// Handle to the ksni system tray service (runs on its own thread)
    handle: Arc<std::sync::Mutex<Option<ksni::blocking::Handle<VpnTray>>>>,
    /// Manages notification throttling, categories, and display
    notifications: NotificationManager,
}

impl TrayBridge {
    pub(crate) fn new(
        handle: Arc<std::sync::Mutex<Option<ksni::blocking::Handle<VpnTray>>>>,
        notifications: NotificationManager,
    ) -> Self {
        Self {
            handle,
            notifications,
        }
    }

    /// Push the current shared state to the system tray.
    ///
    /// Spawns a short-lived thread because ksni's blocking API cannot
    /// be called from an async context.
    pub(crate) fn update(&self, shared_state: &Arc<RwLock<SharedState>>) {
        let current_state = match shared_state.try_read() {
            Ok(guard) => {
                debug!(
                    "update_tray: state={:?}, auto_reconnect={}, kill_switch={}",
                    guard.state, guard.auto_reconnect, guard.kill_switch
                );
                guard.clone()
            }
            Err(_) => {
                warn!("update_tray: Failed to read shared_state");
                return;
            }
        };

        let tray_handle = self.handle.clone();
        std::thread::spawn(move || {
            if let Ok(handle_guard) = tray_handle.lock() {
                if let Some(handle) = handle_guard.as_ref() {
                    let result = handle.update(move |tray: &mut VpnTray| {
                        if let Ok(mut cached) = tray.cached_state.write() {
                            debug!("Tray cached_state updated to: {:?}", current_state.state);
                            *cached = current_state.clone();
                        }
                    });
                    if result.is_none() {
                        warn!("Tray handle.update() returned None - service may be shutdown");
                    }
                } else {
                    warn!("Tray handle is None");
                }
            } else {
                warn!("Failed to lock tray_handle");
            }
        });
    }

    /// Show a desktop notification (delegates to NotificationManager).
    pub(crate) fn notify(&mut self, title: &str, body: &str) {
        use crate::notifications::{Notification, NotificationCategory};

        let category = match title.to_lowercase().as_str() {
            t if t.contains("connected") && !t.contains("dis") && !t.contains("re") => {
                NotificationCategory::Connected
            }
            t if t.contains("disconnected") => NotificationCategory::Disconnected,
            t if t.contains("reconnect") => NotificationCategory::Reconnected,
            t if t.contains("connection lost") => NotificationCategory::ConnectionLost,
            t if t.contains("kill switch") => {
                if body.to_lowercase().contains("enabled")
                    || body.to_lowercase().contains("blocked")
                {
                    NotificationCategory::KillSwitchEnabled
                } else {
                    NotificationCategory::KillSwitchDisabled
                }
            }
            t if t.contains("health") || t.contains("degraded") || t.contains("recovered") => {
                NotificationCategory::HealthDegraded
            }
            t if t.contains("error") || t.contains("failed") => NotificationCategory::Error,
            _ => NotificationCategory::Connected, // fallback
        };

        self.notifications
            .show(Notification::new(category, title, body));
    }
}
