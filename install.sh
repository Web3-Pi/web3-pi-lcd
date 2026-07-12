#!/usr/bin/env bash
#
# web3-pi-lcd installer — installs/updates the Web3 Pi LCD dashboard (w3p-hwm).
#
# Downloads the prebuilt release binary + systemd unit, enables SPI, and starts
# the service. Idempotent: re-run any time to update to the latest release.
#
#   curl -sSL https://raw.githubusercontent.com/Web3-Pi/web3-pi-lcd/main/install.sh | sudo bash
#
# Pin a specific version:
#   curl -sSL .../install.sh | sudo bash -s -- v0.2.0
#
set -euo pipefail

REPO="Web3-Pi/web3-pi-lcd"
BIN_ASSET="w3p-hwm-linux-arm64"
BIN_DEST="/usr/local/bin/w3p-hwm"
SERVICE="w3p-hwm.service"
SERVICE_DEST="/etc/systemd/system/${SERVICE}"
CONFIG_TXT="${CONFIG_TXT:-/boot/firmware/config.txt}"
VERSION="${1:-${VERSION:-latest}}"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
info() { echo -e "${GREEN}[INFO]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

[ "$(id -u)" -eq 0 ] || error "Run as root (e.g. pipe into 'sudo bash')."

ARCH="$(uname -m)"
case "$ARCH" in
    aarch64 | arm64) ;;
    *) error "Unsupported architecture '$ARCH' — web3-pi-lcd ships aarch64 (Raspberry Pi 5) only." ;;
esac

# Resolve the release tag.
if [ "$VERSION" = "latest" ]; then
    info "Resolving latest release of ${REPO}..."
    VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name":' | head -1 | sed -E 's/.*"([^"]+)".*/\1/')"
    [ -n "$VERSION" ] || error "Could not resolve latest release tag (rate-limited or no releases yet)."
fi
info "Installing web3-pi-lcd ${VERSION} ($ARCH)"

BASE="https://github.com/${REPO}/releases/download/${VERSION}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Binary (+ checksum verification when the .sha256 asset is present).
info "Downloading ${BIN_ASSET}..."
curl -fsSL "${BASE}/${BIN_ASSET}" -o "${TMP}/w3p-hwm" \
    || error "Failed to download ${BASE}/${BIN_ASSET}"
if curl -fsSL "${BASE}/${BIN_ASSET}.sha256" -o "${TMP}/w3p-hwm.sha256" 2>/dev/null; then
    EXPECTED="$(awk '{print $1}' "${TMP}/w3p-hwm.sha256")"
    ACTUAL="$(sha256sum "${TMP}/w3p-hwm" | awk '{print $1}')"
    [ "$EXPECTED" = "$ACTUAL" ] || error "Checksum mismatch (expected $EXPECTED, got $ACTUAL)."
    info "Checksum OK."
else
    warn "No checksum asset published for ${VERSION}; skipping verification."
fi

# systemd unit (falls back to a built-in default if the release predates it).
if ! curl -fsSL "${BASE}/${SERVICE}" -o "${TMP}/${SERVICE}" 2>/dev/null; then
    warn "No ${SERVICE} asset in ${VERSION}; writing built-in default unit."
    cat > "${TMP}/${SERVICE}" <<EOF
[Unit]
Description=Web3 Pi LCD dashboard (w3p-hwm)
Documentation=https://github.com/${REPO}
After=local-fs.target

[Service]
ExecStart=${BIN_DEST}
StateDirectory=w3p-hwm
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
fi

# Enable SPI (idempotent). The header SPI (/dev/spidev0.0) needs a reboot to
# appear; on Web3 Pi vOS it is already on by default.
enable_spi() {
    local header_present=0
    for dev in /dev/spidev0.0 /dev/spidev0.1 /dev/spidev1.0 /dev/spidev1.1; do
        [ -e "$dev" ] && header_present=1
    done
    if [ ! -f "$CONFIG_TXT" ]; then
        warn "Boot config $CONFIG_TXT not found — skipping SPI enable (enable dtparam=spi=on manually)."
        return
    fi
    if grep -Eq '^[[:space:]]*dtparam[[:space:]]*=[[:space:]]*spi[[:space:]]*=[[:space:]]*on([[:space:]]*(,|$))' "$CONFIG_TXT"; then
        [ "$header_present" -eq 1 ] && info "SPI already enabled." \
            || warn "SPI enabled in config but /dev/spidev0.0 absent — reboot required."
        return
    fi
    printf '\n[all]\ndtparam=spi=on\n' >> "$CONFIG_TXT"
    warn "Enabled SPI in $CONFIG_TXT — REBOOT REQUIRED for the panel to light up."
}

# Legacy underscore unit from pre-release installs — remove so two instances
# never fight over SPI/GPIO (no-op on fresh systems).
systemctl disable --now w3p_hwm.service 2>/dev/null || true
rm -f /etc/systemd/system/w3p_hwm.service

install -m 755 "${TMP}/w3p-hwm" "$BIN_DEST"
install -m 644 "${TMP}/${SERVICE}" "$SERVICE_DEST"
info "Installed $BIN_DEST and $SERVICE_DEST."

enable_spi

systemctl daemon-reload
systemctl enable "$SERVICE" >/dev/null 2>&1 || true
# restart (not enable --now): picks up a redeployed binary when already running,
# and still starts it on a fresh install.
systemctl restart "$SERVICE"

info "Done. web3-pi-lcd ${VERSION} installed and ${SERVICE} started."
systemctl --no-pager --lines=0 status "$SERVICE" || true
