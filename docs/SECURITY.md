# Security Policy

You found a vulnerability. Thank you. Here's what to do.

---

## Reporting

**Don't open a public issue.** Security vulnerabilities need private disclosure.

### Preferred: GitHub Security Advisories

1. Go to the Security tab on the repository
2. Click "Report a vulnerability"
3. Fill in the details

### Alternative: Direct Contact

If advisories aren't available, contact the maintainers directly through GitHub.

---

## What to Include

The more detail, the faster we can fix it:

- **Description** — What's the vulnerability?
- **Impact** — What can an attacker do?
- **Reproduction** — Step-by-step instructions
- **Affected versions** — Which versions are vulnerable?
- **Proof of concept** — Code or commands that demonstrate it (if safe to share)

---

## Response Timeline

| Step | Target |
|------|--------|
| Acknowledge receipt | 72 hours |
| Initial assessment | 1 week |
| Fix or mitigation plan | 2 weeks |
| Public disclosure | After fix is available |

We'll keep you updated on progress.

---

## Supported Versions

Security fixes are provided for the latest released version.

Older versions don't receive updates. If you're on an old version and a security issue is found, update.

---

## Dependency Audits

We use `cargo audit` to check dependencies against the RustSec Advisory Database:

```bash
./scripts/audit.sh

# Or via CLI
shroud audit
```

This runs in CI. If a vulnerable dependency is found:

1. We assess the actual risk (not all advisories apply to all use cases)
2. We document the risk if we can't update immediately
3. We prioritize a fix in the next release

---

## Security Model

Shroud's security assumptions:

| Trust | Don't Trust |
|-------|-------------|
| The local user | Remote networks |
| NetworkManager | VPN server contents |
| The kernel | D-Bus messages (validated) |
| iptables/nftables | User-provided config (validated) |

### Kill Switch Privileges

The kill switch requires root for iptables. We use sudoers rules that only allow specific commands:

```
%wheel ALL=(ALL) NOPASSWD: /usr/bin/iptables, /usr/bin/ip6tables, ...
```

This limits the attack surface. If someone compromises Shroud, they can manipulate firewall rules but not run arbitrary commands as root.

### No Network Communication

Shroud doesn't phone home. No telemetry. No update checks. No analytics.

The only network communication is:
1. VPN connection (through NetworkManager)
2. Health check pings (to verify tunnel works)

Both are initiated by the user.

---

## Threat Model

### In Scope

- VPN traffic leaks (IP, DNS, IPv6)
- Kill switch bypass
- Privilege escalation via sudoers rule
- State machine manipulation
- Config injection

### Out of Scope

- Attacks requiring root access (you already lost)
- Attacks on NetworkManager or iptables themselves
- Physical access attacks
- VPN protocol vulnerabilities (we just wrap, we don't implement)

---

## The Philosophy

Security through clarity.

Every firewall rule should be auditable. Every design decision should be explainable. If users can't understand what Shroud is doing, they won't trust it.

We'd rather have fewer features that we're confident in than more features with hidden risks.
