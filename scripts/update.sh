#!/bin/bash
# Development tool: build, install, and restart shroud
# Moved from `shroud update` CLI command (Principle VIII: One Binary, One Purpose)

set -e

cd "$(dirname "$0")/.."

echo "Building and installing shroud..."
cargo install --path . --force "${@}"

echo "Copying to ~/.local/bin..."
# Atomic binary replacement: copy to temp file then rename.
# This avoids the rm+cp pattern that triggers /proc/self/exe "(deleted)"
# and breaks the restart path. mv on the same filesystem is atomic.
cp ~/.cargo/bin/shroud ~/.local/bin/.shroud.new
chmod 755 ~/.local/bin/.shroud.new
mv ~/.local/bin/.shroud.new ~/.local/bin/shroud

echo "Restarting daemon..."
shroud restart 2>/dev/null || echo "Daemon not running"

echo ""
shroud --version
echo "✓ Update complete"
