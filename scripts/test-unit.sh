#!/usr/bin/env bash
# Run unit tests only
#
# Usage: ./scripts/test-unit.sh [options]
#
# Options:
#   --verbose       Show detailed output
#   --nocapture     Show test output (println!)

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
            # Pass through to cargo test
            ARGS="$ARGS $1"
            shift
            ;;
    esac
done

echo "=== Unit Tests ==="
cargo test --bins --all-features $ARGS
echo "✓ Unit tests passed"
