---
title: Deployment
---

[← lonk guide](index.md)

# Deployment

Build once, deploy one file — the web UI is embedded in the binary (see
[Customizing the UI](customizing.md) if you want to serve a different one
without rebuilding):

```bash
npm install && npm run build
cargo build --release
```

produces `target/release/lonkd`, a single self-contained binary.

## Linux: systemd

`deploy/lonkd.service`. Install (from its header comment):

```bash
sudo cp target/release/lonkd /usr/local/bin/lonkd
sudo cp deploy/lonkd.service /etc/systemd/system/lonkd.service
sudo systemctl enable --now lonkd
```

The unit runs `lonkd` as a `DynamicUser` with `StateDirectory=lonk`
(systemd creates and owns `/var/lib/lonk`), sets `LONK_DB=/var/lib/lonk/lonk.db`
and `ROCKET_PORT=8000`, restarts on failure, and shows `ROCKET_ADDRESS`
and `LONK_WEB_DIR` as commented-out `Environment=` lines you can uncomment.

Logs go to the journal (stdout/stderr are captured automatically):

```bash
journalctl -u lonkd
journalctl -u lonkd -f     # follow
```

## macOS: launchd

`deploy/com.lonk.lonkd.plist`, a LaunchDaemon that starts at boot and runs
as root. Install (from its header comment):

```bash
sudo cp target/release/lonkd /usr/local/bin/lonkd
sudo mkdir -p /usr/local/var/lonk
sudo cp deploy/com.lonk.lonkd.plist /Library/LaunchDaemons/
sudo launchctl bootstrap system /Library/LaunchDaemons/com.lonk.lonkd.plist
```

For a login-time user agent instead, copy the plist to
`~/Library/LaunchAgents` and use `launchctl bootstrap gui/$(id -u) ...`
(then `LONK_DB` may point into your home directory instead of
`/usr/local/var/lonk`).

The plist sets `LONK_DB=/usr/local/var/lonk/lonk.db` and `ROCKET_PORT=8000`
in `EnvironmentVariables`, with `ROCKET_ADDRESS` and `LONK_WEB_DIR` shown
commented out the same way as the systemd unit.

**Logs:** the plist does not set `StandardOutPath`/`StandardErrorPath`,
so by default launchd discards `lonkd`'s stdout/stderr — nothing is
persisted to a log file. If you need logs, add both keys to the plist
before installing it, e.g.:

```xml
<key>StandardOutPath</key>
<string>/usr/local/var/log/lonkd.log</string>
<key>StandardErrorPath</key>
<string>/usr/local/var/log/lonkd.err.log</string>
```

## Exposing beyond localhost

Both deploy files show `ROCKET_ADDRESS=0.0.0.0` as a commented-out
option. Before uncommenting it, know what you're exposing: `GET
/<id>/status` makes `lonkd` issue a live HEAD/GET request to whatever URL
is stored on a link and report back whether it answered (see [Running the
server](server.md) for the full probe behavior). Anyone who can reach
`lonkd` can use this as a probe oracle — asking your server "is this host
up?" for arbitrary URLs that have been shortened. On a self-hosted
instance this is the same trust level as being able to create links in
the first place (creating a link and then checking its `/status` gets you
the same information), but it's a meaningfully different exposure once
`lonkd` is reachable from outside localhost — anyone on the network can
probe every link that's ever been created, not just their own.

## Backups

The entire database is one SQLite file (`LONK_DB`, `/var/lib/lonk/lonk.db`
or `/usr/local/var/lonk/lonk.db` by default under the two walkthroughs
above). Back it up by copying that file:

```bash
cp /var/lib/lonk/lonk.db /var/lib/lonk/lonk.db.bak
```

For a consistent snapshot while `lonkd` is running, stop the service
first (`sudo systemctl stop lonkd` / `sudo launchctl bootout system
/Library/LaunchDaemons/com.lonk.lonkd.plist`), copy the file, then start
it again — or use `sqlite3 lonk.db ".backup lonk.db.bak"`, which is safe
to run against a live database.
