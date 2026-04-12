#!/bin/bash
# Called by loopback-port-update.service when /var/lib/loopback/vpn_port changes.
# Reads the VPN-assigned port, updates TARGET_PORT in loopback.env, and restarts loopback.

PORT=$(cat /var/lib/loopback/vpn_port 2>/dev/null | tr -d '[:space:]')

if [[ ! "$PORT" =~ ^[0-9]+$ ]]; then
    echo "Invalid port in vpn_port: '$PORT'" >&2
    exit 1
fi

ENV_FILE="${HOME}/opt/loopback.env"
sed -i "s/^TARGET_PORT=.*/TARGET_PORT=$PORT/" "$ENV_FILE"
echo "Updated TARGET_PORT to $PORT in $ENV_FILE"

systemctl --user restart loopback
