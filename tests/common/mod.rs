//! Common test utilities for all Shroud tests
//!
//! This module provides:
//! - Test context management
//! - Mock implementations (NetworkManager, D-Bus, command executor)
//! - Assertions for system state
//! - Test fixtures and data generators

#![allow(dead_code)]
#![allow(unused_imports)]

// Core test utilities
pub mod assertions;
pub mod context;
pub mod results;
pub mod system;

// Mock implementations for integration testing
pub mod fixtures;
pub mod mock_dbus;
pub mod mock_executor;
pub mod mock_nm;

// Re-export core utilities
pub use assertions::*;
pub use context::*;
pub use results::*;
pub use system::*;

// Re-export mocks
pub use fixtures::{TestConfig, TestEnv};
pub use mock_dbus::{MockConnectivity, MockDbus, MockDbusEvent, MockVpnState};
pub use mock_executor::{MockCommand, MockExecutor, MockResult};
pub use mock_nm::{MockNetworkManager, MockNmCall, MockNmError, MockVpnConnection, MockVpnType};

use std::path::PathBuf;
use std::sync::Once;

static INIT: Once = Once::new();

/// Initialize test environment (called once per test run)
pub fn init() {
    INIT.call_once(|| {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::Warn)
            .is_test(true)
            .try_init();
    });
}

/// Get path to project root
pub fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Get path to test results directory
pub fn results_dir() -> PathBuf {
    let dir = project_root().join("target").join("test-results");
    std::fs::create_dir_all(&dir).ok();
    dir
}
