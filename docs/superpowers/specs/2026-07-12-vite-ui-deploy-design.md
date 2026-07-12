# Design: Vite web UI (embedded, overridable), TS client package, daemon deploy files

Date: 2026-07-12
Status: approved (pending spec review)
Branch: `feat/vite-ui-and-deploy`, stacked on `feat/cli-and-e2e` (PR #2).

## Goal

Five related deliverables:

1. Rebuild the web UI as a Vite (vanilla TypeScript) project in `web/`,
   splitting today's single `static/index.html` into hackable sources.
2. Embed the built UI into the `lonkd` binary (`rust-embed`), with a
   `LONK_WEB_DIR` runtime override for serving a bespoke UI from disk —
   no recompile needed to customize.
3. A small zero-dependency TypeScript client package
   (`packages/lonk-client`) wrapping the lonk API, dogfooded by `web/`
   and usable by anyone building a bespoke UI.
4. `deploy/` daemon files (systemd unit + launchd LaunchDaemon plist) and
   README sections so `lonkd` runs at boot on Linux and macOS.
5. A markdown docs guide under `docs/guide/`, published to GitHub Pages by
   a GitHub Actions workflow (plain markdown + Jekyll, no local tooling).

## Decisions already made (with the user)

- Frontend stack: **Vite vanilla-ts**, no framework.
- Build wiring: **the convention, not build.rs-runs-npm** — the frontend is
  built by an explicit `npm run build`; `crates/lonkd/build.rs` only
  *checks* that `web/dist/index.html` exists and fails compilation with the
  exact command to run. Node is never invoked by cargo.
- Embedded by default; `LONK_WEB_DIR` overrides at runtime.
- Client package included, dogfooded by the web UI.

## npm workspaces (root)

A root `package.json` with `"workspaces": ["web", "packages/lonk-client", "e2e"]`
and `"private": true`. Scripts:

```json
{
  "scripts": {
    "build": "npm -w lonk-client run build && npm -w web run build"
  }
}
```

The root `.gitignore`'s "## Node" section (rust-lang boilerplate that
blanket-ignores `package.json`, `package-lock.json`, `node_modules` at any
depth — the trap the e2e task hit) is replaced with correct entries:
`node_modules/` ignored everywhere; manifests and lockfile tracked. The
negation lines in `e2e/.gitignore` become unnecessary and are removed.
One root `package-lock.json` replaces `e2e/package-lock.json` (workspaces
hoist), and e2e keeps working via `npm install` at root.

## `web/` — the UI package

```
web/
  package.json        # name "web", private; scripts: dev, build (tsc && vite build), preview
  tsconfig.json       # vite vanilla-ts defaults, strict
  vite.config.ts
  index.html          # markup only — same element IDs as today
  src/main.ts         # typed DOM logic, uses lonk-client
  src/style.css       # today's inline styles, extracted
```

- Behavior and element IDs are ported 1:1 from `crates/lonkd/static/index.html`
  (`#form`, `#url`, `#error`, `#result`, `#short`, `#copy`, `#qr`,
  `#advanced`, `#headers`, `#add-header`, `.h-name`, `.h-value`,
  `.header-row`) so all seven Playwright specs and the
  `index_serves_create_form` API test pass unchanged.
- `main.ts` calls the API through `lonk-client` (dogfooding) instead of raw
  `fetch`.
- `vite.config.ts`: `build.outDir = "dist"`, and a dev proxy so
  `npm -w web run dev` against a running `lonkd` is a full hot-reload loop:

```ts
server: {
  proxy: {
    '/api': 'http://127.0.0.1:8000',
    '^/[A-Za-z0-9]{7}(/(qr|status))?$': 'http://127.0.0.1:8000',
  },
},
```

- `crates/lonkd/static/` is deleted.

## `packages/lonk-client` — the TypeScript companion package

Zero-dependency, fetch-based, ESM. `package.json` name `lonk-client`
(private for now — publishing is a later decision), `"exports"` pointing at
`dist/index.js` + `dist/index.d.ts`, built by `tsc`.

