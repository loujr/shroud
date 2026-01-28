#!/bin/bash
set -e

# Shroud Deployment Script
# Usage: ./update.sh

CYAN='\033[0;36m'
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${CYAN}=== Shroud Deployment Script ===${NC}"

# 1. Build release binary
echo -e "${CYAN}Building release binary...${NC}"
cargo build --release

# 2. Stop daemon if running
echo -e "${CYAN}Stopping running daemon...${NC}"
pkill -f shroud || true
sleep 1
# Ensure socket is gone
rm -f /run/user/$(id -u)/shroud.sock

# 3. Clean up any stuck firewall rules (Safety net)
echo -e "${CYAN}Cleaning firewall rules (requires sudo)...${NC}"
sudo iptables -D OUTPUT -j SHROUD_KILLSWITCH 2>/dev/null || true
sudo iptables -F SHROUD_KILLSWITCH 2>/dev/null || true
sudo iptables -X SHROUD_KILLSWITCH 2>/dev/null || true
sudo nft delete table inet shroud_killswitch 2>/dev/null || true

# 4. Install binary (copy to Cargo bin)
echo -e "${CYAN}Installing binary...${NC}"
# We manually copy to avoid 'cargo install' recompiling or network checks
BINARY_PATH="$HOME/.cargo/bin/shroud"
mkdir -p "$(dirname "$BINARY_PATH")"
cp target/release/shroud "$BINARY_PATH"
echo -e "${GREEN}Installed to $BINARY_PATH${NC}"

# 5. Restart Daemon (via systemd or manual)
if systemctl --user is-active --quiet shroud; then
    echo -e "${CYAN}Restarting systemd service...${NC}"
    systemctl --user restart shroud
    echo -e "${GREEN}Service restarted.${NC}"
else
    echo -e "${CYAN}Starting daemon in background...${NC}"
    "$BINARY_PATH" &
    # Wait for socket
    for i in {1..5}; do
        if [ -S "/run/user/$(id -u)/shroud.sock" ]; then
            echo -e "${GREEN}Daemon started successfully.${NC}"
            break
        fi
        sleep 1
    done
fi

# 6. Verify version
echo -e "${CYAN}Deployed version:${NC}"
"$BINARY_PATH" --version

echo -e "${GREEN}Deployment Complete!${NC}"
echo -e "Try: shroud killswitch on"
