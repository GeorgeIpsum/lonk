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

| Endpoint          | Description                                               |
| ----------------- | --------------------------------------------------------- |
| `POST /api/links` | `{"url": "https://…"}` → `201` with `{id, url, short_url, qr_url}` |
| `POST /api/valid` | `{"url": "…"}` → `200` with `{valid: true}` or `{valid: false, error}` |
| `GET /<id>`       | `303` redirect to the original URL                         |
| `GET /<id>/qr`    | SVG QR code for the short link                             |

## CLI (`lonk`)

One-time setup, then shorten away:

```bash
lonk setup https://s.example.com     # writes ~/.config/lonk/config.toml
lonk https://example.com/some/very/long/url
lonk --qr https://example.com/a      # + scannable QR in the terminal
lonk --qr-svg https://example.com/a > code.svg
lonk --valid https://maybe.example   # local validation only, no network
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

```bash
cargo test --workspace                # unit + integration tests
cd e2e && npm install && npm test     # Playwright e2e (spawns lonkd)
cd e2e && BASE_URL=https://s.example.com npm test   # against a deployed instance
```
