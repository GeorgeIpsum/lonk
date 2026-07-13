# lonk

Self-hosted link shortener with REST API, QR code generation, and CLI.

**[Full guide](https://georgeipsum.github.io/lonk/)** — API reference, CLI, custom headers, deployment.

## Server (`lonkd`)

```bash
cargo run --bin lonkd
```

By default the SQLite database lives next to the config at
`~/.config/lonk/lonk.db` (respects `$XDG_CONFIG_HOME` and `$LONK_CONFIG_DIR`);
set `LONK_DB` to override the path.

### API

| Endpoint            | Description                                               |
| ------------------- | --------------------------------------------------------- |
| `POST /api/links`   | `{"url": "https://…", "headers": [["Name","value"], …]}` → `201` with `{id, url, short_url, qr_url, headers}` (`headers` optional, max 16 pairs) |
| `POST /api/valid`   | `{"url": "…"}` → `200` with `{valid: true}` or `{valid: false, error}` |
| `GET /<id>`         | `303` redirect to the original URL, carrying the link's custom response headers |
| `GET /<id>/qr`      | SVG QR code for the short link                             |
| `GET /<id>/status`  | Live dead-link check → `200` with `{id, url, alive, http_status?, error?}`; `404` if the slug is unknown |

### Custom response headers

Headers stored on a link are attached to the **303 redirect response** —
they are not (and cannot be) injected into the request your browser then
makes to the destination. Useful values: `Set-Cookie` (cookie on the lonk
domain), `Cache-Control` (redirect cacheability), `Referrer-Policy`
(e.g. `no-referrer` strips the Referer sent onward), `X-*` for
proxies/middleware. Structural headers (`Location`, `Content-Length`,
`Transfer-Encoding`, `Connection`, `Content-Type`, `Content-Encoding`)
are rejected.

### Dead-link checks

`GET /<id>/status` makes the server probe the stored destination: HEAD,
falling back to GET on 405/501, up to 5 redirects, 5s timeout per attempt;
the destination is alive when the final response is 2xx. Note: this means
anyone who can reach your lonkd can make it issue requests to stored URLs
and see whether they answered — on a self-hosted instance this is the same
trust level as creating links.

## CLI (`lonk`)

One-time setup, then shorten away:

```bash
lonk setup https://s.example.com     # writes ~/.config/lonk/config.toml
lonk https://example.com/some/very/long/url
lonk --qr https://example.com/a      # + scannable QR in the terminal
lonk --qr-svg https://example.com/a > code.svg
lonk --valid https://maybe.example   # local validation only, no network
lonk -H "Set-Cookie: seen=1; Path=/" -H "Cache-Control: no-store" https://example.com/a
lonk status Ab3dEf9                        # or: lonk status https://s.example.com/Ab3dEf9
                                           # prints "alive (200)" / "dead (404)" / "dead (<error>)"
                                           # exit 0 alive · 1 dead/unknown · 2 server unreachable
```

Profiles let you target several servers:

```bash
lonk setup --profile work https://links.corp.example
lonk --profile work https://example.com/for-work
```

Multiple URLs print one short link per line, in input order; the first
invalid URL or failed request aborts the run (exit 1 for validation/usage
errors, exit 2 for network/server errors).

## Development

The workspace has three crates: `crates/lonk-core` (validation + wire
types), `crates/lonkd` (the server), `crates/lonk-cli` (the `lonk`
binary — install with `cargo install --path crates/lonk-cli`).

```bash
npm install && npm run build               # build lonk-client + the web UI (required before cargo)
cargo test --workspace                     # unit + integration tests
cd e2e && npm test                         # Playwright e2e (builds the UI, spawns lonkd)
cd e2e && BASE_URL=https://s.example.com npm test   # against a deployed instance
```

Building `lonkd` requires `web/dist` to exist — `build.rs` fails with the
command to run if it doesn't. Cargo never invokes npm itself.

## Running as a daemon

Build once, deploy one file — the web UI is embedded in the binary:

```bash
npm install && npm run build
cargo build --release
```

- **Linux (systemd):** see `deploy/lonkd.service` (install commands in its header).
- **macOS (launchd):** see `deploy/com.lonk.lonkd.plist` (starts at boot; header
  notes the login-time LaunchAgent alternative).

Both files show `LONK_WEB_DIR` (serve your own UI) and `ROCKET_ADDRESS`
(bind beyond localhost) as commented-out options.

## Hacking on the UI

The bundled UI lives in `web/` (Vite + TypeScript) and calls the API through
`packages/lonk-client`. Hot-reload development against a running server:

```bash
cargo run --bin lonkd          # API on :8000
npm -w web run dev             # UI on :5173, /api and short links proxied
```

To serve a **bespoke UI without rebuilding anything**, point `LONK_WEB_DIR`
at any directory with an `index.html`; `lonk-client` gives you typed API
calls (`createClient().createLink({ url })`, `validateUrl`, `linkStatus`).

## Roadmap

Deferred work is captured as pre-spec intent docs in
[`plans/future/`](plans/future/) — each notes the trigger that would make it
worth picking up:

- **[SQLite connection pool](plans/future/sqlite-connection-pool.md)** —
  replace the single mutex-guarded connection with a WAL pool for real
  concurrent reads/writes. Trigger: measurable contention or multi-user load.
- **[CLI polish](plans/future/cli-polish.md)** — small independent fixes
  (`--version`, `--valid` empty-arg exit code, atomic config save).
- **[Frontend & tooling hygiene](plans/future/frontend-and-tooling-hygiene.md)**
  — client robustness and test-cleanup nits.
- **[Untrusted-deployment hardening](plans/future/untrusted-multi-user-hardening.md)**
  — SSRF/open-redirect/rate-limit/auth policy, needed only if lonk ever
  serves link creators you don't trust.
