//! Gateway rule building — pure functions, easily testable.
//!
//! Constructs iptables argument vectors and NAT/forwarding rule
//! descriptions without executing any commands.

/// A gateway iptables rule (pure data).
#[derive(Debug, Clone, PartialEq)]
pub enum GatewayRule {
    /// FORWARD from LAN to VPN with an action.
    Forward {
        in_iface: String,
        out_iface: String,
        action: String,
    },
    /// FORWARD for source-restricted client.
    ForwardClient {
        in_iface: String,
        out_iface: String,
        source: String,
    },
    /// Return-traffic rule (ESTABLISHED,RELATED).
    Related { in_iface: String, out_iface: String },
    /// NAT MASQUERADE on outgoing interface.
    Masquerade { out_iface: String },
}

impl GatewayRule {
    /// Convert to iptables argument vector.
    pub fn to_args(&self) -> Vec<String> {
        match self {
            GatewayRule::Forward {
                in_iface,
                out_iface,
                action,
            } => vec![
                "-A".into(),
                "FORWARD".into(),
                "-i".into(),
                in_iface.clone(),
                "-o".into(),
                out_iface.clone(),
                "-j".into(),
                action.clone(),
            ],
            GatewayRule::ForwardClient {
                in_iface,
                out_iface,
                source,
            } => vec![
                "-A".into(),
                "FORWARD".into(),
                "-i".into(),
                in_iface.clone(),
                "-o".into(),
                out_iface.clone(),
                "-s".into(),
                source.clone(),
                "-j".into(),
                "ACCEPT".into(),
            ],
            GatewayRule::Related {
                in_iface,
                out_iface,
            } => vec![
                "-A".into(),
                "FORWARD".into(),
                "-i".into(),
                in_iface.clone(),
                "-o".into(),
                out_iface.clone(),
                "-m".into(),
                "state".into(),
                "--state".into(),
                "RELATED,ESTABLISHED".into(),
                "-j".into(),
                "ACCEPT".into(),
            ],
            GatewayRule::Masquerade { out_iface } => vec![
                "-t".into(),
                "nat".into(),
                "-A".into(),
                "POSTROUTING".into(),
                "-o".into(),
                out_iface.clone(),
                "-j".into(),
                "MASQUERADE".into(),
            ],
        }
    }
}

/// Build the standard set of gateway rules for LAN→VPN routing.
pub fn build_gateway_rules(lan: &str, vpn: &str) -> Vec<GatewayRule> {
    vec![
        GatewayRule::Forward {
            in_iface: lan.into(),
            out_iface: vpn.into(),
            action: "ACCEPT".into(),
        },
        GatewayRule::Related {
            in_iface: vpn.into(),
            out_iface: lan.into(),
        },
        GatewayRule::Masquerade {
            out_iface: vpn.into(),
        },
    ]
}

/// Build gateway rules restricted to specific source IPs.
pub fn build_client_rules(lan: &str, vpn: &str, sources: &[String]) -> Vec<GatewayRule> {
    let mut rules: Vec<GatewayRule> = sources
        .iter()
        .map(|src| GatewayRule::ForwardClient {
            in_iface: lan.into(),
            out_iface: vpn.into(),
            source: src.clone(),
        })
        .collect();

    rules.push(GatewayRule::Related {
        in_iface: vpn.into(),
        out_iface: lan.into(),
    });
    rules.push(GatewayRule::Masquerade {
        out_iface: vpn.into(),
    });
    rules
}

/// Validate that NAT is required (interfaces are different and non-empty).
pub fn nat_required(lan: &str, vpn: &str) -> bool {
    !lan.is_empty() && !vpn.is_empty() && lan != vpn
}

/// Forwarding state parsed from `/proc/sys/net/ipv4/ip_forward`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwardingState {
    Enabled,
    Disabled,
    Unknown,
}

impl ForwardingState {
    pub fn from_proc(value: &str) -> Self {
        match value.trim() {
            "1" => ForwardingState::Enabled,
            "0" => ForwardingState::Disabled,
            _ => ForwardingState::Unknown,
        }
    }

