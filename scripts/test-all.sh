#!/usr/bin/env bash
# Run all tests (format, lint, unit, integration, security)
#
# Usage: ./scripts/test-all.sh [options]
#
# Options:
#   --privileged    Include privileged tests (requires sudo)
#   --e2e           Also run E2E tests
#   --verbose       Show detailed output

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Options
RUN_PRIVILEGED=false
RUN_E2E=false
VERBOSE=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --privileged)
            RUN_PRIVILEGED=true
            shift
            ;;
        --e2e)
            RUN_E2E=true
            shift
            ;;
        --verbose)
            VERBOSE="--verbose"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

cd "$PROJECT_ROOT"

echo ""
echo -e "${BLUE}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║                 Shroud Test Suite                          ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Stage 1: Quick checks
echo -e "${BLUE}→ Stage 1: Quick Checks${NC}"

echo -e "  ${YELLOW}Format check...${NC}"
if ! cargo fmt --all --check; then
    echo -e "  ${RED}✗ Format check failed${NC}"
    echo -e "  Run 'cargo fmt' to fix"
    exit 1
fi
echo -e "  ${GREEN}✓ Format OK${NC}"

echo -e "  ${YELLOW}Clippy lints...${NC}"
if ! cargo clippy --all-targets --all-features -- -D warnings 2>&1 | tail -5; then
    echo -e "  ${RED}✗ Clippy found issues${NC}"
    exit 1
fi
echo -e "  ${GREEN}✓ Clippy OK${NC}"

# Stage 2: Unit tests
echo ""
echo -e "${BLUE}→ Stage 2: Unit Tests${NC}"
if ! cargo test --bins --all-features $VERBOSE; then
    echo -e "${RED}✗ Unit tests failed${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Unit tests passed${NC}"

# Stage 3: Integration tests
echo ""
echo -e "${BLUE}→ Stage 3: Integration Tests${NC}"
if ! cargo test --test integration --all-features $VERBOSE; then
    echo -e "${RED}✗ Integration tests failed${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Integration tests passed${NC}"

# Stage 4: Security tests (non-privileged)
echo ""
echo -e "${BLUE}→ Stage 4: Security Tests (non-privileged)${NC}"
if ! cargo test --test security --all-features $VERBOSE; then
    echo -e "${RED}✗ Security tests failed${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Security tests passed${NC}"

# Privileged tests
if $RUN_PRIVILEGED; then
    echo ""
    echo -e "${BLUE}→ Stage 5: Security Tests (privileged)${NC}"
    if [[ $EUID -ne 0 ]]; then
        echo -e "${YELLOW}Re-running with sudo...${NC}"
        sudo -E cargo test --test security --all-features -- --ignored $VERBOSE
    else
        cargo test --test security --all-features -- --ignored $VERBOSE
    fi
fi

# E2E tests
if $RUN_E2E; then
    echo ""
    echo -e "${BLUE}→ Stage 6: E2E Tests${NC}"
    
    echo -e "  Building release binary..."
    cargo build --release
    
    if $RUN_PRIVILEGED; then
        "${PROJECT_ROOT}/tests/e2e/run-all.sh" --privileged
    else
        "${PROJECT_ROOT}/tests/e2e/run-all.sh"
    fi
fi

echo ""
echo -e "${GREEN}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║               All Tests Passed! ✓                          ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""
