//! Handler response building — pure functions, easily testable.
//!
//! Extracts status/list/kill-switch response construction and
//! VPN state-change classification out of the async handler methods.

use crate::dbus::NmEvent;
use crate::ipc::protocol::{IpcResponse, VpnConnectionInfo};
use crate::state::VpnState;

// ---------------------------------------------------------------------------
// Status / list helpers
// ---------------------------------------------------------------------------

/// Build an `IpcResponse::Status` from current state.
pub fn build_status_response(state: &VpnState, kill_switch_enabled: bool) -> IpcResponse {
    let (connected, vpn_name, state_str) = match state {
        VpnState::Disconnected => (false, None, "Disconnected".to_string()),
        VpnState::Connecting { server } => (
            false,
            Some(server.clone()),
            format!("Connecting to {}", server),
        ),
        VpnState::Connected { server } => (
            true,
            Some(server.clone()),
            format!("Connected to {}", server),
        ),
        VpnState::Reconnecting {
            server,
            attempt,
            max_attempts,
        } => (
            false,
            Some(server.clone()),
            format!("Reconnecting to {} ({}/{})", server, attempt, max_attempts),
        ),
        VpnState::Degraded { server } => {
            (true, Some(server.clone()), format!("Degraded: {}", server))
        }
        VpnState::Failed { server, reason } => (
            false,
            Some(server.clone()),
            format!("Failed: {} — {}", server, reason),
        ),
    };

    IpcResponse::Status {
        connected,
        vpn_name,
        vpn_type: None,
        state: state_str,
        kill_switch_enabled,
    }
}

/// Build an `IpcResponse::Connections` from a list of VPN names.
pub fn build_list_response(vpns: &[String], active: Option<&str>) -> IpcResponse {
    let connections = vpns
        .iter()
        .map(|name| {
            let status = if Some(name.as_str()) == active {
                "active"
            } else {
                "available"
            };
            VpnConnectionInfo {
                name: name.clone(),
                vpn_type: String::new(),
                status: status.to_string(),
            }
        })
        .collect();

    IpcResponse::Connections { connections }
}

/// Determine whether a disconnect is required before connecting to a new VPN.
pub fn needs_disconnect_first(current_state: &VpnState, target: &str) -> bool {
    match current_state {
        VpnState::Connected { server } if server != target => true,
        VpnState::Connecting { server } if server != target => true,
        _ => false,
    }
}

