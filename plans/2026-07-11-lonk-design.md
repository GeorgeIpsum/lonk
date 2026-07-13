# lonk — Design Spec

**Date:** 2026-07-11
**Status:** Approved

## Overview

lonk is a self-hosted link shortener with a REST API and QR code generation,
built on the existing Rocket 0.5.1 skeleton. Scope is deliberately minimal:
create a short link, redirect through it, and fetch a QR code for it, plus a
single-page web UI.

## Stack

- **Web framework:** Rocket 0.5.1 (already present), with the `json` feature.
- **Storage:** SQLite via `rusqlite` with the `bundled` feature (no system
  SQLite dependency).
- **QR:** `qrcode` crate rendering SVG (pure Rust, no raster image encoder).
- **Validation:** `url` crate — stored URLs must parse and be `http`/`https`.
- **Slugs:** `rand` — 7 random base62 characters, retry on collision.

## Data Model

Single table:

```sql
CREATE TABLE IF NOT EXISTS links (
  id         TEXT PRIMARY KEY,
  url        TEXT NOT NULL,
  created_at TEXT NOT NULL
);
```

- DB file: `lonk.db` in the working directory; overridable via the `LONK_DB`
  environment variable.
- Connection held as `Mutex<Connection>` in Rocket managed state. No pool —
  adequate for a self-hosted, low-traffic tool.

## API

### `POST /api/links`

Request body: `{"url": "https://example.com/some/long/path"}`

- `201 Created` → `{"id": "aB3xK9q", "url": "<original>", "short_url":
  "/aB3xK9q", "qr_url": "/aB3xK9q/qr"}`
- `400 Bad Request` → `{"error": "..."}` when the URL is unparseable or its
  scheme is not `http`/`https`.

### `GET /<id>`

- `303 See Other` with `Location:` set to the stored URL.
- `404 Not Found` → `{"error": "..."}` for unknown ids.

Short links live at the root so they stay short. Rocket's `FileServer`
(rank 10) sits below dynamic routes, so `/` still serves the static page.

### `GET /<id>/qr`

- `200 OK`, `Content-Type: image/svg+xml`. SVG QR code encoding the full
  short URL, built from the request's `Host` header so it works on any
  domain the instance is hosted under.
- `404 Not Found` for unknown ids.

The existing stub routes (`GET/POST /links/create`, `GET /links/<id>`,
`GET /api`) are removed.

## Web UI

`static/index.html`, served at `/` by the existing `FileServer`. One form:
paste a URL, plain-JS `fetch` to `POST /api/links`, then display the short
link (as a clickable anchor) and the QR image inline. No build tooling, no
external assets.

## Error Handling

- All API errors are JSON: `{"error": "<message>"}`.
- Slug collision: regenerate and retry (bounded, e.g. 8 attempts) before
  returning `500`.
- DB open failure at boot: fail fast with a clear message.

## Testing

Rocket local `Client` integration tests against a temp-file DB:

1. Create → response shape correct (`201`, id present, urls formed).
2. Redirect round-trip: `GET /<id>` returns `303` with the original URL.
3. `GET /<id>/qr` returns `200` with `image/svg+xml`.
4. `POST /api/links` with a bad URL (unparseable / `ftp://`) returns `400`.
5. `GET /<unknown>` and `GET /<unknown>/qr` return `404`.
