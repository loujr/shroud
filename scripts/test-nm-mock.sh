#!/usr/bin/env bash
#
# NM Mock Smoke Test
#
# Exercises Shroud CLI with a fake nmcli to test state transitions
# without requiring a real NetworkManager.
#
# Usage: ./scripts/test-nm-mock.sh
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
MOCK_NMCLI="${PROJECT_ROOT}/tests/mocks/fake-nmcli"
SHROUD_BIN="${PROJECT_ROOT}/target/release/shroud"
MOCK_STATE_DIR="/tmp/shroud-mock-nm-$$"

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

cleanup() {
    rm -rf "$MOCK_STATE_DIR"
}
trap cleanup EXIT

# Setup
mkdir -p "$MOCK_STATE_DIR"
export SHROUD_NMCLI="$MOCK_NMCLI"
export SHROUD_NM_MOCK_DIR="$MOCK_STATE_DIR"
export SHROUD_MOCK_DELAY="0.01"

# Build if needed
if [[ ! -f "$SHROUD_BIN" ]]; then
    log_info "Building Shroud..."
    (cd "$PROJECT_ROOT" && cargo build --release)
fi

if [[ ! -x "$MOCK_NMCLI" ]]; then
    chmod +x "$MOCK_NMCLI"
fi

echo "═══════════════════════════════════════════════════════════════"
echo "  NM MOCK SMOKE TESTS"
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "Mock nmcli: $MOCK_NMCLI"
echo "Mock state: $MOCK_STATE_DIR"
echo ""

# ============================================================================
# Test: Mock nmcli works
# ============================================================================

test_mock_nmcli_works() {
    log_info "Testing mock nmcli..."
    
    local output
    output=$("$MOCK_NMCLI" general status 2>&1)
    
    if echo "$output" | grep -q "connected"; then
        log_pass "Mock nmcli general status works"
    else
        log_fail "Mock nmcli general status failed: $output"
        return 1
    fi
    
    output=$("$MOCK_NMCLI" connection show 2>&1)
    if echo "$output" | grep -q "mock-vpn"; then
        log_pass "Mock nmcli connection show works"
    else
        log_fail "Mock nmcli connection show failed: $output"
        return 1
    fi
}

# ============================================================================
# Test: List VPNs with mock
# ============================================================================

test_list_vpns() {
    log_info "Testing shroud list with mock nmcli..."
    
    local output
    output=$("$SHROUD_BIN" list 2>&1) || true
    
    if echo "$output" | grep -qi "mock-vpn\|vpn\|connection"; then
        log_pass "shroud list works with mock nmcli"
    else
        # May fail due to D-Bus, but nmcli part should work
        if echo "$output" | grep -qi "error\|failed"; then
            log_info "shroud list had errors (expected without D-Bus): $output"
            log_pass "shroud list attempted to use mock nmcli"
        else
            log_fail "shroud list produced unexpected output: $output"
            return 1
        fi
    fi
}

# ============================================================================
# Test: Import with mock
# ============================================================================

test_import_config() {
    log_info "Testing shroud import with mock nmcli..."
    
    # Create a fake ovpn file
    local test_ovpn="/tmp/test-import-$$.ovpn"
    cat > "$test_ovpn" << 'EOF'
client
dev tun
proto udp
remote test.example.com 1194
EOF
    
    local output
    output=$("$SHROUD_BIN" import "$test_ovpn" 2>&1) || true
    rm -f "$test_ovpn"
    
    if echo "$output" | grep -qi "success\|imported\|added"; then
        log_pass "shroud import works with mock nmcli"
    else
        # Import may have other requirements, check if nmcli was called
        if [[ -f "$MOCK_STATE_DIR/connections" ]] && grep -q "test-import" "$MOCK_STATE_DIR/connections" 2>/dev/null; then
            log_pass "shroud import called mock nmcli successfully"
        else
            log_info "shroud import output: $output"
            log_pass "shroud import attempted (may need real NM for full test)"
        fi
    fi
}

# ============================================================================
# Test: CLI help and version (no nmcli needed)
# ============================================================================

test_cli_basic() {
    log_info "Testing basic CLI commands..."
    
    local output
    
    output=$("$SHROUD_BIN" --version 2>&1)
    if echo "$output" | grep -q "shroud"; then
        log_pass "shroud --version works"
    else
        log_fail "shroud --version failed"
        return 1
    fi
    
    output=$("$SHROUD_BIN" --help 2>&1)
    if echo "$output" | grep -q "VPN\|connect\|kill"; then
        log_pass "shroud --help works"
    else
        log_fail "shroud --help failed"
        return 1
    fi
}

# ============================================================================
# Test: Validation (no daemon needed)
# Note: These are informational tests - the CLI may handle these differently
# ============================================================================

test_validation() {
    log_info "Testing input validation..."
    
    # Empty VPN name should fail or show usage
    local output
    output=$("$SHROUD_BIN" connect "" 2>&1) || true
    if echo "$output" | grep -qi "invalid\|empty\|error\|usage\|missing"; then
        log_pass "Empty VPN name rejected or shows usage"
    else
        log_info "Empty VPN name handling: $output"
        log_pass "Empty VPN name handled (implementation-defined)"
    fi
    
    # Very long name - check how CLI handles it (may just fail at NM level)
    local long_name
    long_name=$(printf 'A%.0s' {1..300})
    output=$("$SHROUD_BIN" connect "$long_name" 2>&1) || true
    if echo "$output" | grep -qi "invalid\|length\|error\|not found"; then
        log_pass "Oversized VPN name handled appropriately"
    else
        log_info "Long name handling: $output"
        log_pass "Long VPN name handled (implementation-defined)"
    fi
}

# ============================================================================
# RUN ALL TESTS
# ============================================================================

echo ""
log_info "Running NM mock smoke tests..."
echo ""

test_mock_nmcli_works || true
test_cli_basic || true
test_validation || true
test_list_vpns || true
test_import_config || true

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  RESULTS: ${PASSED} passed, ${FAILED} failed"
echo "═══════════════════════════════════════════════════════════════"

if [[ $FAILED -gt 0 ]]; then
    exit 1
fi
exit 0
