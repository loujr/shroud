//! Chaos engineering tests for Shroud
//!
//! These tests verify resilience against various failure modes.
//! Run with: `cargo test --test chaos`
//!
//! Privileged tests (marked #[ignore]) require root:
//! `sudo -E cargo test --test chaos -- --ignored`

mod common;

use common::*;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::time::Duration;

// ============================================================================
// IPC Chaos Tests
// ============================================================================

mod ipc_chaos {
    use super::*;

    #[tokio::test]
    async fn test_ipc_flood() {
        init();
        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());

        shroud.start_headless().await.expect("Failed to start");

        // Flood with concurrent requests
        let binary = shroud_binary();
        let socket = ctx.socket_path();

        let handles: Vec<_> = (0..100)
            .map(|_| {
                let b = binary.clone();
                let s = socket.clone();
                tokio::spawn(async move {
                    let proc = ShroudProcess::new(b, s);
                    proc.run_command(&["status"]).await
                })
            })
            .collect();

        let mut failures = 0;
        for handle in handles {
            if handle.await.unwrap().is_err() {
                failures += 1;
            }
        }

        // Should handle most requests
        assert!(failures < 30, "Too many failures: {}/100", failures);

        // Daemon should still be responsive
        assert!(shroud.is_running());
        assert!(shroud.status().await.is_ok());

        shroud.stop().await.ok();
    }

    #[tokio::test]
    async fn test_ipc_rapid_connect_disconnect() {
        init();
        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());

        shroud.start_headless().await.expect("Failed to start");

        // Rapidly connect and disconnect to socket
        let socket_path = ctx.socket_path();
        for _ in 0..50 {
            if let Ok(stream) = UnixStream::connect(&socket_path) {
                drop(stream);
            }
        }

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Daemon should survive
        assert!(shroud.is_running());
        assert!(shroud.status().await.is_ok());

        shroud.stop().await.ok();
    }

    #[tokio::test]
    async fn test_ipc_oversized_message() {
        init();
        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());

        shroud.start_headless().await.expect("Failed to start");

        // Send oversized message (10MB)
        if let Ok(mut stream) = UnixStream::connect(ctx.socket_path()) {
            let large_data = vec![b'A'; 10 * 1024 * 1024];
            let _ = stream.write_all(&large_data);
        }

        tokio::time::sleep(Duration::from_millis(500)).await;

        // Daemon should survive
        assert!(shroud.is_running(), "Daemon died from oversized message");
        assert!(shroud.status().await.is_ok());

        shroud.stop().await.ok();
    }

    #[tokio::test]
    async fn test_ipc_slow_client() {
        init();
        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());

        shroud.start_headless().await.expect("Failed to start");

        // Connect but write very slowly
        if let Ok(mut stream) = UnixStream::connect(ctx.socket_path()) {
            for byte in b"{\"command\":" {
                let _ = stream.write_all(&[*byte]);
                std::thread::sleep(Duration::from_millis(100));
            }
            // Don't complete the message
        }

        tokio::time::sleep(Duration::from_secs(2)).await;

        // Daemon should survive slow clients
        assert!(shroud.is_running());
        assert!(shroud.status().await.is_ok());

        shroud.stop().await.ok();
    }

    #[tokio::test]
    async fn test_ipc_mixed_garbage_and_valid() {
        init();
        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());

        shroud.start_headless().await.expect("Failed to start");

        // Send garbage followed by valid request
        for _ in 0..10 {
            if let Ok(mut stream) = UnixStream::connect(ctx.socket_path()) {
                let _ = stream.write_all(b"GARBAGE");
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Valid request should still work
        assert!(shroud.status().await.is_ok());

        shroud.stop().await.ok();
    }
}

// ============================================================================
// Signal Chaos Tests
// ============================================================================

mod signal_chaos {
    use super::*;

    #[tokio::test]
    async fn test_signal_storm() {
        init();
        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());

        shroud.start_headless().await.expect("Failed to start");

