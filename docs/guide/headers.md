---
title: Custom response headers
---

[← lonk guide](index.md)

# Custom response headers

A link can carry up to 16 custom HTTP header pairs. They're attached to
the link's **303 redirect response** — the response `lonkd` sends when
someone visits the short URL, telling their browser where to go next.

## What they can and can't do

They **can** set anything a normal HTTP response header can: a cookie
scoped to the lonk domain, a caching directive for the redirect itself, a
referrer policy, a custom marker for downstream infrastructure.

They **cannot** reach the destination. The header is on the redirect
*lonk* sends; it is not (and cannot be) injected into the follow-up
request the browser then makes to the URL you shortened. If you need a
header present on the request to the destination, it has to be set there,
not here.

## Use cases

- **`Set-Cookie`** — drop a cookie on the lonk domain when someone follows
  the link (e.g. `Set-Cookie: seen=1; Path=/`), useful for lightweight
  attribution without a database lookup on your end.
- **`Cache-Control`** — control how long browsers/CDNs cache the redirect
  itself (e.g. `Cache-Control: no-store` if the destination might change,
  or a `max-age` if it won't).
- **`Referrer-Policy`** — e.g. `Referrer-Policy: no-referrer` strips the
  `Referer` header the browser would otherwise send to the destination,
  hiding that the click came via your lonk instance.
- **`X-*`** — arbitrary markers for proxies or middleware sitting in
  front of or behind `lonkd` to key off of.

## Validation rules

Each pair is validated (by both the server and the CLI, using the same
`lonk-core` code) before it's ever stored:

- **Name:** 1–128 bytes, and every byte must be a valid HTTP token
  character (`A-Z a-z 0-9` and `` !#$%&'*+-.^_`|~ ``) — no spaces, no
  colons, no control characters.
- **Name denylist:** six structural headers are rejected regardless of
  case, because lonk (or the HTTP layer under it) already controls them
  for the redirect: `Location`, `Content-Length`, `Transfer-Encoding`,
  `Connection`, `Content-Type`, `Content-Encoding`.
- **Value:** at most 1024 bytes, and every byte must be tab or in the
  visible-ASCII range `0x20`–`0x7E` (space through `~`) — no `CR`, `LF`,
  `NUL`, or non-ASCII bytes. This rules out header injection (a value
  containing `\r\nSet-Cookie: evil=1` is rejected outright).
- **Count:** at most 16 pairs per link (`MAX_HEADERS_PER_LINK`); the
  17th pair fails the whole request with `too many headers (max 16)`.

A rejected header fails the entire `POST /api/links` (or CLI shorten)
call with `400` — no link is created with a partial header set.

## Worked example

Create a link with two custom headers:

```bash
curl -s -X POST http://127.0.0.1:8000/api/links \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://example.com/a","headers":[["Set-Cookie","seen=1; Path=/"],["Cache-Control","no-store"]]}'
```

The pairs are echoed back in the response, in the order given:

```json
{"id":"tm5kE5M","url":"https://example.com/a","short_url":"/tm5kE5M","qr_url":"/tm5kE5M/qr","headers":[["Set-Cookie","seen=1; Path=/"],["Cache-Control","no-store"]]}
```

...and carried on every subsequent redirect:

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

The same thing from the CLI, using repeatable `-H` flags — see
[the CLI guide](cli.md) for the full flag reference:

```bash
lonk -H "Set-Cookie: seen=1; Path=/" -H "Cache-Control: no-store" https://example.com/a
```