/// Validate a connect request against current state.
pub fn validate_connect(
    vpn_name: &str,
    available: &[String],
    current_state: &VpnState,
) -> Result<(), String> {
    if vpn_name.is_empty() {
        return Err("VPN name cannot be empty".into());
    }
    if !available.iter().any(|v| v == vpn_name) {
        return Err(format!("VPN '{}' not found", vpn_name));
    }
    if let VpnState::Connected { server } = current_state {
        if server == vpn_name {
            return Err(format!("Already connected to '{}'", vpn_name));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// NM event classification
// ---------------------------------------------------------------------------

/// What the supervisor should do in response to an NM event.
#[derive(Debug, Clone, PartialEq)]
pub enum StateAction {
    /// No action needed.
    None,
    /// Mark VPN as connected.
    MarkConnected(String),
    /// Mark VPN as disconnected.
    MarkDisconnected,
    /// Connection failed — possibly trigger reconnect.
    TriggerReconnect(String),
    /// Connection failed — mark as failed (no reconnect).
    MarkFailed { server: String, reason: String },
}

/// Classify an `NmEvent` into a `StateAction`.
pub fn classify_nm_event(
    event: &NmEvent,
    current_state: &VpnState,
    auto_reconnect: bool,
) -> StateAction {
    match event {
        NmEvent::VpnActivated { name } => StateAction::MarkConnected(name.clone()),
        NmEvent::VpnActivating { .. } => StateAction::None,
        NmEvent::ConnectivityChanged { .. } => StateAction::None,
        NmEvent::VpnDeactivated { name } => {
            if auto_reconnect && matches!(current_state, VpnState::Connected { .. }) {
                StateAction::TriggerReconnect(name.clone())
            } else {
                StateAction::MarkDisconnected
            }
        }
        NmEvent::VpnFailed { name, reason } => {
            if auto_reconnect {
                StateAction::TriggerReconnect(name.clone())
            } else {
                StateAction::MarkFailed {
                    server: name.clone(),
                    reason: reason.clone(),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- build_status_response ---

    mod status_response {
        use super::*;

        #[test]
        fn test_disconnected() {
            let resp = build_status_response(&VpnState::Disconnected, false);
            match resp {
                IpcResponse::Status {
                    connected,
                    vpn_name,
                    state,
                    kill_switch_enabled,
                    ..
                } => {
                    assert!(!connected);
                    assert!(vpn_name.is_none());
                    assert!(state.contains("Disconnected"));
                    assert!(!kill_switch_enabled);
                }
                _ => panic!("Expected Status response"),
            }
        }

        #[test]
        fn test_connected() {
            let s = VpnState::Connected {
                server: "my-vpn".into(),
            };
            let resp = build_status_response(&s, true);
            match resp {
                IpcResponse::Status {
                    connected,
                    vpn_name,
                    state,
                    kill_switch_enabled,
                    ..
                } => {
                    assert!(connected);
                    assert_eq!(vpn_name, Some("my-vpn".into()));
                    assert!(state.contains("Connected"));
                    assert!(kill_switch_enabled);
                }
                _ => panic!("Expected Status"),
            }
        }

        #[test]
        fn test_connecting() {
            let s = VpnState::Connecting {
                server: "vpn1".into(),
            };
            let resp = build_status_response(&s, false);
            match resp {
                IpcResponse::Status {
                    connected,
                    vpn_name,
                    state,
                    ..
                } => {
                    assert!(!connected);
                    assert_eq!(vpn_name, Some("vpn1".into()));
                    assert!(state.contains("Connecting"));
                }
                _ => panic!("Expected Status"),
            }
        }

        #[test]
        fn test_reconnecting() {
            let s = VpnState::Reconnecting {
                server: "vpn1".into(),
                attempt: 3,
                max_attempts: 10,
            };
            let resp = build_status_response(&s, false);
            match resp {
                IpcResponse::Status { state, .. } => {
                    assert!(state.contains("Reconnecting"));
                    assert!(state.contains("3/10"));
                }
                _ => panic!("Expected Status"),
            }
        }

        #[test]
        fn test_degraded() {
            let s = VpnState::Degraded {
                server: "vpn1".into(),
            };
            let resp = build_status_response(&s, false);
            match resp {
                IpcResponse::Status {
                    connected, state, ..
                } => {
                    assert!(connected);
                    assert!(state.contains("Degraded"));
                }
                _ => panic!("Expected Status"),
            }
        }

        #[test]
        fn test_failed() {
            let s = VpnState::Failed {
                server: "vpn1".into(),
                reason: "timeout".into(),
            };
            let resp = build_status_response(&s, false);
            match resp {
                IpcResponse::Status {
                    connected, state, ..
                } => {
                    assert!(!connected);
                    assert!(state.contains("Failed"));
                    assert!(state.contains("timeout"));
                }
                _ => panic!("Expected Status"),
            }
        }
    }

    // --- build_list_response ---

    mod list_response {
        use super::*;

        #[test]
        fn test_empty() {
            let resp = build_list_response(&[], None);
            match resp {
                IpcResponse::Connections { connections } => {
                    assert!(connections.is_empty());
                }
                _ => panic!("Expected Connections"),
            }
        }

        #[test]
        fn test_with_vpns() {
            let vpns = vec!["vpn1".into(), "vpn2".into()];
            let resp = build_list_response(&vpns, None);
            match resp {
                IpcResponse::Connections { connections } => {
                    assert_eq!(connections.len(), 2);
                    assert!(connections.iter().all(|c| c.status == "available"));
                }
                _ => panic!("Expected Connections"),
            }
        }

        #[test]
        fn test_with_active() {
            let vpns = vec!["vpn1".into(), "vpn2".into()];
            let resp = build_list_response(&vpns, Some("vpn1"));
            match resp {
                IpcResponse::Connections { connections } => {
                    let vpn1 = connections.iter().find(|c| c.name == "vpn1").unwrap();
                    assert_eq!(vpn1.status, "active");
                    let vpn2 = connections.iter().find(|c| c.name == "vpn2").unwrap();
                    assert_eq!(vpn2.status, "available");
                }
                _ => panic!("Expected Connections"),
            }
        }
    }

    // --- needs_disconnect_first ---

    mod disconnect_logic {
        use super::*;

        #[test]
        fn test_disconnected() {
            assert!(!needs_disconnect_first(&VpnState::Disconnected, "vpn1"));
        }

        #[test]
        fn test_connected_same() {
            let s = VpnState::Connected {
                server: "vpn1".into(),
            };
            assert!(!needs_disconnect_first(&s, "vpn1"));
        }

        #[test]
        fn test_connected_different() {
            let s = VpnState::Connected {
                server: "vpn1".into(),
            };
            assert!(needs_disconnect_first(&s, "vpn2"));
        }

        #[test]
        fn test_connecting_different() {
            let s = VpnState::Connecting {
                server: "vpn1".into(),
            };
            assert!(needs_disconnect_first(&s, "vpn2"));
        }

        #[test]
        fn test_failed_no_disconnect() {
            let s = VpnState::Failed {
                server: "vpn1".into(),
                reason: "err".into(),
            };
            assert!(!needs_disconnect_first(&s, "vpn2"));
        }
    }

    // --- validate_connect ---

    mod validate {
        use super::*;

        #[test]
        fn test_valid() {
            assert!(validate_connect("vpn1", &["vpn1".into()], &VpnState::Disconnected).is_ok());
        }

        #[test]
        fn test_empty_name() {
            assert!(validate_connect("", &["vpn1".into()], &VpnState::Disconnected).is_err());
        }

        #[test]
        fn test_not_found() {
            let err = validate_connect("x", &["vpn1".into()], &VpnState::Disconnected);
            assert!(err.unwrap_err().contains("not found"));
        }

        #[test]
        fn test_already_connected() {
            let s = VpnState::Connected {
                server: "vpn1".into(),
            };
            let err = validate_connect("vpn1", &["vpn1".into()], &s);
            assert!(err.unwrap_err().contains("Already"));
        }

        #[test]
        fn test_switch_allowed() {
            let s = VpnState::Connected {
                server: "vpn1".into(),
            };
            assert!(validate_connect("vpn2", &["vpn1".into(), "vpn2".into()], &s).is_ok());
        }
    }

    // --- classify_nm_event ---

    mod classify {
        use super::*;

        #[test]
        fn test_activated() {
            let e = NmEvent::VpnActivated { name: "vpn".into() };
            let a = classify_nm_event(&e, &VpnState::Disconnected, true);
            assert_eq!(a, StateAction::MarkConnected("vpn".into()));
        }

        #[test]
        fn test_activating_is_noop() {
            let e = NmEvent::VpnActivating { name: "vpn".into() };
            let a = classify_nm_event(&e, &VpnState::Disconnected, true);
            assert_eq!(a, StateAction::None);
        }

        #[test]
        fn test_deactivated_no_reconnect() {
            let e = NmEvent::VpnDeactivated { name: "vpn".into() };
            let s = VpnState::Connected {
                server: "vpn".into(),
            };
            let a = classify_nm_event(&e, &s, false);
            assert_eq!(a, StateAction::MarkDisconnected);
        }

        #[test]
        fn test_deactivated_with_reconnect() {
            let e = NmEvent::VpnDeactivated { name: "vpn".into() };
            let s = VpnState::Connected {
                server: "vpn".into(),
            };
            let a = classify_nm_event(&e, &s, true);
            assert_eq!(a, StateAction::TriggerReconnect("vpn".into()));
        }

        #[test]
        fn test_deactivated_from_disconnected_no_reconnect() {
            let e = NmEvent::VpnDeactivated { name: "vpn".into() };
            let a = classify_nm_event(&e, &VpnState::Disconnected, true);
            // Not currently connected, so just mark disconnected
            assert_eq!(a, StateAction::MarkDisconnected);
        }

        #[test]
        fn test_failed_with_reconnect() {
            let e = NmEvent::VpnFailed {
                name: "vpn".into(),
                reason: "timeout".into(),
            };
            let a = classify_nm_event(&e, &VpnState::Disconnected, true);
            assert_eq!(a, StateAction::TriggerReconnect("vpn".into()));
        }

        #[test]
        fn test_failed_no_reconnect() {
            let e = NmEvent::VpnFailed {
                name: "vpn".into(),
                reason: "auth".into(),
            };
            let a = classify_nm_event(&e, &VpnState::Disconnected, false);
            assert_eq!(
                a,
                StateAction::MarkFailed {
                    server: "vpn".into(),
                    reason: "auth".into()
                }
            );
        }

        #[test]
        fn test_connectivity_changed_is_noop() {
            let e = NmEvent::ConnectivityChanged { connected: true };
            let a = classify_nm_event(&e, &VpnState::Disconnected, true);
            assert_eq!(a, StateAction::None);
        }
    }
}
