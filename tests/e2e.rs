//! End-to-end tests for Shroud
//!
//! These tests run the actual shroud binary and verify behavior.
//! Run with: `cargo test --test e2e`
//!
//! Privileged tests (marked #[ignore]) require root:
//! `sudo -E cargo test --test e2e -- --ignored`

mod common;

use common::*;
use std::time::Duration;

// Note: We removed ctor/dtor hooks as they can cause hangs in some CI environments.
// Each test now handles its own cleanup via CleanupGuard.

// ============================================================================
// Headless Mode Tests
// ============================================================================

mod headless {
    use super::*;

    #[tokio::test]
    async fn test_headless_startup_and_shutdown() {
        let _guard = CleanupGuard::new();
        init();

        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let ctx = TestContext::new();
            let mut shroud =
                ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

            let result = shroud.start_headless().await;
            assert!(result.is_ok(), "Failed to start headless: {:?}", result);

            let status = shroud.status().await;
            assert!(status.is_ok(), "Status failed: {:?}", status);

            let stop = shroud.stop().await;
            assert!(stop.is_ok(), "Stop failed: {:?}", stop);

            assert!(!shroud.is_running());
        })
        .await;

        result.expect("Test timed out");
    }

    #[tokio::test]
    async fn test_headless_status_output() {
        let _guard = CleanupGuard::new();
        init();

        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let ctx = TestContext::new();
            let mut shroud =
                ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

            shroud.start_headless().await.expect("Failed to start");

            let status = shroud.status().await.expect("Failed to get status");

            assert!(!status.is_empty(), "Status output is empty");
            // Status can be connected or disconnected depending on environment
            let status_lower = status.to_lowercase();
            assert!(
                status_lower.contains("disconnect")
                    || status_lower.contains("idle")
                    || status_lower.contains("connected")
                    || status_lower.contains("status"),
                "Unexpected status: {}",
                status
            );

            shroud.stop().await.ok();
        })
        .await;

        result.expect("Test timed out");
    }

    #[tokio::test]
    async fn test_headless_list_command() {
        let _guard = CleanupGuard::new();
        init();

        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let ctx = TestContext::new();
            let mut shroud =
                ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

            shroud.start_headless().await.expect("Failed to start");

            let list = shroud.run_command(&["list"]).await;
            assert!(list.is_ok(), "List command failed: {:?}", list);

            shroud.stop().await.ok();
        })
        .await;

        result.expect("Test timed out");
    }

    #[tokio::test]
    async fn test_headless_help_command() {
        let _guard = CleanupGuard::new();
        init();

        let ctx = TestContext::new();
        let shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());

        let help = shroud.run_command(&["--help"]).await;
        assert!(help.is_ok(), "Help command failed: {:?}", help);
        let output = help.unwrap();
        assert!(output.contains("shroud") || output.contains("Shroud"));
    }

    #[tokio::test]
    async fn test_headless_version_command() {
        let _guard = CleanupGuard::new();
        init();

        let ctx = TestContext::new();
        let shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());

        let version = shroud.run_command(&["--version"]).await;
        assert!(version.is_ok(), "Version command failed: {:?}", version);
        let output = version.unwrap();
        assert!(output.contains("shroud") || output.contains("1."));
    }

    #[tokio::test]
    async fn test_headless_invalid_command() {
        let _guard = CleanupGuard::new();
        init();

        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let ctx = TestContext::new();
            let mut shroud =
                ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

            shroud.start_headless().await.expect("Failed to start");

            let result = shroud.run_command(&["not-a-real-command"]).await;
            assert!(result.is_err(), "Invalid command should fail");

            assert!(shroud.is_running());
            assert!(shroud.status().await.is_ok());

            shroud.stop().await.ok();
        })
        .await;

        result.expect("Test timed out");
    }

    #[tokio::test]
    async fn test_headless_multiple_status_calls() {
        let _guard = CleanupGuard::new();
        init();

        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let ctx = TestContext::new();
            let mut shroud =
                ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

            shroud.start_headless().await.expect("Failed to start");

            for i in 0..10 {
                let status = shroud.status().await;
                assert!(status.is_ok(), "Status call {} failed: {:?}", i, status);
            }

            shroud.stop().await.ok();
        })
        .await;

        result.expect("Test timed out");
    }
}

