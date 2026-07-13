# Vite web UI + lonk-client + daemon deploy + docs guide Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the web UI as a Vite vanilla-TS project embedded in `lonkd` (with a `LONK_WEB_DIR` runtime override), add the `lonk-client` TS package, ship systemd/launchd deploy files, and publish a markdown docs guide via GitHub Pages — per `plans/2026-07-12-vite-ui-deploy-design.md`.

**Architecture:** Root npm workspaces (`packages/lonk-client`, `web`, `e2e`) own everything Node; cargo never invokes npm — `crates/lonkd/build.rs` only verifies `web/dist` exists. `rust-embed` bakes `web/dist` into the binary; a `WebSource` managed state (Embedded vs Dir from `LONK_WEB_DIR`) lets bespoke UIs be served from disk without recompiling. Deploy files and the Pages workflow are pure additions.

**Tech Stack:** Vite 6 (vanilla-ts), TypeScript ~5.6, rust-embed 8, Rocket 0.5, GitHub Actions Pages deployment (Jekyll, cayman theme).

## Global Constraints

- Branch `feat/vite-ui-and-deploy` (stacked on `feat/cli-and-e2e`). Rust: 2-space indent, `cargo fmt` before commits. Superpowers docs live in `plans/` (CLAUDE.md).
- **Cargo must never invoke node/npm.** The frontend is built by `npm run build` at the repo root; `build.rs` only checks `web/dist/index.html` exists and panics with: `web/dist not found - build the web UI first: npm install && npm run build (requires node)`.
- Element IDs/classes are preserved verbatim from the current UI (all seven Playwright specs and `index_serves_create_form` must pass unmodified): `#form`, `#url`, `#error`, `#result`, `#short`, `#copy`, `#qr`, `#advanced`, `#headers`, `#add-header`, `.header-row`, `.h-name`, `.h-value`.
- Port registry (one port per server-spawning test): 8907/8908/8909/8910 (cli_live), 8911 (status_live), **8912 (web_override_live, this plan)**, 8808 (Playwright).
- Docs accuracy rule: every command, flag, path, env var, and response shape documented must exist exactly as written.
- `LONK_WEB_DIR` unset or empty → embedded assets; set → serve from that directory (canonicalized; paths escaping the root rejected).
- Suites that gate every task: `cargo test --workspace` and (where stated) `cd e2e && npm test` = 7 passed.

---

### Task 1: Root npm workspaces, .gitignore repair, `lonk-client` package

**Files:**
- Modify: `.gitignore` (the "## Node" section), `e2e/.gitignore`
- Delete: `e2e/package-lock.json`
- Create: `package.json` (root), `packages/lonk-client/package.json`, `packages/lonk-client/tsconfig.json`, `packages/lonk-client/src/index.ts`

**Interfaces:**
- Consumes: nothing.
- Produces: npm workspace root (lockfile at `/package-lock.json`); package `lonk-client` exporting `createClient(baseUrl?: string): LonkClient`, types `HeaderPair`, `CreateLinkRequest`, `LinkResponse`, `ValidResponse`, `StatusResponse`, class `LonkError extends Error { status?: number }`. Task 2's `web/` imports all of these.

- [ ] **Step 1: Repair the root .gitignore's Node section**

The root `.gitignore` is rust-lang boilerplate whose `## Node` section blanket-ignores `package.json`, `package-lock.json`, and `node_modules` at any depth (the trap the e2e task hit). Replace that section's three bare entries with:

```
## Node
node_modules/
web/dist/
packages/lonk-client/dist/
```

In `e2e/.gitignore`, delete the negation lines (`!package.json`, `!package-lock.json`) and their explanatory comment — they existed only to fight the root ignore. Keep `node_modules/`, `test-results/`, `playwright-report/`.

- [ ] **Step 2: Create the workspace root and remove the e2e lockfile**

`package.json` (repo root):

```json
{
  "name": "lonk-workspace",
  "private": true,
  "workspaces": ["packages/lonk-client", "e2e"],
  "scripts": {
    "build": "npm -w lonk-client run build"
  }
}
```

