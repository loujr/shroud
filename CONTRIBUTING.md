# Contributing to Shroud

Thank you for your interest in contributing to Shroud! This document provides guidelines for contributors.

## Before You Start

**Read [PRINCIPLES.md](docs/PRINCIPLES.md) first.** Every contribution must align with Shroud's core principles:

| Principle | Summary |
|-----------|---------|
| I. Wrap, Don't Replace | We enhance NetworkManager, not replace it |
| II. Fail Loud, Recover Quiet | Errors visible, recovery graceful |
| III. Leave No Trace | Clean up all firewall rules on exit |
| IV. The User Is Not the Enemy | No telemetry, no phoning home |
| V. Complexity Is Debt | Every dependency must justify itself |
| VI. Speak the System's Language | Use systemd, D-Bus, XDG natively |
| VII. State Is Sacred | State machine is the source of truth |
| VIII. One Binary, One Purpose | Single binary for daemon and CLI |
| IX. Respect the Disconnect | Don't auto-connect without permission |
| X. Built for the Quiet Majority | Just works, no wiki required |
| XI. Security Through Clarity | Auditable rules, explainable behavior |
| XII. We Ship, Then Improve | Working code today beats perfect code never |

## Development Setup

### Prerequisites

**Arch Linux:**
```bash
sudo pacman -S networkmanager networkmanager-openvpn networkmanager-wireguard \
    iptables nftables rust cargo
```

**Debian/Ubuntu:**
```bash
sudo apt install network-manager network-manager-openvpn network-manager-wireguard \
    iptables nftables rustc cargo
```

**Fedora:**
```bash
sudo dnf install NetworkManager NetworkManager-openvpn NetworkManager-wireguard \
    iptables nftables rust cargo
```

### Build and Run

```bash
# Clone
git clone https://github.com/loujr/shroud.git
cd shroud

# Build
cargo build

# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run

# Build release
cargo build --release
```

## Code Quality Requirements

All contributions must pass these checks:

```bash
# Format code
cargo fmt

# No clippy warnings (required)
cargo clippy -- -D warnings

# All tests pass (required)
cargo test

# Security audit (recommended)
cargo audit
```

## Pull Request Process

1. **Fork** the repository
2. **Create a branch** from `main`:
   ```bash
   git checkout -b fix/issue-description
   # or
   git checkout -b feat/feature-name
   ```
3. **Make changes** following code style guidelines
4. **Add tests** for new functionality
5. **Update documentation** if behavior changes
6. **Run quality checks**:
   ```bash
   cargo fmt && cargo clippy -- -D warnings && cargo test
   ```
7. **Commit** with clear messages:
   ```
   fix: Correct kill switch cleanup on SIGTERM
   
   The cleanup handler was not being called when receiving SIGTERM
   in headless mode. Added signal handler registration in runtime.rs.
   
   Fixes #123
   ```
8. **Push** and create a Pull Request

## Commit Message Format

```
type: Short description (50 chars max)

Longer explanation if needed. Wrap at 72 characters.
Explain what and why, not how.

Fixes #123
```

**Types:**
- `fix` — Bug fixes
- `feat` — New features
- `docs` — Documentation only
- `refactor` — Code change that neither fixes nor adds
- `test` — Adding or updating tests
- `chore` — Build, CI, or tooling changes

## What We're Looking For

### ✅ Great Contributions

- Bug fixes with test coverage
- Documentation improvements and typo fixes
- Performance optimizations (with benchmarks)
- Cross-distro compatibility fixes
- Accessibility improvements
- Security hardening

### 💬 Discuss First

Open an issue before implementing:

- New CLI commands
- New configuration options
- New dependencies
- Architectural changes
- Changes to the state machine

### ❌ Out of Scope

These are intentionally not supported (see PRINCIPLES.md):

- **Cross-platform (macOS, Windows)** — We're Linux-focused (Principle VI)
- **GUI beyond tray icon** — CLI-first tool (Principle VIII)
- **Built-in VPN protocols** — We wrap existing tools (Principle I)
- **Telemetry or analytics** — No phoning home (Principle IV)
- **Auto-update mechanism** — User controls updates (Principle IV)

## Testing

### Unit Tests

```bash
cargo test
```

### E2E Tests

```bash
# Non-privileged tests
./tests/e2e/run-all.sh

# Privileged tests (requires sudo)
sudo ./tests/e2e/run-all.sh --privileged
```

### Manual Testing Checklist

Before submitting a PR that affects core functionality:

- [ ] Desktop mode: Tray icon appears and responds
- [ ] `shroud connect <name>` connects successfully
- [ ] `shroud disconnect` disconnects cleanly
- [ ] `shroud ks on` enables kill switch
- [ ] `shroud ks off` disables and removes rules
- [ ] Kill switch survives VPN reconnect
- [ ] Clean shutdown (`shroud quit`) removes all rules
- [ ] Crash recovery: Rules cleaned on next start
- [ ] Works on target distros (Arch, Debian, Fedora)

### Headless/Gateway Testing

For changes to headless or gateway mode:

- [ ] `shroud --headless` starts without display
- [ ] Systemd service starts and reports ready
- [ ] Auto-connect works on startup
- [ ] Gateway mode forwards traffic correctly
- [ ] Gateway kill switch blocks leaks

## Code Style

### General

- Follow `rustfmt` conventions (run `cargo fmt`)
- Use meaningful variable and function names
- Prefer explicit over implicit
- Handle all errors; no `unwrap()` in production code
- Comment "why", not "what"

### Error Handling

```rust
// ✅ Good: Explicit error handling
match connection.activate().await {
    Ok(()) => log::info!("Connected"),
    Err(e) => {
        log::error!("Connection failed: {}", e);
        return Err(e.into());
    }
}

// ❌ Bad: Silent failure
let _ = connection.activate().await;

// ❌ Bad: Panic in library code
connection.activate().await.unwrap();
```

### Logging

```rust
// Use appropriate log levels
log::error!("Kill switch failed: {}", e);     // User must know
log::warn!("Retrying connection...");          // User might care
log::info!("Connected to {}", server);         // Normal operation
log::debug!("Sending ping to {}", addr);       // Development
log::trace!("Raw packet: {:?}", bytes);        // Deep debugging
```

### Documentation

```rust
/// Brief one-line description.
///
/// Longer description if needed, explaining behavior,
/// edge cases, and any important notes.
///
/// # Arguments
///
/// * `server` - The VPN server name to connect to
///
/// # Returns
///
/// Returns `Ok(())` on successful connection, or an error
/// if the connection could not be established.
///
/// # Errors
///
/// Returns `ConnectionError::NotFound` if the server doesn't exist.
/// Returns `ConnectionError::Timeout` if connection times out.
pub async fn connect(&mut self, server: &str) -> Result<(), ConnectionError> {
    // ...
}
```

## Questions?

- **Bug?** Open an issue with reproduction steps
- **Feature idea?** Open an issue to discuss first
- **Question?** Check existing issues or open a new one

Please be respectful and constructive in all interactions.

---

*Thank you for helping make Shroud better!*