// ============================================================================
// Kill Switch Tests (Privileged)
// ============================================================================

mod killswitch {
    use super::*;

    #[tokio::test]
    #[ignore = "requires root"]
    async fn test_killswitch_enable_creates_chain() {
        let _guard = CleanupGuard::new();
        require_root();
        cleanup_iptables();

        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

        shroud.start_headless().await.expect("Failed to start");

        let result = shroud.ks_enable().await;
        assert!(result.is_ok(), "Failed to enable killswitch: {:?}", result);

        assert_chain_exists("SHROUD_KILLSWITCH");

        shroud.stop().await.ok();
        cleanup_iptables();
    }

    #[tokio::test]
    #[ignore = "requires root"]
    async fn test_killswitch_disable_removes_chain() {
        let _guard = CleanupGuard::new();
        require_root();
        cleanup_iptables();

        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

        shroud.start_headless().await.expect("Failed to start");

        shroud.ks_enable().await.expect("Failed to enable");
        tokio::time::sleep(Duration::from_millis(500)).await;
        shroud.ks_disable().await.expect("Failed to disable");
        tokio::time::sleep(Duration::from_millis(500)).await;

        assert_chain_not_exists("SHROUD_KILLSWITCH");

        shroud.stop().await.ok();
        cleanup_iptables();
    }

    #[tokio::test]
    #[ignore = "requires root"]
    async fn test_killswitch_status_command() {
        let _guard = CleanupGuard::new();
        require_root();
        cleanup_iptables();

        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

        shroud.start_headless().await.expect("Failed to start");

        let status_off = shroud.run_command(&["ks", "status"]).await;
        assert!(status_off.is_ok());

        shroud.ks_enable().await.expect("Failed to enable");
        let status_on = shroud.run_command(&["ks", "status"]).await;
        assert!(status_on.is_ok());

        shroud.stop().await.ok();
        cleanup_iptables();
    }

    #[tokio::test]
    #[ignore = "requires root"]
    async fn test_killswitch_idempotent_enable() {
        let _guard = CleanupGuard::new();
        require_root();
        cleanup_iptables();

        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

        shroud.start_headless().await.expect("Failed to start");

        for _ in 0..5 {
            let result = shroud.ks_enable().await;
            assert!(result.is_ok(), "Enable should be idempotent");
        }

        assert_chain_exists("SHROUD_KILLSWITCH");

        shroud.stop().await.ok();
        cleanup_iptables();
    }

    #[tokio::test]
    #[ignore = "requires root"]
    async fn test_killswitch_idempotent_disable() {
        let _guard = CleanupGuard::new();
        require_root();
        cleanup_iptables();

        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

        shroud.start_headless().await.expect("Failed to start");

        for _ in 0..5 {
            let result = shroud.ks_disable().await;
            assert!(result.is_ok(), "Disable should be idempotent");
        }

        shroud.stop().await.ok();
        cleanup_iptables();
    }
}

// ============================================================================
// Cleanup Tests (Privileged)
// ============================================================================

mod cleanup {
    use super::*;

    #[tokio::test]
    #[ignore = "requires root"]
    async fn test_graceful_shutdown_cleans_killswitch() {
        let _guard = CleanupGuard::new();
        require_root();
        cleanup_iptables();

        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

        shroud.start_headless().await.expect("Failed to start");
        shroud
            .ks_enable()
            .await
            .expect("Failed to enable killswitch");
        assert_chain_exists("SHROUD_KILLSWITCH");

        shroud.stop().await.expect("Failed to stop");
        tokio::time::sleep(Duration::from_millis(500)).await;

        assert_chain_not_exists("SHROUD_KILLSWITCH");
    }