(`web` joins `workspaces` and the build chain in Task 2.)

```bash
git rm e2e/package-lock.json
```

- [ ] **Step 3: Write lonk-client**

`packages/lonk-client/package.json`:

```json
{
  "name": "lonk-client",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "exports": {
    ".": { "types": "./dist/index.d.ts", "import": "./dist/index.js" }
  },
  "files": ["dist"],
  "scripts": { "build": "tsc" },
  "devDependencies": { "typescript": "~5.6.0" }
}
```

`packages/lonk-client/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "lib": ["ES2022", "DOM"],
    "strict": true,
    "declaration": true,
    "outDir": "dist",
    "skipLibCheck": true
  },
  "include": ["src"]
}
```

`packages/lonk-client/src/index.ts`:

```ts
/** A single response-header pair attached to a link's 303 redirect. */
export type HeaderPair = [name: string, value: string];

export interface CreateLinkRequest {
  url: string;
  headers?: HeaderPair[];
}

export interface LinkResponse {
  id: string;
  url: string;
  short_url: string;
  qr_url: string;
  headers: HeaderPair[];
}

export interface ValidResponse {
  valid: boolean;
  error?: string;
}

export interface StatusResponse {
  id: string;
  url: string;
  alive: boolean;
  http_status?: number;
  error?: string;
}

/** Thrown for transport failures (no `status`) and non-2xx responses (with `status`). */
export class LonkError extends Error {
  constructor(
    message: string,
    readonly status?: number,
  ) {
    super(message);
    this.name = 'LonkError';
  }
}

export interface LonkClient {
  createLink(req: CreateLinkRequest): Promise<LinkResponse>;
  validateUrl(url: string): Promise<ValidResponse>;
  linkStatus(id: string): Promise<StatusResponse>;
  /** `${base}/${id}` — the short link itself. */
  shortUrl(id: string): string;
  /** `${base}/${id}/qr` — the SVG QR code. */
  qrUrl(id: string): string;
}

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(url, init);
  } catch {
    throw new LonkError('request failed - is the server up?');
  }
  const body = await res.json().catch(() => undefined);
  if (!res.ok) {
    const message =
      body && typeof body.error === 'string'
        ? body.error
        : `server returned ${res.status}`;
    throw new LonkError(message, res.status);
  }
  return body as T;
}

const JSON_POST = { method: 'POST', headers: { 'Content-Type': 'application/json' } };

/**
 * Create a client for a lonk server. `baseUrl` defaults to "" (same-origin),
 * which is correct both for the bundled UI and for bespoke UIs served via
 * LONK_WEB_DIR. Pass e.g. "https://s.example.com" for cross-origin use.
 */
export function createClient(baseUrl = ''): LonkClient {
  const base = baseUrl.replace(/\/+$/, '');
  return {
    createLink: (req) =>
      request<LinkResponse>(`${base}/api/links`, { ...JSON_POST, body: JSON.stringify(req) }),
    validateUrl: (url) =>
      request<ValidResponse>(`${base}/api/valid`, { ...JSON_POST, body: JSON.stringify({ url }) }),
    linkStatus: (id) => request<StatusResponse>(`${base}/${id}/status`),
    shortUrl: (id) => `${base}/${id}`,
    qrUrl: (id) => `${base}/${id}/qr`,
  };
}
```

- [ ] **Step 4: Install and build**

Run: `npm install` (repo root)
Expected: root `package-lock.json` created; `node_modules/` hoisted; no errors.

Run: `npm run build`
Expected: `packages/lonk-client/dist/index.js` and `index.d.ts` exist.

Run: `git status --short`
Expected: root `package.json` + `package-lock.json` show as new (NOT ignored — proves the gitignore repair); no `dist/` or `node_modules/` entries.

- [ ] **Step 5: Smoke the e2e workspace migration**

