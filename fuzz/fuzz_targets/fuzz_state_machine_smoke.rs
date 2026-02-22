// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-Commercial
// Copyright (C) 2026 Louis Nelson Jr. <https://lousclues.com>

//! Fuzz target: smoke test.
//!
//! Identical logic to fuzz_state_machine (chaos cannon) but designed
//! for quick CI runs. Use with `-max_total_time=60` to verify the fuzz
//! infrastructure works and catch shallow regressions.
//!
//! This target exists because the full MOAB shards run for 5 hours and
//! cannot be included in normal CI. The smoke test proves:
//! - The fuzz binary compiles
//! - The event generator covers all 14 variants + chaos strings
//! - All 13 invariants are checked
//! - No shallow bugs exist (anything findable in 60 seconds)
//!
//! Run manually: cargo +nightly fuzz run fuzz_state_machine_smoke -- -max_total_time=60
//! Run in CI:    cargo +nightly fuzz run fuzz_state_machine_smoke -- -max_total_time=60

#![no_main]

use libfuzzer_sys::fuzz_target;
use shroud::state::{StateMachineConfig, StateMachine};

#[path = "state_machine_common.rs"]
mod common;
use common::{event_from_byte, check_invariants};

fuzz_target!(|data: &[u8]| {
    let config = StateMachineConfig { max_retries: 5 };
    let mut machine = StateMachine::with_config(config);

    for chunk in data.chunks(2) {
        let event_byte = chunk[0];
        let timing_byte = chunk.get(1).copied().unwrap_or(0);

        let iterations: u8 = if timing_byte < 10 { 3 } else { 1 };

        let event = event_from_byte(event_byte);

        for _ in 0..iterations {
            let _reason = machine.handle_event(event.clone());
            check_invariants(&machine);
        }
    }

    check_invariants(&machine);
});
