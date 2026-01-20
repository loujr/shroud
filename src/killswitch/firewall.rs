//! nftables-based VPN kill switch
//!
//! Uses a dedicated nftables table to block all traffic except:
//! - Traffic through VPN tunnel interfaces (tun*)
//! - Traffic to the VPN server IP (to establish connection)
//! - Local loopback traffic
//! - Established/related connections
//!
//! The kill switch uses a separate table "vpn_killswitch" to avoid
//! interfering with other firewall rules.

use log::{debug, error, info, warn};
use std::net::IpAddr;
use std::process::Stdio;
use tokio::process::Command;

/// Name of the nftables table for the kill switch
const NFT_TABLE: &str = "vpn_killswitch";

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

/// VPN Kill Switch using nftables
pub struct KillSwitch {
    /// Whether the kill switch is currently enabled
    enabled: bool,
    /// Current VPN server IP (allowed through even when kill switch is on)
    vpn_server_ip: Option<IpAddr>,
    /// VPN tunnel interface name (e.g., "tun0")
    vpn_interface: Option<String>,
}

impl KillSwitch {
    /// Create a new kill switch instance
    pub fn new() -> Self {
        Self {
            enabled: false,
            vpn_server_ip: None,
            vpn_interface: None,
        }
    }

    /// Check if nftables is available
    pub async fn is_available() -> bool {
        Command::new("nft")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Check if we have permission to modify nftables
    pub async fn has_permission() -> bool {
        // Try to list tables - this will fail if we don't have permission
        Command::new("nft")
            .args(["list", "tables"])
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

    /// Enable the kill switch
    ///
    /// Creates nftables rules that:
    /// 1. Allow loopback traffic
    /// 2. Allow established/related connections
    /// 3. Allow traffic to VPN server IP
    /// 4. Allow traffic through VPN tunnel interface
    /// 5. Drop everything else
    pub async fn enable(&mut self) -> Result<(), String> {
        if self.enabled {
            debug!("Kill switch already enabled");
            return Ok(());
        }

        info!("Enabling VPN kill switch");

        // First, ensure any old rules are cleaned up
        let _ = self.cleanup_table().await;

        // Create the kill switch table and chains
        self.create_table().await?;

        self.enabled = true;
        info!("VPN kill switch enabled");
        Ok(())
    }

    /// Disable the kill switch
    pub async fn disable(&mut self) -> Result<(), String> {
        if !self.enabled {
            debug!("Kill switch already disabled");
            return Ok(());
        }

        info!("Disabling VPN kill switch");
        self.cleanup_table().await?;
        self.enabled = false;
        info!("VPN kill switch disabled");
        Ok(())
    }

    /// Update the kill switch rules (e.g., when VPN interface changes)
    pub async fn update(&mut self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        debug!("Updating kill switch rules");
        // Recreate the table with updated rules
        let _ = self.cleanup_table().await;
        self.create_table().await
    }

    /// Create the nftables table and rules
    async fn create_table(&self) -> Result<(), String> {
        let vpn_iface = self.vpn_interface.as_deref().unwrap_or("tun+");
        
        // Build the nftables ruleset
        let mut rules = format!(
            r#"
table inet {table} {{
    chain output {{
        type filter hook output priority 0; policy drop;
        
        # Allow loopback
        oifname "lo" accept
        
        # Allow established/related connections
        ct state established,related accept
        
        # Allow DHCP
        udp dport 67 accept
        udp sport 68 accept
        
        # Allow DNS to localhost (for local resolvers)
        ip daddr 127.0.0.0/8 accept
        ip6 daddr ::1 accept
        
        # Allow traffic through VPN tunnel
        oifname "{vpn_iface}" accept
"#,
            table = NFT_TABLE,
            vpn_iface = vpn_iface
        );

        // Add rule for VPN server IP if known
        if let Some(ip) = self.vpn_server_ip {
            match ip {
                IpAddr::V4(v4) => {
                    rules.push_str(&format!(
                        "        \n        # Allow traffic to VPN server\n        ip daddr {} accept\n",
                        v4
                    ));
                }
                IpAddr::V6(v6) => {
                    rules.push_str(&format!(
                        "        \n        # Allow traffic to VPN server\n        ip6 daddr {} accept\n",
                        v6
                    ));
                }
            }
        }

        // Add input chain for incoming traffic
        rules.push_str(&format!(
            r#"
        # Log dropped packets (rate limited)
        limit rate 1/second log prefix "[VPN-KS DROP] " drop
    }}
    
    chain input {{
        type filter hook input priority 0; policy drop;
        
        # Allow loopback
        iifname "lo" accept
        
        # Allow established/related connections
        ct state established,related accept
        
        # Allow traffic from VPN tunnel
        iifname "{vpn_iface}" accept
        
        # Allow DHCP responses
        udp sport 67 accept
        
        # Log dropped packets (rate limited)
        limit rate 1/second log prefix "[VPN-KS DROP] " drop
    }}
}}
"#,
            vpn_iface = vpn_iface
        ));

        // Apply the rules
        self.run_nft(&["-f", "-"], Some(&rules)).await
    }

    /// Remove the kill switch table
    async fn cleanup_table(&self) -> Result<(), String> {
        // Delete the table if it exists (this removes all chains and rules)
        let result = self.run_nft(&["delete", "table", "inet", NFT_TABLE], None).await;
        
        // Ignore "No such file or directory" errors (table doesn't exist)
        match result {
            Ok(_) => Ok(()),
            Err(e) if e.contains("No such file") || e.contains("does not exist") => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Run an nft command
    async fn run_nft(&self, args: &[&str], stdin_data: Option<&str>) -> Result<(), String> {
        let mut cmd = Command::new("nft");
        cmd.args(args);
        
        if stdin_data.is_some() {
            cmd.stdin(Stdio::piped());
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn nft: {}", e))?;

        if let Some(data) = stdin_data {
            use tokio::io::AsyncWriteExt;
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(data.as_bytes())
                    .await
                    .map_err(|e| format!("Failed to write to nft stdin: {}", e))?;
            }
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| format!("Failed to wait for nft: {}", e))?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("nft command failed: {}", stderr.trim()))
        }
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
            // Look for tun interfaces
            if line.contains("tun") && line.contains("state UP") {
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
            warn!("Run 'sudo nft delete table inet {}' to clean up", NFT_TABLE);
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
    fn test_kill_switch_set_interface() {
        let mut ks = KillSwitch::new();
        ks.set_vpn_interface(Some("tun0".to_string()));
        assert_eq!(ks.vpn_interface, Some("tun0".to_string()));
    }
}
