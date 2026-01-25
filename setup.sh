#!/bin/bash
# setup.sh - Native Arch Linux setup/update script for Shroud VPN Manager
#
# This script:
# 1. Installs system dependencies via pacman
# 2. Builds the Rust binary in release mode
# 3. Installs the binary to ~/.local/bin
# 4. Sets up systemd user service
# 5. Configures XDG autostart
#
# Usage: ./setup.sh

set -e

BINARY_NAME="shroud"
INSTALL_DIR="$HOME/.local/bin"
SYSTEMD_USER_DIR="$HOME/.config/systemd/user"
SERVICE_NAME="shroud.service"
AUTOSTART_DIR="$HOME/.config/autostart"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Check if running on Arch Linux
check_arch() {
    if [ ! -f /etc/arch-release ]; then
        echo -e "${YELLOW}Warning: This script is designed for Arch Linux.${NC}"
        echo -e "${YELLOW}You may need to adapt package names for your distribution.${NC}"
        read -p "Continue anyway? [y/N] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    fi
}

# Print colored status
info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check for required commands
check_command() {
    if ! command -v "$1" &> /dev/null; then
        return 1
    fi
    return 0
}

# Check for a dependency, install if missing
check_dependency() {
    local cmd="$1"
    local pkg="$2"
    local install_cmd="$3"
    
    if check_command "$cmd"; then
        success "$cmd is installed"
        return 0
    else
        info "$cmd not found, installing $pkg..."
        eval "$install_cmd"
        if check_command "$cmd"; then
            success "$pkg installed successfully"
            return 0
        else
            error "Failed to install $pkg"
            return 1
        fi
    fi
}

# Main setup
main() {
    echo -e "${BLUE}╔═══════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║       Shroud VPN Manager Setup            ║${NC}"
    echo -e "${BLUE}╚═══════════════════════════════════════════╝${NC}"
    echo

    check_arch

    info "Checking dependencies..."

    # Check for pacman
    if ! check_command "pacman"; then
        error "pacman not found. This script requires Arch Linux."
        exit 1
    fi

    # Check and install dependencies
    if ! check_dependency "cargo" "rust" "sudo pacman -S --noconfirm rust"; then
        error "Rust installation failed"
        exit 1
    fi

    if ! check_dependency "nmcli" "networkmanager" "sudo pacman -S --noconfirm networkmanager"; then
        error "NetworkManager installation failed"
        exit 1
    fi

    # OpenVPN is optional but recommended
    if ! check_command "openvpn"; then
        info "Installing openvpn (required for OpenVPN connections)..."
        sudo pacman -S --noconfirm openvpn networkmanager-openvpn || true
    fi
    
    # nftables for kill switch
    if ! check_dependency "nft" "nftables" "sudo pacman -S --noconfirm nftables"; then
        error "nftables installation failed"
        exit 1
    fi

    echo
    info "Building Shroud in release mode..."
    cargo build --release

    if [ ! -f "target/release/$BINARY_NAME" ]; then
        error "Build failed - binary not found"
        exit 1
    fi
    success "Build completed"

    # Create install directory
    mkdir -p "$INSTALL_DIR"
    
    # Stop existing service if running
    if systemctl --user is-active --quiet "$SERVICE_NAME" 2>/dev/null; then
        info "Stopping existing service..."
        systemctl --user stop "$SERVICE_NAME" || true
    fi

    # Kill any running instance
    pkill -f "$BINARY_NAME" 2>/dev/null || true
    sleep 1

    # Install binary
    info "Installing binary to $INSTALL_DIR..."
    cp "target/release/$BINARY_NAME" "$INSTALL_DIR/"
    chmod +x "$INSTALL_DIR/$BINARY_NAME"
    success "Binary installed to $INSTALL_DIR/$BINARY_NAME"

    # Setup systemd user service
    echo
    info "Setting up systemd user service..."
    mkdir -p "$SYSTEMD_USER_DIR"
    cp "systemd/$SERVICE_NAME" "$SYSTEMD_USER_DIR/"
    
    # Reload systemd
    systemctl --user daemon-reload
    success "Systemd service installed"

    # Enable and start service
    info "Enabling and starting service..."
    systemctl --user enable "$SERVICE_NAME"
    systemctl --user start "$SERVICE_NAME"
    
    if systemctl --user is-active --quiet "$SERVICE_NAME"; then
        success "Service is running"
    else
        error "Service failed to start. Check: journalctl --user -u $SERVICE_NAME"
    fi

    # Setup XDG autostart
    echo
    info "Setting up XDG autostart..."
    mkdir -p "$AUTOSTART_DIR"
    cp "autostart/shroud.desktop" "$AUTOSTART_DIR/"
    success "Autostart entry installed to $AUTOSTART_DIR/shroud.desktop"

    # Migrate old config if present
    OLD_CONFIG_DIR="$HOME/.config/openvpn-tray"
    NEW_CONFIG_DIR="$HOME/.config/shroud"
    if [ -d "$OLD_CONFIG_DIR" ] && [ ! -d "$NEW_CONFIG_DIR" ]; then
        info "Migrating config from $OLD_CONFIG_DIR to $NEW_CONFIG_DIR..."
        mkdir -p "$NEW_CONFIG_DIR"
        cp "$OLD_CONFIG_DIR/config.toml" "$NEW_CONFIG_DIR/" 2>/dev/null || true
        chmod 700 "$NEW_CONFIG_DIR"
        chmod 600 "$NEW_CONFIG_DIR/config.toml" 2>/dev/null || true
        success "Config migrated"
    fi

    # Summary
    echo
    echo -e "${GREEN}╔═══════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║       Installation Complete!              ║${NC}"
    echo -e "${GREEN}╚═══════════════════════════════════════════╝${NC}"
    echo
    echo -e "Binary:    ${YELLOW}$INSTALL_DIR/$BINARY_NAME${NC}"
    echo -e "Service:   ${YELLOW}$SYSTEMD_USER_DIR/$SERVICE_NAME${NC}"
    echo -e "Config:    ${YELLOW}$HOME/.config/shroud/config.toml${NC}"
    echo
    echo -e "${BLUE}Useful commands:${NC}"
    echo -e "  Check status:  ${YELLOW}systemctl --user status $SERVICE_NAME${NC}"
    echo -e "  View logs:     ${YELLOW}journalctl --user -u $SERVICE_NAME -f${NC}"
    echo -e "  Restart:       ${YELLOW}systemctl --user restart $SERVICE_NAME${NC}"
    echo -e "  Stop:          ${YELLOW}systemctl --user stop $SERVICE_NAME${NC}"
    echo
    echo -e "${BLUE}To import VPN configs:${NC}"
    echo -e "  ${YELLOW}nmcli connection import type openvpn file your-config.ovpn${NC}"
    echo
}

main "$@"
