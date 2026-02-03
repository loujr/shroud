#!/usr/bin/env bash
# Run E2E tests
#
# Usage: ./scripts/test-e2e.sh [options]
#
# Options:
#   --desktop       Run desktop tests only
#   --headless      Run headless tests only
#   --privileged    Include privileged tests (requires sudo)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

MODE="all"
PRIVILEGED=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --desktop)
            MODE="desktop"
            shift
            ;;
        --headless)
            MODE="headless"
            shift
            ;;
        --privileged)
            PRIVILEGED="--privileged"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Build release binary first
echo "=== Building Release Binary ==="
cargo build --release
echo ""

echo "=== E2E Tests ==="

case $MODE in
    desktop)
        ./tests/e2e/desktop/run-desktop-tests.sh $PRIVILEGED
        ;;
    headless)
        ./tests/e2e/headless/run-tests.sh $PRIVILEGED
        ;;
    all)
        ./tests/e2e/run-all.sh $PRIVILEGED
        ;;
esac

echo "✓ E2E tests completed"
