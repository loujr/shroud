# Shroud

**A provider-agnostic VPN connection manager for Linux.**

[![License: MIT/Apache-2.0](https://img.shields.io/badge/License-MIT%2FApache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

Shroud wraps around NetworkManager and OpenVPN like a protective shroud around a lock mechanism — hardening security without replacing the tools you already have.

---

## Why "Shroud"?

```
┌─────────────────────────────────────────┐
│                                         │
│         ┌───────────┐                   │
│         │  SHROUD   │ ← Protective      │
│         │ ┌───────┐ │    outer casing   │
│         │ │ LOCK  │ │                   │
│         │ │MECHANISM│ ← The vulnerable  │
│         │ └───────┘ │    internals      │
│         └───────────┘                   │
│                                         │
└─────────────────────────────────────────┘
```

A **lock shroud** is the protective metal casing that surrounds a padlock's shackle, preventing tampering. That's exactly what this tool does:

| Lock Shroud | Shroud (This Tool) |
|-------------|-------------------|
| Wraps around the lock | Wraps around NetworkManager + OpenVPN |
| Protects the mechanism | Kill switch protects against leaks |
| Doesn't replace the lock | Doesn't replace NM, works alongside it |
| Hardens against attack | Hardens against connection failures, stale states |

The name has layers:
1. **Concealment** — A VPN shrouds your traffic
2. **Lock hardware** — Protective shell around the mechanism
3. **Architecture** — Surrounds and binds to existing tools

---

## Features

- **Provider-agnostic** — Works with any `.ovpn` config file (NordVPN, Mullvad, ProtonVPN, self-hosted, corporate VPNs)
- **Kill switch** — nftables-based traffic blocking with DNS and IPv6 leak protection
- **Auto-reconnect** — Health monitoring with exponential backoff retry
- **Formal state machine** — Disconnected → Connecting → Connected → Degraded → Reconnecting → Failed
- **Works alongside NetworkManager** — Wraps, doesn't replace (Principle I)
- **System tray integration** — KDE Plasma, GNOME with AppIndicator extension, etc.
- **Configurable via TOML** — All settings persisted across restarts
- **No telemetry** — No phoning home, no analytics (Principle IV)
- **Single binary** — No daemon, no client-server split (Principle VIII)

---

## Screenshots

*Coming soon*

---

## Installation

### Dependencies

```bash
# Arch Linux
sudo pacman -S networkmanager networkmanager-openvpn nftables polkit

# Debian/Ubuntu
sudo apt install network-manager network-manager-openvpn nftables policykit-1

# Fedora
sudo dnf install NetworkManager NetworkManager-openvpn nftables polkit
```

### From Source

```bash
git clone https://github.com/loujr/shroud.git
cd shroud
cargo build --release
cp target/release/shroud ~/.local/bin/
```

### Arch Linux (AUR)

*Coming soon*

---

## Configuration

Shroud stores configuration in `~/.config/shroud/config.toml`:

```toml
# Config version for migration support
version = 1

# Automatically reconnect when VPN drops
auto_reconnect = true

# Last successfully connected server (for quick reconnect)
last_server = "us-east-1"

# Health check interval in seconds (0 to disable)
health_check_interval_secs = 30

# Latency threshold for degraded state (ms)
health_degraded_threshold_ms = 2000

# Maximum reconnection attempts before giving up
max_reconnect_attempts = 10

# Enable kill switch (blocks non-VPN traffic)
kill_switch_enabled = false

# DNS leak protection mode: "tunnel" | "localhost" | "any"
# - tunnel: DNS only via VPN tunnel interfaces (most secure, default)
# - localhost: DNS only to 127.0.0.0/8, ::1 (for local resolvers like systemd-resolved)
# - any: DNS to any destination (legacy, least secure)
dns_mode = "tunnel"

# IPv6 leak protection: "block" | "tunnel" | "off"
# - block: Drop all IPv6 except loopback (most secure, default)
# - tunnel: Allow IPv6 only via VPN tunnel interfaces
# - off: No special IPv6 handling (legacy)
ipv6_mode = "block"
```

---

## Usage

### Importing VPN Configs

First, import your `.ovpn` files into NetworkManager:

```bash
# Import a single config
nmcli connection import type openvpn file /path/to/config.ovpn

# The connection will be named after the file (e.g., "us-east-1")
```

### Starting Shroud

```bash
# Run directly
shroud

# Or with debug logging
RUST_LOG=debug shroud
```

### Systemd User Service

```bash
# Copy the service file
cp systemd/shroud.service ~/.config/systemd/user/

# Enable and start
systemctl --user daemon-reload
systemctl --user enable --now shroud.service

# Check status
systemctl --user status shroud.service

# View logs
journalctl --user -u shroud.service -f
```

### XDG Autostart

```bash
# Copy the desktop file
cp autostart/shroud.desktop ~/.config/autostart/
```

---

## Security

### Kill Switch

When enabled, the kill switch creates nftables rules that:

1. **Allow** loopback traffic
2. **Allow** established/related connections
3. **Allow** traffic through VPN tunnel interfaces (tun*, wg*, tap*)
4. **Allow** traffic to VPN server IPs (for connection establishment)
5. **Allow** local network access (192.168.0.0/16, 10.0.0.0/8, 172.16.0.0/12)
6. **Allow** DHCP for network configuration
7. **Drop** everything else

### DNS Leak Protection

| Mode | Behavior | Use Case |
|------|----------|----------|
| `tunnel` (default) | DNS only through VPN interface | Maximum security |
| `localhost` | DNS to 127.0.0.0/8, ::1, 127.0.0.53 | systemd-resolved, local DNS cache |
| `any` | DNS to any destination | Legacy compatibility (not recommended) |

### IPv6 Leak Protection

| Mode | Behavior | Use Case |
|------|----------|----------|
| `block` (default) | Drop all IPv6 except loopback | Most VPNs don't tunnel IPv6 |
| `tunnel` | IPv6 only through VPN interface | VPN properly tunnels IPv6 |
| `off` | No IPv6 restrictions | Full IPv6 connectivity (may leak) |

### Auditing Rules

You can inspect exactly what Shroud applies:

```bash
# View active kill switch rules
sudo nft list table inet shroud_killswitch

# View all tables
sudo nft list tables
```

**Security through clarity** — if you can't explain what a rule does, it shouldn't exist.

---

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for detailed system design, including:

- Module structure
- State machine transitions
- Data flow diagrams
- Concurrency model
- Key design decisions

---

## Principles

See [PRINCIPLES.md](PRINCIPLES.md) for the core values that guide Shroud's development.

Key highlights:

- **Wrap, Don't Replace** — We enhance NetworkManager, not compete with it
- **Fail Loud, Recover Quiet** — No silent failures, graceful recovery
- **Leave No Trace** — Clean shutdown, no orphaned rules
- **The User Is Not the Enemy** — No telemetry, no phoning home
- **State Is Sacred** — Every transition logged, no ambiguity

---

## Troubleshooting

### Tray Icon Not Appearing

1. Ensure your DE supports StatusNotifierItem (SNI)
2. For GNOME, install the AppIndicator extension
3. Check if `XDG_RUNTIME_DIR` and `DBUS_SESSION_BUS_ADDRESS` are set

### Kill Switch Not Working

1. Verify nftables is installed: `nft --version`
2. Check polkit is running: `systemctl status polkit`
3. Try enabling manually and check for pkexec prompt

### VPN Connection Fails

1. Test with nmcli directly: `nmcli con up "connection-name"`
2. Check NetworkManager logs: `journalctl -u NetworkManager -f`
3. Verify OpenVPN plugin is installed

### Debug Logging

```bash
RUST_LOG=debug shroud 2>&1 | tee shroud.log
```

---

## Contributing

Contributions are welcome! Please read [PRINCIPLES.md](PRINCIPLES.md) first — they guide all design decisions.

Before submitting a PR:

1. `cargo fmt` — Format code
2. `cargo clippy -D warnings` — No warnings
3. `cargo test` — All tests pass
4. Consider: "Does this make Shroud more like a shroud, or more like NordVPN?"

---

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.

---

*Shroud: Wrap your VPN in armor, not bloatware.*
