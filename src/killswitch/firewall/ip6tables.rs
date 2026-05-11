// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-Commercial
// Copyright (C) 2026 Louis Nelson Jr. <https://lousclues.com>

//! ip6tables (IPv6) backend helpers.
//!
//! IPv6 leak protection is implemented in two pieces:
//! 1. A small, pure script-fragment generator ([`build_ipv6_script`]) that
//!    appends the `ip6tables` rules required by the configured [`Ipv6Mode`]
//!    onto the iptables script in [`super::builder`].
//! 2. The list of IPv6 rule patterns ([`IPV6_OUTPUT_RULES`]) that
//!    [`super::chains`]'s `robust_iptables_cleanup` iterates over to remove
//!    every duplicate IPv6 rule the kill switch may have inserted.
//!
//! All command construction uses `Command::new().args()` upstream — this
//! module only emits string fragments that are then split into argv by
//! `KillSwitch::run_single_script`.

use crate::config::Ipv6Mode;
use crate::killswitch::paths::ip6tables;

/// Build the ip6tables portion of the kill switch script for the given IPv6 mode.
///
/// The fragment is appended to the IPv4 iptables script and consumed by
/// [`super::KillSwitch::run_single_script`]. Trailing `2>/dev/null || true` is
/// stripped by the runner; failures here are tolerated because the IPv6 stack
/// may legitimately be absent on some kernels.
pub(super) fn build_ipv6_script(ipv6_mode: Ipv6Mode) -> String {
    let mut s = String::new();
    match ipv6_mode {
        Ipv6Mode::Block => {
            s.push_str(&format!(
                "{} -I OUTPUT 1 -o lo -j ACCEPT 2>/dev/null || true\n",
                ip6tables()
            ));
            s.push_str(&format!(
                "{} -I OUTPUT 2 -j DROP 2>/dev/null || true\n",
                ip6tables()
            ));
        }
        Ipv6Mode::Tunnel => {
            s.push_str(&format!(
                "{} -I OUTPUT 1 -o lo -j ACCEPT 2>/dev/null || true\n",
                ip6tables()
            ));
            s.push_str(&format!(
                "{} -I OUTPUT 2 -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT 2>/dev/null || true\n",
                ip6tables()
            ));
            s.push_str(&format!(
                "{} -I OUTPUT 3 -o tun+ -j ACCEPT 2>/dev/null || true\n",
                ip6tables()
            ));
            s.push_str(&format!(
                "{} -I OUTPUT 4 -d fe80::/10 -j ACCEPT 2>/dev/null || true\n",
                ip6tables()
            ));
            s.push_str(&format!(
                "{} -I OUTPUT 5 -j DROP 2>/dev/null || true\n",
                ip6tables()
            ));
        }
        Ipv6Mode::Off => {}
    }
    s
}

/// IPv6 OUTPUT-chain rule patterns the kill switch may have inserted.
///
/// Used by `super::chains::robust_iptables_cleanup` to remove every
/// duplicate copy of each rule (rules can accumulate if the kill switch is
/// repeatedly enabled across crashes — the cleanup loop deletes one at a
/// time until the kernel reports the rule is gone).
pub(super) const IPV6_OUTPUT_RULES: &[&[&str]] = &[
    &["-D", "OUTPUT", "-j", "DROP"],
    &["-D", "OUTPUT", "-o", "lo", "-j", "ACCEPT"],
    &[
        "-D",
        "OUTPUT",
        "-m",
        "conntrack",
        "--ctstate",
        "ESTABLISHED,RELATED",
        "-j",
        "ACCEPT",
    ],
    &["-D", "OUTPUT", "-o", "tun+", "-j", "ACCEPT"],
    &["-D", "OUTPUT", "-d", "fe80::/10", "-j", "ACCEPT"],
];
