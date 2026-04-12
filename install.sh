#!/bin/bash
# Installs all requirements and systemd units for the loopback service.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SYSTEMD_DIR="${HOME}/.config/systemd/user"
OPT_DIR="${HOME}/opt"

# ── System packages ────────────────────────────────────────────────────────────
install_pkg() {
    if ! command -v "$2" &>/dev/null; then
        echo "Installing $1..."
        sudo apt-get install -y "$1"
    fi
}

install_pkg natpmp-utils natpmpc
install_pkg cargo cargo

# ── Directories ────────────────────────────────────────────────────────────────
sudo mkdir -p /var/lib/loopback
sudo chown "$USER" /var/lib/loopback

mkdir -p "$OPT_DIR"
mkdir -p "$SYSTEMD_DIR"

# ── Env file ───────────────────────────────────────────────────────────────────
if [ ! -f "$OPT_DIR/loopback.env" ]; then
    cp "$SCRIPT_DIR/loopback.env.example" "$OPT_DIR/loopback.env"
    echo "Created $OPT_DIR/loopback.env from example — edit it before starting."
fi

# ── Build ──────────────────────────────────────────────────────────────────────
echo "Building loopback..."
cd "$SCRIPT_DIR"
cargo build --release
systemctl --user stop loopback 2>/dev/null || true
cp target/release/loopback "$OPT_DIR/loopback"

# ── Scripts ────────────────────────────────────────────────────────────────────
chmod +x "$SCRIPT_DIR/protonvpn-portforward.sh"
chmod +x "$SCRIPT_DIR/update-loopback-port.sh"

# ── Systemd units ──────────────────────────────────────────────────────────────
cp "$SCRIPT_DIR/systemd/loopback-port-update.path" "$SYSTEMD_DIR/"
cp "$SCRIPT_DIR/systemd/loopback-port-update.service" "$SYSTEMD_DIR/"
cp "$SCRIPT_DIR/systemd/protonvpn-portforward.service" "$SYSTEMD_DIR/"

# Install loopback.service if not already present
if [ ! -f "$SYSTEMD_DIR/loopback.service" ]; then
    cat > "$SYSTEMD_DIR/loopback.service" <<EOF
[Unit]
Description=Uptime Monitoring Tool
After=network.target

[Service]
ExecStart=$OPT_DIR/loopback
Restart=always
EnvironmentFile=$OPT_DIR/loopback.env

[Install]
WantedBy=default.target
EOF
fi

# ── Linger (start user services at boot without login) ─────────────────────────
loginctl enable-linger "$USER"

# ── Enable and start ───────────────────────────────────────────────────────────
systemctl --user daemon-reload
systemctl --user enable --now loopback.service
systemctl --user enable --now loopback-port-update.path

echo ""
echo "Done. To enable ProtonVPN port forwarding once wg-quick@wgproton is running:"
echo "  systemctl --user enable --now protonvpn-portforward.service"
