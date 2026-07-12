---
title: Customizing the UI
---

[← lonk guide](index.md)

# Customizing the UI

## Repo layout

- **`web/`** — the bundled UI: Vite + TypeScript, a plain form that calls
  the API and shows the short link, QR code, and a header-pairs editor.
  Builds to `web/dist/`, which `lonkd` embeds into its binary at compile
  time.
- **`packages/lonk-client`** — a small, dependency-free TypeScript client
  (`createClient()`) that `web/` (and any other TypeScript UI) can call
  instead of hand-rolling `fetch` calls. It's a private npm workspace
  package, not published to a registry.

## Hot-reload development

Run the API and the UI dev server side by side:

```bash
cargo run --bin lonkd          # API on :8000
npm -w web run dev             # UI on :5173, /api and short links proxied
```

Vite's dev server (`web/vite.config.ts`) proxies both `/api/*` and the
short-link routes (`/<7-char-id>`, optionally followed by `/qr` or
`/status`) to `http://127.0.0.1:8000`, so links created through the UI at
`:5173` are clickable and QR codes resolve, without a CORS dance.

## Building

```bash
npm install && npm run build
```

is two steps under the hood (the root `package.json`'s `build` script):
`lonk-client` compiles first (`tsc`), then `web` builds against it
(`tsc && vite build`), producing `web/dist/`.

`lonkd`'s `build.rs` checks for `web/dist/index.html` before compiling
and fails loudly if it's missing:

```
web/dist not found - build the web UI first: npm install && npm run build (requires node)
```

Cargo never invokes npm itself — the two toolchains stay separate; you run
the npm build once (or whenever `web/` or `packages/lonk-client` change),
then `cargo build`/`cargo run` picks up whatever is in `web/dist/`.

## Serving a bespoke UI (`LONK_WEB_DIR`)

To replace the UI entirely, without touching Rust or Node at all, point
`LONK_WEB_DIR` at a directory containing your own `index.html` (and any
other static assets):

```bash
LONK_WEB_DIR=/path/to/your/ui cargo run --bin lonkd
```

`lonkd` serves `index.html` for `/` and any other file under that
directory by relative path, with no rebuild of the binary and no npm
step required — it works with **any** static files, not just something
built from `web/`'s toolchain. This is what the [deploy
files](deployment.md) show as a commented-out option for production use.

### Route precedence

Short-link routes are matched before UI-asset routes, so a handful of
`LONK_WEB_DIR` paths never reach your files:

- **Top-level files without a dot in the name** (e.g. `logo`, `LICENSE`)
  are captured by the redirect route (`GET /<id>`), because that route's
  id guard only rejects params containing a `.` — a dot-free filename
  looks exactly like a link id to it. Since it's not a real id, you get
  `404 {"error":"no such link"}` instead of the file. Give asset files
  extensions (`logo.svg`, `LICENSE.txt`) to avoid this.
- **Two-segment paths shaped like `/<something>/qr` or
  `/<something>/status`** are captured by the QR-code and dead-link-check
  routes (`GET /<id>/qr`, `GET /<id>/status`) regardless of whether
  `<something>` is a real link id — so e.g. `docs/status` in your
  `LONK_WEB_DIR` is unreachable and also 404s with `{"error":"no such
  link"}`. Avoid asset paths shaped like this.
- **Dotfiles and dot-directories** (e.g. `.well-known/...`) are rejected
  by the router itself before any route body runs, and can never be
  served from `LONK_WEB_DIR` — you'll get a `422` with a generic JSON
  error body, not your file.
- Any other dotted path segment that isn't a real link id and isn't a
  real asset (e.g. a `favicon.ico` you haven't added yet) 404s with the
  generic JSON error body (`{"error":"Not Found"}`) rather than `"no
  such link"` — that message is specific to the redirect route, which
  dotted single segments never reach.

## A minimal bespoke UI

Since `lonk-client` is a private, unpublished package, a UI with no build
step at all can't `import` it from a CDN — call the JSON API directly
with `fetch`. This is a complete, working `index.html`:

```html
<!doctype html>
<html>
  <body>
    <input id="url" placeholder="https://example.com/a/long/url" size="50" />
    <button id="go">Shorten</button>
    <p id="out"></p>
    <script type="module">
      document.querySelector('#go').addEventListener('click', async () => {
        const url = document.querySelector('#url').value;
        const res = await fetch('/api/links', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ url }),
        });
        const body = await res.json();
        document.querySelector('#out').textContent = res.ok
          ? location.origin + body.short_url
          : `error: ${body.error}`;
      });
    </script>
  </body>
</html>
```

Drop that in a directory and point `LONK_WEB_DIR` at it — no build step.

If your project already has a bundler and vendors (or workspace-links)
`lonk-client`, the equivalent using `createClient()` is:

```html
<!-- requires a bundler or import map: 'lonk-client' is a bare specifier -->
<!doctype html>
<html>
  <body>
    <input id="url" placeholder="https://example.com/a/long/url" size="50" />
    <button id="go">Shorten</button>
    <p id="out"></p>
    <script type="module">
      import { createClient } from 'lonk-client';

      const client = createClient(); // '' = same-origin, correct under LONK_WEB_DIR
      document.querySelector('#go').addEventListener('click', async () => {
        const url = document.querySelector('#url').value;
        try {
          const link = await client.createLink({ url });
          document.querySelector('#out').textContent = location.origin + client.shortUrl(link.id);
        } catch (err) {
          document.querySelector('#out').textContent = `error: ${err.message}`;
        }
      });
    </script>
  </body>
</html>
```

`createClient()` also exposes `validateUrl(url)` and `linkStatus(id)`,
mirroring `POST /api/valid` and `GET /<id>/status` — see [Running the
server](server.md) for the full API reference.
