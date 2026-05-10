#!/bin/bash
# mandleROT first-boot provisioning. Run once on a fresh Raspberry Pi OS Lite.
# Usage:
#   sudo ./install.sh
#
# This script:
#   1. creates the mandlerot user + groups
#   2. sets up /opt/mandlerot and /var/lib/mandlerot directories
#   3. installs the systemd unit
#   4. appends composite + SPI lines to /boot/firmware/config.txt
#   5. enables and starts the service

set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
    echo "Run as root (sudo)" >&2
    exit 1
fi

INSTALL_DIR=/opt/mandlerot
STATE_DIR=/var/lib/mandlerot
SERVICE_NAME=mandlerot
USER_NAME=mandlerot
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "[1/5] Creating user $USER_NAME..."
if ! id "$USER_NAME" >/dev/null 2>&1; then
    useradd --system --no-create-home --shell /usr/sbin/nologin "$USER_NAME"
fi
for g in input audio gpio spi tty render video; do
    if getent group "$g" >/dev/null; then
        usermod -aG "$g" "$USER_NAME"
    fi
done

echo "[2/5] Creating directories..."
mkdir -p "$INSTALL_DIR" "$STATE_DIR/log"
chown -R "$USER_NAME:$USER_NAME" "$STATE_DIR"

echo "[3/5] Installing systemd unit..."
install -m 644 "$SCRIPT_DIR/mandlerot.service" "/etc/systemd/system/$SERVICE_NAME.service"
systemctl daemon-reload

echo "[4/5] Updating /boot/firmware/config.txt..."
CONFIG_FILE=/boot/firmware/config.txt
if [ ! -f "$CONFIG_FILE" ]; then
    CONFIG_FILE=/boot/config.txt
fi
if ! grep -q "mandleROT" "$CONFIG_FILE"; then
    echo "" >> "$CONFIG_FILE"
    echo "# mandleROT additions" >> "$CONFIG_FILE"
    cat "$SCRIPT_DIR/boot-config-additions.txt" >> "$CONFIG_FILE"
    echo "  -> appended (will take effect on next reboot)"
else
    echo "  -> already configured (no-op)"
fi

echo "[5/5] Enabling service..."
systemctl enable "$SERVICE_NAME"
echo ""
echo "Provisioning complete."
echo "Next steps:"
echo "  1. scp the binary to $INSTALL_DIR/mandlerot:"
echo "       rsync -avz target/.../release/mandlerot $USER_NAME@<host>:$INSTALL_DIR/"
echo "  2. scp scenes/ and config.toml + keymap.toml to $INSTALL_DIR/"
echo "  3. sudo systemctl start $SERVICE_NAME"
echo "  4. Reboot once to activate composite/SPI config: sudo reboot"
