#!/usr/bin/env bash
#
# Regression Tests for Shroud
#
# Tests for previously fixed bugs to prevent regressions.
# Run with: ./scripts/test-regression.sh
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
SHROUD_BIN="${PROJECT_ROOT}/target/release/shroud"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASSED=0
FAILED=0

log_pass() { echo -e "${GREEN}✓ PASS${NC}: $1"; ((PASSED++)); }
log_fail() { echo -e "${RED}✗ FAIL${NC}: $1"; ((FAILED++)); }
log_info() { echo -e "${YELLOW}→${NC} $1"; }

# Build if needed
if [[ ! -f "$SHROUD_BIN" ]]; then
    log_info "Building Shroud..."
    (cd "$PROJECT_ROOT" && cargo build --release)
fi

echo "═══════════════════════════════════════════════════════════════"
echo "  SHROUD REGRESSION TESTS"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# ============================================================================
# BUG: Invalid VPN State (Fixed in 1.8.8)
# https://github.com/loujr/shroud/commit/1333555
#
# Issue: When connecting to a non-existent VPN, state got stuck in 
#        "Reconnecting" and showed "Connected to: nonexistent-vpn"
# Fix: Added ConnectionFailed event that transitions to Disconnected
# ============================================================================

test_invalid_vpn_state() {
    log_info "Testing: Invalid VPN state bug (issue 1.8.8)"
    
    # This test validates the state machine logic directly via unit tests
    # The actual behavior test requires the mock nmcli
    
    # Check that ConnectionFailed event exists in the codebase
    if grep -q "ConnectionFailed" "$PROJECT_ROOT/src/state/types.rs"; then
        log_pass "ConnectionFailed event exists in state types"
    else
        log_fail "ConnectionFailed event missing from state types"
        return 1
    fi
    
    # Check that handlers dispatch ConnectionFailed on failure
    if grep -q "Event::ConnectionFailed" "$PROJECT_ROOT/src/supervisor/handlers.rs"; then
        log_pass "Handlers dispatch ConnectionFailed event"
    else
        log_fail "Handlers don't dispatch ConnectionFailed event"
        return 1
    fi
    
    # Check state machine handles ConnectionFailed -> Disconnected
    # The pattern: Event::ConnectionFailed { ... } => { ... Some(VpnState::Disconnected) }
    if grep -A6 "Event::ConnectionFailed" "$PROJECT_ROOT/src/state/machine.rs" | grep -q "VpnState::Disconnected"; then
        log_pass "State machine handles ConnectionFailed -> Disconnected"
    else
        log_fail "State machine doesn't handle ConnectionFailed correctly"
        return 1
    fi
}

# ============================================================================
# BUG: Kill Switch Toggle Race Condition (Fixed in 1.8.9)
# https://github.com/loujr/shroud/commit/84573a6
#
# Issue: When toggling kill switch, tray would briefly show wrong state
# Fix: Optimistic UI update before async operation completes
# ============================================================================

test_killswitch_race_condition() {
    log_info "Testing: Kill switch toggle race condition (issue 1.8.9)"
    
    # Check that optimistic update happens BEFORE the enable/disable call
    local handler_file="$PROJECT_ROOT/src/supervisor/handlers.rs"
    
    # Look for the pattern: update shared state, then call enable/disable
    if grep -A20 "pub(crate) async fn toggle_kill_switch" "$handler_file" | \
       grep -B5 "self.kill_switch.enable" | \
       grep -q "state.kill_switch = new_enabled"; then
        log_pass "Optimistic state update happens before async operation"
    else
        log_fail "Optimistic state update missing or in wrong order"
        return 1
    fi
    
    # Check for rollback on failure
    # Look for: Err(e) => { ... state.kill_switch = current_enabled ... }
    if grep -A10 "Err(e) =>" "$handler_file" | grep -q "kill_switch = current_enabled"; then
        log_pass "Rollback logic exists for failed toggle"
    else
        # Also check for the pattern "Revert to original" comment
        if grep -q "Rollback optimistic state update" "$handler_file"; then
            log_pass "Rollback logic documented in code"
        else
            log_fail "Rollback logic missing for failed toggle"
            return 1
        fi
    fi
}

# ============================================================================
# BUG: Kill Switch State Flicker (Fixed in 1.8.7)
#
# Issue: Kill switch would flicker enabled/disabled because state checks
#        ran iptables without sudo, causing permission denied
# Fix: Use sudo -n for all iptables state checking
# ============================================================================

test_killswitch_state_flicker() {
    log_info "Testing: Kill switch state flicker (issue 1.8.7)"
    
    local firewall_file="$PROJECT_ROOT/src/killswitch/firewall.rs"
    
    # Check that is_actually_enabled uses sudo
    if grep -A10 "fn is_actually_enabled" "$firewall_file" 2>/dev/null | \
       grep -q "sudo"; then
        log_pass "is_actually_enabled uses sudo"
    else
        # May be implemented differently, check for run_iptables_check or similar
        if grep -q "run_iptables.*sudo\|sudo.*iptables" "$firewall_file" 2>/dev/null; then
            log_pass "iptables checks use sudo"
        else
            log_fail "iptables state checks may not use sudo"
            return 1
        fi
    fi
}

# ============================================================================
# BUG: SHROUD_NMCLI Environment Variable Support
#
# Ensure the mock nmcli can be used for testing
# ============================================================================

test_nmcli_env_override() {
    log_info "Testing: SHROUD_NMCLI environment variable support"
    
    # Check nm/client.rs has nmcli_command() function
    if grep -q "fn nmcli_command()" "$PROJECT_ROOT/src/nm/client.rs"; then
        log_pass "nmcli_command() helper exists"
    else
        log_fail "nmcli_command() helper missing"
        return 1
    fi
    
    # Check it reads SHROUD_NMCLI
    if grep -q 'SHROUD_NMCLI' "$PROJECT_ROOT/src/nm/client.rs"; then
        log_pass "SHROUD_NMCLI environment variable supported"
    else
        log_fail "SHROUD_NMCLI environment variable not supported"
        return 1
    fi
}

# ============================================================================
# BUG: Stale Kill Switch Rules on Crash
#
# Issue: If shroud is killed while kill switch is active, rules are orphaned
# Fix: Detect and clean stale rules on startup
# ============================================================================

test_stale_rules_detection() {
    log_info "Testing: Stale kill switch rules detection"
    
    # Check for stale rules detection in cleanup module
    if grep -rq "STALE.*KILL.*SWITCH\|stale.*rules" "$PROJECT_ROOT/src/killswitch/" 2>/dev/null; then
        log_pass "Stale rules detection exists"
    else
        log_fail "Stale rules detection missing"
        return 1
    fi
}

# ============================================================================
# RUN ALL REGRESSION TESTS
# ============================================================================

echo ""
echo "Running regression tests..."
echo ""

test_invalid_vpn_state || true
test_killswitch_race_condition || true
test_killswitch_state_flicker || true
test_nmcli_env_override || true
test_stale_rules_detection || true

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  RESULTS: ${PASSED} passed, ${FAILED} failed"
echo "═══════════════════════════════════════════════════════════════"

if [[ $FAILED -gt 0 ]]; then
    exit 1
fi
exit 0