    pub fn is_enabled(&self) -> bool {
        matches!(self, ForwardingState::Enabled)
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    mod gateway_rule {
        use super::*;

        #[test]
        fn test_forward_args() {
            let rule = GatewayRule::Forward {
                in_iface: "eth0".into(),
                out_iface: "tun0".into(),
                action: "ACCEPT".into(),
            };
            let args = rule.to_args();
            assert!(args.contains(&"-i".into()));
            assert!(args.contains(&"eth0".into()));
            assert!(args.contains(&"-o".into()));
            assert!(args.contains(&"tun0".into()));
            assert!(args.contains(&"-j".into()));
            assert!(args.contains(&"ACCEPT".into()));
        }

        #[test]
        fn test_forward_client_args() {
            let rule = GatewayRule::ForwardClient {
                in_iface: "eth0".into(),
                out_iface: "tun0".into(),
                source: "192.168.1.50".into(),
            };
            let args = rule.to_args();
            assert!(args.contains(&"-s".into()));
            assert!(args.contains(&"192.168.1.50".into()));
        }

        #[test]
        fn test_related_args() {
            let rule = GatewayRule::Related {
                in_iface: "tun0".into(),
                out_iface: "eth0".into(),
            };
            let args = rule.to_args();
            assert!(args.contains(&"RELATED,ESTABLISHED".into()));
            assert!(args.contains(&"ACCEPT".into()));
        }

        #[test]
        fn test_masquerade_args() {
            let rule = GatewayRule::Masquerade {
                out_iface: "tun0".into(),
            };
            let args = rule.to_args();
            assert!(args.contains(&"nat".into()));
            assert!(args.contains(&"MASQUERADE".into()));
            assert!(args.contains(&"tun0".into()));
        }
    }

    mod build_rules {
        use super::*;

        #[test]
        fn test_gateway_rules_count() {
            let rules = build_gateway_rules("eth0", "tun0");
            assert_eq!(rules.len(), 3);
        }

        #[test]
        fn test_gateway_rules_types() {
            let rules = build_gateway_rules("eth0", "tun0");
            assert!(matches!(rules[0], GatewayRule::Forward { .. }));
            assert!(matches!(rules[1], GatewayRule::Related { .. }));
            assert!(matches!(rules[2], GatewayRule::Masquerade { .. }));
        }

        #[test]
        fn test_client_rules_with_sources() {
            let rules = build_client_rules(
                "eth0",
                "tun0",
                &["192.168.1.10".into(), "192.168.1.20".into()],
            );
            // 2 client + 1 related + 1 masquerade = 4
            assert_eq!(rules.len(), 4);
            assert!(matches!(rules[0], GatewayRule::ForwardClient { .. }));
            assert!(matches!(rules[1], GatewayRule::ForwardClient { .. }));
        }

        #[test]
        fn test_client_rules_empty_sources() {
            let rules = build_client_rules("eth0", "tun0", &[]);
            // 0 client + 1 related + 1 masquerade = 2
            assert_eq!(rules.len(), 2);
        }
    }

    mod nat {
        use super::*;

        #[test]
        fn test_nat_required_different() {
            assert!(nat_required("eth0", "tun0"));
        }

        #[test]
        fn test_nat_not_required_same() {
            assert!(!nat_required("eth0", "eth0"));
        }

        #[test]
        fn test_nat_not_required_empty() {
            assert!(!nat_required("", "tun0"));
            assert!(!nat_required("eth0", ""));
        }
    }

    mod forwarding {
        use super::*;

        #[test]
        fn test_from_proc_enabled() {
            assert_eq!(ForwardingState::from_proc("1"), ForwardingState::Enabled);
            assert_eq!(ForwardingState::from_proc("1\n"), ForwardingState::Enabled);
        }

        #[test]
        fn test_from_proc_disabled() {
            assert_eq!(ForwardingState::from_proc("0"), ForwardingState::Disabled);
            assert_eq!(ForwardingState::from_proc("0\n"), ForwardingState::Disabled);
        }

        #[test]
        fn test_from_proc_unknown() {
            assert_eq!(
                ForwardingState::from_proc("invalid"),
                ForwardingState::Unknown
            );
            assert_eq!(ForwardingState::from_proc(""), ForwardingState::Unknown);
        }

        #[test]
        fn test_is_enabled() {
            assert!(ForwardingState::Enabled.is_enabled());
            assert!(!ForwardingState::Disabled.is_enabled());
            assert!(!ForwardingState::Unknown.is_enabled());
        }
    }
}
