// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-Commercial
// Copyright (C) 2026 Louis Nelson Jr. <https://lousclues.com>

//! Library re-exports for integration testing and fuzz targets.
//!
//! Shroud is primarily a binary crate. This thin library target exposes
//! leaf modules (no deep dependency chains) so that `tests/` integration
//! tests and `fuzz/` targets can exercise real types (StateMachine,
//! HealthChecker, Config, IpcCommand, etc.) instead of relying on
//! fragile `include_str!` assertions.

// Leaf modules with no heavy dependencies
pub mod config;
pub mod health;
pub mod notifications;
pub mod state;

// CLI validation is a leaf module (no crate-internal deps)
pub mod cli {
    pub mod validation;
}

// IPC protocol is a near-leaf module (only depends on cli::validation)
pub mod ipc {
    pub mod protocol;
}
