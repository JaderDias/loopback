#!/bin/bash
# Called by loopback-port-update.service when /var/lib/loopback/vpn_port changes.
# The service reads the port directly from the file, so we only need to update
# the firewall and restart.

PORT=$(cat /var/lib/loopback/vpn_port 2>/dev/null | tr -d '[:space:]')

if [[ ! "$PORT" =~ ^[0-9]+$ ]]; then
    echo "Invalid port in vpn_port: '$PORT'" >&2
    exit 1
fi

# Update firewall: replace old loopback UDP rule with new port
sudo sed -i "/--dport .* -j ACCEPT.*LOOPBACK_VPN/d" /etc/iptables/rules.v4
sudo sed -i "/--dport 8124 -j ACCEPT/a -A INPUT -p udp -m udp --dport $PORT -j ACCEPT -m comment --comment LOOPBACK_VPN" /etc/iptables/rules.v4
sudo iptables-restore < /etc/iptables/rules.v4
echo "Firewall updated for UDP port $PORT"

systemctl --user restart loopback
