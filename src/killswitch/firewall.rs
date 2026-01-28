//! iptables-based VPN kill switch
//!
//! Uses a dedicated iptables chain to block all outbound traffic except:
//! - Traffic through VPN tunnel interfaces (tun*, wg*, tap*)
//! - Traffic to the VPN server IP (to establish connection)
//! - Local loopback traffic
//! - Established/related connections
//! - Local network traffic (192.168.0.0/16, etc)
//! - DHCP
//!
//! The kill switch uses a separate chain "SHROUD_KILLSWITCH" in the filter table
//! and inserts a jump rule at the top of the OUTPUT chain.
//!
//! ## DNS Leak Protection
//!
//! Controlled by `dns_mode` config:
//! - `tunnel`: DNS only via VPN tunnel interfaces (most secure)
//! - `localhost`: DNS only to 127.0.0.0/8, ::1, 127.0.0.53
//! - `any`: DNS to any destination (legacy, least secure)
//!
//! ## IPv6 Leak Protection
//!
//! Controlled by `ipv6_mode` config:
//! - `block`: Drop all IPv6 except loopback (most secure)
//! - `tunnel`: Allow IPv6 only via VPN tunnel interfaces
//! - `off`: No special IPv6 handling (legacy)

#![allow(dead_code)]

use log::{debug, info, warn};
use std::net::IpAddr;
use std::process::Stdio;
use thiserror::Error;
use tokio::process::Command;

use crate::config::{DnsMode, Ipv6Mode};

/// Name of the iptables chain for the kill switch
const CHAIN_NAME: &str = "SHROUD_KILLSWITCH";

