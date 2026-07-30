# blabber-root

Headless, GUI-free build of the Blabber node, meant to run permanently on a
server as a **blind relay**: it propagates ciphertext for the spaces it joins but
never holds a decryption key and cannot read or write real content.

## Setup

1. Build:
   ```bash
   cargo build -p blabber-root --release
   ```
2. Install the binary and create the service user:
   ```bash
   sudo install -m 0755 target/release/blabber-root /usr/local/bin/blabber-root
   sudo useradd --system --home-dir /var/lib/blabber-root --shell /usr/sbin/nologin blabber-root
   ```
3. Run the setup wizard:
   ```bash
   sudo blabber-root setup
   ```
   Answer its prompts for display name, data directory, password, and
   (optionally) a relay invite ticket - get one from the desktop app's "Get
   relay invite" action on the space you want this relay to join. This
   writes `/etc/blabber-root/config.toml` and the password file.
4. Install and start the systemd unit:
   ```bash
   sudo install -m 0644 blabber/blabber-root/blabber-root.service /etc/systemd/system/
   sudo systemctl daemon-reload
   sudo systemctl enable --now blabber-root
   ```
5. Confirm it's running:
   ```bash
   journalctl -u blabber-root -f
   ```

## Operating

- Add a space without restarting: append an invite ticket to `invites` in
  `config.toml`, then `sudo systemctl reload blabber-root`.
- Restart (required after changing `display_name` or `password_file`):
  `sudo systemctl restart blabber-root`.
- Stop: `sudo systemctl stop blabber-root`.
- Re-run `sudo blabber-root setup` any time to change settings; it detects
  an existing `identity.bin` and won't let you silently break it with a
  mismatched password.

## Reference

`config.toml` fields (`--config <path>` to use a non-default location):
`display_name`, `data_dir`, `password_file`, `invites` - see the comments
`blabber-root setup` writes into the file itself.

Manual password provisioning, if not using `blabber-root setup`:
- **systemd credential:**
  ```bash
  sudo install -o root -g root -m 0600 /path/to/password /etc/blabber-root/password.source
  ```
  Set `password_file` to `/run/credentials/blabber-root.service/password`
  (the unit's `LoadCredential=` line exposes it there. This exact path must
  be hardcoded in the config).
- **plain file:**
  ```bash
  sudo mkdir -p /etc/blabber-root
  echo -n "your-password" | sudo tee /etc/blabber-root/password > /dev/null
  sudo chown blabber-root:blabber-root /etc/blabber-root/password
  sudo chmod 600 /etc/blabber-root/password
  ```
  Set `password_file` to `/etc/blabber-root/password`.

Data layout under `data_dir`:
```
identity.bin                              encrypted identity
blobs/<display_name>/                     Iroh blob storage
spaces/<display_name>/<space-uuid>/       synchronized space and room data
```
