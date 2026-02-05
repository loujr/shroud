//! Test harness that ensures cleanup even on panic

use std::future::Future;
use std::time::Duration;

/// Run a test with automatic cleanup and timeout
pub async fn run_test<F, Fut>(name: &str, timeout_secs: u64, test_fn: F)
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    let result = tokio::time::timeout(Duration::from_secs(timeout_secs), test_fn()).await;

    // Always cleanup after test
    super::process::cleanup_all_shroud_processes();

    match result {
        Ok(()) => {}
        Err(_) => panic!("Test '{}' timed out after {}s", name, timeout_secs),
    }
}

/// Guard that cleans up on drop (even on panic)
pub struct CleanupGuard;

impl CleanupGuard {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CleanupGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        // Kill any orphaned processes
        super::process::cleanup_all_shroud_processes();
    }
}
