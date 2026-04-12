# Deploying updates

After building a new release, restart the service with:

```bash
cargo build --release
systemctl --user stop loopback
cp target/release/loopback "$HOME/opt/loopback"
sudo setcap cap_net_raw+ep "$HOME/opt/loopback"
systemctl --user start loopback
```

Verify it started cleanly:

```bash
systemctl --user status loopback --no-pager
