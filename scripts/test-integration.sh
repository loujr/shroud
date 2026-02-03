#!/usr/bin/env bash
# Run integration tests
#
# Usage: ./scripts/test-integration.sh [options]
#
# Options:
#   --verbose       Show detailed output
#   --nocapture     Show test output

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

ARGS=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --verbose)
            ARGS="$ARGS --verbose"
            shift
            ;;
        --nocapture)
            ARGS="$ARGS -- --nocapture"
            shift
            ;;
        *)
            ARGS="$ARGS $1"
            shift
            ;;
    esac
done

echo "=== Integration Tests ==="
cargo test --test integration --all-features $ARGS
echo "✓ Integration tests passed"
