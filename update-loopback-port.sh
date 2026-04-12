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

# Update firewall: replace old loopback UDP rule with new port
sudo sed -i "/--dport .* -j ACCEPT.*LOOPBACK_VPN/d" /etc/iptables/rules.v4
sudo sed -i "/--dport 8124 -j ACCEPT/a -A INPUT -p udp -m udp --dport $PORT -j ACCEPT -m comment --comment LOOPBACK_VPN" /etc/iptables/rules.v4
sudo iptables-restore < /etc/iptables/rules.v4
echo "Firewall updated for UDP port $PORT"

systemctl --user restart loopback
