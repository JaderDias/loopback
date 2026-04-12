#!/bin/bash
# Requests and renews ProtonVPN port forwarding via NAT-PMP (natpmpc).
# Runs in a loop, renewing every 45s (lease is 60s).
# Writes the assigned port to /var/lib/loopback/vpn_port, which
# triggers loopback-port-update.path to update TARGET_PORT and restart
# the loopback service.
#
# Requires: natpmpc (apt install natpmp-utils)
# The WireGuard gateway IP must match your ProtonVPN config (DNS address).

GATEWAY="${PROTONVPN_GATEWAY:-10.2.0.1}"
PORT_FILE="/var/lib/loopback/vpn_port"

echo "Requesting ProtonVPN port forwarding via $GATEWAY..."

while true; do
    result=$(natpmpc -a 1 0 udp 60 -g "$GATEWAY" 2>&1)
    port=$(echo "$result" | grep -oP 'Mapped public port \K[0-9]+')

    if [[ "$port" =~ ^[0-9]+$ ]]; then
        current=$(cat "$PORT_FILE" 2>/dev/null | tr -d '[:space:]')
        if [ "$port" != "$current" ]; then
            echo "Port forwarding: $port"
            echo "$port" > "$PORT_FILE"
        fi
    else
        echo "natpmpc failed: $result" >&2
    fi

    sleep 45
done
