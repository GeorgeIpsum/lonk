# lonk

Self-hosted link shortener with REST API + QR code generation.

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

`GET /<id>/status` makes the server probe the stored destination (HEAD,
falling back to GET on 405/501, up to 5 redirects, 5s timeout per attempt
(HEAD, plus the one GET retry)); the destination is alive when the final
response is 2xx. Note: this means anyone who can reach your lonkd can
make it issue requests to stored URLs and see whether they answered —
on a self-hosted instance this is
the same trust level as creating links.

## Upgrading from pre-CLI versions

Two deploy-affecting changes landed alongside the `lonk` CLI:

- The server binary was renamed from `lonk` to `lonkd`. Update whatever
  starts it (systemd unit, Dockerfile `CMD`, process manager, etc).
- The default SQLite path moved from `./lonk.db` (the process's current
  working directory) to `<config-dir>/lonk.db`
  (`~/.config/lonk/lonk.db` by default). If you relied on the old default
  and don't set `LONK_DB` explicitly, the next restart will silently open a
  fresh, empty database instead of your existing one.

  To keep your existing data, either:

  ```bash
  export LONK_DB=./lonk.db   # keep using the old path
  ```

  or move the file to the new default location:

  ```bash
  mkdir -p ~/.config/lonk
  mv ./lonk.db ~/.config/lonk/lonk.db
  ```

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
cargo test --workspace                # unit + integration tests
cd e2e && npm install && npm test     # Playwright e2e (spawns lonkd)
cd e2e && BASE_URL=https://s.example.com npm test   # against a deployed instance
```
