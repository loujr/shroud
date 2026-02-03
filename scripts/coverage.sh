#!/usr/bin/env bash
# Generate code coverage report
#
# Usage: ./scripts/coverage.sh [options]
#
# Options:
#   --html          Generate HTML report
#   --open          Open HTML report in browser

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

OPEN_REPORT=false
HTML_ONLY=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --html)
            HTML_ONLY=true
            shift
            ;;
        --open)
            OPEN_REPORT=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Check for tarpaulin
if ! command -v cargo-tarpaulin &> /dev/null; then
    echo "Installing cargo-tarpaulin..."
    cargo install cargo-tarpaulin --locked
fi

echo "=== Generating Coverage Report ==="

mkdir -p coverage

OUTPUT_ARGS="--out html --output-dir coverage"
if ! $HTML_ONLY; then
    OUTPUT_ARGS="$OUTPUT_ARGS --out xml"
fi

cargo tarpaulin \
    --verbose \
    --all-features \
    --workspace \
    --timeout 300 \
    $OUTPUT_ARGS \
    --skip-clean \
    --engine llvm \
    || echo "Tarpaulin completed with warnings"

echo ""
echo "✓ Coverage report generated in coverage/"

if $OPEN_REPORT; then
    if [[ -f coverage/tarpaulin-report.html ]]; then
        xdg-open coverage/tarpaulin-report.html 2>/dev/null || open coverage/tarpaulin-report.html 2>/dev/null || true
    fi
fi