`src/index.ts` (whole public surface):

```ts
export type HeaderPair = [name: string, value: string];
export interface CreateLinkRequest { url: string; headers?: HeaderPair[] }
export interface LinkResponse {
  id: string; url: string; short_url: string; qr_url: string; headers: HeaderPair[];
}
export interface ValidResponse { valid: boolean; error?: string }
export interface StatusResponse {
  id: string; url: string; alive: boolean; http_status?: number; error?: string;
}

export class LonkError extends Error { constructor(message: string, readonly status?: number) }

export function createClient(baseUrl?: string): LonkClient
export interface LonkClient {
  createLink(req: CreateLinkRequest): Promise<LinkResponse>;
  validateUrl(url: string): Promise<ValidResponse>;
  linkStatus(id: string): Promise<StatusResponse>;
  shortUrl(id: string): string;   // `${base}/${id}`
  qrUrl(id: string): string;      // `${base}/${id}/qr`
}
```

- `baseUrl` defaults to `""` (same-origin — what `web/` uses); a bespoke UI
  served from `LONK_WEB_DIR` is also same-origin, so the default covers the
  main audience. Non-2xx responses throw `LonkError` carrying the server's
  `{error}` message when present.
- Types mirror `lonk_core::types` by hand; the wire contract is 4 small
  shapes and changes rarely. No codegen (YAGNI).
- Tested by dogfood: `web/` uses it for every API call, and the Playwright
  suite drives `web/` in a real browser. No separate unit-test rig.

## Embedding + runtime override (crates/lonkd)

- New dependency: `rust-embed = "8"`.
- `src/web_assets.rs` (new module, replaces the `FileServer` mount):

```rust
#[derive(rust_embed::RustEmbed)]
#[folder = "../../web/dist"]
struct Assets;

/// Where the UI comes from, resolved once at startup.
pub enum WebSource { Embedded, Dir(PathBuf) }  // from LONK_WEB_DIR
```

  Two routes, both branching on the managed `WebSource`:
  - `GET /` → `index.html` (from dir if overridden, else embedded).
  - `GET /<path..>` with `rank = 20` → asset lookup by relative path,
    `ContentType::from_extension`, 404 JSON (existing catcher) when absent.
    Directory traversal is not a concern for the embedded branch (lookup is
    by exact embedded key); the dir branch canonicalizes and rejects paths
    escaping the root.
- Route precedence is unchanged from FileServer days: `/<id>` still wins
  single segments; multi-segment asset paths (`assets/index-*.js`) fall to
  the catch-all.
- `rocket_app` gains no new parameters: `WebSource` is resolved inside it
  from `LONK_WEB_DIR` (unset/empty → Embedded) and managed as state.
- rust-embed serves from disk automatically in debug builds (its default),
  so `npm run build` output is picked up without recompiling during dev.
- `crates/lonkd/build.rs`: emits `cargo:rerun-if-changed=../../web/dist`
  and panics with `web/dist not found — run: npm install && npm run build
  (requires node)` when `web/dist/index.html` is missing. Nothing else.

## `deploy/`

