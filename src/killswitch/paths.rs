//! Firewall binary path detection.
//!
//! Detects iptables/ip6tables/nft binaries across distros and caches results.

use log::debug;
use std::path::PathBuf;
use std::sync::OnceLock;

static IPTABLES_PATH: OnceLock<PathBuf> = OnceLock::new();
static IP6TABLES_PATH: OnceLock<PathBuf> = OnceLock::new();
static NFT_PATH: OnceLock<PathBuf> = OnceLock::new();

const IPTABLES_CANDIDATES: &[&str] = &[
    "/usr/bin/iptables",
    "/usr/sbin/iptables",
    "/bin/iptables",
    "/sbin/iptables",
];

const IP6TABLES_CANDIDATES: &[&str] = &[
    "/usr/bin/ip6tables",
    "/usr/sbin/ip6tables",
    "/bin/ip6tables",
    "/sbin/ip6tables",
];

const NFT_CANDIDATES: &[&str] = &["/usr/bin/nft", "/usr/sbin/nft", "/bin/nft", "/sbin/nft"];

fn find_binary(candidates: &[&str], name: &str) -> PathBuf {
    for candidate in candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            debug!("Found {} at {}", name, candidate);
            return path;
        }
    }

    if let Ok(output) = std::process::Command::new("which").arg(name).output() {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout);
            let path = PathBuf::from(path_str.trim());
            if path.exists() {
                debug!("Found {} via which: {}", name, path.display());
                return path;
            }
        }
    }

    debug!("Could not find {}, defaulting to /usr/sbin/{}", name, name);
    PathBuf::from(format!("/usr/sbin/{}", name))
}

pub fn iptables_path() -> &'static PathBuf {
    IPTABLES_PATH.get_or_init(|| find_binary(IPTABLES_CANDIDATES, "iptables"))
}

pub fn ip6tables_path() -> &'static PathBuf {
    IP6TABLES_PATH.get_or_init(|| find_binary(IP6TABLES_CANDIDATES, "ip6tables"))
}

pub fn nft_path() -> &'static PathBuf {
    NFT_PATH.get_or_init(|| find_binary(NFT_CANDIDATES, "nft"))
}

pub fn iptables() -> &'static str {
    iptables_path().to_str().unwrap_or("/usr/sbin/iptables")
}

pub fn ip6tables() -> &'static str {
    ip6tables_path().to_str().unwrap_or("/usr/sbin/ip6tables")
}

pub fn nft() -> &'static str {
    nft_path().to_str().unwrap_or("/usr/sbin/nft")
}

pub fn log_detected_paths() {
    log::debug!("Firewall binary paths:");
    log::debug!("  iptables:  {}", iptables());
    log::debug!("  ip6tables: {}", ip6tables());
    log::debug!("  nft:       {}", nft());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_strings_are_absolute() {
        assert!(iptables().starts_with('/'));
        assert!(ip6tables().starts_with('/'));
        assert!(nft().starts_with('/'));
    }

    #[test]
    fn test_paths_are_absolute() {
        assert!(iptables_path().is_absolute());
        assert!(ip6tables_path().is_absolute());
        assert!(nft_path().is_absolute());
    }
}
