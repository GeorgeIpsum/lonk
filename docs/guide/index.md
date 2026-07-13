---
title: lonk guide
---

# lonk

lonk is a self-hosted link shortener: a REST API for creating short links
(with QR codes and optional custom response headers), live dead-link
checks, a `lonk` command-line client, and an embeddable web UI — all served
from a single Rust binary with a SQLite database.

## Quick start

```bash
git clone https://github.com/GeorgeIpsum/lonk && cd lonk
npm install && npm run build
cargo run --bin lonkd            # web UI + API on http://127.0.0.1:8000
cargo install --path crates/lonk-cli
lonk setup http://127.0.0.1:8000 && lonk https://example.com/some/long/url
```

The first two commands build the web UI (required once, before `cargo`
builds `lonkd` — see [Customizing the UI](customizing.md)). `cargo run
--bin lonkd` starts the server; `cargo install` puts the `lonk` binary on
your `PATH`; the last line configures the CLI's default profile and prints
your first short link.

## Guide

- [Running the server](server.md) — `lonkd`, environment variables, full
  API reference with request/response examples.
- [The `lonk` CLI](cli.md) — install, `setup`, shortening URLs, headers,
  QR codes, `status`, exit codes.
- [Custom response headers](headers.md) — what they can and can't do,
  use cases, validation rules.
- [Customizing the UI](customizing.md) — the bundled web UI, hot-reload
  development, and serving your own UI via `LONK_WEB_DIR`.
- [Deployment](deployment.md) — running `lonkd` as a systemd service or
  launchd daemon, logs, exposure caveats, backups.