    #[tokio::test]
    #[ignore = "requires root"]
    async fn test_sigterm_cleans_killswitch() {
        let _guard = CleanupGuard::new();
        require_root();
        cleanup_iptables();

        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

        shroud.start_headless().await.expect("Failed to start");
        shroud
            .ks_enable()
            .await
            .expect("Failed to enable killswitch");
        assert_chain_exists("SHROUD_KILLSWITCH");

        if let Some(pid) = shroud.pid() {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }

        tokio::time::sleep(Duration::from_secs(2)).await;

        assert_chain_not_exists("SHROUD_KILLSWITCH");
    }

    #[tokio::test]
    #[ignore = "Requires D-Bus session - run locally with: cargo test --test e2e -- --ignored"]
    async fn test_socket_cleanup_on_exit() {
        let _guard = CleanupGuard::new();
        init();

        let ctx = TestContext::new();
        let socket = ctx.socket_path();
        let mut shroud = ShroudProcess::new(shroud_binary(), &socket).with_ci_timeouts();

        shroud.start_headless().await.expect("Failed to start");

        assert!(socket.exists(), "Socket should exist while running");

        shroud.stop().await.expect("Failed to stop");

        tokio::time::sleep(Duration::from_secs(2)).await;

        assert!(!socket.exists(), "Socket should be cleaned up on exit");
    }
}

// ============================================================================
// Configuration Tests
// ============================================================================

mod config {
    use super::*;

    #[tokio::test]
    async fn test_handles_missing_config() {
        let _guard = CleanupGuard::new();
        init();

        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let ctx = TestContext::new();
            let mut shroud =
                ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

            let result = shroud.start_headless().await;
            assert!(result.is_ok(), "Should handle missing config: {:?}", result);

            shroud.stop().await.ok();
        })
        .await;

        result.expect("Test timed out");
    }

    #[tokio::test]
    async fn test_handles_corrupted_config() {
        let _guard = CleanupGuard::new();
        init();

        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let ctx = TestContext::new();

            let config_path = ctx.config_dir().join("config.toml");
            std::fs::write(&config_path, "{{{{NOT VALID TOML}}}}").unwrap();

            let mut shroud =
                ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

            let result = shroud.start_headless().await;
            assert!(result.is_ok(), "Should handle corrupted config");

            shroud.stop().await.ok();
        })
        .await;

        result.expect("Test timed out");
    }

    #[tokio::test]
    async fn test_handles_empty_config() {
        let _guard = CleanupGuard::new();
        init();

        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let ctx = TestContext::new();

            let config_path = ctx.config_dir().join("config.toml");
            std::fs::write(&config_path, "").unwrap();

            let mut shroud =
                ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

            let result = shroud.start_headless().await;
            assert!(result.is_ok(), "Should handle empty config");

            shroud.stop().await.ok();
        })
        .await;

        result.expect("Test timed out");
    }
}

// ============================================================================
// IPC Tests
// ============================================================================