        if let Some(pid) = shroud.pid() {
            // Send rapid signals
            for _ in 0..50 {
                unsafe {
                    libc::kill(pid as i32, libc::SIGUSR1);
                    libc::kill(pid as i32, libc::SIGHUP);
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;

        // Should survive signal storm
        assert!(shroud.is_running(), "Daemon died from signal storm");
        assert!(shroud.status().await.is_ok());

        shroud.stop().await.ok();
    }

    #[tokio::test]
    async fn test_sigstop_sigcont() {
        init();
        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());

        shroud.start_headless().await.expect("Failed to start");

        if let Some(pid) = shroud.pid() {
            // Pause process
            unsafe {
                libc::kill(pid as i32, libc::SIGSTOP);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            // Resume process
            unsafe {
                libc::kill(pid as i32, libc::SIGCONT);
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        // Should resume correctly
        assert!(shroud.is_running());
        assert!(
            shroud.status().await.is_ok(),
            "Failed after SIGSTOP/SIGCONT"
        );

        shroud.stop().await.ok();
    }

    #[tokio::test]
    async fn test_multiple_sighup() {
        init();
        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());

        shroud.start_headless().await.expect("Failed to start");

        if let Some(pid) = shroud.pid() {
            // Send multiple SIGHUPs (config reload)
            for _ in 0..10 {
                unsafe {
                    libc::kill(pid as i32, libc::SIGHUP);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;

        // Should handle reload signals
        assert!(shroud.is_running());
        assert!(shroud.status().await.is_ok());

        shroud.stop().await.ok();
    }
}

// ============================================================================
// Config Chaos Tests
// ============================================================================

mod config_chaos {
    use super::*;

    #[tokio::test]
    async fn test_config_changes_while_running() {
        init();
        let ctx = TestContext::new();
        let config_path = ctx.config_dir().join("config.toml");

        // Start with valid config
        std::fs::write(&config_path, "[killswitch]\nenabled = false\n").unwrap();

        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());
        shroud.start_headless().await.expect("Failed to start");

        // Modify config while running
        std::fs::write(&config_path, "[killswitch]\nenabled = true\n").unwrap();

        // Trigger reload
        if let Some(pid) = shroud.pid() {
            unsafe {
                libc::kill(pid as i32, libc::SIGHUP);
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;

        // Should still be running
        assert!(shroud.is_running());
        assert!(shroud.status().await.is_ok());

        shroud.stop().await.ok();
    }

    #[tokio::test]
    async fn test_config_deleted_while_running() {
        init();
        let ctx = TestContext::new();
        let config_path = ctx.config_dir().join("config.toml");

        // Start with valid config
        std::fs::write(&config_path, "[killswitch]\nenabled = false\n").unwrap();

        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());
        shroud.start_headless().await.expect("Failed to start");

        // Delete config while running
        std::fs::remove_file(&config_path).ok();

        // Trigger reload
        if let Some(pid) = shroud.pid() {
            unsafe {
                libc::kill(pid as i32, libc::SIGHUP);
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;

        // Should still be running (use defaults)
        assert!(shroud.is_running());
        assert!(shroud.status().await.is_ok());

        shroud.stop().await.ok();
    }

    #[tokio::test]
    async fn test_config_becomes_corrupted() {
        init();
        let ctx = TestContext::new();
        let config_path = ctx.config_dir().join("config.toml");

        // Start with valid config
        std::fs::write(&config_path, "[killswitch]\nenabled = false\n").unwrap();

        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());
        shroud.start_headless().await.expect("Failed to start");

        // Corrupt config while running
        std::fs::write(&config_path, "{{{{NOT TOML}}}}").unwrap();

        // Trigger reload
        if let Some(pid) = shroud.pid() {
            unsafe {
                libc::kill(pid as i32, libc::SIGHUP);
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;

        // Should still be running (keep old config)
        assert!(shroud.is_running());
        assert!(shroud.status().await.is_ok());

        shroud.stop().await.ok();
    }
}

// ============================================================================
// Crash Recovery Tests (Privileged)
// ============================================================================

mod crash_recovery {
    use super::*;

    #[tokio::test]
    #[ignore = "requires root"]
    async fn test_sigkill_recovery() {
        require_root();
        cleanup_iptables();

        let ctx = TestContext::new();
        let socket = ctx.socket_path();
        let mut shroud = ShroudProcess::new(shroud_binary(), &socket);

        shroud.start_headless().await.expect("Failed to start");
        shroud.ks_enable().await.expect("Failed to enable killswitch");

        // SIGKILL - unrecoverable
        shroud.kill();
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Start again - should clean up stale state
        let mut shroud2 = ShroudProcess::new(shroud_binary(), &socket);
        let result = shroud2.start_headless().await;

        assert!(result.is_ok(), "Failed to recover after SIGKILL: {:?}", result);
        assert!(shroud2.status().await.is_ok());

        shroud2.stop().await.ok();
        cleanup_iptables();
    }

    #[tokio::test]
    async fn test_stale_lock_recovery() {
        init();
        let ctx = TestContext::new();

        // Create stale lock file
        let lock_path = ctx.temp_dir.path().join("shroud.lock");
        std::fs::write(&lock_path, "99999").unwrap(); // Non-existent PID

        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());

        // Should handle stale lock
        let result = shroud.start_headless().await;
        assert!(result.is_ok(), "Should handle stale lock: {:?}", result);

        shroud.stop().await.ok();
    }
}

// ============================================================================
// Kill Switch Chaos Tests (Privileged)
// ============================================================================

mod killswitch_chaos {
    use super::*;

    #[tokio::test]
    #[ignore = "requires root"]
    async fn test_rapid_killswitch_toggle() {
        require_root();
        cleanup_iptables();

        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());

        shroud.start_headless().await.expect("Failed to start");

        // Rapid toggle
        for i in 0..20 {
            if i % 2 == 0 {
                let _ = shroud.ks_enable().await;
            } else {
                let _ = shroud.ks_disable().await;
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;

        // Should be in consistent state
        assert!(shroud.is_running());
        let status = shroud.run_command(&["ks", "status"]).await;
        assert!(status.is_ok(), "Status failed after rapid toggle");

        shroud.stop().await.ok();
        cleanup_iptables();
    }

    #[tokio::test]
    #[ignore = "requires root"]
    async fn test_concurrent_killswitch_commands() {
        require_root();
        cleanup_iptables();

        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());

        shroud.start_headless().await.expect("Failed to start");

        let binary = shroud_binary();
        let socket = ctx.socket_path();

        // Concurrent enable/disable
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let b = binary.clone();
                let s = socket.clone();
                tokio::spawn(async move {
                    let proc = ShroudProcess::new(b, s);
                    if i % 2 == 0 {
                        proc.run_command(&["ks", "on"]).await
                    } else {
                        proc.run_command(&["ks", "off"]).await
                    }
                })
            })
            .collect();

        for handle in handles {
            let _ = handle.await;
        }

        tokio::time::sleep(Duration::from_secs(1)).await;

        // Should be in consistent state
        assert!(shroud.is_running());

        shroud.stop().await.ok();
        cleanup_iptables();
    }

    #[tokio::test]
    #[ignore = "requires root"]
    async fn test_killswitch_survives_iptables_flush() {
        require_root();
        cleanup_iptables();

        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());

        shroud.start_headless().await.expect("Failed to start");
        shroud.ks_enable().await.expect("Failed to enable killswitch");

        // External process flushes our chain
        let _ = std::process::Command::new("sudo")
            .args(["iptables", "-F", "SHROUD_KILLSWITCH"])
            .output();

        tokio::time::sleep(Duration::from_secs(1)).await;

        // Daemon should still be running
        assert!(shroud.is_running());

        // Re-enable should work
        let result = shroud.ks_enable().await;
        assert!(result.is_ok(), "Re-enable failed: {:?}", result);

        shroud.stop().await.ok();
        cleanup_iptables();
    }
}

// ============================================================================
// Resource Exhaustion Tests
// ============================================================================

mod resource_exhaustion {
    use super::*;

    #[tokio::test]
    async fn test_many_open_connections() {
        init();
        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());

        shroud.start_headless().await.expect("Failed to start");

        // Open many connections without closing them
        let mut streams = Vec::new();
        for _ in 0..50 {
            if let Ok(stream) = UnixStream::connect(ctx.socket_path()) {
                streams.push(stream);
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;

        // Daemon should still work
        assert!(shroud.is_running());
        assert!(shroud.status().await.is_ok());

        // Clean up
        drop(streams);
        shroud.stop().await.ok();
    }

    #[tokio::test]
    async fn test_long_running_stability() {
        init();
        let ctx = TestContext::new();
        let mut shroud = ShroudProcess::new(shroud_binary(), ctx.socket_path());

        shroud.start_headless().await.expect("Failed to start");

        // Run for a while with periodic activity
        for i in 0..10 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let status = shroud.status().await;
            assert!(status.is_ok(), "Status failed at iteration {}", i);
        }

        // Should still be healthy
        assert!(shroud.is_running());

        shroud.stop().await.ok();
    }
}
