#!/bin/bash
# mandleROT first-boot provisioning. Run once on a fresh Raspberry Pi OS Lite
# (Bookworm aarch64 recommended).
#
# Usage:
#   sudo ./install.sh
#
# Idempotent — safe to re-run. Steps:
#   1. install runtime shared libs (alsa, udev, egl, gbm, drm) + logrotate
#   2. create the mandlerot service user + group memberships
#   3. set up /opt/mandlerot and /var/lib/mandlerot directories
#   4. drop the systemd unit
#   5. install log rotation + journald cap (prevents the SD card from
#      filling up with stdout logs or persistent journals)
#   6. NOPASSWD sudoers for the deploy user (so `make deploy` rsync can
#      use --rsync-path="sudo rsync" without prompting)
#   7. append composite + SPI + fbtft lines to /boot/firmware/config.txt
#   8. append spidev.bufsiz=524288 to /boot/firmware/cmdline.txt (the
#      status panel sends a full ~300 KB frame per flush)
#   9. enable (but don't start) the service — needs a reboot to pick up
#      the boot config + cmdline changes

set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
    echo "Run as root (sudo)" >&2
    exit 1
fi

INSTALL_DIR=/opt/mandlerot
STATE_DIR=/var/lib/mandlerot
SERVICE_NAME=mandlerot
USER_NAME=mandlerot
DEPLOY_USER="${SUDO_USER:-pi}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Detect Pi generation from device-tree model (Pi 3 / 4 / 5). Drives the
# systemd Environment drop-in below (`MANDLEROT_PI_GEN`,
# `MANDLEROT_RENDER_SCALE`) and gates the composite=1 overlay edit — Pi 4
# and Pi 5 have no analog jack, so that param is Pi-3-only. Falls back to
# Pi3 on unknown hardware to keep the existing shipping default.
PI_MODEL="$(tr -d '\0' < /proc/device-tree/model 2>/dev/null || true)"
case "$PI_MODEL" in
    *"Raspberry Pi 5"*) PI_GEN=Pi5; PI_RENDER_SCALE=1.0  ;;
    *"Raspberry Pi 4"*) PI_GEN=Pi4; PI_RENDER_SCALE=0.66 ;;
    *"Raspberry Pi 3"*) PI_GEN=Pi3; PI_RENDER_SCALE=0.33 ;;
    *)                  PI_GEN=Pi3; PI_RENDER_SCALE=0.33 ;;
esac
echo "Detected: ${PI_MODEL:-unknown} -> PiGen=$PI_GEN, render_scale=$PI_RENDER_SCALE"

echo "[1/9] Installing runtime libraries..."
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install --assume-yes --no-install-recommends \
    libasound2 libudev1 libegl1 libgbm1 libdrm2 logrotate

echo "[2/9] Creating user $USER_NAME..."
if ! id "$USER_NAME" >/dev/null 2>&1; then
    useradd --system --no-create-home --shell /usr/sbin/nologin "$USER_NAME"
fi
for g in input audio gpio spi tty render video; do
    if getent group "$g" >/dev/null; then
        usermod -aG "$g" "$USER_NAME"
    fi
done

echo "[3/9] Creating directories..."
mkdir -p "$INSTALL_DIR" "$STATE_DIR/log"
chown -R "$USER_NAME:$USER_NAME" "$STATE_DIR"

echo "[4/9] Installing systemd unit..."
install -m 644 "$SCRIPT_DIR/mandlerot.service" "/etc/systemd/system/$SERVICE_NAME.service"
systemctl daemon-reload

echo "[5/9] Installing logrotate config + journald cap..."
# logrotate runs from /etc/cron.daily on Debian — no extra timer needed.
install -m 644 "$SCRIPT_DIR/logrotate.mandlerot" "/etc/logrotate.d/mandlerot"
# journald drop-in: capped persistent storage so the journal can't fill the
# SD card. Reload journald so the new limits take effect immediately and any
# excess is trimmed on the next vacuum.
mkdir -p /etc/systemd/journald.conf.d
install -m 644 "$SCRIPT_DIR/journald-mandlerot.conf" "/etc/systemd/journald.conf.d/mandlerot.conf"
systemctl kill --kill-who=main --signal=SIGUSR2 systemd-journald 2>/dev/null || true
systemctl restart systemd-journald
# Validate the logrotate config — fails loud if syntax breaks.
if command -v logrotate >/dev/null 2>&1; then
    logrotate --debug /etc/logrotate.d/mandlerot >/dev/null 2>&1 \
        || echo "  -> warning: logrotate --debug reported errors"
fi

