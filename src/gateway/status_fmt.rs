//! Gateway status formatting — pure functions, easily testable.
//!
//! Builds the human-readable gateway status display without
//! querying any system interfaces.

use std::fmt;

/// Gateway status snapshot (pure data, no I/O in construction for tests).
#[derive(Debug, Clone, Default)]
pub struct GatewaySnapshot {
    pub enabled: bool,
    pub forwarding_enabled: bool,
    pub lan_interface: Option<String>,
    pub lan_ip: Option<String>,
    pub lan_subnet: Option<String>,
    pub vpn_interface: Option<String>,
    pub vpn_ip: Option<String>,
    pub kill_switch_active: bool,
    pub forward_rules: Vec<String>,
}

impl fmt::Display for GatewaySnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Gateway Status")?;
        writeln!(f, "==============")?;
        writeln!(f)?;

        let enabled_str = if self.enabled {
            "✓ enabled"
        } else {
            "✗ disabled"
        };
        writeln!(f, "Gateway:           {}", enabled_str)?;

        let fwd_str = if self.forwarding_enabled {
            "✓ enabled"
        } else {
            "✗ disabled"
        };
        writeln!(f, "IP Forwarding:     {}", fwd_str)?;

        let ks_str = if self.kill_switch_active {
            "✓ active"
        } else {
            "✗ inactive"
        };
        writeln!(f, "Forward Kill SW:   {}", ks_str)?;

        writeln!(f)?;
        writeln!(f, "LAN Interface")?;
        writeln!(f, "-------------")?;
        if let Some(ref iface) = self.lan_interface {
            writeln!(f, "  Interface:       {}", iface)?;
            if let Some(ref ip) = self.lan_ip {
                writeln!(f, "  IP Address:      {}", ip)?;
            }
            if let Some(ref subnet) = self.lan_subnet {
                writeln!(f, "  Subnet:          {}", subnet)?;
            }
        } else {
            writeln!(f, "  Not detected")?;
        }

        writeln!(f)?;
        writeln!(f, "VPN Interface")?;
        writeln!(f, "-------------")?;
        if let Some(ref iface) = self.vpn_interface {
            writeln!(f, "  Interface:       {}", iface)?;
            if let Some(ref ip) = self.vpn_ip {
                writeln!(f, "  IP Address:      {}", ip)?;
            }
        } else {
            writeln!(f, "  Not detected (VPN not connected?)")?;
        }

        if !self.forward_rules.is_empty() && self.enabled {
            writeln!(f)?;
            writeln!(f, "FORWARD Chain Rules")?;
            writeln!(f, "-------------------")?;
            for rule in &self.forward_rules {
                writeln!(f, "  {}", rule)?;
            }
        }

        Ok(())
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_snapshot() {
        let s = GatewaySnapshot::default();
        assert!(!s.enabled);
        assert!(!s.forwarding_enabled);
        assert!(s.lan_interface.is_none());
        assert!(s.vpn_interface.is_none());
        assert!(!s.kill_switch_active);
        assert!(s.forward_rules.is_empty());
    }

    #[test]
    fn test_display_disabled() {
        let s = GatewaySnapshot::default();
        let text = format!("{}", s);
        assert!(text.contains("✗ disabled"));
        assert!(text.contains("Gateway Status"));
        assert!(text.contains("Not detected"));
    }

    #[test]
    fn test_display_enabled_with_interfaces() {
        let s = GatewaySnapshot {
            enabled: true,
            forwarding_enabled: true,
            lan_interface: Some("eth0".into()),
            lan_ip: Some("192.168.1.100".into()),
            lan_subnet: Some("192.168.1.0/24".into()),
            vpn_interface: Some("tun0".into()),
            vpn_ip: Some("10.0.0.2".into()),
            kill_switch_active: true,
            forward_rules: vec!["rule1".into(), "rule2".into()],
        };
        let text = format!("{}", s);
        assert!(text.contains("✓ enabled"));
        assert!(text.contains("eth0"));
        assert!(text.contains("192.168.1.100"));
        assert!(text.contains("tun0"));
        assert!(text.contains("10.0.0.2"));
        assert!(text.contains("✓ active"));
        assert!(text.contains("FORWARD Chain Rules"));
        assert!(text.contains("rule1"));
    }

    #[test]
    fn test_display_no_vpn() {
        let s = GatewaySnapshot {
            lan_interface: Some("eth0".into()),
            ..Default::default()
        };
        let text = format!("{}", s);
        assert!(text.contains("eth0"));
        assert!(text.contains("Not detected (VPN not connected?)"));
    }

    #[test]
    fn test_display_no_forward_rules_when_disabled() {
        let s = GatewaySnapshot {
            enabled: false,
            forward_rules: vec!["rule".into()],
            ..Default::default()
        };
        let text = format!("{}", s);
        // Should NOT show forward rules section when gateway is disabled
        assert!(!text.contains("FORWARD Chain Rules"));
    }

    #[test]
    fn test_display_forward_rules_when_enabled() {
        let s = GatewaySnapshot {
            enabled: true,
            forward_rules: vec!["rule-a".into(), "rule-b".into()],
            ..Default::default()
        };
        let text = format!("{}", s);
        assert!(text.contains("FORWARD Chain Rules"));
        assert!(text.contains("rule-a"));
        assert!(text.contains("rule-b"));
    }

    #[test]
    fn test_display_empty_forward_rules_when_enabled() {
        let s = GatewaySnapshot {
            enabled: true,
            forward_rules: vec![],
            ..Default::default()
        };
        let text = format!("{}", s);
        assert!(!text.contains("FORWARD Chain Rules"));
    }

    #[test]
    fn test_clone() {
        let s = GatewaySnapshot {
            enabled: true,
            lan_interface: Some("eth0".into()),
            ..Default::default()
        };
        let cloned = s.clone();
        assert_eq!(cloned.enabled, s.enabled);
        assert_eq!(cloned.lan_interface, s.lan_interface);
    }
}
