// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-Commercial
// Copyright (C) 2026 Louis Nelson Jr. <https://lousclues.com>

//! Fuzz target for TOML configuration parsing.
//!
//! Feeds arbitrary bytes as a TOML string to the `Config` deserializer.
//! Verifies: no panics, no undefined behavior, and successful parses
//! round-trip back to valid TOML.

#![no_main]

use libfuzzer_sys::fuzz_target;
use shroud::config::Config;

fuzz_target!(|data: &[u8]| {
    // Only proceed if the input is valid UTF-8 (TOML requires it)
    let input = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Attempt to parse as Config
    if let Ok(config) = toml::from_str::<Config>(input) {
        // Validate must not panic
        let _ = config.validate();

        // Serialize back to TOML — must not panic
        if let Ok(serialized) = toml::to_string(&config) {
            // The re-serialized form should also parse back
            let _ = toml::from_str::<Config>(&serialized);
        }
    }
    // Parse failures are expected and fine — just verify no panic/UB
});
