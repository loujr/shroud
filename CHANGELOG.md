# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-01-25

### Added
- Initial release as "Shroud" (rebranded from openvpn-tray)
- Provider-agnostic VPN connection management via NetworkManager
- Kill switch implementation using nftables
- DNS leak protection with three modes: tunnel (default), localhost, any
- IPv6 leak protection with three modes: block (default), tunnel, off
- Auto-reconnect with configurable retry attempts and exponential backoff
- Health monitoring with degraded state detection
- System tray integration via ksni (StatusNotifierItem)
- D-Bus monitoring for real-time NetworkManager events
- Polling fallback for event verification
- Formal state machine with logged transitions
- Config file versioning with automatic migration
- Config migration from old openvpn-tray location
- Systemd user service support
- XDG autostart desktop file
- XDG-compliant configuration and runtime paths
- Comprehensive documentation (README, PRINCIPLES, ARCHITECTURE)

### Changed
- Renamed from "openvpn-tray" to "Shroud"
- Config path changed from `~/.config/openvpn-tray/` to `~/.config/shroud/`
- Lock file changed from `shroud.lock` to `shroud.lock`
- nftables table changed from `vpn_killswitch` to `shroud_killswitch`
- Binary name changed from `openvpn-tray` to `shroud`

### Security
- Atomic config file writes to prevent corruption
- File permissions set at creation time (0600 for files, 0700 for directories)
- Kill switch rules auditable via `nft list table inet shroud_killswitch`