mod ipc {
    use super::*;
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    #[tokio::test]
    async fn test_ipc_malformed_request() {
        let _guard = CleanupGuard::new();
        init();

        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let ctx = TestContext::new();
            let mut shroud =
                ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

            shroud.start_headless().await.expect("Failed to start");

            if let Ok(mut stream) = UnixStream::connect(ctx.socket_path()) {
                let _ = stream.write_all(b"NOT JSON AT ALL\n");
            }

            tokio::time::sleep(Duration::from_millis(200)).await;

            assert!(shroud.is_running(), "Daemon died from malformed IPC");
            assert!(shroud.status().await.is_ok());

            shroud.stop().await.ok();
        })
        .await;

        result.expect("Test timed out");
    }

    #[tokio::test]
    async fn test_ipc_empty_request() {
        let _guard = CleanupGuard::new();
        init();

        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let ctx = TestContext::new();
            let mut shroud =
                ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

            shroud.start_headless().await.expect("Failed to start");

            if let Ok(mut stream) = UnixStream::connect(ctx.socket_path()) {
                let _ = stream.write_all(b"");
            }

            tokio::time::sleep(Duration::from_millis(200)).await;

            assert!(shroud.is_running());

            shroud.stop().await.ok();
        })
        .await;

        result.expect("Test timed out");
    }

    #[tokio::test]
    async fn test_ipc_binary_garbage() {
        let _guard = CleanupGuard::new();
        init();

        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let ctx = TestContext::new();
            let mut shroud =
                ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

            shroud.start_headless().await.expect("Failed to start");

            if let Ok(mut stream) = UnixStream::connect(ctx.socket_path()) {
                let _ = stream.write_all(&[0x00, 0xFF, 0xFE, 0x00, 0x01, 0x02]);
            }

            tokio::time::sleep(Duration::from_millis(200)).await;

            assert!(shroud.is_running());
            assert!(shroud.status().await.is_ok());

            shroud.stop().await.ok();
        })
        .await;

        result.expect("Test timed out");
    }

    #[tokio::test]
    async fn test_stale_socket_recovery() {
        let _guard = CleanupGuard::new();
        init();

        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let ctx = TestContext::new();
            let socket = ctx.socket_path();

            std::fs::write(&socket, "stale").unwrap();

            let mut shroud = ShroudProcess::new(shroud_binary(), &socket).with_ci_timeouts();

            let result = shroud.start_headless().await;
            assert!(result.is_ok(), "Should handle stale socket: {:?}", result);
            assert!(shroud.status().await.is_ok());

            shroud.stop().await.ok();
        })
        .await;

        result.expect("Test timed out");
    }
}

// ============================================================================
// Concurrency Tests
// ============================================================================

mod concurrency {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_status_requests() {
        let _guard = CleanupGuard::new();
        init();

        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let ctx = TestContext::new();
            let mut shroud =
                ShroudProcess::new(shroud_binary(), ctx.socket_path()).with_ci_timeouts();

            shroud.start_headless().await.expect("Failed to start");

            let binary = shroud_binary();
            let socket = ctx.socket_path();

            let handles: Vec<_> = (0..20)
                .map(|_| {
                    let b = binary.clone();
                    let s = socket.clone();
                    tokio::spawn(async move {
                        let proc = ShroudProcess::new(b, s);
                        proc.run_command(&["status"]).await
                    })
                })
                .collect();

            let mut successes = 0;
            for handle in handles {
                if let Ok(result) = tokio::time::timeout(Duration::from_secs(5), handle).await {
                    if result.is_ok() && result.unwrap().is_ok() {
                        successes += 1;
                    }
                }
            }

            assert!(
                successes >= 15,
                "Only {}/20 concurrent requests succeeded",
                successes
            );

            assert!(shroud.status().await.is_ok());

            shroud.stop().await.ok();
        })
        .await;

        result.expect("Test timed out");
    }

    #[tokio::test]
    #[ignore = "Flaky test - depends on socket path isolation"]
    async fn test_prevents_multiple_instances() {
        let _guard = CleanupGuard::new();
        init();

        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let ctx = TestContext::new();
            let socket = ctx.socket_path();

            let mut shroud1 = ShroudProcess::new(shroud_binary(), &socket).with_ci_timeouts();
            shroud1
                .start_headless()
                .await
                .expect("First instance failed");

            let mut shroud2 = ShroudProcess::new(shroud_binary(), &socket).with_ci_timeouts();
            let result =
                tokio::time::timeout(Duration::from_secs(3), shroud2.start_headless()).await;

            // Second should fail to start or timeout
            assert!(
                result.is_err() || result.unwrap().is_err(),
                "Second instance should fail"
            );

            assert!(shroud1.status().await.is_ok(), "First instance died");

            shroud1.stop().await.ok();
        })
        .await;

        result.expect("Test timed out");
    }
}
