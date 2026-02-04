# Gateway Mode

Turn your Shroud machine into a VPN router. One connection protects your whole network.

---

## The Idea

```
┌────────────────────────────────────────────────────────────────────┐
│                          YOUR NETWORK                              │
├────────────────────────────────────────────────────────────────────┤
│                                                                    │
│     Phone ─────┐                                                   │
│     Laptop ────┼────► Shroud Gateway ────► VPN ────► Internet     │
│     Smart TV ──┤                                                   │
│     IoT Junk ──┘                                                   │
│                                                                    │
│     One VPN connection. Everything protected.                      │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

Your Smart TV can't run a VPN client. Your gaming console doesn't support WireGuard. That IoT thermostat definitely isn't getting a VPN app.

But they can all route through a gateway.

---

## What You Get

- **Protect everything** — Devices that can't run VPNs get protected anyway
- **One connection** — One VPN subscription, whole network covered
- **Kill switch for all** — When VPN drops, forwarded traffic stops too
- **No per-device setup** — Point devices at the gateway and forget

---

## Quick Start

### 1. Install Shroud in Headless Mode

Gateway mode works best on a dedicated machine (Raspberry Pi, old laptop, home server):

```bash
sudo ./setup.sh --headless
```

### 2. Connect to VPN

```bash
shroud connect your-vpn-name
```

### 3. Enable Gateway

```bash
shroud gateway on
```

### 4. Point Devices at It

On each device, set the default gateway to your Shroud machine's IP:

```bash
# Example: Shroud is at 192.168.1.10
# On a Linux client:
sudo ip route replace default via 192.168.1.10
```

Or configure your router's DHCP to hand out the Shroud machine as the gateway. Then it's automatic for everything.

---

## How It Works

Under the hood, gateway mode does three things:

1. **Enables IP forwarding** — The Linux kernel forwards packets between interfaces
2. **Sets up NAT** — Source IP rewritten so the VPN server can reply
3. **Adds kill switch for FORWARD chain** — No VPN = no forwarding

```
┌─────────────────────────────────────────────────────────────────┐
│                       GATEWAY INTERNALS                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   Client (192.168.1.50)                                        │
│          │                                                      │
│          ▼                                                      │
│   ┌─────────────┐                                              │
│   │   eth0      │ ← LAN interface                              │
│   └─────────────┘                                              │
│          │                                                      │
│          │ IP forwarding                                        │
│          ▼                                                      │
│   ┌─────────────┐                                              │
│   │   NAT       │ ← Rewrite source IP                          │
│   └─────────────┘                                              │
│          │                                                      │
│          ▼                                                      │
│   ┌─────────────┐                                              │
│   │   tun0      │ ← VPN tunnel                                 │
│   └─────────────┘                                              │
│          │                                                      │
│          ▼                                                      │
│      Internet (through VPN)                                     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Configuration

Gateway options live in the `[gateway]` section:

```toml
[gateway]
# Enable gateway on startup
enabled = true

# LAN interface (auto-detected if not set)
lan_interface = "eth0"

# Who can use the gateway
# Options: "all", "192.168.1.0/24", or ["192.168.1.50", "192.168.1.51"]
allowed_clients = "all"

# Block forwarded traffic if VPN drops
kill_switch_forwarding = true

# Keep IP forwarding after Shroud exits
persist_ip_forward = false

# Route IPv6 (disabled by default — most VPNs don't tunnel it)
enable_ipv6 = false
```

### Options Explained

| Option | Default | What It Does |
|--------|---------|--------------|
| `enabled` | `false` | Start gateway automatically |
| `lan_interface` | auto | Which interface faces your LAN |
| `allowed_clients` | `"all"` | Restrict who can use the gateway |
| `kill_switch_forwarding` | `true` | Block forwarded traffic when VPN drops |
| `persist_ip_forward` | `false` | Keep kernel forwarding on after exit |
| `enable_ipv6` | `false` | Forward IPv6 (risky if VPN doesn't tunnel it) |

---

## CLI Commands

```bash
shroud gateway on       # Enable gateway mode
shroud gateway off      # Disable gateway mode
shroud gateway status   # Check what's happening

# Aliases work too
shroud gw on
shroud gw status
```

### Status Output

```
Gateway Status
==============

Gateway:           ✓ enabled
IP Forwarding:     ✓ enabled
Forward Kill SW:   ✓ active

LAN Interface
─────────────
  Interface:       eth0
  IP Address:      192.168.1.10
  Subnet:          192.168.1.0/24

VPN Interface
─────────────
  Interface:       tun0
  IP Address:      10.8.0.2
```

---

## Client Configuration

### Option A: Per-Device Route

Set the gateway manually on each device:

**Linux:**
```bash
sudo ip route replace default via 192.168.1.10
```

**macOS:**
```bash
sudo route delete default
sudo route add default 192.168.1.10
```

**Windows (Admin PowerShell):**
```powershell
route delete 0.0.0.0
route add 0.0.0.0 mask 0.0.0.0 192.168.1.10
```

### Option B: Router DHCP

Configure your router to give out the Shroud machine as the default gateway via DHCP options. Then every device on the network uses it automatically.

### Option C: Replace Your Router

Put the Shroud machine between your router and your network. All traffic flows through it.

---

## Troubleshooting

### Gateway won't enable

```bash
# Is VPN connected?
shroud status

# Gateway requires an active VPN
shroud connect your-vpn
shroud gateway on
```

### Clients can't reach the internet

```bash
# Check gateway status
shroud gateway status

# Is IP forwarding on?
cat /proc/sys/net/ipv4/ip_forward  # Should be 1

# Are NAT rules in place?
sudo iptables -t nat -L POSTROUTING -n -v

# Are FORWARD rules correct?
sudo iptables -L FORWARD -n -v
```

### Traffic leaks when VPN drops

```bash
# Is forwarding kill switch active?
shroud gateway status

# Check the chain
sudo iptables -L SHROUD_GATEWAY_KS -n -v
```

### IPv6 leaks

IPv6 forwarding is disabled by default because most VPNs don't tunnel it. If you enable it, make sure your VPN actually supports IPv6.

```toml
[gateway]
enable_ipv6 = false  # Leave this false unless you're sure
```

---

## Use Cases

### Home Network VPN

Protect everything in your house without installing apps on each device:
- Smart TVs
- Gaming consoles
- IoT gadgets
- Guest phones

### Small Office

One VPN connection for the whole office. Central management. Cost savings.

### Travel Router

A Raspberry Pi running Shroud in gateway mode becomes a portable VPN router. Plug it into hotel WiFi, connect your devices to it, everything's protected.

---

## Security Notes

1. **Keep kill_switch_forwarding enabled** — Otherwise, devices leak when VPN drops
2. **Keep enable_ipv6 disabled** — Unless your VPN tunnels IPv6 properly
3. **Use allowed_clients** — If you don't want everyone on the network routing through
4. **DNS matters** — Clients should use DNS through the VPN, not their own resolvers
5. **Audit regularly** — `shroud gateway status` shows you the rules

---

## The Philosophy

Some devices can't protect themselves. They're locked down, proprietary, or just too dumb to run a VPN client.

Gateway mode extends your protection to them. One machine, one VPN, whole network covered.

It's the same philosophy as everything else in Shroud: we wrap existing tools, we protect what's already there, and we get out of the way.