/// Errors that can occur during kill switch operations.
#[derive(Error, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum KillSwitchError {
    /// iptables is not installed or not in PATH
    #[error("iptables is not available. Install with: sudo apt install iptables")]
    NotFound,

    /// Permission denied - need elevated privileges
    #[error("Permission denied. Kill switch requires root privileges via pkexec.")]
    Permission,

    /// Failed to spawn iptables/pkexec process
    #[error("Failed to spawn iptables process: {0}")]
    Spawn(#[source] std::io::Error),

    /// iptables command returned error
    #[error("iptables command failed: {0}")]
    Command(String),

    /// Failed to write to process stdin
    #[error("Failed to write to process: {0}")]
    Write(#[source] std::io::Error),

    /// Failed waiting for iptables process
    #[error("Failed waiting for iptables process: {0}")]
    Wait(#[source] std::io::Error),
}

/// Kill switch status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillSwitchStatus {
    /// Kill switch is disabled (normal traffic allowed)
    Disabled,
    /// Kill switch is enabled and active (only VPN traffic allowed)
    Active,
    /// Kill switch encountered an error
    Error,
}

/// VPN Kill Switch using iptables
pub struct KillSwitch {
    /// Whether the kill switch is currently enabled
    enabled: bool,
    /// Current VPN server IP (allowed through even when kill switch is on)
    vpn_server_ip: Option<IpAddr>,
    /// VPN tunnel interface name (e.g., "tun0")
    vpn_interface: Option<String>,
    /// DNS leak protection mode
    dns_mode: DnsMode,
    /// IPv6 leak protection mode
    ipv6_mode: Ipv6Mode,
}

impl KillSwitch {
    /// Create a new kill switch instance with default (secure) settings
    pub fn new() -> Self {
        Self {
            enabled: false,
            vpn_server_ip: None,
            vpn_interface: None,
            dns_mode: DnsMode::default(),
            ipv6_mode: Ipv6Mode::default(),
        }
    }

    /// Create a kill switch with specific DNS and IPv6 modes
    pub fn with_config(dns_mode: DnsMode, ipv6_mode: Ipv6Mode) -> Self {
        Self {
            enabled: false,
            vpn_server_ip: None,
            vpn_interface: None,
            dns_mode,
            ipv6_mode,
        }
    }

    /// Update configuration (DNS and IPv6 modes)
    pub fn set_config(&mut self, dns_mode: DnsMode, ipv6_mode: Ipv6Mode) {
        self.dns_mode = dns_mode;
        self.ipv6_mode = ipv6_mode;
    }

    /// Check if iptables is available
    pub async fn is_available() -> bool {
        Command::new("iptables")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Check if we have permission to modify iptables
    pub async fn has_permission() -> bool {
        // Try to list filter table - this will fail if we don't have permission
        Command::new("iptables")
            .args(["-t", "filter", "-nL", "OUTPUT"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Get current status
    pub fn status(&self) -> KillSwitchStatus {
        if self.enabled {
            KillSwitchStatus::Active
        } else {
            KillSwitchStatus::Disabled
        }
    }

    /// Check if kill switch is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set the VPN server IP to allow through the firewall
    pub fn set_vpn_server(&mut self, ip: Option<IpAddr>) {
        self.vpn_server_ip = ip;
    }

    /// Set the VPN tunnel interface
    pub fn set_vpn_interface(&mut self, iface: Option<String>) {
        self.vpn_interface = iface;
    }

    /// Check actual state of rules, not just our flag
    pub fn is_actually_enabled(&self) -> bool {
        // Synchronous check - used for status queries
        use std::process::{Command, Stdio};

        let result = Command::new("iptables")
            .args(["-C", "OUTPUT", "-j", CHAIN_NAME])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        matches!(result, Ok(status) if status.success())
    }

    /// Sync our internal state with actual iptables state
    pub fn sync_state(&mut self) {
        self.enabled = self.is_actually_enabled();
    }

    /// Enable the kill switch
    pub async fn enable(&mut self) -> Result<(), KillSwitchError> {
        if self.enabled && self.verify_rules_exist().await {
            debug!("Kill switch already enabled");
            return Ok(());
        }

        info!("Enabling VPN kill switch");

        // Detect VPN server IPs first
        let vpn_server_ips = Self::detect_all_vpn_server_ips().await;
        if !vpn_server_ips.is_empty() {
            info!(
                "Detected {} VPN server IPs to whitelist",
                vpn_server_ips.len()
            );
        }

        // Build ONE script with ALL rules
        let script = self.build_complete_script(&vpn_server_ips);

        // Run with ONE pkexec call
        self.run_single_script(&script).await?;

        // Note: We cannot run verification here because iptables -C
        // requires root or special permissions which the user might not have
        // without pkexec (which prompts). We trust run_single_script status.

        self.enabled = true;
        info!("VPN kill switch enabled");
        Ok(())
    }

    fn build_complete_script(&self, vpn_ips: &[IpAddr]) -> String {
        let mut s = String::new();

        // Cleanup (always runs, errors ignored)
        s.push_str("iptables -D OUTPUT -j SHROUD_KILLSWITCH 2>/dev/null || true\n");
        s.push_str("iptables -F SHROUD_KILLSWITCH 2>/dev/null || true\n");
        s.push_str("iptables -X SHROUD_KILLSWITCH 2>/dev/null || true\n");
        s.push_str("nft delete table inet shroud_killswitch 2>/dev/null || true\n");

        // Create chain
        s.push_str("iptables -N SHROUD_KILLSWITCH\n");
        s.push_str("iptables -I OUTPUT 1 -j SHROUD_KILLSWITCH\n");

        // Rules
        s.push_str("iptables -A SHROUD_KILLSWITCH -o lo -j ACCEPT\n");
        s.push_str(
            "iptables -A SHROUD_KILLSWITCH -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT\n",
        );
        s.push_str("iptables -A SHROUD_KILLSWITCH -o tun+ -j ACCEPT\n");
        s.push_str("iptables -A SHROUD_KILLSWITCH -o tap+ -j ACCEPT\n");
        s.push_str("iptables -A SHROUD_KILLSWITCH -o wg+ -j ACCEPT\n");

        for ip in vpn_ips {
            if let IpAddr::V4(v4) = ip {
                s.push_str(&format!(
                    "iptables -A SHROUD_KILLSWITCH -d {} -j ACCEPT\n",
                    v4
                ));
            }
        }

        s.push_str("iptables -A SHROUD_KILLSWITCH -d 192.168.0.0/16 -j ACCEPT\n");
        s.push_str("iptables -A SHROUD_KILLSWITCH -d 10.0.0.0/8 -j ACCEPT\n");
        s.push_str("iptables -A SHROUD_KILLSWITCH -d 172.16.0.0/12 -j ACCEPT\n");
        s.push_str("iptables -A SHROUD_KILLSWITCH -p udp --dport 67 -j ACCEPT\n");
        s.push_str("iptables -A SHROUD_KILLSWITCH -p udp --sport 68 -j ACCEPT\n");

        // DNS based on mode
        match self.dns_mode {
            DnsMode::Localhost => {
                s.push_str(
                    "iptables -A SHROUD_KILLSWITCH -d 127.0.0.0/8 -p udp --dport 53 -j ACCEPT\n",
                );
                s.push_str(
                    "iptables -A SHROUD_KILLSWITCH -d 127.0.0.0/8 -p tcp --dport 53 -j ACCEPT\n",
                );
            }
            DnsMode::Any => {
                s.push_str("iptables -A SHROUD_KILLSWITCH -p udp --dport 53 -j ACCEPT\n");
                s.push_str("iptables -A SHROUD_KILLSWITCH -p tcp --dport 53 -j ACCEPT\n");
            }
            DnsMode::Tunnel => {}
        }

        s.push_str("iptables -A SHROUD_KILLSWITCH -m limit --limit 1/sec -j LOG --log-prefix '[SHROUD-KS DROP] ' --log-level 4\n");
        s.push_str("iptables -A SHROUD_KILLSWITCH -j DROP\n");

        // IPv6
        match self.ipv6_mode {
            Ipv6Mode::Block => {
                s.push_str("ip6tables -I OUTPUT 1 -o lo -j ACCEPT 2>/dev/null || true\n");
                s.push_str("ip6tables -I OUTPUT 2 -j DROP 2>/dev/null || true\n");
            }
            Ipv6Mode::Tunnel => {
                s.push_str("ip6tables -I OUTPUT 1 -o lo -j ACCEPT 2>/dev/null || true\n");
                s.push_str("ip6tables -I OUTPUT 2 -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT 2>/dev/null || true\n");
                s.push_str("ip6tables -I OUTPUT 3 -o tun+ -j ACCEPT 2>/dev/null || true\n");
                s.push_str("ip6tables -I OUTPUT 4 -d fe80::/10 -j ACCEPT 2>/dev/null || true\n");
                s.push_str("ip6tables -I OUTPUT 5 -j DROP 2>/dev/null || true\n");
            }
            Ipv6Mode::Off => {}
        }

        s
    }

    async fn run_single_script(&self, script: &str) -> Result<(), KillSwitchError> {
        use std::process::Stdio;
        use tokio::io::AsyncWriteExt;
        use tokio::process::Command;

        let mut child = Command::new("pkexec")
            .arg("sh")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| KillSwitchError::Spawn(e))?;

        // Write script to stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(script.as_bytes())
                .await
                .map_err(|e| KillSwitchError::Write(e))?;
            // MUST drop stdin to signal EOF
            drop(stdin);
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| KillSwitchError::Wait(e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let code = output.status.code().unwrap_or(-1);

            if code == 126 || code == 127 {
                return Err(KillSwitchError::Permission);
            }

            return Err(KillSwitchError::Command(format!(
                "Script failed (exit {}): {}",
                code,
                stderr.trim()
            )));
        }

        Ok(())
    }

    /// Verify our rules are actually in place
    async fn verify_rules_exist(&self) -> bool {
        // Check if our chain exists and has the jump rule
        let output = Command::new("iptables")
            .args(["-C", "OUTPUT", "-j", CHAIN_NAME])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        matches!(output, Ok(status) if status.success())
    }

    /// Disable the kill switch
    pub async fn disable(&mut self) -> Result<(), KillSwitchError> {
        info!("Disabling VPN kill switch");

        // We run cleanup regardless of enabled status to ensuring we don't leave
        // the user stranded if the internal state is out of sync.

        let script = r#"
iptables -D OUTPUT -j SHROUD_KILLSWITCH 2>/dev/null || true
iptables -F SHROUD_KILLSWITCH 2>/dev/null || true
iptables -X SHROUD_KILLSWITCH 2>/dev/null || true
ip6tables -D OUTPUT -j DROP 2>/dev/null || true
ip6tables -D OUTPUT -o lo -j ACCEPT 2>/dev/null || true
ip6tables -D OUTPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT 2>/dev/null || true
ip6tables -D OUTPUT -o tun+ -j ACCEPT 2>/dev/null || true
ip6tables -D OUTPUT -d fe80::/10 -j ACCEPT 2>/dev/null || true
nft delete table inet shroud_killswitch 2>/dev/null || true
"#;

        self.run_single_script(script).await?;

        self.enabled = false;
        info!("VPN kill switch disabled");
        Ok(())
    }

    /// Update the kill switch rules (e.g., when VPN interface changes)
    pub async fn update(&mut self) -> Result<(), KillSwitchError> {
        if !self.enabled {
            return Ok(());
        }

        // Just toggle off/on to refresh rules
        self.disable().await?;
        self.enable().await
    }

    /// Detect the VPN tunnel interface from the system
    pub async fn detect_vpn_interface() -> Option<String> {
        let output = Command::new("ip")
            .args(["link", "show"])
            .output()
            .await
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            // Look for tun/tap interfaces
            if (line.contains("tun") || line.contains("tap") || line.contains("wg"))
                && line.contains("state UP")
            {
                // Extract interface name (format: "X: tunN: <FLAGS>...")
                if let Some(name) = line.split(':').nth(1) {
                    return Some(name.trim().to_string());
                }
            }
        }

        None
    }

    /// Get the VPN server IP from the active OpenVPN connection
    pub async fn detect_vpn_server_ip() -> Option<IpAddr> {
        // Try to get the remote IP from the tun interface route
        let output = Command::new("ip")
            .args(["route", "show", "dev", "tun0"])
            .output()
            .await
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Look for the VPN gateway in the route output
        for line in stdout.lines() {
            if line.contains("via") {
                if let Some(ip_str) = line.split_whitespace().nth(2) {
                    if let Ok(ip) = ip_str.parse() {
                        return Some(ip);
                    }
                }
            }
        }

        None
    }

    /// Detect all VPN server IPs from NetworkManager connection configs
    pub async fn detect_all_vpn_server_ips() -> Vec<IpAddr> {
        let mut ips = Vec::new();

        // Get VPN connection details from nmcli
        let output = Command::new("nmcli")
            .args(["-t", "-f", "NAME,TYPE", "connection", "show"])
            .output()
            .await;

        let connections = match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
            _ => return ips,
        };

        // Find VPN connections and get their remote IPs
        for line in connections.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 && parts[1] == "vpn" {
                let conn_name = parts[0];

                // Get the remote IP for this VPN connection
                if let Some(ip) = Self::get_vpn_remote_ip(conn_name).await {
                    if !ips.contains(&ip) {
                        info!("Found VPN server IP for '{}': {}", conn_name, ip);
                        ips.push(ip);
                    }
                }
            }
        }

        ips
    }

    /// Get the remote IP address for a specific VPN connection
    async fn get_vpn_remote_ip(conn_name: &str) -> Option<IpAddr> {
        // Get VPN connection details
        let output = Command::new("nmcli")
            .args(["-t", "-f", "vpn.data", "connection", "show", conn_name])
            .output()
            .await
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse vpn.data
        for line in stdout.lines() {
            if line.starts_with("vpn.data:") {
                let data = line.trim_start_matches("vpn.data:");
                for item in data.split(',') {
                    let item = item.trim();
                    if item.starts_with("remote") {
                        if let Some(value) = item.split('=').nth(1) {
                            let remote = value.trim();
                            let host = if let Some(colon_pos) = remote.rfind(':') {
                                if remote[colon_pos + 1..].parse::<u16>().is_ok() {
                                    &remote[..colon_pos]
                                } else {
                                    remote
                                }
                            } else {
                                remote
                            };

                            if let Ok(ip) = host.parse::<IpAddr>() {
                                return Some(ip);
                            }
                            if let Some(ip) = Self::resolve_hostname(host).await {
                                return Some(ip);
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Resolve a hostname to an IP address
    async fn resolve_hostname(hostname: &str) -> Option<IpAddr> {
        let output = Command::new("getent")
            .args(["ahosts", hostname])
            .output()
            .await
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some(ip_str) = line.split_whitespace().next() {
                if let Ok(ip) = ip_str.parse::<IpAddr>() {
                    if ip.is_ipv4() {
                        return Some(ip);
                    }
                }
            }
        }

        None
    }
}

/// Synchronously clean up any stale kill switch rules
///
/// This is a standalone function that can be called from:
/// - Signal handlers (which are synchronous)
/// - Startup cleanup (before async runtime is available)
///
/// Uses blocking std::process::Command
pub fn cleanup_stale_rules() {
    use std::process::{Command, Stdio};

    let cleanup_script = format!(
        r#"
        iptables -D OUTPUT -j {CHAIN_NAME} 2>/dev/null
        iptables -F {CHAIN_NAME} 2>/dev/null
        iptables -X {CHAIN_NAME} 2>/dev/null
        ip6tables -D OUTPUT -j DROP 2>/dev/null
        ip6tables -D OUTPUT -o lo -j ACCEPT 2>/dev/null
        ip6tables -D OUTPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT 2>/dev/null
        ip6tables -D OUTPUT -o tun+ -j ACCEPT 2>/dev/null
        ip6tables -D OUTPUT -o wg+ -j ACCEPT 2>/dev/null
        ip6tables -D OUTPUT -d fe80::/10 -j ACCEPT 2>/dev/null
        nft delete table inet shroud_killswitch 2>/dev/null
        exit 0
    "#
    );

    // Try with pkexec first
    let result = Command::new("pkexec")
        .args(["sh", "-c", &cleanup_script])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match result {
        Ok(status) if status.success() => {
            info!("Cleaned up stale kill switch rules");
        }
        Ok(_) => {
            // pkexec might have been cancelled, try sudo as fallback
            let _ = Command::new("sudo")
                .args(["-n", "sh", "-c", &cleanup_script]) // -n = non-interactive
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        Err(e) => {
            warn!("Failed to clean up kill switch rules: {}", e);
        }
    }
}

/// Check if rules exist (synchronous)
pub fn rules_exist() -> bool {
    use std::process::{Command, Stdio};

    let result = Command::new("iptables")
        .args(["-C", "OUTPUT", "-j", CHAIN_NAME])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    matches!(result, Ok(status) if status.success())
}

impl Default for KillSwitch {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for KillSwitch {
    fn drop(&mut self) {
        if self.enabled {
            warn!("Kill switch dropped while enabled - rules may persist!");
            warn!("Run 'sudo iptables -F {}' to clean up", CHAIN_NAME);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kill_switch_status() {
        let ks = KillSwitch::new();
        assert_eq!(ks.status(), KillSwitchStatus::Disabled);
        assert!(!ks.is_enabled());
    }

    #[test]
    fn test_kill_switch_set_server() {
        let mut ks = KillSwitch::new();
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        ks.set_vpn_server(Some(ip));
        assert_eq!(ks.vpn_server_ip, Some(ip));
    }

    #[test]
    fn test_kill_switch_with_config() {
        let ks = KillSwitch::with_config(DnsMode::Localhost, Ipv6Mode::Tunnel);
        assert_eq!(ks.dns_mode, DnsMode::Localhost);
        assert_eq!(ks.ipv6_mode, Ipv6Mode::Tunnel);
    }

    #[test]
    fn test_kill_switch_configuration_update() {
        let mut ks = KillSwitch::new();
        ks.set_config(DnsMode::Any, Ipv6Mode::Off);
        assert_eq!(ks.dns_mode, DnsMode::Any);
        assert_eq!(ks.ipv6_mode, Ipv6Mode::Off);
    }

    // Verify format of complete script
    #[test]
    fn test_build_complete_script() {
        let ks = KillSwitch::new();
        let script = ks.build_complete_script(&[]);
        assert!(script.contains("iptables -N SHROUD_KILLSWITCH"));
        assert!(script.contains("iptables -I OUTPUT 1 -j SHROUD_KILLSWITCH"));
        assert!(script.contains("nft delete table inet shroud_killswitch"));
        // Check for cleanup commands at start
        assert!(script.contains("iptables -X SHROUD_KILLSWITCH 2>/dev/null || true"));
    }
}
