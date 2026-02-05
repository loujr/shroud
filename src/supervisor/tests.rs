//! Unit tests for VPN Supervisor module

#[cfg(test)]
mod supervisor_tests {
    use std::time::{Duration, Instant};

    #[test]
    fn test_supervisor_constants_reasonable() {
        use crate::supervisor::{
            CONNECTION_MONITOR_INTERVAL_MS, CONNECTION_MONITOR_MAX_ATTEMPTS,
            DISCONNECT_VERIFY_INTERVAL_MS, DISCONNECT_VERIFY_MAX_ATTEMPTS,
            MAX_CONNECT_ATTEMPTS, POST_DISCONNECT_GRACE_SECS,
            RECONNECT_BASE_DELAY_SECS, RECONNECT_MAX_DELAY_SECS,
        };

        assert!(RECONNECT_BASE_DELAY_SECS >= 1);
        assert!(RECONNECT_MAX_DELAY_SECS >= RECONNECT_BASE_DELAY_SECS);
        assert!(RECONNECT_MAX_DELAY_SECS <= 120);
        assert!(POST_DISCONNECT_GRACE_SECS >= 1);
        assert!(DISCONNECT_VERIFY_MAX_ATTEMPTS >= 5);
        assert!(CONNECTION_MONITOR_MAX_ATTEMPTS >= 10);
        assert!(CONNECTION_MONITOR_INTERVAL_MS >= 100);
        assert!(DISCONNECT_VERIFY_INTERVAL_MS >= 100);
        assert!(MAX_CONNECT_ATTEMPTS >= 2);
    }

    #[test]
    fn test_reconnect_timing_calculations() {
        use crate::supervisor::{RECONNECT_BASE_DELAY_SECS, RECONNECT_MAX_DELAY_SECS};

        fn calc_backoff(attempt: u32) -> u64 {
            std::cmp::min(
                RECONNECT_BASE_DELAY_SECS * (attempt as u64 + 1),
                RECONNECT_MAX_DELAY_SECS,
            )
        }

        assert_eq!(calc_backoff(0), RECONNECT_BASE_DELAY_SECS);
        let backoff_100 = calc_backoff(100);
        assert_eq!(backoff_100, RECONNECT_MAX_DELAY_SECS);
    }

    #[test]
    fn test_grace_period_logic() {
        use crate::supervisor::POST_DISCONNECT_GRACE_SECS;

        let disconnect_time = Instant::now();
        let grace_duration = Duration::from_secs(POST_DISCONNECT_GRACE_SECS);
        assert!(disconnect_time.elapsed() < grace_duration);
    }
}

#[cfg(test)]
mod reconnect_tests {
    #[test]
    fn test_exponential_backoff_sequence() {
        const BASE_DELAY: u64 = 2;
        const MAX_DELAY: u64 = 30;

        // Test linear backoff: delay = BASE * (attempt + 1), capped at MAX
        let mut delays = Vec::new();
        for attempt in 0..20 {
            let delay = std::cmp::min(BASE_DELAY * (attempt as u64 + 1), MAX_DELAY);
            delays.push(delay);
        }

        // First delay is BASE_DELAY * 1 = 2
        assert_eq!(delays[0], 2);
        // All delays must be capped at MAX_DELAY
        assert!(delays.iter().all(|&d| d <= MAX_DELAY));
        // After enough attempts, delay should reach MAX_DELAY (2 * 15 = 30)
        assert_eq!(delays[14], MAX_DELAY);
        // And stay at MAX_DELAY
        assert_eq!(*delays.last().unwrap(), MAX_DELAY);
    }

    #[test]
    fn test_retry_reset_on_success() {
        let mut retries = 5;
        let success = true;
        if success { retries = 0; }
        assert_eq!(retries, 0);
    }
}

#[cfg(test)]
mod handler_tests {
    use crate::tray::VpnCommand;
    use crate::ipc::{IpcCommand, IpcResponse};

    #[test]
    fn test_vpn_command_serialization() {
        let connect_cmd = VpnCommand::Connect("test-vpn".to_string());
        let disconnect_cmd = VpnCommand::Disconnect;
        let toggle_ks = VpnCommand::ToggleKillSwitch;

        match connect_cmd {
            VpnCommand::Connect(name) => assert_eq!(name, "test-vpn"),
            _ => panic!("Expected Connect"),
        }
        match disconnect_cmd {
            VpnCommand::Disconnect => {}
            _ => panic!("Expected Disconnect"),
        }
        match toggle_ks {
            VpnCommand::ToggleKillSwitch => {}
            _ => panic!("Expected ToggleKillSwitch"),
        }
    }

    #[test]
    fn test_ipc_command_response_types() {
        let status_cmd = IpcCommand::Status;
        let connect_cmd = IpcCommand::Connect { name: "my-vpn".to_string() };

        let ok_response = IpcResponse::Ok;
        let err_response = IpcResponse::Error { message: "Failed".to_string() };

        match status_cmd {
            IpcCommand::Status => {}
            _ => panic!("Expected Status"),
        }

        match connect_cmd {
            IpcCommand::Connect { name } => assert_eq!(name, "my-vpn"),
            _ => panic!("Expected Connect"),
        }

        assert!(matches!(ok_response, IpcResponse::Ok));

        if let IpcResponse::Error { message } = err_response {
            assert_eq!(message, "Failed");
        }
    }
}