echo "[6/9] Granting NOPASSWD sudo to deploy user '$DEPLOY_USER'..."
SUDOERS_FILE="/etc/sudoers.d/010_${DEPLOY_USER}-nopasswd"
if [ ! -f "$SUDOERS_FILE" ]; then
    echo "$DEPLOY_USER ALL=(ALL) NOPASSWD:ALL" > "$SUDOERS_FILE"
    chmod 440 "$SUDOERS_FILE"
    echo "  -> $SUDOERS_FILE created"
else
    echo "  -> already configured (no-op)"
fi

echo "[7/9] Updating /boot/firmware/config.txt..."
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
    # Re-run path: marker block exists from a previous install, but the
    # additions file may have grown new keys (e.g. `temp_soft_limit=70`
    # added after the soft-thermal-limit composite blackout was diagnosed).
    # Ensure each line in the additions file is represented in config.txt
    # so re-running this script picks up any source-side updates.
    added=0
    while IFS= read -r line; do
        case "$line" in
            ''|\#*) continue ;;
            dtoverlay=*)
                # Match by overlay name, not full line, so a tweaked param
                # set still counts as "already present". First segment after
                # `dtoverlay=` up to the first comma identifies the overlay.
                overlay=$(printf '%s' "$line" | sed -E 's/^dtoverlay=([^,]+).*/\1/')
                if ! grep -qE "^dtoverlay=${overlay}(,|$)" "$CONFIG_FILE"; then
                    echo "$line" >> "$CONFIG_FILE"
                    added=$((added+1))
                fi
                ;;
            *)
                key="${line%%=*}"
                if ! grep -qE "^${key}(=|$)" "$CONFIG_FILE"; then
                    echo "$line" >> "$CONFIG_FILE"
                    added=$((added+1))
                fi
                ;;
        esac
    done < "$SCRIPT_DIR/boot-config-additions.txt"
    if [ "$added" -gt 0 ]; then
        echo "  -> $added new line(s) appended to existing mandleROT block"
    else
        echo "  -> already configured (no-op)"
    fi
fi

# Enable composite on the active vc4-kms overlay (Bookworm). Pi 4 and Pi 5
# physically removed the analog jack — appending composite=1 there either
# errors at boot or is a silent no-op, so this branch is Pi-3-only.
# Idempotent: only appends if missing.
if [ "$PI_GEN" = "Pi3" ]; then
    if grep -qE '^dtoverlay=vc4-kms-v3d(,|$)' "$CONFIG_FILE" \
       && ! grep -qE '^dtoverlay=vc4-kms-v3d.*composite=1' "$CONFIG_FILE"; then
        sed -i 's/^dtoverlay=vc4-kms-v3d.*/&,composite=1/' "$CONFIG_FILE"
        echo "  -> added composite=1 to vc4-kms-v3d overlay"
    fi
else
    echo "  -> skipping composite=1 ($PI_GEN has no analog video jack)"
fi

# Per-gen runtime env: drops a systemd unit override that pins the Pi gen
# the binary thinks it's on and the global render_scale to a tier-
# appropriate default. Lives outside `/opt/mandlerot/config.toml` so
# `make deploy` re-rsyncs of config.toml don't stomp the per-host value.
DROPIN_DIR=/etc/systemd/system/${SERVICE_NAME}.service.d
mkdir -p "$DROPIN_DIR"
cat > "$DROPIN_DIR/pi-gen.conf" <<EOF
# Generated by deploy/install.sh — re-run install to refresh.
[Service]
Environment=MANDLEROT_PI_GEN=$PI_GEN
Environment=MANDLEROT_RENDER_SCALE=$PI_RENDER_SCALE
EOF
echo "  -> wrote $DROPIN_DIR/pi-gen.conf (PiGen=$PI_GEN, render_scale=$PI_RENDER_SCALE)"
systemctl daemon-reload

echo "[8/9] Updating /boot/firmware/cmdline.txt..."
CMDLINE_FILE=/boot/firmware/cmdline.txt
if [ ! -f "$CMDLINE_FILE" ]; then
    CMDLINE_FILE=/boot/cmdline.txt
fi
# cmdline.txt must remain a single line. Append the kernel param if missing.
if ! grep -q "spidev.bufsiz=" "$CMDLINE_FILE"; then
    sed -i 's/$/ spidev.bufsiz=524288/' "$CMDLINE_FILE"
    echo "  -> added spidev.bufsiz=524288"
else
    echo "  -> already configured (no-op)"
fi

echo "[9/9] Enabling service..."
systemctl enable "$SERVICE_NAME"
echo ""
echo "Provisioning complete."
echo ""
echo "Next steps (from your dev machine):"
echo "  make deploy HOST=<this-pi> SSH_USER=$DEPLOY_USER"
echo "  ssh $DEPLOY_USER@<this-pi> sudo reboot     # required once after install"
echo "  ssh $DEPLOY_USER@<this-pi> sudo systemctl start $SERVICE_NAME"