Run: `cd e2e && npx playwright --version`
Expected: prints a version (dependency resolves through the hoisted root `node_modules`). The full suite runs in Task 3.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: npm workspace root and lonk-client typescript package"
```

---

### Task 2: `web/` — the Vite UI, consuming lonk-client

**Files:**
- Create: `web/package.json`, `web/tsconfig.json`, `web/vite.config.ts`, `web/index.html`, `web/src/main.ts`, `web/src/style.css`
- Modify: root `package.json` (workspaces + build chain)
- Reference (port from, do not modify yet): `crates/lonkd/static/index.html`

**Interfaces:**
- Consumes: `lonk-client` (Task 1) via the workspace.
- Produces: `npm run build` (root) → `web/dist/` containing `index.html` + hashed `assets/*`. Task 3 embeds exactly this directory.

- [ ] **Step 1: Add web to the workspace**

Root `package.json` becomes:

```json
{
  "name": "lonk-workspace",
  "private": true,
  "workspaces": ["packages/lonk-client", "web", "e2e"],
  "scripts": {
    "build": "npm -w lonk-client run build && npm -w web run build"
  }
}
```

- [ ] **Step 2: Write the web package**

`web/package.json`:

```json
{
  "name": "web",
  "private": true,
  "version": "0.0.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview"
  },
  "dependencies": { "lonk-client": "*" },
  "devDependencies": { "typescript": "~5.6.0", "vite": "^6.0.0" }
}
```

`web/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noEmit": true,
    "isolatedModules": true,
    "skipLibCheck": true
  },
  "include": ["src"]
}
```

`web/vite.config.ts`:

```ts
import { defineConfig } from 'vite';

// Dev loop: run `cargo run --bin lonkd` (port 8000), then `npm -w web run dev`.
// API calls and short-link routes proxy through so links are clickable.
export default defineConfig({
  server: {
    proxy: {
      '/api': 'http://127.0.0.1:8000',
      '^/[A-Za-z0-9]{7}(/(qr|status))?$': 'http://127.0.0.1:8000',
    },
  },
});
```

`web/index.html` — the current markup (element IDs verbatim from `crates/lonkd/static/index.html`), styles and script extracted:

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>lonk</title>
</head>
<body>
  <h1>lonk</h1>
  <p>Paste a long URL, get a short one — with a QR code.</p>
  <form id="form">
    <input type="url" id="url" placeholder="https://example.com/very/long/url" required>
    <button type="submit">Shorten</button>
  </form>
  <p id="error" role="alert" hidden></p>
  <section id="result" hidden>
    <a id="short" href=""></a>
    <button id="copy" type="button">Copy</button>
    <img id="qr" alt="QR code for the short link" width="256" height="256">
  </section>
  <details id="advanced">
    <summary>Advanced: response headers</summary>
    <p class="hint">Sent with the 303 redirect response (e.g. Set-Cookie, Cache-Control, Referrer-Policy) — not to the destination.</p>
    <div id="headers"></div>
    <button id="add-header" type="button">Add header</button>
  </details>
  <script type="module" src="/src/main.ts"></script>
</body>
</html>
```

`web/src/style.css` — today's styles, extracted verbatim (including the advanced-section additions):

```css
body { font-family: system-ui, sans-serif; max-width: 40rem; margin: 4rem auto; padding: 0 1rem; }
form { display: flex; gap: 0.5rem; }
input[type="url"] { flex: 1; padding: 0.5rem; font-size: 1rem; }
button { padding: 0.5rem 1rem; font-size: 1rem; cursor: pointer; }
#result { margin-top: 2rem; }
#result a { font-size: 1.25rem; word-break: break-all; }
#qr { display: block; margin-top: 1rem; }
#error { color: #b00020; }
[hidden] { display: none; }
#advanced { margin-top: 1rem; }
#advanced .hint { font-size: 0.85rem; color: #666; }
.header-row { display: flex; gap: 0.5rem; margin-bottom: 0.5rem; }
.header-row input { flex: 1; padding: 0.35rem; }
```

`web/src/main.ts` — the current inline script, typed, with API calls through lonk-client (dogfooding):

```ts
import './style.css';
import { createClient, type HeaderPair } from 'lonk-client';

const client = createClient();

const form = document.querySelector<HTMLFormElement>('#form')!;
const urlInput = document.querySelector<HTMLInputElement>('#url')!;
const error = document.querySelector<HTMLParagraphElement>('#error')!;
const result = document.querySelector<HTMLElement>('#result')!;
const short = document.querySelector<HTMLAnchorElement>('#short')!;
const qr = document.querySelector<HTMLImageElement>('#qr')!;
const headersBox = document.querySelector<HTMLDivElement>('#headers')!;

document.querySelector<HTMLButtonElement>('#add-header')!.addEventListener('click', () => {
  const row = document.createElement('div');
  row.className = 'header-row';
  row.innerHTML =
    '<input class="h-name" placeholder="Header-Name">' +
    '<input class="h-value" placeholder="value">';
  headersBox.appendChild(row);
});

function collectHeaders(): HeaderPair[] {
  return [...headersBox.querySelectorAll<HTMLDivElement>('.header-row')]
    .map((row): HeaderPair => [
      row.querySelector<HTMLInputElement>('.h-name')!.value.trim(),
      row.querySelector<HTMLInputElement>('.h-value')!.value.trim(),
    ])
    .filter(([name]) => name !== '');
}

form.addEventListener('submit', async (e) => {
  e.preventDefault();
  error.hidden = true;
  result.hidden = true;
  try {
    const link = await client.createLink({ url: urlInput.value, headers: collectHeaders() });
    short.href = link.short_url;
    short.textContent = location.origin + link.short_url;
    qr.src = link.qr_url;
    result.hidden = false;
  } catch (err) {
    error.textContent = err instanceof Error ? err.message : 'Something went wrong.';
    error.hidden = false;
  }
});

document.querySelector<HTMLButtonElement>('#copy')!.addEventListener('click', () => {
  navigator.clipboard.writeText(short.textContent ?? '');
});
```

- [ ] **Step 3: Build and verify the output**

Run: `npm install && npm run build`
Expected: `web/dist/index.html` and `web/dist/assets/index-*.js` (+ `.css`) exist.

Run: `grep -o 'id="url"' web/dist/index.html`
Expected: `id="url"` (the built page keeps the markup the tests assert on).

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: vite vanilla-ts web ui consuming lonk-client"
```

---

### Task 3: Embed the UI in lonkd with `LONK_WEB_DIR` override

**Files:**
- Create: `crates/lonkd/build.rs`, `crates/lonkd/src/web_assets.rs`, `crates/lonkd/tests/web_override_live.rs`
- Modify: `crates/lonkd/Cargo.toml`, `crates/lonkd/src/lib.rs`, `e2e/package.json`
- Delete: `crates/lonkd/static/` (git rm -r)

**Interfaces:**
- Consumes: `web/dist` (Task 2). Existing: `lonkd::rocket_app`, the default JSON catcher (missing assets 404 through it).
- Produces: `web_assets::{WebSource, web_source_from_env, index, assets}`; `rocket_app` manages a `WebSource`. Route precedence unchanged: `/<id>` still beats the `rank = 20` catch-all for single segments.

- [ ] **Step 1: build.rs — the existence check (never runs npm)**

`crates/lonkd/build.rs`:

```rust
fn main() {
  println!("cargo:rerun-if-changed=../../web/dist");
  let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
  let index = std::path::Path::new(&manifest_dir).join("../../web/dist/index.html");
  if !index.exists() {
    panic!("web/dist not found - build the web UI first: npm install && npm run build (requires node)");
  }
}
```

- [ ] **Step 2: The asset module**

Add to `crates/lonkd/Cargo.toml` `[dependencies]`:

```toml
rust-embed = "8"
```

`crates/lonkd/src/web_assets.rs`:

```rust
use rocket::http::ContentType;
use rocket::State;
use std::path::{Path, PathBuf};

#[derive(rust_embed::RustEmbed)]
#[folder = "../../web/dist"]
struct Assets;

/// Where UI bytes come from, resolved once at startup.
pub enum WebSource {
  Embedded,
  Dir(PathBuf),
}

/// LONK_WEB_DIR set and non-empty -> serve that directory; else embedded.
pub fn web_source_from_env() -> WebSource {
  match std::env::var("LONK_WEB_DIR") {
    Ok(dir) if !dir.is_empty() => WebSource::Dir(PathBuf::from(dir)),
    _ => WebSource::Embedded,
  }
}

fn content_type_for(rel: &str) -> ContentType {
  Path::new(rel)
    .extension()
    .and_then(|ext| ext.to_str())
    .and_then(ContentType::from_extension)
    .unwrap_or(ContentType::Bytes)
}

fn load(source: &WebSource, rel: &str) -> Option<Vec<u8>> {
  match source {
    WebSource::Embedded => Assets::get(rel).map(|file| file.data.into_owned()),
    WebSource::Dir(root) => {
      let root = root.canonicalize().ok()?;
      let full = root.join(rel).canonicalize().ok()?;
      if !full.starts_with(&root) {
        return None; // path escaped the override root
      }
      std::fs::read(full).ok()
    }
  }
}

#[rocket::get("/")]
pub fn index(source: &State<WebSource>) -> Option<(ContentType, Vec<u8>)> {
  load(source, "index.html").map(|bytes| (ContentType::HTML, bytes))
}

/// Multi-segment asset paths (e.g. assets/index-<hash>.js). Single segments
/// still match /<id> first, exactly as with the old FileServer.
#[rocket::get("/<path..>", rank = 20)]
pub fn assets(path: PathBuf, source: &State<WebSource>) -> Option<(ContentType, Vec<u8>)> {
  let rel = path.to_str()?;
  load(source, rel).map(|bytes| (content_type_for(rel), bytes))
}
```

`crates/lonkd/src/lib.rs` — full replacement:

```rust
pub mod db;
pub mod routes;
pub mod web_assets;

use rocket::{Build, Rocket};

pub fn rocket_app(db_path: &str) -> Rocket<Build> {
  let db = db::Db::open(db_path).expect("failed to open database");
  rocket::build()
    .manage(db)
    .manage(web_assets::web_source_from_env())
    .mount(
      "/",
      rocket::routes![
        routes::create_link,
        routes::follow_link,
        routes::qr_svg,
        routes::valid_url,
        routes::link_status,
        web_assets::index,
        web_assets::assets
      ],
    )
    .register("/", rocket::catchers![routes::default_catcher])
}
```

Delete the old assets:

```bash
git rm -r crates/lonkd/static
```

- [ ] **Step 3: Existing guard passes (embedded path)**

Run: `cargo test -p lonkd --test api index_serves_create_form`
Expected: PASS — `GET /` now serves the embedded `web/dist/index.html`, which contains `id="url"`.

Run: `cargo test --workspace`
Expected: all suites green.

- [ ] **Step 4: Live test for the override (port 8912)**

`crates/lonkd/tests/web_override_live.rs`:

```rust
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

// Port registry: 8912 belongs to this file. One server alive at a time.
const PORT: u16 = 8912;

struct ServerGuard(Child);

impl Drop for ServerGuard {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

fn start_server(dbdir: &std::path::Path, web_dir: Option<&std::path::Path>) -> ServerGuard {
  let mut cmd = Command::new(env!("CARGO_BIN_EXE_lonkd"));
  cmd
    .env("LONK_DB", dbdir.join("web.db"))
    .env("ROCKET_ADDRESS", "127.0.0.1")
    .env("ROCKET_PORT", PORT.to_string())
    .env("ROCKET_LOG_LEVEL", "off")
    .env_remove("LONK_WEB_DIR")
    .stdout(Stdio::null())
    .stderr(Stdio::null());
  if let Some(dir) = web_dir {
    cmd.env("LONK_WEB_DIR", dir);
  }
  let child = cmd.spawn().expect("spawn lonkd");
  let deadline = Instant::now() + Duration::from_secs(15);
  while Instant::now() < deadline {
    if TcpStream::connect(("127.0.0.1", PORT)).is_ok() {
      return ServerGuard(child);
    }
    std::thread::sleep(Duration::from_millis(100));
  }
  panic!("lonkd did not start listening on {PORT}");
}

fn get_index() -> String {
  ureq::get(&format!("http://127.0.0.1:{PORT}/"))
    .call()
    .expect("GET /")
    .into_string()
    .expect("body")
}

#[test]
fn web_dir_overrides_embedded_ui() {
  let tmp = tempfile::tempdir().unwrap();
  let custom = tmp.path().join("custom-ui");
  std::fs::create_dir(&custom).unwrap();
  std::fs::write(
    custom.join("index.html"),
    "<!DOCTYPE html><html><body>custom-ui-marker</body></html>",
  )
  .unwrap();
  std::fs::write(custom.join("extra.css"), "body{}").unwrap();

  // override: serves the bespoke directory
  {
    let _server = start_server(tmp.path(), Some(&custom));
    let body = get_index();
    assert!(body.contains("custom-ui-marker"), "body: {body}");
    let css = ureq::get(&format!("http://127.0.0.1:{PORT}/extra.css"))
      .call()
      .expect("GET css");
    let content_type = css.header("Content-Type").expect("content-type header");
    assert!(content_type.starts_with("text/css"), "got {content_type}");
  } // ServerGuard dropped -> port free

  // default: embedded UI
  let _server = start_server(tmp.path(), None);
  let body = get_index();
  assert!(body.contains("id=\"url\""), "body: {body}");
}
```

Run: `cargo test -p lonkd --test web_override_live`
Expected: PASS (this is new behavior implemented in Step 2; if it fails, the implementation is wrong — fix `web_assets.rs`, not the test).

- [ ] **Step 5: Wire the e2e pretest and run the browser suite**

In `e2e/package.json`, change scripts to:

```json
  "scripts": {
    "pretest": "npm --prefix .. run build",
    "test": "playwright test"
  }
```

Run: `cd e2e && npm test`
Expected: 7 passed — a real browser loading the embedded Vite bundle (hashed assets served by the catch-all, `main.ts` running through lonk-client).

- [ ] **Step 6: Full suite and commit**

Run: `cargo fmt && cargo test --workspace`
Expected: all green.

```bash
git add -A && git commit -m "feat: embed vite ui in lonkd with LONK_WEB_DIR runtime override"
```

---

### Task 4: `deploy/` daemon files + README sections

**Files:**
- Create: `deploy/lonkd.service`, `deploy/com.lonk.lonkd.plist`
- Modify: `README.md`

**Interfaces:**
- Consumes: the single-binary property from Task 3 (no `static/` dir needed at runtime).
- Produces: user-facing deploy artifacts; no code interfaces.

- [ ] **Step 1: systemd unit**

`deploy/lonkd.service`:

```ini
# lonk server as a systemd service.
# Install:
#   sudo cp target/release/lonkd /usr/local/bin/lonkd
#   sudo cp deploy/lonkd.service /etc/systemd/system/lonkd.service
#   sudo systemctl enable --now lonkd
[Unit]
Description=lonk link shortener
After=network.target

[Service]
ExecStart=/usr/local/bin/lonkd
Environment=LONK_DB=/var/lib/lonk/lonk.db
# Bind beyond localhost only if you accept that /<id>/status lets anyone who
# can reach lonkd probe stored URLs (see the README's dead-link-checks note):
#Environment=ROCKET_ADDRESS=0.0.0.0
Environment=ROCKET_PORT=8000
# Serve a bespoke UI from disk instead of the embedded one:
#Environment=LONK_WEB_DIR=/var/lib/lonk/web
DynamicUser=yes
StateDirectory=lonk
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

- [ ] **Step 2: launchd LaunchDaemon**

`deploy/com.lonk.lonkd.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!--
  lonk server as a macOS LaunchDaemon (starts at boot, runs as root).
  Install:
    sudo cp target/release/lonkd /usr/local/bin/lonkd
    sudo mkdir -p /usr/local/var/lonk
    sudo cp deploy/com.lonk.lonkd.plist /Library/LaunchDaemons/
    sudo launchctl bootstrap system /Library/LaunchDaemons/com.lonk.lonkd.plist
  For a login-time user agent instead, copy to ~/Library/LaunchAgents and use
  `launchctl bootstrap gui/$(id -u) ...` (then LONK_DB may use your home dir).
-->
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.lonk.lonkd</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/local/bin/lonkd</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>LONK_DB</key>
    <string>/usr/local/var/lonk/lonk.db</string>
    <key>ROCKET_PORT</key>
    <string>8000</string>
    <!-- Serve a bespoke UI from disk instead of the embedded one:
    <key>LONK_WEB_DIR</key>
    <string>/usr/local/var/lonk/web</string>
    -->
  </dict>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
</dict>
</plist>
```

Run: `plutil -lint deploy/com.lonk.lonkd.plist`
Expected: `deploy/com.lonk.lonkd.plist: OK`

- [ ] **Step 3: README sections**

In `README.md`:

1. In the Development section, replace the test-commands block with:

```markdown
```bash
npm install && npm run build               # build lonk-client + the web UI (required before cargo)
cargo test --workspace                     # unit + integration tests
cd e2e && npm test                         # Playwright e2e (builds the UI, spawns lonkd)
cd e2e && BASE_URL=https://s.example.com npm test   # against a deployed instance
```

Building `lonkd` requires `web/dist` to exist — `build.rs` fails with the
command to run if it doesn't. Cargo never invokes npm itself.
```

2. Add after the Development section:

```markdown
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
```

- [ ] **Step 4: Prove the single-binary property**

```bash
npm run build && cargo build --release -p lonkd
TESTDIR=$(mktemp -d)
cp target/release/lonkd "$TESTDIR/"
cd "$TESTDIR" && LONK_DB="$TESTDIR/t.db" ROCKET_PORT=8913 ./lonkd &
sleep 2
curl -s http://127.0.0.1:8913/ | grep -o 'id="url"'   # expect: id="url"
kill %1; cd -
```

Expected: the binary, run from an empty directory with no repo checkout in sight, serves the full UI. (Port 8913 is used only for this manual check, never by a test.)

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: systemd and launchd deploy files, daemon and ui-hacking docs"
```

---

### Task 5: `docs/guide/` + GitHub Pages workflow

**Files:**
- Create: `docs/guide/_config.yml`, `docs/guide/index.md`, `docs/guide/server.md`, `docs/guide/cli.md`, `docs/guide/headers.md`, `docs/guide/customizing.md`, `docs/guide/deployment.md`, `.github/workflows/docs.yml`
- Modify: `README.md` (link to the guide)

**Interfaces:**
- Consumes: everything shipped (docs must match reality — verify each command/flag/path/response against the code, same standard as previous README tasks).
- Produces: the published guide at `https://georgeipsum.github.io/lonk/`.

- [ ] **Step 1: Jekyll config and workflow**

`docs/guide/_config.yml`:

```yaml
title: lonk
description: Self-hosted link shortener with REST API, QR codes, and a CLI
theme: jekyll-theme-cayman
baseurl: "/lonk"
```

`.github/workflows/docs.yml`:

```yaml
# Publishes docs/guide/ to GitHub Pages.
# One-time setup: repo Settings -> Pages -> Source = "GitHub Actions".
name: docs
on:
  push:
    branches: [main]
    paths: ['docs/guide/**', '.github/workflows/docs.yml']
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: pages
  cancel-in-progress: true

jobs:
  deploy:
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/configure-pages@v5
      - uses: actions/jekyll-build-pages@v1
        with:
          source: ./docs/guide
          destination: ./_site
      - uses: actions/upload-pages-artifact@v3
      - id: deployment
        uses: actions/deploy-pages@v4
```

- [ ] **Step 2: Write the six pages**

Every page starts with Jekyll front matter (`---\ntitle: <Title>\n---`) and, except the index, a first line `[← lonk guide](index.md)`. Markdown links between pages use `.md` targets (GitHub Pages' jekyll-relative-links converts them). Content per page — port from the named source and verify against the code; every command and response shape must be real:

- `index.md` — one-paragraph pitch (self-hosted shortener; REST API + QR + response headers + dead-link checks + CLI + embeddable UI); a five-command quick start:

  ```bash
  git clone https://github.com/GeorgeIpsum/lonk && cd lonk
  npm install && npm run build
  cargo run --bin lonkd            # web UI + API on http://127.0.0.1:8000
  cargo install --path crates/lonk-cli
  lonk setup http://127.0.0.1:8000 && lonk https://example.com/some/long/url
  ```

  and a linked table of contents to the other five pages.
- `server.md` — running `lonkd`; an env-var table (`LONK_DB`, `LONK_CONFIG_DIR`, `LONK_WEB_DIR`, `ROCKET_ADDRESS`, `ROCKET_PORT` — meaning + default for each, defaults verified against `crates/lonk-core/src/paths.rs` and `crates/lonkd/src/main.rs`); the full API reference — the README table expanded so each endpoint gets a curl example with its real request and response JSON (create with and without headers, valid/invalid `POST /api/valid`, the 303 + Location redirect shown with `curl -i`, `GET /<id>/qr`, `GET /<id>/status` alive/dead/unknown shapes).
- `cli.md` — the README CLI section expanded: install, `setup` + interactive first-run prompt + `--profile`, shortening multiple URLs (input-order, fail-fast), `-H` repeatable header flag with the validation rules summary, `--qr` / `--qr-svg` (single-URL rule), `--valid`, `status <slug|short-url>`, and the exit-code table (0 / 1 / 2) with what each covers.
- `headers.md` — the README "Custom response headers" section expanded: what redirect response headers can and cannot do (not injected into the follow-up request), the four use cases (Set-Cookie / Cache-Control / Referrer-Policy / X-*), the validation rules (token names ≤128 bytes, values ≤1024 visible-ASCII, no CR/LF/NUL, max 16 pairs) and the six denylisted structural headers, with one worked curl example showing the pairs echoed and carried on the 303.
- `customizing.md` — the README "Hacking on the UI" section expanded: repo UI layout (`web/`, `packages/lonk-client`), the dev-proxy loop, the two-step build and the `build.rs` guard message, serving a bespoke UI via `LONK_WEB_DIR` (works with any static files; no Rust or Node rebuild), and a complete minimal bespoke example: a single `index.html` using `lonk-client` from a CDN-less inline `<script type="module">` that calls `fetch('/api/links', ...)` directly, plus the same example written with `createClient()` for projects that vendor the package.
- `deployment.md` — the README "Running as a daemon" section expanded: release build, the two install walkthroughs copied from the deploy files' headers (systemd + launchd), log locations (`journalctl -u lonkd`; launchd StandardOutPath note that the plist doesn't set one by default), the `ROCKET_ADDRESS` exposure caveat with the status probe-oracle explanation, and backup advice (the DB is one SQLite file; `cp` it).

- [ ] **Step 3: Link the guide from the README**

Under the top-line description in `README.md`, add:

```markdown
**[Full guide](https://georgeipsum.github.io/lonk/)** — API reference, CLI, custom headers, deployment.
```

- [ ] **Step 4: Verify accuracy**

For each page: run every command shown (server briefly on a temp config dir; CLI against it; curls). Fix any drift. Run `cd e2e && npm test` once more if `web/` was touched (it should not be in this task).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "docs: github pages guide and deploy workflow"
```

Note for the controller (not the implementer): after merge to main, the user must flip repo Settings → Pages → Source = "GitHub Actions" once; the workflow header says so.
