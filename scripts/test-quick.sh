#!/usr/bin/env bash
# Quick test for development iteration
# Runs only unit tests for fast feedback
#
# Usage: ./scripts/test-quick.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

echo "=== Quick Test ==="
echo ""

# Run unit tests with maximum parallelism
cargo test --bins --all-features -- --test-threads=4

echo ""
echo "✓ Quick tests passed"
