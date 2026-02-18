// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-Commercial
// Copyright (C) 2026 Louis Nelson Jr. <https://lousclues.com>

//! Fuzz target for VPN name validation.
//!
//! Feeds arbitrary bytes as a VPN connection name to `validate_vpn_name()`.
//! Verifies: always returns Ok or Err, never panics.

#![no_main]

use libfuzzer_sys::fuzz_target;
use shroud::cli::validation::validate_vpn_name;

fuzz_target!(|data: &[u8]| {
    // Only proceed if the input is valid UTF-8 (VPN names are strings)
    let input = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return,
    };

    // validate_vpn_name must return Ok or Err — never panic
    let _ = validate_vpn_name(input);
});
