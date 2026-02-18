// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-Commercial
// Copyright (C) 2026 Louis Nelson Jr. <https://lousclues.com>

//! Fuzz target for IPC command parsing.
//!
//! Feeds arbitrary bytes to the JSON deserializer for `IpcCommand`.
//! Verifies: no panics, no undefined behavior, and successful parses
//! round-trip back to valid JSON.

#![no_main]

use libfuzzer_sys::fuzz_target;
use shroud::ipc::protocol::IpcCommand;

fuzz_target!(|data: &[u8]| {
    // Attempt to parse arbitrary bytes as an IpcCommand
    if let Ok(cmd) = serde_json::from_slice::<IpcCommand>(data) {
        // If parsing succeeds, the result must serialize back to valid JSON
        let serialized = serde_json::to_string(&cmd)
            .expect("successfully parsed IpcCommand must serialize back to JSON");

        // The re-serialized form must also parse back
        let _roundtrip: IpcCommand = serde_json::from_str(&serialized)
            .expect("re-serialized IpcCommand must parse again");

        // Validate method must not panic
        let _ = cmd.validate();
    }
    // If parsing fails, that's fine — we just verify no panic/UB
});
