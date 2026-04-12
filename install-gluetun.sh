#!/bin/bash
# Installs gluetun + loopback port-update systemd units for the current user.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SYSTEMD_DIR="${HOME}/.config/systemd/user"

if [ ! -f "${HOME}/opt/gluetun.env" ]; then
    echo "Missing ~/opt/gluetun.env — copy gluetun.env.example and fill in credentials."
    exit 1
fi

chmod +x "$SCRIPT_DIR/gluetun-run.sh"
chmod +x "$SCRIPT_DIR/update-loopback-port.sh"

mkdir -p "$SYSTEMD_DIR"
cp "$SCRIPT_DIR/systemd/gluetun.service" "$SYSTEMD_DIR/"
cp "$SCRIPT_DIR/systemd/loopback-port-update.path" "$SYSTEMD_DIR/"
cp "$SCRIPT_DIR/systemd/loopback-port-update.service" "$SYSTEMD_DIR/"

systemctl --user daemon-reload
systemctl --user enable --now gluetun.service
systemctl --user enable --now loopback-port-update.path

echo "Done. gluetun and loopback-port-update are running."
