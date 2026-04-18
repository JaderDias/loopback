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

# ── Mimir ─────────────────────────────────────────────────────────────────────
MIMIR_BIN="${OPT_DIR}/mimir"
if [ ! -f "$MIMIR_BIN" ]; then
    echo "Downloading Mimir..."
    MIMIR_VERSION=$(curl -sf https://api.github.com/repos/grafana/mimir/releases/latest \
        | grep '"tag_name"' | sed 's/.*"mimir-\(.*\)".*/\1/')
    curl -L "https://github.com/grafana/mimir/releases/download/mimir-${MIMIR_VERSION}/mimir-linux-arm64" \
        -o "$MIMIR_BIN"
    chmod +x "$MIMIR_BIN"
fi

cp "$SCRIPT_DIR/mimir/mimir.yaml" "$OPT_DIR/mimir.yaml"
mkdir -p "${HOME}/var/mimir"

cp "$SCRIPT_DIR/systemd/mimir.service" "$SYSTEMD_DIR/"

# ── Build ──────────────────────────────────────────────────────────────────────
echo "Building loopback..."
cd "$SCRIPT_DIR"
cargo build --release
systemctl --user stop loopback 2>/dev/null || true
cp target/release/loopback "$OPT_DIR/loopback"
sudo setcap cap_net_raw+ep "$OPT_DIR/loopback"

# ── WireGuard config ───────────────────────────────────────────────────────────
sudo mkdir -p /etc/wireguard
sudo cp "$SCRIPT_DIR/wgproton.conf" /etc/wireguard/wgproton.conf
sudo chmod 600 /etc/wireguard/wgproton.conf
sudo systemctl enable wg-quick@wgproton

# ── Scripts ────────────────────────────────────────────────────────────────────
chmod +x "$SCRIPT_DIR/protonvpn-portforward.sh"
chmod +x "$SCRIPT_DIR/update-loopback-port.sh"

# ── Systemd units ──────────────────────────────────────────────────────────────
cp "$SCRIPT_DIR/systemd/loopback-port-update.path" "$SYSTEMD_DIR/"
cp "$SCRIPT_DIR/systemd/loopback-port-update.service" "$SYSTEMD_DIR/"
cp "$SCRIPT_DIR/systemd/protonvpn-portforward.service" "$SYSTEMD_DIR/"

cat > "$SYSTEMD_DIR/loopback.service" <<EOF
[Unit]
Description=Uptime Monitoring Tool
After=network.target mimir.service

[Service]
ExecStart=$OPT_DIR/loopback
Restart=always
EnvironmentFile=$OPT_DIR/loopback.env

[Install]
WantedBy=default.target
EOF

# ── Firewall ───────────────────────────────────────────────────────────────────
# Remove stale web-dashboard rule (port 8124, replaced by Mimir).
OLD_WEB=$(sudo iptables -L INPUT -n --line-numbers | awk '/dpt:8124/{print $1}' | sort -rn)
for RULE_NUM in $OLD_WEB; do sudo iptables -D INPUT "$RULE_NUM"; done

# Allow Grafana (192.168.5.2) to reach Mimir (port 9009) — idempotent.
if ! sudo iptables -L INPUT -n | grep -q MIMIR; then
    sudo iptables -I INPUT -s 192.168.5.2 -p tcp --dport 9009 -j ACCEPT -m comment --comment MIMIR
fi

sudo sh -c 'iptables-save > /etc/iptables/rules.v4'

# ── Linger (start user services at boot without login) ─────────────────────────
loginctl enable-linger "$USER"

# ── Enable and start ───────────────────────────────────────────────────────────
systemctl --user daemon-reload
systemctl --user enable --now mimir.service
systemctl --user enable --now loopback.service
systemctl --user enable --now loopback-port-update.path

echo ""
echo "Done. To enable ProtonVPN port forwarding once wg-quick@wgproton is running:"
echo "  systemctl --user enable --now protonvpn-portforward.service"
