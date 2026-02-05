#!/usr/bin/env bash
#
# Desktop E2E Tests under Xvfb
#
# Runs desktop E2E tests with a virtual X server for CI environments.
#
# Usage: ./tests/e2e/desktop/run-desktop-xvfb.sh [OPTIONS]
#
# Options passed through to run-desktop-tests.sh
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$(dirname "$(dirname "$SCRIPT_DIR")")")"

# Check for Xvfb
if ! command -v Xvfb &> /dev/null; then
    echo "Xvfb not found. Installing..."
    if command -v apt-get &> /dev/null; then
        sudo apt-get update && sudo apt-get install -y xvfb
    elif command -v dnf &> /dev/null; then
        sudo dnf install -y xorg-x11-server-Xvfb
    elif command -v pacman &> /dev/null; then
        sudo pacman -S --noconfirm xorg-server-xvfb
    else
        echo "Please install Xvfb manually"
        exit 1
    fi
fi

# Find free display
DISPLAY_NUM=99
while [ -e "/tmp/.X${DISPLAY_NUM}-lock" ]; do
    DISPLAY_NUM=$((DISPLAY_NUM + 1))
done

export DISPLAY=":${DISPLAY_NUM}"

echo "═══════════════════════════════════════════════════════════════"
echo "  DESKTOP E2E TESTS (Xvfb)"
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "Display: $DISPLAY"
echo ""

# Start Xvfb
Xvfb "$DISPLAY" -screen 0 1024x768x24 &
XVFB_PID=$!

cleanup() {
    echo ""
    echo "Stopping Xvfb (PID: $XVFB_PID)..."
    kill "$XVFB_PID" 2>/dev/null || true
    wait "$XVFB_PID" 2>/dev/null || true
}
trap cleanup EXIT

# Wait for Xvfb to start
sleep 1

# Verify Xvfb is running
if ! kill -0 "$XVFB_PID" 2>/dev/null; then
    echo "Failed to start Xvfb"
    exit 1
fi

echo "Xvfb started (PID: $XVFB_PID)"
echo ""

# Run the actual tests
exec "${SCRIPT_DIR}/run-desktop-tests.sh" "$@"
