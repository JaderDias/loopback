#!/bin/bash
# Runs gluetun as a rootless podman container.
# gluetun establishes the ProtonVPN WireGuard tunnel and handles port
# forwarding. When a port is assigned it writes it to
# /var/lib/loopback/vpn_port, which triggers loopback-port-update.path
# to update TARGET_PORT in loopback.env and restart the loopback service.
#
# Prerequisites:
#   - Copy gluetun.env.example to /home/pi/opt/gluetun.env and fill in credentials
#   - sudo sysctl -w net.ipv4.conf.all.src_valid_mark=1  (persist in /etc/sysctl.d/)

set -euo pipefail

ENV_FILE="${HOME}/opt/gluetun.env"

if [ ! -f "$ENV_FILE" ]; then
    echo "Missing $ENV_FILE — copy gluetun.env.example and fill in credentials."
    exit 1
fi

exec podman run --rm \
    --name gluetun \
    --cap-add NET_ADMIN \
    --device /dev/net/tun:/dev/net/tun \
    --sysctl net.ipv4.conf.all.src_valid_mark=1 \
    --volume gluetun:/gluetun \
    --volume /var/lib/loopback:/var/lib/loopback \
    --env-file "$ENV_FILE" \
    qmcgaw/gluetun:latest
