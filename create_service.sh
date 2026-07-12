#!/bin/bash
# Dev helper: install a LOCALLY built w3p-hwm binary + the canonical systemd
# unit. For release installs on a device use install.sh instead.
# Build off-device first: mise run build-aarch64 (cargo zigbuild).
# Assets (font/img) are embedded in the binary — nothing else to copy.
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    echo "This script must be run with sudo"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SERVICE_NAME="w3p-hwm"
BIN_DEST="/usr/local/bin/w3p-hwm"
SERVICE_FILE="/etc/systemd/system/$SERVICE_NAME.service"

BIN_SRC=""
for candidate in "$SCRIPT_DIR/w3p-hwm" "$SCRIPT_DIR/target/aarch64-unknown-linux-gnu/release/w3p-hwm"; do
    if [[ -f "$candidate" ]]; then
        BIN_SRC="$candidate"
        break
    fi
done
if [[ -z "$BIN_SRC" ]]; then
    echo "Prebuilt binary not found (looked for ./w3p-hwm and target/aarch64-unknown-linux-gnu/release/w3p-hwm)."
    echo "Build it off-device with: mise run build-aarch64"
    exit 1
fi

install -m 755 "$BIN_SRC" "$BIN_DEST"

# Remove the legacy underscore unit from pre-release installs so two dashboard
# instances never fight over SPI/GPIO (no-op on fresh systems).
systemctl disable --now w3p_hwm.service 2>/dev/null || true
rm -f /etc/systemd/system/w3p_hwm.service

install -m 644 "$SCRIPT_DIR/packaging/w3p-hwm.service" "$SERVICE_FILE"

systemctl daemon-reload
systemctl enable "$SERVICE_NAME.service"
# restart (not `enable --now`): picks up a redeployed binary/unit when the
# service is already running, and still starts it on a fresh install.
systemctl restart "$SERVICE_NAME.service"

echo "Installed $BIN_DEST (assets embedded)."
echo "The service $SERVICE_NAME has been created and started."
