#!/bin/bash
# Called by loopback-port-update.service when /var/lib/loopback/vpn_port changes.
# The service reads the port directly from the file, so we only need to update
# the firewall and restart.

PORT=$(cat /var/lib/loopback/vpn_port 2>/dev/null | tr -d '[:space:]')

if [[ ! "$PORT" =~ ^[0-9]+$ ]]; then
    echo "Invalid port in vpn_port: '$PORT'" >&2
    exit 1
fi

# Update live iptables rules.
# Delete all existing LOOPBACK_VPN rules (by line number, high-to-low to preserve indices),
# then add the new one. This avoids the shell-redirect sudo issue that made
# `sudo iptables-restore < file` fail with "Permission denied".
OLD_RULES=$(sudo iptables -L INPUT -n --line-numbers | awk '/LOOPBACK_VPN/{print $1}' | sort -rn)
for RULE_NUM in $OLD_RULES; do
    sudo iptables -D INPUT "$RULE_NUM"
done
sudo iptables -A INPUT -p udp -m udp --dport "$PORT" -j ACCEPT -m comment --comment LOOPBACK_VPN

# Persist the updated live rules for next boot
sudo sh -c 'iptables-save > /etc/iptables/rules.v4'
echo "Firewall updated for UDP port $PORT"

systemctl --user restart loopback