- `deploy/lonkd.service`: `DynamicUser=yes`, `StateDirectory=lonk`,
  `Environment=LONK_DB=/var/lib/lonk/lonk.db`, `Restart=on-failure`,
  `After=network.target`, commented-out `ROCKET_ADDRESS=0.0.0.0` (with a
  pointer to the README's probe-oracle note) and commented
  `LONK_WEB_DIR=/var/lib/lonk/web` example.
- `deploy/com.lonk.lonkd.plist`: LaunchDaemon (at-boot, per requirement),
  `RunAtLoad` + `KeepAlive`, `ProgramArguments → /usr/local/bin/lonkd`,
  `EnvironmentVariables` with explicit `LONK_DB=/usr/local/var/lonk/lonk.db`
  (root's `$HOME` makes the config-dir default wrong for daemons), commented
  `LONK_WEB_DIR` example; a comment notes the `~/Library/LaunchAgents`
  alternative for login-time user agents.
- README gains:
  - "Running as a daemon": build (`npm install && npm run build`, then
    `cargo build --release`), copy the single binary, `systemctl enable
    --now lonkd` / `launchctl bootstrap system deploy/com.lonk.lonkd.plist`.
  - "Hacking on the UI": the dev-proxy loop, the two-step build, and the
    bespoke-UI story (`LONK_WEB_DIR` + `lonk-client`).
  - Development section: note that building `lonkd` now requires the web
    build to exist (the build.rs check and its message).

## Docs guide (GitHub Pages)

Decisions (with the user): **plain markdown + Jekyll** (no local doc
toolchain — GitHub's builder renders it), deployed by a **GitHub Actions
workflow** (the repo's first CI file).

- Content lives in `docs/guide/` — deliberately a subdirectory, so the
  internal planning docs in `docs/superpowers/` are never published.
- Pages (each starts with a small front-matter block and a one-line nav
  linking back to the index; Pages' built-in themes have no sidebar):
  - `index.md` — what lonk is, quick start (server + CLI in five commands)
  - `server.md` — running `lonkd`, env vars (`LONK_DB`, `LONK_CONFIG_DIR`,
    `LONK_WEB_DIR`, `ROCKET_*`), full API reference (the README table,
    expanded with request/response examples)
  - `cli.md` — setup, profiles, shortening, `-H`, `--qr`/`--qr-svg`,
    `--valid`, `status`, exit codes
  - `headers.md` — response-headers guide: what redirect response headers
    can and cannot do, use cases, validation rules and denylist
  - `customizing.md` — hacking the bundled UI (Vite dev loop), serving a
    bespoke UI via `LONK_WEB_DIR`, using `lonk-client`
  - `deployment.md` — the daemon guides (systemd + launchd), single-binary
    builds, the status probe-oracle note
- `docs/guide/_config.yml`: `theme: jekyll-theme-cayman` (a Pages built-in),
  site title/description. Nothing else.
- `.github/workflows/docs.yml`: on push to `main` touching `docs/guide/**`
  (plus `workflow_dispatch`), `actions/checkout` →
  `actions/jekyll-build-pages` with `source: ./docs/guide` →
  `actions/upload-pages-artifact` → `actions/deploy-pages`, with the
  standard `pages: write` / `id-token: write` permissions and a
  `github-pages` environment.
- One-time manual step (documented in the workflow header comment): repo
  Settings → Pages → Source = "GitHub Actions".
- Content accuracy rule: every command, flag, path, and response shape in
  the guide must exist exactly as documented (same standard as the README
  task last plan). The guide complements the README; the README stays the
  short version and gains a link to the published guide.

## Testing

- Existing suites are the primary gates: `cargo test --workspace` (the
  `index_serves_create_form` test now exercises the embedded index) and
  the 7 Playwright specs (a real browser executing the built `main.ts`
  bundle — which now transitively exercises `lonk-client`).
- e2e `package.json` gains `"pretest": "npm --prefix .. run build"` (the
  root workspace build) so `cd e2e && npm test` remains one command; the
  Playwright `webServer` entry is unchanged.
- New live test `crates/lonkd/tests/web_override_live.rs`, **port 8912**
  (per the one-port-per-server-spawning-test invariant): spawn `lonkd`
  with `LONK_WEB_DIR` pointing at a temp dir containing a marker
  `index.html` → `GET /` returns the marker content; then, after dropping
  that ServerGuard, a second sequential spawn on the same port 8912 without
  the env var asserts the embedded default serves the real UI (one test fn,
  one server alive at a time).
- The redirect/API tests are untouched — this design changes only how the
  UI bytes are produced and served.

## Port registry addition

8912: `crates/lonkd/tests/web_override_live.rs`.

## Out of scope

- Publishing `lonk-client` to npm (package stays `private: true`).
- Any visual redesign of the UI — this is a mechanical split.
- Windows service files.
