# blabber-root

A headless, GUI-free build of the Blabber node, meant to run permanently on
a server. It joins the spaces you configure and stays online as a normal
 Member, so those spaces keep syncing and message/room history
stays reachable.

It reuses `blabber-core::node::Node` as the desktop app
(`blabber-app`) does.

## Building

```bash
cargo build -p blabber-root --release
```

Unlike `blabber-app`, this does **not** need `libasound2-dev`/ALSA or any
other audio system library installed: `blabber-core`'s `cpal`/`symphonia`
dependencies are gated behind a Cargo feature (`audio`, default-on) that
`blabber-root` opts out of.

The binary is produced at `target/release/blabber-root`.

## Configuration

`blabber-root` reads a TOML config file, by default at
`/etc/blabber-root/config.toml` (override with `--config <path>`). If the
file doesn't exist, a commented out template is written there and the process
exits with instructions, it never boots with guessed defaults.

| Field           | Meaning                                                                                      |
|-----------------|----------------------------------------------------------------------------------------------|
| `display_name`  | How this node appears as a Member of every space it joins. Default: `"blabber-root"`.        |
| `data_dir`      | Root directory for `identity.bin`, `blobs/`, and `spaces/`.                                  |
| `password_file` | Path to a file whose trimmed contents is the identity password. See below.                   |
| `invites`       | List of invite ticket strings to auto-join at startup and on `SIGHUP` reload. Default: `[]`. |

On first run, an identity is created
automatically using `display_name` and encrypted with the password from
`password_file`.

### Getting an invite ticket

In the desktop app, use the existing "Get invite" / "Copy invite" action for
a space, and paste the resulting code into the `invites` array.

## Password file

The identity is encrypted at rest the same way the desktop app encrypts it
(Argon2 + ChaCha20Poly1305), `password_file` just supplies that password
without interaction. Two ways to provision it:

**Option A: systemd credential:**
Provision a root-only-readable source file once:
```bash
install -o root -g root -m 0600 /path/to/password /etc/blabber-root/password.source
```
The unit's `LoadCredential=password:/etc/blabber-root/password.source` line
makes systemd expose it read-only, only to this service, at:
```
/run/credentials/blabber-root.service/password
```
`config.toml`'s `password_file` must be set to that literal path, systemd
credential specifiers only expand inside unit-file directives, not inside
files a program reads at runtime, so this path has to be hardcoded in the
config.

**Option B: plain file:** skip
`LoadCredential=` entirely, provision the password directly:
```bash
mkdir -p /etc/blabber-root
echo -n "your-password" > /etc/blabber-root/password
chown blabber-root:blabber-root /etc/blabber-root/password
chmod 600 /etc/blabber-root/password
```
and point `password_file` at `/etc/blabber-root/password`.

## Running as a systemd service

1. Create a dedicated user and the data directory:
   ```bash
   useradd --system --home-dir /var/lib/blabber-root --shell /usr/sbin/nologin blabber-root
   ```
2. Build and install the binary:
   ```bash
   cargo build -p blabber-root --release
   install -m 0755 target/release/blabber-root /usr/local/bin/blabber-root
   ```
3. Provision the password (Option A or B above) and write
   `/etc/blabber-root/config.toml` (or run the service once to get a
   template written, then edit it).
4. Install and start the unit:
   ```bash
   install -m 0644 blabber-root/blabber-root.service /etc/systemd/system/
   systemctl daemon-reload
   systemctl enable --now blabber-root
   ```
5. Watch it come up:
   ```bash
   journalctl -u blabber-root -f
   ```

### Adding a space without restarting

Append an invite ticket to `invites` in `config.toml`, then:
```bash
systemctl reload blabber-root
```
This sends `SIGHUP`, which re-reads the config and joins all invites not
already known, existing spaces and connections are untouched.
`display_name` and `password_file` changes are only applied on a full
restart (`systemctl restart blabber-root`), not on reload.

### Stopping

```bash
systemctl stop blabber-root
```
Sends `SIGTERM` the node shuts down its router cleanly before exiting.

## Data layout

Under `data_dir`:
```
identity.bin                              encrypted identity
blobs/<display_name>/                     Iroh blob storage
spaces/<display_name>/<space-uuid>/       synchronized space and room data
```
Mirrors the layout the desktop app uses under its own app-data directory.
