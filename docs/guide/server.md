---
title: Running the server
---

[← lonk guide](index.md)

# Running the server

```bash
cargo run --bin lonkd
```

`lonkd` serves the REST API and the web UI from the same port (the web UI
is embedded in the binary — see [Customizing the UI](customizing.md) to
swap it out or run it separately in development). Data is stored in a
single SQLite file.

## Environment variables

| Variable          | Meaning                                                        | Default |
| ------------------ | --------------------------------------------------------------- | ------- |
| `LONK_CONFIG_DIR`  | Directory used to resolve the default `LONK_DB` path.           | `$XDG_CONFIG_HOME/lonk` if set, else `~/.config/lonk` |
| `LONK_DB`          | Path to the SQLite database file.                                | `<LONK_CONFIG_DIR>/lonk.db` (the directory is created automatically when this default is used; if you set `LONK_DB` explicitly, its parent directory must already exist) |
| `LONK_WEB_DIR`     | Serve the web UI from this directory instead of the build embedded in the binary. Any non-empty value switches to directory mode; unset (or empty) serves the embedded UI. Short-link routes take precedence over your files for some paths — see [Route precedence](customizing.md#route-precedence). | unset (embedded UI) |
| `ROCKET_ADDRESS`   | Address `lonkd` binds to.                                        | `127.0.0.1` |
| `ROCKET_PORT`      | Port `lonkd` binds to.                                           | `8000` |

`LONK_CONFIG_DIR` and `LONK_DB` are independent: `LONK_CONFIG_DIR` only
matters for computing the *default* database path (and, for the CLI, its
`config.toml`); setting `LONK_DB` directly overrides that computation
entirely.

## API reference

| Endpoint            | Description                                               |
| ------------------- | --------------------------------------------------------- |
| `POST /api/links`   | Create a short link, optionally with custom response headers. |
| `POST /api/valid`   | Validate a URL locally, no link is created.                |
| `GET /<id>`          | `303` redirect to the original URL, carrying the link's custom response headers. |
| `GET /<id>/qr`       | SVG QR code that encodes the short link's URL.              |
| `GET /<id>/status`   | Live dead-link check against the stored destination.        |

All error responses share one shape: `{"error": "<message>"}`, with an
HTTP status appropriate to the failure (`400` for validation errors, `404`
for unknown links, `500` for server-side failures).

### `POST /api/links`

Body: `{"url": "<http(s) url>", "headers": [["Name", "value"], ...]}` —
`headers` is optional and defaults to `[]`; at most 16 pairs (see
[Custom response headers](headers.md) for the validation rules).

Without headers:

```bash
curl -s -X POST http://127.0.0.1:8000/api/links \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://example.com/some/long/url"}'
```

```json
{"id":"QnbppZy","url":"https://example.com/some/long/url","short_url":"/QnbppZy","qr_url":"/QnbppZy/qr","headers":[]}
```

With headers:

```bash
curl -s -X POST http://127.0.0.1:8000/api/links \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://example.com/a","headers":[["Set-Cookie","seen=1; Path=/"],["Cache-Control","no-store"]]}'
```

```json
{"id":"tm5kE5M","url":"https://example.com/a","short_url":"/tm5kE5M","qr_url":"/tm5kE5M/qr","headers":[["Set-Cookie","seen=1; Path=/"],["Cache-Control","no-store"]]}
```

The response is `201 Created`. Rejected input comes back `400`, e.g. too
many header pairs:

```json
{"error":"too many headers (max 16)"}
```

or an invalid URL:

```json
{"error":"invalid url: relative URL without a base"}
```

### `POST /api/valid`

Body: `{"url": "<string>"}`. Always `200`; validates locally, no link is
created and no network request is made.

```bash
curl -s -X POST http://127.0.0.1:8000/api/valid \
  -H 'Content-Type: application/json' -d '{"url":"https://example.com/a"}'
# {"valid":true}

curl -s -X POST http://127.0.0.1:8000/api/valid \
  -H 'Content-Type: application/json' -d '{"url":"not a url"}'
# {"valid":false,"error":"invalid url: relative URL without a base"}
```

### `GET /<id>`

`303 See Other` to the stored URL. Any custom response headers saved on
the link are attached to this response (see
[Custom response headers](headers.md)):

```bash
curl -i -s http://127.0.0.1:8000/QnbppZy
```

```
HTTP/1.1 303 See Other
location: https://example.com/some/long/url
content-length: 0
```

```bash
curl -i -s http://127.0.0.1:8000/tm5kE5M
```

```
HTTP/1.1 303 See Other
location: https://example.com/a
set-cookie: seen=1; Path=/
cache-control: no-store
content-length: 0
```

Unknown ids return `404`: `{"error":"no such link"}`.

### `GET /<id>/qr`

Returns `200` with `Content-Type: image/svg+xml` and an SVG QR code that
encodes the full short URL (scheme + host from the request, or
`X-Forwarded-Proto`/`Host` behind a proxy):

```bash
curl -s http://127.0.0.1:8000/QnbppZy/qr | head -c 120
# <?xml version="1.0" standalone="yes"?><svg xmlns="http://www.w3.org/2000/svg" ...
```

Unknown ids return `404`: `{"error":"no such link"}`.

### `GET /<id>/status`

Makes `lonkd` probe the stored destination live (see the note below) and
reports whether it's reachable. Always `200` for a known id; `404` for an
unknown one.

Alive:

```bash
curl -s http://127.0.0.1:8000/jtrstNQ/status
```

```json
{"id":"jtrstNQ","url":"https://example.com/","alive":true,"http_status":200}
```

Dead, with an HTTP status from the destination:

```json
{"id":"LZiPehB","url":"https://example.com/nonexistent-lonk-test-path","alive":false,"http_status":404}
```

Dead/unknown, transport failure (DNS, connection refused, timeout — no
HTTP status to report):

```json
{"id":"fzpAKv9","url":"https://this-domain-should-not-exist-lonk-test.invalid/","alive":false,"error":"https://this-domain-should-not-exist-lonk-test.invalid/: Dns Failed: resolve dns name 'this-domain-should-not-exist-lonk-test.invalid:443': failed to lookup address information: nodename nor servname provided, or not known"}
```

(the exact transport-error text is platform-dependent; the shape — `alive:
false` with `error` set and no `http_status` — is what's guaranteed.)

Unknown id:

```bash
curl -s -i http://127.0.0.1:8000/doesnotexist/status
```

```
HTTP/1.1 404 Not Found
content-type: application/json

{"error":"no such link"}
```

The probe is HEAD, falling back to a one-shot GET on `405`/`501`, up to 5
redirects, 5s timeout; `alive` is true when the final response is `2xx`.
**Note:** anyone who can reach your `lonkd` can use `/status` to make it
issue requests to any URL that's ever been shortened and observe whether
it answered — on a self-hosted instance this is the same trust level as
being able to create links, but it's worth knowing before binding
`ROCKET_ADDRESS` beyond localhost (see [Deployment](deployment.md)).
